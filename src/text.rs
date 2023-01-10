use core::iter::{self};
use core::mem::replace;
use core::ops::Range;
use itertools::Itertools;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;

pub struct Text {
    source: String,
    line_break: String,
    start_offset: usize,
    end_offset: usize,
    lines: Vec<Range<usize>>,
}

pub struct TextLines<'a> {
    text: &'a Text,
    line: usize,
}

impl<'a> Iterator for TextLines<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.line == self.text.lines.len() { return None; }
        let item = self.text.lines[self.line].clone();
        self.line += 1;
        Some(&self.text.source[item])
    }
}

impl Text {
    pub fn new(source: String, line_break: String) -> Self {
        Text {
            source,
            line_break,
            start_offset: 0,
            end_offset: 0,
            lines: Vec::new()
        }
    }

    pub fn height(&self) -> usize { self.lines.len() }

    fn load_next_line(&mut self) {
        let mut offset = self.end_offset;
        loop {
            let text = &self.source[offset ..];
            if text.starts_with(&self.line_break) {
                offset += self.line_break.len();
                break;
            }
            let skip = text.chars().next().map(|x| x.len_utf8());
            if let Some(skip) = skip {
                offset += skip;
            } else {
                break;
            }
        }
        self.lines.push(self.end_offset .. offset);
        self.end_offset = offset;
    }

    fn load_prev_line(&mut self) {
        let mut offset = self.start_offset;
        if offset == 0 { return; }
        offset -= self.line_break.len();
        loop {
            let text = &self.source[.. offset];
            if text.ends_with(&self.line_break) { break; }
            let skip = text.chars().rev().next().map(|x| x.len_utf8());
            if let Some(skip) = skip {
                offset -= skip;
            } else {
                break;
            }
        }
        self.lines.insert(0, offset .. self.start_offset);
        self.start_offset = offset;
    }

    fn drop_first_line(&mut self) {
        let line = self.lines.remove(0);
        self.start_offset += line.len();
    }

    fn drop_last_line(&mut self) {
        let line = self.lines.pop().unwrap();
        self.end_offset -= line.len();
    }

    pub fn resize(&mut self, lines_count: usize) {
        if lines_count < self.lines.len() {
            self.lines.truncate(lines_count);
        } else if lines_count > self.lines.len() {
            for _ in self.lines.len() .. lines_count {
                self.load_next_line();
            }
        }
    }

    pub fn line_down(&mut self) {
        if !self.lines.is_empty() {
            self.drop_first_line();
            self.load_next_line();
        }
    }

    pub fn line_up(&mut self) {
        if !self.lines.is_empty() {
            self.drop_last_line();
            self.load_prev_line();
        }
    }

    pub fn lines(&self) -> TextLines {
        TextLines { text: self, line: 0 }
    }

    pub fn move_left(&self, line: usize, column: usize) -> usize {
        let line_range = self.lines[line].clone();
        let width = self.source[line_range].graphemes(true)
            .map(|grapheme| grapheme.chars().map(|c| c.width().unwrap_or(1)).sum::<usize>())
            .chain(iter::repeat(1))
            .scan(0, |column, width| { *column += width; Some((width, *column)) })
            .find_map(|(w, c)| if c == column { Some(w) } else { None })
            .unwrap();
        return column - width;
    }

    pub fn move_right(&self, line: usize, column: usize) -> usize {
        let line_range = self.lines[line].clone();
        let width = self.source[line_range].graphemes(true)
            .map(|grapheme| grapheme.chars().map(|c| c.width().unwrap_or(1)).sum::<usize>())
            .chain(iter::repeat(1))
            .scan(0, |column, width| Some((width, replace(column, *column + width))))
            .find_map(|(w, c)| if c == column { Some(w) } else { None })
            .unwrap();
        return column + width;
    }

    pub fn coerce_column(&self, line: usize, column: usize) -> usize {
        let line_range = self.lines[line].clone();
        self.source[line_range].graphemes(true)
            .map(|grapheme| grapheme.chars().map(|c| c.width().unwrap_or(1)).sum::<usize>())
            .chain(iter::repeat(1))
            .scan(0, |column, width| { let prev_column = replace(column, *column + width); Some(prev_column .. *column) })
            .find(|columns| columns.contains(&column))
            .unwrap()
            .start
    }

    pub fn insert(&mut self, line: usize, column: usize, c: char) {
        assert_ne!(self.line_break.chars().exactly_one().ok(), Some(c));
        let line_range = self.lines[line].clone();
        let i = self.source[line_range.clone()].grapheme_indices(true)
            .map(|(i, grapheme)| (i, grapheme.chars().map(|c| c.width().unwrap_or(1)).sum::<usize>()))
            .scan(0, |column, (i, width)| Some((i, replace(column, *column + width))))
            .find_map(|(i, c)| if c == column { Some(i) } else { None })
            .unwrap();
        let offset = line_range.start + i;
        self.source.insert(offset, c);
        let len = c.len_utf8();
        self.lines[line].end += len;
        for line in &mut self.lines[line + 1 ..] {
            line.start += len;
            line.end += len;
        }
    }

    pub fn insert_str(&mut self, line: usize, column: usize, s: &str) {
        assert!(!s.contains(&self.line_break));
        let line_range = self.lines[line].clone();
        let i = self.source[line_range.clone()].grapheme_indices(true)
            .map(|(i, grapheme)| (i, grapheme.chars().map(|c| c.width().unwrap_or(1)).sum::<usize>()))
            .scan(0, |column, (i, width)| Some((i, replace(column, *column + width))))
            .find_map(|(i, c)| if c == column { Some(i) } else { None })
            .unwrap();
        let offset = line_range.start + i;
        self.source.insert_str(offset, s);
        let len = s.len();
        self.lines[line].end += len;
        for line in &mut self.lines[line + 1 ..] {
            line.start += len;
            line.end += len;
        }
    }
}
