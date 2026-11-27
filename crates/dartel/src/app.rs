use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dartel_core::pty::Size;
use dartel_core::search::{Case, Query};
use dartel_core::term::{Modes, Point, Span};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{
    ElementState, Ime, KeyEvent, Modifiers, MouseButton, MouseScrollDelta, StartCause, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{UserAttentionType, Window, WindowAttributes, WindowId};

use crate::clipboard;
use crate::gpu::Gpu;
use crate::input;
use crate::scene::{Overlay, Scene};
use crate::session::{Session, Signal};

const FONT_SIZE: f64 = 13.0;
const PADDING: f64 = 4.0;
const COLUMNS: u32 = 80;
const LINES: u32 = 24;
const WHEEL_LINES: f64 = 3.0;
const SEARCH_LIMIT: usize = 10_000;
const SYNC_TIMEOUT: Duration = Duration::from_millis(150);

pub struct App {
    proxy: EventLoopProxy<Signal>,
    view: Option<View>,
}

#[derive(Default)]
struct Search {
    query: String,
    matches: Vec<Span>,
    current: usize,
}

struct View {
    window: Arc<Window>,
    gpu: Gpu,
    scene: Scene,
    session: Session,
    modifiers: Modifiers,
    preedit: String,
    focused: bool,
    pointer: PhysicalPosition<f64>,
    anchor: Option<Point>,
    selection: Option<Span>,
    search: Option<Search>,
    wheel: f64,
    held: Option<Instant>,
    caret: Option<(usize, usize)>,
}

#[cfg(target_os = "macos")]
fn attributes() -> WindowAttributes {
    use winit::platform::macos::{OptionAsAlt, WindowAttributesExtMacOS};
    Window::default_attributes()
        .with_title("Dartel")
        .with_inner_size(LogicalSize::new(800.0, 520.0))
        .with_option_as_alt(OptionAsAlt::OnlyLeft)
}

#[cfg(not(target_os = "macos"))]
fn attributes() -> WindowAttributes {
    Window::default_attributes()
        .with_title("Dartel")
        .with_inner_size(LogicalSize::new(800.0, 520.0))
}

fn pty_size(scene: &Scene, size: PhysicalSize<u32>) -> Size {
    let (lines, cols) = scene.grid_size(size.width, size.height);
    let clamp = |value: usize| u16::try_from(value).unwrap_or(u16::MAX);
    Size {
        lines: clamp(lines),
        cols: clamp(cols),
        width: clamp(size.width as usize),
        height: clamp(size.height as usize),
    }
}

impl App {
    pub fn new(proxy: EventLoopProxy<Signal>) -> App {
        App { proxy, view: None }
    }
}

impl View {
    fn new(
        event_loop: &ActiveEventLoop,
        proxy: EventLoopProxy<Signal>,
    ) -> Result<View, Box<dyn Error>> {
        let window = Arc::new(event_loop.create_window(attributes())?);
        window.set_ime_allowed(true);
        let scale = window.scale_factor();
        let padding = (PADDING * scale).round();
        let scene = Scene::new((FONT_SIZE * scale) as f32, padding as i32)
            .ok_or("no usable font was found")?;
        let size = window
            .request_inner_size(PhysicalSize::new(
                COLUMNS * scene.metrics().width + 2 * padding as u32,
                LINES * scene.metrics().height + 2 * padding as u32,
            ))
            .unwrap_or_else(|| window.inner_size());
        let mut gpu = Gpu::new(Arc::clone(&window))?;
        gpu.resize(size.width, size.height);
        Ok(View {
            session: Session::spawn(pty_size(&scene, size), proxy)?,
            window,
            gpu,
            scene,
            modifiers: Modifiers::default(),
            preedit: String::new(),
            focused: true,
            pointer: PhysicalPosition::new(0.0, 0.0),
            anchor: None,
            selection: None,
            search: None,
            wheel: 0.0,
            held: None,
            caret: None,
        })
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.gpu.resize(size.width, size.height);
        self.session.resize(pty_size(&self.scene, size));
        self.selection = None;
        self.find();
    }

    fn rescale(&mut self, scale: f64) {
        self.scene
            .rescale((FONT_SIZE * scale) as f32, (PADDING * scale).round() as i32);
        self.resize(self.window.inner_size());
    }

    fn redraw(&mut self) {
        let banner = self.search.as_ref().map(|search| {
            format!(
                "find: {}{}  {}/{}",
                search.query,
                self.preedit,
                (search.current + 1).min(search.matches.len()),
                search.matches.len()
            )
        });
        let (clear, clipboard, bell, caret) = {
            let mut term = self.session.frame();
            if term.modes().contains(Modes::SYNC_UPDATE)
                && self.held.get_or_insert_with(Instant::now).elapsed() < SYNC_TIMEOUT
            {
                return;
            }
            self.held = None;
            if term.take_title_change() {
                self.window.set_title(term.title());
            }
            self.scene.compose(
                &self.gpu,
                &term,
                &Overlay {
                    focused: self.focused,
                    preedit: &self.preedit,
                    selection: self.selection,
                    matches: self
                        .search
                        .as_ref()
                        .map_or(&[], |search| search.matches.as_slice()),
                    current: self
                        .search
                        .as_ref()
                        .and_then(|search| search.matches.get(search.current).copied()),
                    banner: banner.as_deref(),
                },
            );
            term.reset_damage();
            (
                term.palette().background,
                term.take_clipboard(),
                term.take_bell(),
                (term.cursor().line, term.cursor().column),
            )
        };
        if let Some(text) = clipboard {
            clipboard::write(&text);
        }
        if bell && !self.focused {
            self.window
                .request_user_attention(Some(UserAttentionType::Informational));
        }
        if self.caret != Some(caret) {
            self.caret = Some(caret);
            let origin = self.scene.cell_origin(caret.0, caret.1);
            self.window.set_ime_cursor_area(
                PhysicalPosition::new(origin.0, origin.1),
                PhysicalSize::new(self.scene.metrics().width, self.scene.metrics().height),
            );
        }
        self.window.pre_present_notify();
        if !self.gpu.draw(
            self.scene.quads(),
            [
                f64::from(clear.r) / 255.0,
                f64::from(clear.g) / 255.0,
                f64::from(clear.b) / 255.0,
            ],
        ) {
            self.window.request_redraw();
        }
    }

    fn focus(&mut self, focused: bool) {
        self.focused = focused;
        if self.session.term().modes().contains(Modes::FOCUS) {
            self.session
                .send(if focused { b"\x1b[I" } else { b"\x1b[O" }.to_vec());
        }
        self.window.request_redraw();
    }

    fn pointed(&self) -> Point {
        let (line, column) = self.scene.cell_at(self.pointer.x, self.pointer.y);
        let term = self.session.term();
        Point {
            line: term.grid().history() - term.display_offset() + line.min(term.lines() - 1),
            column: column.min(term.cols() - 1),
        }
    }

    fn press(&mut self, state: ElementState) {
        if state == ElementState::Pressed {
            self.anchor = Some(self.pointed());
            self.selection = None;
            self.window.request_redraw();
        } else {
            self.anchor = None;
        }
    }

    fn point(&mut self, position: PhysicalPosition<f64>) {
        self.pointer = position;
        if let Some(anchor) = self.anchor {
            let head = self.pointed();
            let selection = (anchor != head).then_some(Span {
                start: anchor.min(head),
                end: anchor.max(head),
            });
            if selection != self.selection {
                self.selection = selection;
                self.window.request_redraw();
            }
        }
    }

    fn scroll(&mut self, lines: f64) {
        self.wheel += lines;
        let whole = self.wheel.trunc();
        self.wheel -= whole;
        if whole == 0.0 {
            return;
        }
        let mut term = self.session.term();
        if !term.modes().contains(Modes::ALTERNATE) {
            term.scroll_display(whole as isize);
            self.window.request_redraw();
        } else if term.modes().contains(Modes::ALTERNATE_SCROLL) {
            let key: &[u8] = match (whole > 0.0, term.modes().contains(Modes::APP_CURSOR)) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1b[A",
                (false, true) => b"\x1bOB",
                (false, false) => b"\x1b[B",
            };
            self.session.send(key.repeat(whole.abs() as usize));
        }
    }

    fn wheel(&mut self, delta: MouseScrollDelta) {
        self.scroll(match delta {
            MouseScrollDelta::LineDelta(_, lines) => f64::from(lines) * WHEEL_LINES,
            MouseScrollDelta::PixelDelta(position) => {
                position.y / f64::from(self.scene.metrics().height)
            }
        });
    }

    fn paste(&self) {
        let Some(text) = clipboard::read() else {
            return;
        };
        self.session.send(
            if self.session.term().modes().contains(Modes::BRACKETED_PASTE) {
                format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', ""))
            } else {
                text.replace("\r\n", "\r").replace('\n', "\r")
            }
            .into_bytes(),
        );
    }

    fn copy(&self) {
        if let Some(span) = self.selection {
            let text = self.session.term().text(span);
            clipboard::write(&text);
        }
    }

    fn find(&mut self) {
        if let Some(search) = &mut self.search {
            search.matches = Query::regex(&search.query, Case::Smart)
                .or_else(|_| Query::literal(&search.query, Case::Smart))
                .map(|query| self.session.term().search(&query, SEARCH_LIMIT))
                .unwrap_or_default();
            search.current = search.matches.len().saturating_sub(1);
        }
        self.reveal();
    }

    fn step(&mut self, forward: bool) {
        if let Some(search) = &mut self.search
            && !search.matches.is_empty()
        {
            search.current = (search.current + if forward { 1 } else { search.matches.len() - 1 })
                % search.matches.len();
        }
        self.reveal();
    }

    fn reveal(&mut self) {
        if let Some(span) = self
            .search
            .as_ref()
            .and_then(|search| search.matches.get(search.current))
        {
            let mut term = self.session.term();
            let first = term.grid().history() - term.display_offset();
            if !(first..first + term.lines() - 1).contains(&span.start.line) {
                let delta = term
                    .grid()
                    .history()
                    .saturating_sub(span.start.line.saturating_sub(term.lines() / 2))
                    as isize
                    - term.display_offset() as isize;
                term.scroll_display(delta);
            }
        }
        self.window.request_redraw();
    }

    fn search_key(&mut self, event: &KeyEvent) {
        match (&event.logical_key, &mut self.search) {
            (Key::Named(NamedKey::Escape), _) => {
                self.search = None;
                self.window.request_redraw();
            }
            (Key::Named(NamedKey::Enter), _) => self.step(self.modifiers.state().shift_key()),
            (Key::Named(NamedKey::Backspace), Some(search)) => {
                search.query.pop();
                self.find();
            }
            (_, Some(search)) => {
                if let Some(text) = event
                    .text
                    .as_deref()
                    .filter(|text| !text.chars().any(char::is_control))
                {
                    search.query.push_str(text);
                    self.find();
                }
            }
            (_, None) => {}
        }
    }

    fn shortcut(&mut self, event: &KeyEvent) -> bool {
        let state = self.modifiers.state();
        let command = if cfg!(target_os = "macos") {
            state.super_key()
        } else {
            state.control_key() && state.shift_key()
        };
        match &event.logical_key {
            Key::Character(text) if command && text.eq_ignore_ascii_case("v") => self.paste(),
            Key::Character(text) if command && text.eq_ignore_ascii_case("c") => self.copy(),
            Key::Character(text) if command && text.eq_ignore_ascii_case("f") => {
                self.search = Some(Search::default());
                self.window.request_redraw();
            }
            Key::Named(NamedKey::PageUp) if state.shift_key() => {
                let page = self.session.term().lines() as f64;
                self.scroll(page);
            }
            Key::Named(NamedKey::PageDown) if state.shift_key() => {
                let page = self.session.term().lines() as f64;
                self.scroll(-page);
            }
            _ => return state.super_key(),
        }
        true
    }

    fn key(&mut self, event: &KeyEvent) {
        if self.shortcut(event) || !self.preedit.is_empty() {
            return;
        }
        if self.search.is_some() {
            self.search_key(event);
            return;
        }
        let modes = self.session.term().modes();
        if let Some(bytes) = input::encode(event, self.modifiers, modes) {
            self.session.term().reset_display();
            self.selection = None;
            self.session.send(bytes);
            self.window.request_redraw();
        }
    }

    fn ime(&mut self, ime: Ime) {
        match ime {
            Ime::Preedit(text, _) => self.preedit = text,
            Ime::Commit(text) => {
                self.preedit.clear();
                match &mut self.search {
                    Some(search) => {
                        search.query.push_str(&text);
                        self.find();
                    }
                    None => self.session.send(text.into_bytes()),
                }
            }
            Ime::Enabled | Ime::Disabled => self.preedit.clear(),
        }
        self.window.request_redraw();
    }
}

impl ApplicationHandler<Signal> for App {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. })
            && let Some(view) = &self.view
        {
            view.window.request_redraw();
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.view.is_some() {
            return;
        }
        match View::new(event_loop, self.proxy.clone()) {
            Ok(view) => self.view = Some(view),
            Err(error) => {
                eprintln!("dartel: {error}");
                event_loop.exit();
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, signal: Signal) {
        match signal {
            Signal::Wake => {
                if let Some(view) = &self.view {
                    view.window.request_redraw();
                }
            }
            Signal::Exit => event_loop.exit(),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(view) = &mut self.view else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => view.resize(size),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => view.rescale(scale_factor),
            WindowEvent::RedrawRequested => view.redraw(),
            WindowEvent::Focused(focused) => view.focus(focused),
            WindowEvent::ModifiersChanged(modifiers) => view.modifiers = modifiers,
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                view.key(&event);
            }
            WindowEvent::Ime(ime) => view.ime(ime),
            WindowEvent::MouseWheel { delta, .. } => view.wheel(delta),
            WindowEvent::CursorMoved { position, .. } => view.point(position),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => view.press(state),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(match self.view.as_ref().and_then(|view| view.held) {
            Some(since) => ControlFlow::WaitUntil(since + SYNC_TIMEOUT),
            None => ControlFlow::Wait,
        });
    }
}
