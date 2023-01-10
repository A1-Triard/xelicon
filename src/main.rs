#![feature(iter_collect_into)]

use tuifw_screen::{Bg, Event, Fg, Key, Point, Rect, Thickness, Vector};
use tuifw_window::{RenderPort, Window, WindowTree};
use tuifw::{RenderPortExt, WindowManager, WindowRenderer, WindowRendererState};
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

mod text;
use text::*;

struct App {
    window_renderer: WindowRenderer<App>,
    text: Text,
    cursor: Vector,
}

impl WindowRendererState for App {
    fn window_renderer(&self) -> &WindowRenderer<App> { &self.window_renderer }
}

fn control_char_map(c: char) -> char {
    match c {
        '\x00' => '\x00',
        '\x01' => '☺',
        '\x02' => '☻',
        '\x03' => '♥',
        '\x04' => '♦',
        '\x05' => '♣',
        '\x06' => '♠',
        '\x07' => '•',
        '\x08' => '◘',
        '\x09' => '○',
        '\x0A' => '┘', // '◙'
        '\x0B' => '♂',
        '\x0C' => '♀',
        '\x0D' => '♪',
        '\x0E' => '♫',
        '\x0F' => '☼',
  	'\x10' => '►',
        '\x11' => '◄',
        '\x12' => '↕',
        '\x13' => '‼',
        '\x14' => '¶',
        '\x15' => '§',
        '\x16' => '▬',
        '\x17' => '↨',
        '\x18' => '↑',
        '\x19' => '↓',
        '\x1A' => '→',
        '\x1B' => '←',
        '\x1C' => '∟',
        '\x1D' => '↔',
        '\x1E' => '▲',
        '\x1F' => '▼',
        c => c
    }
}

fn render_window_1(
    tree: &WindowTree<App>,
    window: Window,
    rp: &mut RenderPort,
    app: &mut App,
) {
    let bounds = window.inner_bounds(tree);
    rp.fill_bg(Bg::Blue);
    rp.h_line(bounds.bl_inner(), bounds.w(), true, Fg::LightGray, Bg::Blue);
    rp.v_line(bounds.tl, bounds.h(), true, Fg::LightGray, Bg::Blue);
    rp.v_line(bounds.tr_inner(), bounds.h(), true, Fg::LightGray, Bg::Blue);
    rp.bl_edge(bounds.bl_inner(), true, Fg::LightGray, Bg::Blue);
    rp.br_edge(bounds.br_inner(), true, Fg::LightGray, Bg::Blue);
    let text_bounds = Thickness::new(1, 0, 1, 1).shrink_rect(bounds);
    app.text.resize((text_bounds.h() as u16).into());
    let mut normalized_line = String::new();
    for (n, line) in app.text.lines().enumerate() {
        let line_len = line.grapheme_indices(true).take((text_bounds.w() as u16).into()).last().map_or(0, |(i, s)| i + s.len());
        let line = &line[.. line_len];
        normalized_line.clear();
        line.nfc().map(control_char_map).collect_into(&mut normalized_line);
        rp.out(Point { x: 1, y: u16::try_from(n).unwrap() as i16 }, Fg::LightGray, Bg::Blue, &normalized_line);
    }
    if text_bounds.h() != 0 {
        rp.cursor(Point { x: 1, y: 0 }.offset(app.cursor));
    }
}

fn window_1_bounds(screen_size: Vector) -> Rect {
    Rect { tl: Point { x: 0, y: 0 }, size: screen_size }
}

fn main() {
    let screen = unsafe { tuifw_screen::init() }.unwrap();
    let windows = &mut WindowTree::new(screen, <WindowRenderer<App>>::render);
    let window_manager = &mut WindowManager::new();
    let mut window_renderer = WindowRenderer::new();
    let window_1 = window_manager.new_window(windows, None, None, window_1_bounds);
    window_renderer.add_window(window_1, windows, render_window_1);
    let mut app = App {
        window_renderer,
        text: Text::new("Sim大ple text.\nLorem ip\x01\x02sum大.\n".into(), "\n".into()),
        cursor: Vector { x: 0, y: 0 },
    };
    windows.invalidate_screen();
    loop {
        if let Some(event) = WindowTree::update(windows, true, &mut app).unwrap() {
            window_manager.update(windows, event);
            if matches!(event, Event::Key(_, Key::Escape)) { break; }
            match event {
                Event::Key(n, Key::Char(c)) => {
                    for _ in 0 .. n.get() {
                        app.text.insert((app.cursor.y as u16).into(), (app.cursor.x as u16).into(), c);
                        app.cursor.x = app.text.move_right((app.cursor.y as u16).into(), (app.cursor.x as u16).into())
                            .try_into().unwrap();
                    }
                    window_1.invalidate(windows);
                },
                Event::Key(n, Key::Left) => {
                    for _ in 0 .. n.get() {
                        app.cursor.x = app.text.move_left((app.cursor.y as u16).into(), (app.cursor.x as u16).into())
                            .try_into().unwrap();
                    }
                    window_1.invalidate(windows);
                },
                Event::Key(n, Key::Right) => {
                    for _ in 0 .. n.get() {
                        app.cursor.x = app.text.move_right((app.cursor.y as u16).into(), (app.cursor.x as u16).into())
                            .try_into().unwrap();
                    }
                    window_1.invalidate(windows);
                },
                Event::Key(n, Key::Up) => {
                    for _ in 0 .. n.get() {
                        let text_height = app.text.height();
                        if text_height != 0 {
                            if usize::from(app.cursor.y as u16) > 0 {
                                app.cursor.y = app.cursor.y.wrapping_sub(1);
                            } else {
                                app.text.line_up();
                            }
                            app.cursor.x = app.text.coerce_column((app.cursor.y as u16).into(), (app.cursor.x as u16).into())
                                .try_into().unwrap();
                        }
                    }
                    window_1.invalidate(windows);
                },
                Event::Key(n, Key::Down) => {
                    for _ in 0 .. n.get() {
                        let text_height = app.text.height();
                        if text_height != 0 {
                            if usize::from(app.cursor.y as u16) < text_height - 1 {
                                app.cursor.y = app.cursor.y.wrapping_add(1);
                            } else {
                                app.text.line_down();
                            }
                            app.cursor.x = app.text.coerce_column((app.cursor.y as u16).into(), (app.cursor.x as u16).into())
                                .try_into().unwrap();
                        }
                    }
                    window_1.invalidate(windows);
                },
                _ => { },
            };
        }
    }
}
