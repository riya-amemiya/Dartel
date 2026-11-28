use std::env;
use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dartel_config::{Config, FONT_SIZE, Locations};
use dartel_pty::Size;
use dartel_render::{Gpu, Overlay, Scene};
use dartel_search::{Case, Query};
use dartel_term::{Modes, Point, Span, Target};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{
    ElementState, Ime, KeyEvent, Modifiers, MouseButton, MouseScrollDelta, StartCause, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{UserAttentionType, Window, WindowAttributes, WindowId};

use crate::clipboard;
use crate::input;
use crate::opener;
use crate::session::{Session, Signal};

const WHEEL_LINES: f64 = 3.0;
const SEARCH_LIMIT: usize = 10_000;
const SYNC_TIMEOUT: Duration = Duration::from_millis(150);
const BLINK: Duration = Duration::from_millis(500);
const NOTICE: Duration = Duration::from_secs(4);
const MULTI_CLICK: Duration = Duration::from_millis(500);
const TAB_KEYS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

pub struct App {
    proxy: EventLoopProxy<Signal>,
    config: Config,
    locations: Option<Locations>,
    view: Option<View>,
}

#[derive(Default)]
struct Search {
    query: String,
    matches: Vec<Span>,
    current: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transfer {
    Save,
    Load,
}

struct Ask {
    transfer: Transfer,
    path: String,
}

struct Tab {
    session: Session,
    title: String,
    anchor: Option<Point>,
    selection: Option<Span>,
    search: Option<Search>,
    held: Option<Instant>,
}

struct View {
    window: Arc<Window>,
    gpu: Gpu,
    scene: Scene,
    proxy: EventLoopProxy<Signal>,
    config: Config,
    locations: Option<Locations>,
    home: Option<PathBuf>,
    host: String,
    base_size: f64,
    size: PhysicalSize<u32>,
    tabs: Vec<Tab>,
    labels: Vec<String>,
    active: usize,
    spawned: u64,
    ask: Option<Ask>,
    notice: Option<(String, Instant)>,
    modifiers: Modifiers,
    preedit: String,
    focused: bool,
    pointer: PhysicalPosition<f64>,
    wheel: f64,
    pressed: Option<u8>,
    swallowed: u8,
    reported: Option<(usize, usize)>,
    blink: Instant,
    blinking: bool,
    caret: Option<(usize, usize)>,
    clicks: Option<(Instant, Point, u8)>,
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

fn cell_size(scene: &Scene) -> [u16; 2] {
    [scene.metrics().width, scene.metrics().height]
        .map(|side| u16::try_from(side).unwrap_or(u16::MAX))
}

fn pty_size(scene: &Scene, size: PhysicalSize<u32>) -> Size {
    let (lines, cols) = scene.grid_size(size.width, size.height);
    let [width, height] = cell_size(scene).map(usize::from);
    let clamp = |value: usize| u16::try_from(value).unwrap_or(u16::MAX);
    Size {
        lines: clamp(lines),
        cols: clamp(cols),
        width: clamp(cols * width),
        height: clamp(lines * height),
    }
}

fn scene(config: &Config, scale: f64) -> Option<Scene> {
    Scene::new(
        &config.font_family,
        (config.font_size * scale) as f32,
        (config.padding * scale).round() as i32,
    )
}

fn caption(title: &str) -> &str {
    if title.is_empty() { "Dartel" } else { title }
}

fn label(index: usize, title: &str) -> String {
    format!("{} {title}", index + 1)
}

impl Transfer {
    fn prompt(self) -> &'static str {
        match self {
            Transfer::Save => "save settings to: ",
            Transfer::Load => "load settings from: ",
        }
    }
}

impl App {
    pub fn new(proxy: EventLoopProxy<Signal>, config: Config, locations: Option<Locations>) -> App {
        App {
            proxy,
            config,
            locations,
            view: None,
        }
    }

    fn finish(&mut self, event_loop: &ActiveEventLoop) {
        if self.view.as_ref().is_some_and(|view| view.tabs.is_empty()) {
            self.view = None;
            event_loop.exit();
        }
    }
}

impl View {
    fn new(
        event_loop: &ActiveEventLoop,
        proxy: EventLoopProxy<Signal>,
        config: Config,
        locations: Option<Locations>,
    ) -> Result<View, Box<dyn Error>> {
        let window = Arc::new(event_loop.create_window(attributes())?);
        window.set_ime_allowed(true);
        let scale = window.scale_factor();
        let scene = scene(&config, scale).ok_or("no usable font was found")?;
        let padding = (config.padding * scale).round() as u32;
        let size = window
            .request_inner_size(PhysicalSize::new(
                u32::from(config.columns) * scene.metrics().width + 2 * padding,
                u32::from(config.lines) * scene.metrics().height + 2 * padding,
            ))
            .unwrap_or_else(|| window.inner_size());
        let mut gpu = Gpu::new(Arc::clone(&window))?;
        gpu.resize(size.width, size.height);
        let mut view = View {
            window,
            gpu,
            scene,
            proxy,
            base_size: config.font_size,
            config,
            locations,
            home: env::var_os("HOME").map(PathBuf::from),
            host: dartel_pty::host_name(),
            size,
            tabs: Vec::new(),
            labels: Vec::new(),
            active: 0,
            spawned: 0,
            ask: None,
            notice: None,
            modifiers: Modifiers::default(),
            preedit: String::new(),
            focused: true,
            pointer: PhysicalPosition::new(0.0, 0.0),
            wheel: 0.0,
            pressed: None,
            swallowed: 0,
            reported: None,
            blink: Instant::now(),
            blinking: false,
            caret: None,
            clicks: None,
        };
        view.open()?;
        Ok(view)
    }

    fn session(&self) -> &Session {
        &self.tabs[self.active].session
    }

    fn directory(&self) -> Option<PathBuf> {
        let term = self.tabs.get(self.active)?.session.term();
        term.directory()
            .filter(|(host, _)| {
                host.is_empty() || *host == "localhost" || host.eq_ignore_ascii_case(&self.host)
            })
            .map(|(_, path)| PathBuf::from(path))
    }

    fn open(&mut self) -> io::Result<()> {
        self.scene.show_tabs(!self.tabs.is_empty());
        let opened = Session::spawn(
            self.spawned,
            pty_size(&self.scene, self.size),
            cell_size(&self.scene),
            self.directory(),
            self.config.palette(),
            self.config.scrollback as usize,
            self.proxy.clone(),
        )
        .map(|session| {
            self.leave();
            self.spawned += 1;
            self.tabs.push(Tab {
                session,
                title: String::new(),
                anchor: None,
                selection: None,
                search: None,
                held: None,
            });
            self.select(self.tabs.len() - 1);
        });
        self.arrange();
        opened
    }

    fn leave(&self) {
        if let Some(tab) = self.tabs.get(self.active)
            && self.focused
            && tab.session.term().modes().contains(Modes::FOCUS)
        {
            tab.session.send(b"\x1b[O".to_vec());
        }
    }

    fn select(&mut self, index: usize) {
        self.active = index;
        self.scene.invalidate();
        self.preedit.clear();
        self.pressed = None;
        self.reported = None;
        self.caret = None;
        self.blink = Instant::now();
        let tab = &self.tabs[index];
        self.window.set_title(caption(&tab.title));
        if self.focused && tab.session.term().modes().contains(Modes::FOCUS) {
            tab.session.send(b"\x1b[I".to_vec());
        }
        self.find();
    }

    fn arrange(&mut self) {
        self.scene.show_tabs(self.tabs.len() > 1);
        let size = pty_size(&self.scene, self.size);
        for tab in &mut self.tabs {
            tab.session.resize(size, cell_size(&self.scene));
            tab.anchor = None;
            tab.selection = None;
        }
        self.labels = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| label(index, &tab.title))
            .collect();
        self.find();
    }

    fn wake(&mut self, id: u64) {
        let Some(index) = self.tabs.iter().position(|tab| tab.session.id() == id) else {
            return;
        };
        if index == self.active {
            self.window.request_redraw();
            return;
        }
        let tab = &mut self.tabs[index];
        let (clipboard, bell) = {
            let mut term = tab.session.frame();
            if term.take_title_change() {
                term.title().clone_into(&mut tab.title);
                self.labels[index] = label(index, &tab.title);
                self.window.request_redraw();
            }
            (term.take_clipboard(), term.take_bell())
        };
        self.deliver(clipboard, bell);
    }

    fn close(&mut self, id: u64) {
        let Some(index) = self.tabs.iter().position(|tab| tab.session.id() == id) else {
            return;
        };
        self.tabs.remove(index).session.close();
        if self.tabs.is_empty() {
            return;
        }
        if index == self.active {
            self.select(index.min(self.tabs.len() - 1));
        } else if index < self.active {
            self.active -= 1;
        }
        self.arrange();
    }

    fn zoom(&mut self, size: f64) {
        self.config.font_size = size.clamp(*FONT_SIZE.start(), *FONT_SIZE.end());
        self.caret = None;
        self.rescale(self.window.scale_factor());
    }

    fn jump(&self, forward: bool) {
        let mut term = self.session().term();
        let first = term.grid().history() - term.display_offset();
        let prompt = if forward {
            term.prompt_after(first)
        } else {
            term.prompt_before(first)
        };
        if forward || prompt.is_some() {
            let delta = prompt.map_or(0, |line| term.grid().history().saturating_sub(line))
                as isize
                - term.display_offset() as isize;
            term.scroll_display(delta);
        }
        self.window.request_redraw();
    }

    fn copy_output(&self) {
        let output = {
            let term = self.session().term();
            term.last_output().map(|span| term.text(span))
        };
        if let Some(text) = output {
            clipboard::write(text.trim_end());
        }
    }

    fn follow(&self) {
        let point = self.pointed();
        let target = self.session().term().target_at(point);
        match target {
            Some(Target::Url(url)) => opener::url(&url),
            Some(Target::Path(path)) => {
                opener::path(&path, self.directory().as_deref(), self.home.as_deref());
            }
            None => {}
        }
    }

    fn click(&mut self) {
        let point = self.pointed();
        let count = match self.clicks {
            Some((at, last, count)) if at.elapsed() < MULTI_CLICK && last == point && count < 3 => {
                count + 1
            }
            _ => 1,
        };
        self.clicks = Some((Instant::now(), point, count));
        let tab = &mut self.tabs[self.active];
        tab.selection = match count {
            2 => tab.session.term().word_at(point),
            3 => Some(tab.session.term().line_at(point)),
            _ => None,
        };
        tab.anchor = (count == 1).then_some(point);
        self.window.request_redraw();
    }

    fn commanding(&self) -> bool {
        let state = self.modifiers.state();
        if cfg!(target_os = "macos") {
            state.super_key()
        } else {
            state.control_key() && state.shift_key()
        }
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.size = size;
        self.gpu.resize(size.width, size.height);
        self.arrange();
    }

    fn rescale(&mut self, scale: f64) {
        self.scene.rescale(
            (self.config.font_size * scale) as f32,
            (self.config.padding * scale).round() as i32,
        );
        self.resize(self.window.inner_size());
    }

    fn notify(&mut self, message: String) {
        self.notice = Some((message, Instant::now()));
        self.window.request_redraw();
    }

    fn deliver(&self, clipboard: Option<String>, bell: bool) {
        if let Some(text) = clipboard {
            clipboard::write(&text);
        }
        if bell && !self.focused {
            self.window
                .request_user_attention(Some(UserAttentionType::Informational));
        }
    }

    fn redraw(&mut self) {
        if self
            .notice
            .as_ref()
            .is_some_and(|(_, since)| since.elapsed() >= NOTICE)
        {
            self.notice = None;
        }
        let tab = &mut self.tabs[self.active];
        let banner = match (&self.ask, &tab.search, &self.notice) {
            (Some(ask), _, _) => Some(format!(
                "{}{}{}",
                ask.transfer.prompt(),
                ask.path,
                self.preedit
            )),
            (None, Some(search), _) => Some(format!(
                "find: {}{}  {}/{}",
                search.query,
                self.preedit,
                (search.current + 1).min(search.matches.len()),
                search.matches.len()
            )),
            (None, None, Some((message, _))) => Some(message.clone()),
            (None, None, None) => None,
        };
        let (clear, clipboard, bell, caret) = {
            let mut term = tab.session.frame();
            if term.modes().contains(Modes::SYNC_UPDATE)
                && tab.held.get_or_insert_with(Instant::now).elapsed() < SYNC_TIMEOUT
            {
                return;
            }
            tab.held = None;
            if term.take_title_change() {
                term.title().clone_into(&mut tab.title);
                self.labels[self.active] = label(self.active, &tab.title);
                self.window.set_title(caption(&tab.title));
            }
            self.blinking = self.focused && term.cursor().blinking && term.cursor().visible;
            self.scene.compose(
                &self.gpu,
                &term,
                &Overlay {
                    focused: self.focused,
                    caret: self.ask.is_none()
                        && tab.search.is_none()
                        && (!self.blinking
                            || (self.blink.elapsed().as_millis() / BLINK.as_millis())
                                .is_multiple_of(2)),
                    preedit: &self.preedit,
                    selection: tab.selection,
                    matches: tab
                        .search
                        .as_ref()
                        .map_or(&[], |search| search.matches.as_slice()),
                    current: tab
                        .search
                        .as_ref()
                        .and_then(|search| search.matches.get(search.current).copied()),
                    banner: banner.as_deref(),
                    tabs: &self.labels,
                    active: self.active,
                    width: self.size.width,
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
        self.deliver(clipboard, bell);
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
            self.scene.layers(),
            [
                f64::from(clear.r) / 255.0,
                f64::from(clear.g) / 255.0,
                f64::from(clear.b) / 255.0,
            ],
        ) {
            self.window.request_redraw();
        }
    }

    fn deadline(&self) -> Option<Instant> {
        [
            self.tabs[self.active]
                .held
                .map(|since| since + SYNC_TIMEOUT),
            self.blinking.then(|| {
                self.blink
                    + BLINK * (self.blink.elapsed().as_millis() / BLINK.as_millis() + 1) as u32
            }),
            self.notice.as_ref().map(|(_, since)| *since + NOTICE),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn focus(&mut self, focused: bool) {
        self.focused = focused;
        self.blink = Instant::now();
        if self.session().term().modes().contains(Modes::FOCUS) {
            self.session()
                .send(if focused { b"\x1b[I" } else { b"\x1b[O" }.to_vec());
        }
        self.window.request_redraw();
    }

    fn pointed(&self) -> Point {
        let (line, column) = self.scene.cell_at(self.pointer.x, self.pointer.y);
        let term = self.session().term();
        Point {
            line: term.grid().history() - term.display_offset() + line.min(term.lines() - 1),
            column: column.min(term.cols() - 1),
        }
    }

    fn reporting(&self) -> Option<Modes> {
        Some(self.session().term().modes())
            .filter(|modes| modes.contains(Modes::ANY_MOUSE) && !self.modifiers.state().shift_key())
    }

    fn report(&self, modes: Modes, button: u8, released: bool) {
        let (line, column) = {
            let (line, column) = self.scene.cell_at(self.pointer.x, self.pointer.y);
            let term = self.session().term();
            (line.min(term.lines() - 1), column.min(term.cols() - 1))
        };
        let state = self.modifiers.state();
        let code = button
            + 4 * u8::from(state.shift_key())
            + 8 * u8::from(state.alt_key())
            + 16 * u8::from(state.control_key());
        if modes.contains(Modes::MOUSE_SGR) {
            self.session().send(
                format!(
                    "\x1b[<{code};{};{}{}",
                    column + 1,
                    line + 1,
                    if released { 'm' } else { 'M' }
                )
                .into_bytes(),
            );
        } else if line < 223 && column < 223 {
            self.session().send(vec![
                0x1B,
                b'[',
                b'M',
                32 + if released { code | 3 } else { code },
                33 + column as u8,
                33 + line as u8,
            ]);
        }
    }

    fn press(&mut self, button: MouseButton, state: ElementState) {
        let code = match button {
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
            _ => return,
        };
        if state == ElementState::Pressed && self.scene.over_tabs(self.pointer.y) {
            self.swallowed |= 1 << code;
            if let Some(index) = self
                .scene
                .tab_at(self.pointer.x, self.tabs.len(), self.size.width)
                && button == MouseButton::Left
                && index != self.active
            {
                self.leave();
                self.select(index);
            }
            return;
        }
        let swallowed = self.swallowed & (1 << code) != 0;
        self.swallowed &= !(1 << code);
        if swallowed && state == ElementState::Released {
            return;
        }
        if button == MouseButton::Left && state == ElementState::Pressed && self.commanding() {
            self.swallowed |= 1;
            self.follow();
            return;
        }
        if let Some(modes) = self.reporting() {
            self.pressed = (state == ElementState::Pressed).then_some(code);
            self.report(modes, code, state == ElementState::Released);
        } else if button == MouseButton::Left && state == ElementState::Pressed {
            self.click();
        } else if button == MouseButton::Left {
            self.tabs[self.active].anchor = None;
        }
    }

    fn point(&mut self, position: PhysicalPosition<f64>) {
        self.pointer = position;
        if let Some(modes) = self.reporting() {
            let cell = self.scene.cell_at(position.x, position.y);
            if self.reported != Some(cell) {
                self.reported = Some(cell);
                match self.pressed {
                    Some(code)
                        if modes.contains(Modes::MOUSE_DRAG)
                            || modes.contains(Modes::MOUSE_MOTION) =>
                    {
                        self.report(modes, code + 32, false);
                    }
                    None if modes.contains(Modes::MOUSE_MOTION) => self.report(modes, 35, false),
                    _ => {}
                }
            }
            return;
        }
        if let Some(anchor) = self.tabs[self.active].anchor {
            let head = self.pointed();
            let selection = (anchor != head).then_some(Span {
                start: anchor.min(head),
                end: anchor.max(head),
            });
            if selection != self.tabs[self.active].selection {
                self.tabs[self.active].selection = selection;
                self.window.request_redraw();
            }
        }
    }

    fn scroll(&mut self, lines: isize) {
        let mut term = self.session().term();
        if !term.modes().contains(Modes::ALTERNATE) {
            term.scroll_display(lines);
            self.window.request_redraw();
        } else if term.modes().contains(Modes::ALTERNATE_SCROLL) {
            let key: &[u8] = match (lines > 0, term.modes().contains(Modes::APP_CURSOR)) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1b[A",
                (false, true) => b"\x1bOB",
                (false, false) => b"\x1b[B",
            };
            self.session().send(key.repeat(lines.unsigned_abs()));
        }
    }

    fn wheel(&mut self, delta: MouseScrollDelta) {
        self.wheel += match delta {
            MouseScrollDelta::LineDelta(_, lines) => f64::from(lines) * WHEEL_LINES,
            MouseScrollDelta::PixelDelta(position) => {
                position.y / f64::from(self.scene.metrics().height)
            }
        };
        let lines = self.wheel.trunc() as isize;
        self.wheel -= lines as f64;
        if lines == 0 {
            return;
        }
        match self.reporting() {
            Some(modes) => {
                for _ in 0..lines.unsigned_abs() {
                    self.report(modes, if lines > 0 { 64 } else { 65 }, false);
                }
            }
            None => self.scroll(lines),
        }
    }

    fn paste(&self) {
        let Some(text) = clipboard::read() else {
            return;
        };
        self.session().send(
            if self
                .session()
                .term()
                .modes()
                .contains(Modes::BRACKETED_PASTE)
            {
                format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', ""))
            } else {
                text.replace("\r\n", "\r").replace('\n', "\r")
            }
            .into_bytes(),
        );
    }

    fn copy(&self) {
        if let Some(span) = self.tabs[self.active].selection {
            let text = self.session().term().text(span);
            clipboard::write(&text);
        }
    }

    fn find(&mut self) {
        let tab = &mut self.tabs[self.active];
        if let Some(search) = &mut tab.search {
            search.matches = Query::regex(&search.query, Case::Smart)
                .or_else(|_| Query::literal(&search.query, Case::Smart))
                .map_or_default(|query| tab.session.term().search(&query, SEARCH_LIMIT));
            search.current = search.matches.len().saturating_sub(1);
        }
        self.reveal();
    }

    fn step(&mut self, forward: bool) {
        if let Some(search) = &mut self.tabs[self.active].search
            && !search.matches.is_empty()
        {
            search.current = (search.current + if forward { 1 } else { search.matches.len() - 1 })
                % search.matches.len();
        }
        self.reveal();
    }

    fn reveal(&mut self) {
        let tab = &self.tabs[self.active];
        if let Some(span) = tab
            .search
            .as_ref()
            .and_then(|search| search.matches.get(search.current))
        {
            let mut term = tab.session.term();
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
        match (&event.logical_key, &mut self.tabs[self.active].search) {
            (Key::Named(NamedKey::Escape), search) => {
                *search = None;
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

    fn prompt(&mut self, transfer: Transfer) {
        self.ask = Some(Ask {
            transfer,
            path: self
                .locations
                .as_ref()
                .map_or_default(|locations| locations.file.to_string_lossy().into_owned()),
        });
        self.window.request_redraw();
    }

    fn ask_key(&mut self, event: &KeyEvent) {
        match (&event.logical_key, &mut self.ask) {
            (Key::Named(NamedKey::Escape), ask) => *ask = None,
            (Key::Named(NamedKey::Enter), ask) => {
                if let Some(ask) = ask.take() {
                    self.transfer(&ask);
                }
            }
            (Key::Named(NamedKey::Backspace), Some(ask)) => {
                ask.path.pop();
            }
            (_, Some(ask)) => {
                if let Some(text) = event
                    .text
                    .as_deref()
                    .filter(|text| !text.chars().any(char::is_control))
                {
                    ask.path.push_str(text);
                }
            }
            (_, None) => {}
        }
        self.window.request_redraw();
    }

    fn transfer(&mut self, ask: &Ask) {
        let path = dartel_config::expand(&ask.path, self.home.as_deref());
        let message = match (ask.transfer, &self.locations) {
            (Transfer::Save, _) => match dartel_config::save(&self.config, &path) {
                Ok(()) => format!("settings saved to {}", path.display()),
                Err(error) => format!("settings were not saved: {error}"),
            },
            (Transfer::Load, Some(locations)) => {
                match dartel_config::load(&path, &locations.state) {
                    Ok(config) => {
                        self.apply(config);
                        format!("settings loaded from {}", path.display())
                    }
                    Err(error) => format!("settings were not loaded: {error}"),
                }
            }
            (Transfer::Load, None) => {
                "settings were not loaded: the home directory is unknown".to_owned()
            }
        };
        self.notify(message);
    }

    fn apply(&mut self, config: Config) {
        if let Some(scene) = scene(&config, self.window.scale_factor()) {
            self.scene = scene;
        }
        for tab in &self.tabs {
            tab.session.term().set_palette(config.palette());
        }
        self.base_size = config.font_size;
        self.config = config;
        self.caret = None;
        self.arrange();
    }

    fn shortcut(&mut self, event: &KeyEvent) -> bool {
        let state = self.modifiers.state();
        let command = self.commanding();
        if command && let PhysicalKey::Code(code) = event.physical_key {
            let size = match code {
                KeyCode::Equal => Some(self.config.font_size + 1.0),
                KeyCode::Minus => Some(self.config.font_size - 1.0),
                KeyCode::Digit0 => Some(self.base_size),
                _ => None,
            };
            if let Some(size) = size {
                self.zoom(size);
                return true;
            }
            if let Some(index) = TAB_KEYS.iter().position(|key| *key == code) {
                if index < self.tabs.len() && index != self.active {
                    self.leave();
                    self.select(index);
                }
                return true;
            }
        }
        match &event.logical_key {
            Key::Character(text) if command && text.eq_ignore_ascii_case("v") => self.paste(),
            Key::Character(text) if command && text.eq_ignore_ascii_case("c") => self.copy(),
            Key::Character(text) if command && text.eq_ignore_ascii_case("o") => {
                self.copy_output();
            }
            Key::Character(text) if command && text.eq_ignore_ascii_case("w") => {
                let id = self.session().id();
                self.close(id);
            }
            Key::Character(text) if command && text == "+" => {
                self.zoom(self.config.font_size + 1.0);
            }
            Key::Named(NamedKey::ArrowUp) if command => self.jump(false),
            Key::Named(NamedKey::ArrowDown) if command => self.jump(true),
            Key::Character(text) if command && text.eq_ignore_ascii_case("f") => {
                self.ask = None;
                self.tabs[self.active].search = Some(Search::default());
                self.window.request_redraw();
            }
            Key::Character(text) if command && text.eq_ignore_ascii_case("t") => {
                if let Err(error) = self.open() {
                    self.notify(format!("a tab could not be opened: {error}"));
                }
            }
            Key::Character(text) if command && text.eq_ignore_ascii_case("s") => {
                self.prompt(Transfer::Save);
            }
            Key::Character(text) if command && text.eq_ignore_ascii_case("l") => {
                self.prompt(Transfer::Load);
            }
            Key::Named(NamedKey::PageUp) if state.shift_key() => {
                let page = self.session().term().lines() as isize;
                self.scroll(page);
            }
            Key::Named(NamedKey::PageDown) if state.shift_key() => {
                let page = self.session().term().lines() as isize;
                self.scroll(-page);
            }
            _ => return state.super_key(),
        }
        true
    }

    fn key(&mut self, event: &KeyEvent) {
        let pressed = event.state == ElementState::Pressed;
        if pressed && (self.shortcut(event) || !self.preedit.is_empty()) {
            return;
        }
        if pressed && self.ask.is_some() {
            self.ask_key(event);
            return;
        }
        if pressed && self.tabs[self.active].search.is_some() {
            self.search_key(event);
            return;
        }
        if !pressed
            && (self.commanding()
                || self.modifiers.state().super_key()
                || self.ask.is_some()
                || self.tabs[self.active].search.is_some()
                || !self.preedit.is_empty())
        {
            return;
        }
        let (modes, keyboard) = {
            let term = self.session().term();
            (term.modes(), term.keyboard())
        };
        if let Some(bytes) = input::encode(event, self.modifiers, modes, keyboard) {
            if pressed {
                self.session().term().reset_display();
                self.tabs[self.active].selection = None;
                self.blink = Instant::now();
            }
            self.session().send(bytes);
            self.window.request_redraw();
        }
    }

    fn ime(&mut self, ime: Ime) {
        match ime {
            Ime::Preedit(text, _) => self.preedit = text,
            Ime::Commit(text) => {
                self.preedit.clear();
                match (&mut self.ask, &mut self.tabs[self.active].search) {
                    (Some(ask), _) => ask.path.push_str(&text),
                    (None, Some(search)) => {
                        search.query.push_str(&text);
                        self.find();
                    }
                    (None, None) => self.session().send(text.into_bytes()),
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
        match View::new(
            event_loop,
            self.proxy.clone(),
            self.config.clone(),
            self.locations.clone(),
        ) {
            Ok(view) => self.view = Some(view),
            Err(error) => {
                eprintln!("dartel: {error}");
                event_loop.exit();
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, signal: Signal) {
        let Some(view) = &mut self.view else {
            return;
        };
        match signal {
            Signal::Wake(id) => view.wake(id),
            Signal::Exit(id) => view.close(id),
        }
        self.finish(event_loop);
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
            WindowEvent::KeyboardInput { event, .. } => view.key(&event),
            WindowEvent::Ime(ime) => view.ime(ime),
            WindowEvent::MouseWheel { delta, .. } => view.wheel(delta),
            WindowEvent::CursorMoved { position, .. } => view.point(position),
            WindowEvent::MouseInput { state, button, .. } => view.press(button, state),
            _ => {}
        }
        self.finish(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(
            self.view
                .as_ref()
                .and_then(View::deadline)
                .map_or(ControlFlow::Wait, ControlFlow::WaitUntil),
        );
    }
}
