#![feature(iter_collect_into)]

use tuifw_screen::{Bg, Event, Fg, Key, Point, Rect, Thickness, Vector};
use tuifw_window::{RenderPort, Window, WindowTree};
use tuifw::{RenderPortExt, WindowManager, WindowRenderer, WindowRendererState};

mod text;
use text::*;

struct App {
    window_renderer: WindowRenderer<App>,
    text: Text,
    view: TextView,
    cursor: Vector,
}

impl WindowRendererState for App {
    fn window_renderer(&self) -> &WindowRenderer<App> { &self.window_renderer }
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
    app.view.resize_lines((text_bounds.h() as u16).into(), &mut app.text);
    let columns_start = app.view.columns(&app.text).start;
    app.view.set_columns(columns_start .. columns_start.saturating_add((text_bounds.w() as u16).into()), &mut app.text);
    app.view.prepare_display(&mut app.text);
    for (n, line) in app.view.lines(&app.text).enumerate() {
        let (padding, line) = app.view.display_line(line, &app.text);
        rp.out(Point {
            x: 1i16.wrapping_add(padding as u16 as i16),
            y: u16::try_from(n).unwrap() as i16
        }, Fg::LightGray, Bg::Blue, line);
    }
    if text_bounds.h() != 0 {
        rp.cursor(Point { x: 1, y: 0 }.offset(app.cursor));
    }
}

fn window_1_bounds(screen_size: Vector) -> Rect {
    Rect { tl: Point { x: 0, y: 0 }, size: screen_size }
}

fn main() {
    let screen = unsafe { tuifw_screen::init(None, None) }.unwrap();
    let windows = &mut WindowTree::new(screen, <WindowRenderer<App>>::render);
    let window_manager = &mut WindowManager::new();
    let mut window_renderer = WindowRenderer::new();
    let window_1 = window_manager.new_window(windows, None, None, window_1_bounds);
    window_renderer.add_window(window_1, windows, render_window_1);
    let mut text = Text::new("Sim大ple text.\nLorem ip\x01\x02sum大.\n".into(), "\n".into());
    let view = TextView::new(&mut text);
    let mut app = App {
        window_renderer,
        cursor: Vector { x: 0, y: 0 },
        text, view,
    };
    windows.invalidate_screen();
    loop {
        if let Some(event) = WindowTree::update(windows, true, &mut app).unwrap() {
            window_manager.update(windows, event);
            if matches!(event, Event::Key(_, Key::Escape)) { break; }
            match event {
                /*
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
                */
                _ => { },
            };
        }
    }
}
