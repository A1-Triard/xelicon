use components_arena::{Arena, Component, Id, NewtypeComponentId};
use core::cmp::{max, min};
use core::mem::{MaybeUninit, forget, replace};
use core::ops::Range;
use core::ptr::{self};
use core::slice::{self};
use iter_identify_first_last::IteratorIdentifyFirstLastExt;
use itertools::Itertools;
use macro_attr_2018::macro_attr;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub struct Text {
    content: String,
    line_break: String,
    views: Arena<TextViewData>,
    cursors: Arena<TextCursorData>,
}

impl Text {
    pub fn new(content: String, line_break: String) -> Self {
        assert!(!line_break.is_empty() && line_break.chars().all(|c| line_break.chars().filter(|&x| x == c).count() == 1));
        Text {
            content,
            line_break,
            views: Arena::new(),
            cursors: Arena::new(),
        }
    }

    pub fn insert(&mut self, pos: TextCursor, s: &str) -> Result<(), OomErr> {
        assert!(!s.contains(&self.line_break));
        let pos_data = &self.cursors[pos.0];
        let line = pos_data.line;
        let column = pos_data.column;
        let index = pos_data.index;
        let spaces = pos_data.spaces;
        self.content.try_reserve(spaces.checked_add(s.len()).ok_or(OomErr)?).map_err(|_| OomErr)?;
        let line_end = self.content[index ..].find(&self.line_break).map_or(self.content.len(), |x| index + x);
        let old_width: usize = self.content[index .. line_end].graphemes(true).map(grapheme_width).sum();
        self.content.insert_str(index, s);
        unsafe {
            ptr::copy(
                self.content.as_ptr().add(index),
                self.content.as_mut_ptr().add(index + spaces),
                self.content.len() - index
            );
            slice::from_raw_parts_mut(self.content.as_mut_ptr().add(index) as *mut MaybeUninit<u8>, spaces)
                .fill(MaybeUninit::new(b' '));
            let new_len = self.content.len() + spaces;
            self.content.as_mut_vec().set_len(new_len);
        }
        let new_width: Option<usize> = self.content[index .. line_end + spaces + s.len()]
            .graphemes(true).map(grapheme_width).try_fold(0usize, |sum, w| sum.checked_add(w))
            .filter(|&x| x <= isize::MAX as usize);
        let Some(new_width) = new_width else {
            self.content.replace_range(index .. index + spaces + s.len(), "");
            return Err(OomErr);
        };
        let shift = new_width - old_width;
        if let Some(max_cursor_column) = self.cursors.items().values().filter(|x| x.line == line).map(|x| x.column).max() {
            if isize::MAX as usize - max_cursor_column < shift {
                self.content.replace_range(index .. index + spaces + s.len(), "");
                return Err(OomErr);
            }
        }
        for cursor in self.cursors.items_mut().values_mut() {
            if cursor.line == line {
                if cursor.column > column {
                    cursor.column += shift;
                    cursor.index += max((spaces + s.len()) as isize - cursor.spaces as isize, 0) as usize;
                    cursor.spaces = max(cursor.spaces as isize - (spaces + s.len()) as isize, 0) as usize;
                } else {
                    cursor.index += replace(&mut cursor.spaces, 0);
                }
            } else if cursor.line > line {
                cursor.index += spaces + s.len();
            }
        }
        for view in self.views.items_mut().values_mut() {
            if (view.lines_start .. view.lines_start + view.lines.len()).contains(&line) {
                view.range.end += spaces + s.len();
                let recalc_line_view = if view.columns.end > column { Some(view.columns.clone()) } else { None };
                let changed_line = &mut view.lines[line - view.lines_start];
                changed_line.display_cache = None;
                changed_line.range.end += spaces + s.len();
                if let Some(recalc_line_view) = recalc_line_view {
                    changed_line.view = changed_line.range.start .. changed_line.range.start;
                    changed_line.expand_to_right(recalc_line_view.end, &self.content, &self.line_break);
                    changed_line.shrink_from_left(recalc_line_view.start, &self.content);
                }
                for line in view.lines[line - view.lines_start ..].iter_mut().skip(1) {
                    line.range.end += spaces + s.len();
                    line.range.start += spaces + s.len();
                    line.view.end += spaces + s.len();
                    line.view.start += spaces + s.len();
                }
            } else if view.lines_start > line {
                view.range.end += spaces + s.len();
                view.range.start += spaces + s.len();
                for line in &mut view.lines {
                    line.range.end += spaces + s.len();
                    line.range.start += spaces + s.len();
                    line.view.end += spaces + s.len();
                    line.view.start += spaces + s.len();
                }
            }
        }
        Ok(())
    }
}

fn grapheme_width(g: &str) -> usize {
    let g_width = g.width();
    if g_width != 0 { return g_width; }
    let c = g.chars().exactly_one().unwrap();
    if c <= '\x7F' { return 2; }
    '\u{2426}'.width().unwrap()
}

fn strip_line_break<'a>(text: &'a str, line_break: &str) -> &'a str {
    text.strip_suffix(line_break).unwrap_or(text)
}

struct Line {
    range: Range<usize>,
    view: Range<usize>,
    offset: usize,
    spaces: usize,
    display_cache: Option<String>,
}

impl Line {
    fn new(range: Range<usize>, columns: Range<usize>, text: &str, line_break: &str) -> Self {
        debug_assert!(range.end >= range.start);
        let mut this = Line {
            range: range.clone(),
            view: range.start .. range.start,
            offset: 0, spaces: 0, display_cache: None
        };
        if columns.end > 0 {
            this.expand_to_right(columns.end, text, line_break);
        }
        if columns.start > 0 {
            this.shrink_from_left(columns.start, text);
        }
        this
    }

    fn prepare_display(&mut self, text: &str, line_break: &str) -> Result<(), OomErr> {
        if self.display_cache.is_some() { return Ok(()); }
        let text = &text[self.view.clone()];
        let mut display = String::new();
        display.try_reserve(text.len()).map_err(|_| OomErr)?; // approx.
        for (is_first, g) in strip_line_break(text, line_break).graphemes(true).identify_first() {
            if is_first && self.offset != 0 { continue; }
            let g_width = g.width();
            if g_width != 0 {
                display.try_reserve(g.len()).map_err(|_| OomErr)?;
                g.nfc().for_each(|c| display.push(c));
                continue;
            }
            let c = g.chars().exactly_one().unwrap();
            if c <= '\x7F' {
                display.try_reserve(2).map_err(|_| OomErr)?;
                display.push('^');
                display.push((((c as u8) + 0x40) % 0x80) as char);
                continue;
            }
            display.try_reserve('\u{2426}'.len_utf8()).map_err(|_| OomErr)?;
            display.push('\u{2426}');
        }
        self.display_cache = Some(display);
        Ok(())
    }

    fn display(&self) -> (usize, &str) {
        (self.offset, self.display_cache.as_ref().unwrap())
    }

    fn expand_to_left(&mut self, width: usize, text: &str) {
        if self.offset >= width {
            self.offset -= width;
            if self.offset == 0 {
                self.display_cache = None;
            }
            return;
        }
        self.display_cache = None;
        let mut width = width - self.offset;
        for (i, g) in text[self.range.start .. self.view.start].grapheme_indices(true).rev() {
            let g_width = grapheme_width(g);
            if g_width >= width {
                self.view.start = self.range.start + i;
                self.offset = g_width - width;
                return;
            } else {
                width -= g_width;
            }
        }
        unreachable!()
    }

    fn expand_to_right(&mut self, width: usize, text: &str, line_break: &str) {
        self.display_cache = None;
        let mut width = width + self.spaces;
        for g in strip_line_break(&text[self.view.end .. self.range.end], line_break).graphemes(true) {
            let g_width = grapheme_width(g);
            if g_width > width {
                break;
            } else {
                self.view.end += g.len();
                width -= g_width;
            }
        }
        self.spaces = width;
    }

    fn shrink_from_left(&mut self, width: usize, text: &str) {
        self.display_cache = None;
        let mut width = width + self.offset;
        for g in text[self.view.clone()].graphemes(true) {
            let g_width = grapheme_width(g);
            if g_width > width {
                break;
            } else {
                self.view.start += g.len();
                width -= g_width;
            }
        }
        self.offset = width;
    }

    fn shrink_from_right(&mut self, width: usize, text: &str) {
        if self.spaces >= width {
            self.spaces -= width;
            return;
        }
        self.display_cache = None;
        let mut width = width - self.spaces;
        for (i, g) in text[self.view.clone()].grapheme_indices(true).rev() {
            let g_width = grapheme_width(g);
            if g_width >= width {
                self.view.end = self.view.start + i;
                self.spaces = g_width - width;
                return;
            } else {
                width -= g_width;
            }
        }
        unreachable!()
    }
}

macro_attr! {
    #[derive(Component!)]
    struct TextViewData {
        range: Range<usize>,
        dummy_lines: Range<usize>,
        lines_start: usize,
        lines: Vec<Line>,
        columns: Range<usize>,
    }
}

macro_attr! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, NewtypeComponentId!)]
    pub struct TextView(Id<TextViewData>);
}

#[derive(Debug)]
pub struct OomErr;

impl TextView {
    pub fn new(text: &mut Text) -> Self {
        text.views.insert(|id| (TextViewData {
            range: 0 .. 0,
            dummy_lines: 0 .. 0,
            lines_start: 0,
            lines: Vec::new(),
            columns: 0 .. 0
        }, TextView(id)))
    }

    pub fn drop(self, text: &mut Text) {
        text.views.remove(self.0);
    }

    pub fn prepare_display(self, text: &mut Text) -> Result<(), OomErr> {
        let data = &mut text.views[self.0];
        for line in &mut data.lines {
            line.prepare_display(&text.content, &text.line_break)?;
        }
        Ok(())
    }

    pub fn display_line(self, line: usize, text: &Text) -> (usize, &str) {
        let data = &text.views[self.0];
        let lines_start = data.lines_start;
        data.lines[line.checked_sub(lines_start).unwrap()].display()
    }

    pub fn lines(self, text: &Text) -> Range<usize> {
        let data = &text.views[self.0];
        data.lines_start .. data.lines_start + data.lines.len()
    }

    pub fn columns(self, text: &Text) -> Range<usize> {
        let data = &text.views[self.0];
        data.columns.clone()
    }

    pub fn set_columns(self, columns: Range<usize>, text: &mut Text) {
        let data = &mut text.views[self.0];
        if columns.start < data.columns.start {
            let width = data.columns.start - columns.start;
            for line in &mut data.lines {
                line.expand_to_left(width, &text.content);
            }
        }
        if columns.end > data.columns.end {
            let width = columns.end - data.columns.end;
            for line in &mut data.lines {
                line.expand_to_right(width, &text.content, &text.line_break);
            }
        }
        if columns.start > data.columns.start {
            let width = columns.start - data.columns.start;
            for line in &mut data.lines {
                line.shrink_from_left(width, &text.content);
            }
        }
        if columns.end < data.columns.end {
            let width = data.columns.end - columns.end;
            for line in &mut data.lines {
                line.shrink_from_right(width, &text.content);
            }
        }
        data.columns = columns;
    }

    pub fn scroll_lines(self, lines_start: usize, text: &mut Text) -> Result<(), OomErr> {
        let data = &mut text.views[self.0];
        let lines_len = data.lines.len();
        if lines_start < data.lines_start {
            let keep = max((lines_start + lines_len) as isize - data.lines_start as isize, 0);
            for line in data.lines[keep as usize ..].iter_mut().rev() {
                line.display_cache = None;
                data.range.end = line.range.start;
                if line.range.is_empty() { data.dummy_lines.end -= 1; }
            }
            unsafe { ptr::copy(
                data.lines.as_ptr(),
                data.lines.as_mut_ptr().offset(lines_len as isize - keep),
                keep as usize
            ); }
            for _ in 0 .. max(data.lines_start as isize - (lines_start + lines_len) as isize, 0) {
                if data.dummy_lines.start != 0 {
                    data.dummy_lines.start -= 1;
                } else {
                    data.range.start = strip_line_break(&text.content[.. data.range.start], &text.line_break)
                        .rfind(&text.line_break).map_or(0, |x| x + text.line_break.len());
                }
                if data.dummy_lines.end != 0 {
                    data.dummy_lines.end -= 1;
                } else {
                    data.range.end = strip_line_break(&text.content[.. data.range.end], &text.line_break).rfind(&text.line_break)
                        .map_or(0, |x| x + text.line_break.len());
                }
            }
            for line in data.lines[.. lines_len - keep as usize].iter_mut().rev() {
                let line_end = if data.dummy_lines.start != 0 {
                    data.dummy_lines.start -= 1;
                    debug_assert_eq!(data.range.start, data.range.end);
                    data.range.start
                } else {
                    let line_end = data.range.start;
                    data.range.start = strip_line_break(&text.content[.. data.range.start], &text.line_break)
                        .rfind(&text.line_break).map_or(0, |x| x + text.line_break.len());
                    line_end
                };
                forget(replace(line,
                    Line::new(data.range.start .. line_end, data.columns.clone(), &text.content, &text.line_break)
                ));
            }
        } else if lines_start > data.lines_start {
            if lines_start > isize::MAX as usize || isize::MAX as usize - lines_start < lines_len { return Err(OomErr); }
            let keep = max((data.lines_start + lines_len) as isize - lines_start as isize, 0);
            for line in &mut data.lines[.. lines_len - keep as usize] {
                line.display_cache = None;
                data.range.start = line.range.end;
                if line.range.is_empty() { data.dummy_lines.start += 1; }
            }
            unsafe { ptr::copy(
                data.lines.as_ptr().offset(lines_len as isize - keep),
                data.lines.as_mut_ptr(),
                keep as usize
            ); }
            for _ in 0 .. max(lines_start as isize - (data.lines_start + lines_len) as isize, 0) {
                if data.range.start == text.content.len() {
                    data.dummy_lines.start += 1;
                } else {
                    data.range.start = text.content[data.range.start ..].find(&text.line_break)
                        .map_or(text.content.len(), |x| data.range.start + x + text.line_break.len());
                }
                if data.range.end == text.content.len() {
                    data.dummy_lines.end += 1;
                } else {
                    data.range.end = text.content[data.range.end ..].find(&text.line_break)
                        .map_or(text.content.len(), |x| data.range.end + x + text.line_break.len());
                }
            }
            for line in &mut data.lines[keep as usize ..] {
                let line_end = text.content[data.range.end ..].find(&text.line_break)
                    .map_or(text.content.len(), |x| data.range.end + x + text.line_break.len());
                forget(replace(line,
                    Line::new(data.range.end .. line_end, data.columns.clone(), &text.content, &text.line_break)
                ));
                if data.range.end == line_end {
                    data.dummy_lines.end += 1;
                } else {
                    data.range.end = line_end;
                }
            }
        }
        data.lines_start = lines_start;
        Ok(())
    }

    pub fn resize_lines(self, lines_len: usize, text: &mut Text) -> Result<(), OomErr> {
        let data = &mut text.views[self.0];
        if lines_len < data.lines.len() {
            data.lines.truncate(lines_len);
            data.range.end = data.lines.last().map_or(data.range.start, |x| x.range.end);
            let reduce_lines = data.lines.len() - lines_len;
            data.dummy_lines.end = max(data.dummy_lines.end as isize - reduce_lines as isize, 0) as usize;
            data.dummy_lines.start = min(data.dummy_lines.start, data.dummy_lines.end);
        } else if lines_len > data.lines.len() {
            if lines_len > isize::MAX as usize || isize::MAX as usize - lines_len < data.lines_start { return Err(OomErr); }
            data.lines.try_reserve(lines_len - data.lines.len()).map_err(|_| OomErr)?;
            for _ in data.lines.len() .. lines_len {
                let line_start = if data.range.end == text.content.len() {
                    data.dummy_lines.end += 1;
                    data.range.end
                } else {
                    let line_start = data.range.end;
                    data.range.end = text.content[line_start ..].find(&text.line_break)
                        .map_or(text.content.len(), |x| line_start + x + text.line_break.len());
                    line_start
                };
                data.lines.push(Line::new(line_start .. data.range.end, data.columns.clone(), &text.content, &text.line_break));
            }
        }
        Ok(())
    }
}

macro_attr! {
    #[derive(Clone, Component!)]
    struct TextCursorData {
        line: usize,
        column: usize,
        index: usize,
        spaces: usize,
        offset: usize,
    }
}

macro_attr! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, NewtypeComponentId!)]
    pub struct TextCursor(Id<TextCursorData>);
}

impl TextCursor {
    pub fn new(text: &mut Text) -> Self {
        text.cursors.insert(|id| (TextCursorData {
            line: 0,
            column: 0,
            index: 0,
            offset: 0,
            spaces: 0,
        }, TextCursor(id)))
    }

    pub fn drop(self, text: &mut Text) {
        text.cursors.remove(self.0);
    }

    pub fn clone(self, text: &mut Text) -> Self {
        let data = text.cursors[self.0].clone();
        text.cursors.insert(|id| (data, TextCursor(id)))
    }

    pub fn line(self, text: &Text) -> usize {
        let data = &text.cursors[self.0];
        data.line
    }

    pub fn column(self, text: &Text) -> usize {
        let data = &text.cursors[self.0];
        data.column
    }

    pub fn move_right(self, text: &mut Text) -> Result<(), OomErr> {
        let data = &mut text.cursors[self.0];
        if data.spaces != 0 {
            if isize::MAX as usize - 1 < data.column { return Err(OomErr); }
            if isize::MAX as usize - 1 < data.index + data.spaces { return Err(OomErr); }
            data.column += 1;
            data.spaces += 1;
            debug_assert_eq!(data.offset, 0);
            return Ok(());
        }
        let line = text.content[data.index ..].split(&text.line_break).next().unwrap();
        if let Some(g) = line.graphemes(true).next() {
            let width = grapheme_width(g);
            debug_assert!(width <= isize::MAX as usize);
            if isize::MAX as usize - width < data.column { return Err(OomErr); }
            if g.len() > isize::MAX as usize || isize::MAX as usize - g.len() < data.index { return Err(OomErr); }
            data.column += width;
            data.index += g.len();
            data.offset = 0;
        } else {
            if isize::MAX as usize - 1 < data.column { return Err(OomErr); }
            if isize::MAX as usize - 1 < data.index { return Err(OomErr); }
            data.column += 1;
            data.spaces = 1;
            data.offset = 0;
        }
        Ok(())
    }

    pub fn move_left(self, text: &mut Text) -> bool {
        let data = &mut text.cursors[self.0];
        if data.spaces != 0 {
            data.column -= 1;
            data.spaces -= 1;
            debug_assert_eq!(data.offset, 0);
            return true;
        }
        let line = text.content[.. data.index].rsplit(&text.line_break).next().unwrap();
        if let Some(g) = line.graphemes(true).rev().next() {
            let width = grapheme_width(g);
            data.column -= width;
            data.index -= g.len();
            data.offset = 0;
            true
        } else {
            false
        }
    }

    pub fn move_down(self, text: &mut Text) -> Result<bool, OomErr> {
        let data = &mut text.cursors[self.0];
        let Some(line_start) = text.content[data.index ..].find(&text.line_break) else { return Ok(false); };
        let line_start = data.index + line_start + text.line_break.len();
        let line_ = &text.content[line_start ..];
        if line_.is_empty() { return  Ok(false); }
        if data.line == isize::MAX as usize { return Err(OomErr); }
        let line = line_.split(&text.line_break).next().unwrap();
        let column = data.column + data.offset;
        let mut width = 0;
        for (i, g) in line.grapheme_indices(true) {
            let g_width = grapheme_width(g);
            if width + g_width > column {
                data.line += 1;
                data.index = line_start + i;
                data.spaces = 0;
                data.column = width;
                data.offset = column - width;
                return Ok(true);
            }
            width += g_width;
        }
        let index = line_start + line.len();
        let spaces = column - width;
        if isize::MAX as usize - index < spaces { return Err(OomErr); }
        data.line += 1;
        data.index = index;
        data.spaces = spaces;
        data.column = column;
        data.offset = 0;
        Ok(true)
    }

    pub fn move_up(self, text: &mut Text) -> bool {
        let data = &mut text.cursors[self.0];
        if data.line == 0 { return false; }
        data.line -= 1;
        let line_end = text.content[.. data.index].rfind(&text.line_break).unwrap();
        let line_ = &text.content[.. line_end];
        let line = line_.rsplit(&text.line_break).next().unwrap();
        let column = data.column + data.offset;
        let mut width = 0;
        for (i, g) in line.grapheme_indices(true) {
            let g_width = grapheme_width(g);
            if width + g_width > column {
                data.index = line_end - line.len() + i;
                data.spaces = 0;
                data.column = width;
                data.offset = column - width;
                return true;
            }
            width += g_width;
        }
        data.index = line_end;
        data.spaces = column - width;
        data.column = column;
        data.offset = 0;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(text_view: TextView, text: &mut Text) -> Vec<(usize, &str)> {
        text_view.prepare_display(text).unwrap();
        let mut res = Vec::new();
        for line in text_view.lines(text) {
            res.push(text_view.display_line(line, text));
        }
        res
    }

    fn assert_cursor(cursor: TextCursor, line: usize, column: usize, g: &str, text: &Text) {
        assert_eq!(cursor.line(text), line);
        assert_eq!(cursor.column(text), column);
        let index = text.cursors[cursor.0].index;
        assert!(text.content[index ..].starts_with(g));
    }

    #[test]
    fn view_display() {
        let text = &mut Text::new("First line.\r\n二 line.\r\nThird line.\r\n".into(), "\r\n".into());
        let view = TextView::new(text);
        view.resize_lines(2, text).unwrap();
        view.set_columns(1 .. 9, text);
        assert_eq!(&display(view, text), &[(0, "irst lin"), (1, " line.")]);
        view.set_columns(0 .. 1, text);
        assert_eq!(&display(view, text), &[(0, "F"), (0, "")]);
        view.set_columns(1 .. 4, text);
        assert_eq!(&display(view, text), &[(0, "irs"), (1, " l")]);
        view.scroll_lines(1, text).unwrap();
        assert_eq!(&display(view, text), &[(1, " l"), (0, "hir")]);
        view.scroll_lines(2, text).unwrap();
        assert_eq!(&display(view, text), &[(0, "hir"), (1, "")]);
        view.scroll_lines(1, text).unwrap();
        assert_eq!(&display(view, text), &[(1, " l"), (0, "hir")]);
    }

    #[test]
    fn scroll_far_down() {
        let text = &mut Text::new("First line.\r\n二 line.\r\nThird line.\r\n".into(), "\r\n".into());
        let view = TextView::new(text);
        view.resize_lines(3, text).unwrap();
        view.set_columns(1 .. 9, text);
        assert_eq!(&display(view, text), &[(0, "irst lin"), (1, " line."), (0, "hird lin")]);
        view.scroll_lines(7, text).unwrap();
        assert_eq!(&display(view, text), &[(1, ""), (1, ""), (1, "")]);
        view.scroll_lines(4, text).unwrap();
        assert_eq!(&display(view, text), &[(1, ""), (1, ""), (1, "")]);
        view.scroll_lines(0, text).unwrap();
        assert_eq!(&display(view, text), &[(0, "irst lin"), (1, " line."), (0, "hird lin")]);
    }

    #[test]
    fn cursor_move() {
        let text = &mut Text::new("First line.\r\nThe 二 line.\r\nThird line.\r\n".into(), "\r\n".into());
        let cursor = TextCursor::new(text);
        assert_cursor(cursor, 0, 0, "F", text);
        cursor.move_right(text).unwrap();
        assert_cursor(cursor, 0, 1, "i", text);
        cursor.move_right(text).unwrap();
        assert_cursor(cursor, 0, 2, "r", text);
        cursor.move_right(text).unwrap();
        assert_cursor(cursor, 0, 3, "s", text);
        cursor.move_right(text).unwrap();
        assert_cursor(cursor, 0, 4, "t", text);
        cursor.move_right(text).unwrap();
        assert_cursor(cursor, 0, 5, " ", text);
        cursor.move_down(text).unwrap();
        assert_cursor(cursor, 1, 4, "二", text);
        cursor.move_down(text).unwrap();
        assert_cursor(cursor, 2, 5, " ", text);
        cursor.move_up(text);
        assert_cursor(cursor, 1, 4, "二", text);
        cursor.move_left(text);
        assert_cursor(cursor, 1, 3, " ", text);
        cursor.move_right(text).unwrap();
        assert_cursor(cursor, 1, 4, "二", text);
        cursor.move_down(text).unwrap();
        assert_cursor(cursor, 2, 4, "d", text);
    }

    #[test]
    fn text_insert() {
        let text = &mut Text::new("First line.\r\nThe 二 line.\r\nThird line.\r\n".into(), "\r\n".into());
        let cursor_1 = TextCursor::new(text);
        cursor_1.move_down(text).unwrap();
        cursor_1.move_right(text).unwrap();
        cursor_1.move_right(text).unwrap();
        cursor_1.move_right(text).unwrap();
        cursor_1.move_right(text).unwrap();
        assert_cursor(cursor_1, 1, 4, "二", text);
        let cursor_2 = cursor_1.clone(text);
        cursor_2.move_right(text).unwrap();
        cursor_2.move_right(text).unwrap();
        assert_cursor(cursor_2, 1, 7, "l", text);
        let cursor_3 = cursor_2.clone(text);
        cursor_3.move_right(text).unwrap();
        cursor_3.move_right(text).unwrap();
        cursor_3.move_right(text).unwrap();
        cursor_3.move_right(text).unwrap();
        cursor_3.move_right(text).unwrap();
        cursor_3.move_right(text).unwrap();
        assert_cursor(cursor_3, 1, 13, "", text);
        let cursor_4 = cursor_3.clone(text);
        cursor_4.move_down(text).unwrap();
        assert_cursor(cursor_4, 2, 13, "", text);
        text.insert(cursor_1, "XXX").unwrap();
        assert_cursor(cursor_1, 1, 4, "X", text);
        assert_cursor(cursor_2, 1, 10, "l", text);
        assert_cursor(cursor_3, 1, 16, "", text);
        assert_cursor(cursor_4, 2, 13, "", text);
        text.insert(cursor_3, "二").unwrap();
        assert_cursor(cursor_1, 1, 4, "X", text);
        assert_cursor(cursor_2, 1, 10, "l", text);
        assert_cursor(cursor_3, 1, 16, "二", text);
        assert_cursor(cursor_4, 2, 13, "", text);
    }
}
