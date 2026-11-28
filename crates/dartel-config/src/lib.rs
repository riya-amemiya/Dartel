pub mod toml;

use std::env;
use std::ffi::OsString;
use std::fmt::{self, Write as _};
use std::fs;
use std::io;
use std::ops::{BitOr, RangeInclusive};
use std::path::{Path, PathBuf};

use dartel_term::{Palette, Rgb};

use toml::{Entry, Table, Value};

#[cfg(target_os = "macos")]
const FONT_FAMILY: &str = "Menlo";
#[cfg(not(target_os = "macos"))]
const FONT_FAMILY: &str = "DejaVu Sans Mono";

#[cfg(target_os = "macos")]
const PRIMARY: Modifiers = Modifiers::CMD;
#[cfg(not(target_os = "macos"))]
const PRIMARY: Modifiers = Modifiers(Modifiers::CTRL.0 | Modifiers::SHIFT.0);

pub const FONT_SIZE: RangeInclusive<f64> = 4.0..=96.0;
const PADDING: RangeInclusive<f64> = 0.0..=64.0;
const COLUMNS: RangeInclusive<i64> = 2..=1000;
const LINES: RangeInclusive<i64> = 1..=1000;
const SCROLLBACK: RangeInclusive<i64> = 0..=1_000_000;
const SHADES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];
const MAGIC: &[u8; 8] = b"DARTEL\x00\x02";
const ACTIONS: usize = 24;
const ACTION_NAMES: [&str; ACTIONS] = [
    "paste",
    "copy",
    "copy-output",
    "search",
    "previous-prompt",
    "next-prompt",
    "new-tab",
    "close-tab",
    "tab-1",
    "tab-2",
    "tab-3",
    "tab-4",
    "tab-5",
    "tab-6",
    "tab-7",
    "tab-8",
    "tab-9",
    "zoom-in",
    "zoom-out",
    "zoom-reset",
    "save-settings",
    "load-settings",
    "page-up",
    "page-down",
];
const MODIFIER_NAMES: [(&str, Modifiers); 4] = [
    ("cmd", Modifiers::CMD),
    ("ctrl", Modifiers::CTRL),
    ("shift", Modifiers::SHIFT),
    ("alt", Modifiers::ALT),
];
const KEY_NAMES: [(&str, Named); 27] = [
    ("up", Named::Up),
    ("down", Named::Down),
    ("left", Named::Left),
    ("right", Named::Right),
    ("pageup", Named::PageUp),
    ("pagedown", Named::PageDown),
    ("home", Named::Home),
    ("end", Named::End),
    ("enter", Named::Enter),
    ("tab", Named::Tab),
    ("escape", Named::Escape),
    ("space", Named::Space),
    ("backspace", Named::Backspace),
    ("delete", Named::Delete),
    ("insert", Named::Insert),
    ("f1", Named::F1),
    ("f2", Named::F2),
    ("f3", Named::F3),
    ("f4", Named::F4),
    ("f5", Named::F5),
    ("f6", Named::F6),
    ("f7", Named::F7),
    ("f8", Named::F8),
    ("f9", Named::F9),
    ("f10", Named::F10),
    ("f11", Named::F11),
    ("f12", Named::F12),
];
const NAMED_BASE: u8 = 0x80;
const DEFAULT_KEYS: [Option<Shortcut>; ACTIONS] = [
    Some(primary(Key::Character('v'))),
    Some(primary(Key::Character('c'))),
    Some(primary(Key::Character('o'))),
    Some(primary(Key::Character('f'))),
    Some(primary(Key::Named(Named::Up))),
    Some(primary(Key::Named(Named::Down))),
    Some(primary(Key::Character('t'))),
    Some(primary(Key::Character('w'))),
    Some(primary(Key::Character('1'))),
    Some(primary(Key::Character('2'))),
    Some(primary(Key::Character('3'))),
    Some(primary(Key::Character('4'))),
    Some(primary(Key::Character('5'))),
    Some(primary(Key::Character('6'))),
    Some(primary(Key::Character('7'))),
    Some(primary(Key::Character('8'))),
    Some(primary(Key::Character('9'))),
    Some(primary(Key::Character('+'))),
    Some(primary(Key::Character('-'))),
    Some(primary(Key::Character('0'))),
    Some(primary(Key::Character('s'))),
    Some(primary(Key::Character('l'))),
    Some(Shortcut {
        modifiers: Modifiers::SHIFT,
        key: Key::Named(Named::PageUp),
    }),
    Some(Shortcut {
        modifiers: Modifiers::SHIFT,
        key: Key::Named(Named::PageDown),
    }),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Modifiers = Modifiers(0);
    pub const CMD: Modifiers = Modifiers(1);
    pub const CTRL: Modifiers = Modifiers(2);
    pub const SHIFT: Modifiers = Modifiers(4);
    pub const ALT: Modifiers = Modifiers(8);

    #[must_use]
    pub const fn contains(self, other: Modifiers) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }
}

impl BitOr for Modifiers {
    type Output = Modifiers;

    fn bitor(self, other: Modifiers) -> Modifiers {
        Modifiers(self.0 | other.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Named {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Enter,
    Tab,
    Escape,
    Space,
    Backspace,
    Delete,
    Insert,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Character(char),
    Named(Named),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub modifiers: Modifiers,
    pub key: Key,
}

impl fmt::Display for Shortcut {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, modifier) in MODIFIER_NAMES {
            if self.modifiers.contains(modifier) {
                write!(formatter, "{name}+")?;
            }
        }
        match self.key {
            Key::Character(character) => write!(formatter, "{character}"),
            Key::Named(named) => formatter.write_str(KEY_NAMES[named as usize].0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Paste,
    Copy,
    CopyOutput,
    Search,
    PreviousPrompt,
    NextPrompt,
    NewTab,
    CloseTab,
    Tab1,
    Tab2,
    Tab3,
    Tab4,
    Tab5,
    Tab6,
    Tab7,
    Tab8,
    Tab9,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    SaveSettings,
    LoadSettings,
    PageUp,
    PageDown,
}

impl Action {
    pub const ALL: [Action; ACTIONS] = [
        Action::Paste,
        Action::Copy,
        Action::CopyOutput,
        Action::Search,
        Action::PreviousPrompt,
        Action::NextPrompt,
        Action::NewTab,
        Action::CloseTab,
        Action::Tab1,
        Action::Tab2,
        Action::Tab3,
        Action::Tab4,
        Action::Tab5,
        Action::Tab6,
        Action::Tab7,
        Action::Tab8,
        Action::Tab9,
        Action::ZoomIn,
        Action::ZoomOut,
        Action::ZoomReset,
        Action::SaveSettings,
        Action::LoadSettings,
        Action::PageUp,
        Action::PageDown,
    ];

    #[must_use]
    pub fn name(self) -> &'static str {
        ACTION_NAMES[self as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Keys([Option<Shortcut>; ACTIONS]);

impl Default for Keys {
    fn default() -> Keys {
        Keys(DEFAULT_KEYS)
    }
}

impl Keys {
    #[must_use]
    pub fn get(&self, action: Action) -> Option<Shortcut> {
        self.0[action as usize]
    }

    pub fn set(&mut self, action: Action, shortcut: Option<Shortcut>) {
        self.0[action as usize] = shortcut;
    }

    fn holder(&self, action: Action) -> Option<Action> {
        let shortcut = self.get(action)?;
        Action::ALL
            .into_iter()
            .find(|other| *other != action && self.get(*other) == Some(shortcut))
    }
}

enum Fault<'t> {
    Shape,
    Modifier(&'t str),
    Key(&'t str),
}

const fn primary(key: Key) -> Shortcut {
    Shortcut {
        modifiers: PRIMARY,
        key,
    }
}

fn chord(text: &str) -> Result<Shortcut, Fault<'_>> {
    let (head, name) = match text.rsplit_once('+') {
        None => (None, text),
        Some(("", "")) => (None, "+"),
        Some((head, "")) => (Some(head.strip_suffix('+').ok_or(Fault::Shape)?), "+"),
        Some((head, name)) => (Some(head), name),
    };
    let modifiers = head.map_or(Ok(Modifiers::NONE), |head| {
        head.split('+')
            .try_fold(Modifiers::NONE, |modifiers, part| {
                MODIFIER_NAMES
                    .iter()
                    .find(|(modifier, _)| part.eq_ignore_ascii_case(modifier))
                    .map(|(_, modifier)| modifiers | *modifier)
                    .ok_or(Fault::Modifier(part))
            })
    })?;
    let mut characters = name.chars();
    let key = match (characters.next(), characters.next()) {
        (Some(character), None) if character.is_ascii_graphic() => {
            Key::Character(character.to_ascii_lowercase())
        }
        _ => KEY_NAMES
            .iter()
            .find(|(known, _)| name.eq_ignore_ascii_case(known))
            .map(|(_, named)| Key::Named(*named))
            .ok_or(Fault::Key(name))?,
    };
    Ok(Shortcut { modifiers, key })
}

fn shortcut(section: &str, entry: &Entry<'_>) -> Result<Option<Shortcut>, toml::Error> {
    let text = text(section, entry)?;
    if text.is_empty() {
        return Ok(None);
    }
    chord(text).map(Some).map_err(|fault| match fault {
        Fault::Shape => invalid(section, entry, "a shortcut such as \"ctrl+shift+c\""),
        Fault::Modifier(name) => invalid(
            section,
            entry,
            &format!("a shortcut whose modifiers are cmd, ctrl, shift, or alt, not `{name}`"),
        ),
        Fault::Key(name) => invalid(
            section,
            entry,
            &format!("a shortcut that ends in a known key, not `{name}`"),
        ),
    })
}

fn code(shortcut: Option<Shortcut>) -> [u8; 2] {
    shortcut.map_or([0, 0], |shortcut| {
        [
            shortcut.modifiers.0,
            match shortcut.key {
                Key::Character(character) => character as u8,
                Key::Named(named) => NAMED_BASE + named as u8,
            },
        ]
    })
}

fn decoded([modifiers, key]: [u8; 2]) -> Option<Shortcut> {
    let key = match key {
        0x21..=0x7E if !key.is_ascii_uppercase() => Key::Character(char::from(key)),
        NAMED_BASE.. => Key::Named(KEY_NAMES.get(usize::from(key - NAMED_BASE))?.1),
        _ => return None,
    };
    (modifiers < 16).then_some(Shortcut {
        modifiers: Modifiers(modifiers),
        key,
    })
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Toml(toml::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(error) => error.fmt(formatter),
            Error::Toml(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Error {
        Error::Io(error)
    }
}

impl From<toml::Error> for Error {
    fn from(error: toml::Error) -> Error {
        Error::Toml(error)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub font_family: String,
    pub font_size: f64,
    pub columns: u16,
    pub lines: u16,
    pub padding: f64,
    pub scrollback: u32,
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    pub ansi: [Rgb; 16],
    pub keys: Keys,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locations {
    pub file: PathBuf,
    pub state: PathBuf,
}

fn invalid(section: &str, entry: &Entry<'_>, expected: &str) -> toml::Error {
    toml::Error {
        line: entry.line,
        message: format!("`{section}{}` must be {expected}", entry.key),
    }
}

fn unknown(section: &str, entry: &Entry<'_>) -> toml::Error {
    toml::Error {
        line: entry.line,
        message: format!("unknown key `{section}{}`", entry.key),
    }
}

fn table<'t, 'a>(section: &str, entry: &'t Entry<'a>) -> Result<&'t Table<'a>, toml::Error> {
    match &entry.value {
        Value::Table(table) => Ok(table),
        _ => Err(invalid(section, entry, "a table")),
    }
}

fn text<'t>(section: &str, entry: &'t Entry<'_>) -> Result<&'t str, toml::Error> {
    match &entry.value {
        Value::String(text) => Ok(text),
        _ => Err(invalid(section, entry, "a string")),
    }
}

fn number(
    section: &str,
    entry: &Entry<'_>,
    range: &RangeInclusive<f64>,
) -> Result<f64, toml::Error> {
    match entry.value {
        Value::Float(value) if range.contains(&value) => Ok(value),
        Value::Integer(value) if range.contains(&(value as f64)) => Ok(value as f64),
        _ => Err(invalid(
            section,
            entry,
            &format!("a number from {} to {}", range.start(), range.end()),
        )),
    }
}

fn integer(
    section: &str,
    entry: &Entry<'_>,
    range: &RangeInclusive<i64>,
) -> Result<i64, toml::Error> {
    match entry.value {
        Value::Integer(value) if range.contains(&value) => Ok(value),
        _ => Err(invalid(
            section,
            entry,
            &format!("an integer from {} to {}", range.start(), range.end()),
        )),
    }
}

fn color(section: &str, entry: &Entry<'_>) -> Result<Rgb, toml::Error> {
    match &entry.value {
        Value::String(text) if text.starts_with('#') => Rgb::parse(text),
        _ => None,
    }
    .ok_or_else(|| invalid(section, entry, "a color such as \"#212733\""))
}

fn hex(rgb: Rgb) -> String {
    format!("\"#{:02X}{:02X}{:02X}\"", rgb.r, rgb.g, rgb.b)
}

fn quoted(text: &str) -> String {
    let mut quoted = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            c if c.is_control() => {
                let _ = write!(quoted, "\\u{:04X}", u32::from(c));
            }
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

impl Default for Config {
    fn default() -> Config {
        let palette = Palette::default();
        Config {
            font_family: FONT_FAMILY.into(),
            font_size: 13.0,
            columns: 80,
            lines: 24,
            padding: 4.0,
            scrollback: 10_000,
            foreground: palette.foreground,
            background: palette.background,
            cursor: palette.cursor,
            ansi: std::array::from_fn(|index| palette.indexed(index as u8)),
            keys: Keys::default(),
        }
    }
}

impl Config {
    pub fn from_toml(text: &str) -> Result<Config, toml::Error> {
        let mut config = Config::default();
        for entry in toml::parse(text)?.entries() {
            match &*entry.key {
                "font" => config.font(table("", entry)?)?,
                "window" => config.window(table("", entry)?)?,
                "scrollback" => config.history(table("", entry)?)?,
                "colors" => config.colors(table("", entry)?)?,
                "keys" => config.keys(table("", entry)?)?,
                _ => return Err(unknown("", entry)),
            }
        }
        Ok(config)
    }

    fn font(&mut self, table: &Table<'_>) -> Result<(), toml::Error> {
        for entry in table.entries() {
            match &*entry.key {
                "family" => text("font.", entry)?.clone_into(&mut self.font_family),
                "size" => self.font_size = number("font.", entry, &FONT_SIZE)?,
                _ => return Err(unknown("font.", entry)),
            }
        }
        Ok(())
    }

    fn window(&mut self, table: &Table<'_>) -> Result<(), toml::Error> {
        for entry in table.entries() {
            match &*entry.key {
                "columns" => self.columns = integer("window.", entry, &COLUMNS)? as u16,
                "lines" => self.lines = integer("window.", entry, &LINES)? as u16,
                "padding" => self.padding = number("window.", entry, &PADDING)?,
                _ => return Err(unknown("window.", entry)),
            }
        }
        Ok(())
    }

    fn history(&mut self, table: &Table<'_>) -> Result<(), toml::Error> {
        for entry in table.entries() {
            match &*entry.key {
                "lines" => self.scrollback = integer("scrollback.", entry, &SCROLLBACK)? as u32,
                _ => return Err(unknown("scrollback.", entry)),
            }
        }
        Ok(())
    }

    fn colors(&mut self, colors: &Table<'_>) -> Result<(), toml::Error> {
        for entry in colors.entries() {
            match &*entry.key {
                "foreground" => self.foreground = color("colors.", entry)?,
                "background" => self.background = color("colors.", entry)?,
                "cursor" => self.cursor = color("colors.", entry)?,
                "normal" => self.shades("colors.normal.", table("colors.", entry)?, 0)?,
                "bright" => self.shades("colors.bright.", table("colors.", entry)?, 8)?,
                _ => return Err(unknown("colors.", entry)),
            }
        }
        Ok(())
    }

    fn shades(
        &mut self,
        section: &str,
        shades: &Table<'_>,
        offset: usize,
    ) -> Result<(), toml::Error> {
        for entry in shades.entries() {
            let index = SHADES
                .iter()
                .position(|name| *name == entry.key)
                .ok_or_else(|| unknown(section, entry))?;
            self.ansi[offset + index] = color(section, entry)?;
        }
        Ok(())
    }

    fn keys(&mut self, table: &Table<'_>) -> Result<(), toml::Error> {
        let mut assigned = Vec::new();
        for entry in table.entries() {
            let action = Action::ALL
                .into_iter()
                .find(|action| action.name() == entry.key)
                .ok_or_else(|| unknown("keys.", entry))?;
            self.keys.set(action, shortcut("keys.", entry)?);
            assigned.push((action, entry));
        }
        for (action, entry) in assigned.into_iter().rev() {
            if let Some(holder) = self.keys.holder(action) {
                return Err(invalid(
                    "keys.",
                    entry,
                    &format!("a shortcut that `keys.{}` does not use", holder.name()),
                ));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn to_toml(&self) -> String {
        let mut text = format!(
            "[font]\nfamily = {}\nsize = {:?}\n\n[window]\ncolumns = {}\nlines = {}\npadding = {:?}\n\n[scrollback]\nlines = {}\n\n[colors]\nforeground = {}\nbackground = {}\ncursor = {}\n",
            quoted(&self.font_family),
            self.font_size,
            self.columns,
            self.lines,
            self.padding,
            self.scrollback,
            hex(self.foreground),
            hex(self.background),
            hex(self.cursor),
        );
        for (section, shades) in [("normal", &self.ansi[..8]), ("bright", &self.ansi[8..])] {
            let _ = write!(text, "\n[colors.{section}]\n");
            for (name, rgb) in SHADES.iter().zip(shades) {
                let _ = writeln!(text, "{name} = {}", hex(*rgb));
            }
        }
        text.push_str("\n[keys]\n");
        for action in Action::ALL {
            let shortcut = self
                .keys
                .get(action)
                .map_or_default(|shortcut| shortcut.to_string());
            let _ = writeln!(text, "{} = {}", action.name(), quoted(&shortcut));
        }
        text
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(89 + 2 * ACTIONS + self.font_family.len());
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&self.font_size.to_le_bytes());
        bytes.extend_from_slice(&self.padding.to_le_bytes());
        bytes.extend_from_slice(&self.columns.to_le_bytes());
        bytes.extend_from_slice(&self.lines.to_le_bytes());
        bytes.extend_from_slice(&self.scrollback.to_le_bytes());
        for rgb in [self.foreground, self.background, self.cursor]
            .iter()
            .chain(&self.ansi)
        {
            bytes.extend_from_slice(&[rgb.r, rgb.g, rgb.b]);
        }
        for action in Action::ALL {
            bytes.extend_from_slice(&code(self.keys.get(action)));
        }
        bytes.extend_from_slice(self.font_family.as_bytes());
        bytes
    }

    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Config> {
        let (magic, rest) = bytes.split_first_chunk::<8>()?;
        let (font_size, rest) = rest.split_first_chunk::<8>()?;
        let (padding, rest) = rest.split_first_chunk::<8>()?;
        let (columns, rest) = rest.split_first_chunk::<2>()?;
        let (lines, rest) = rest.split_first_chunk::<2>()?;
        let (scrollback, rest) = rest.split_first_chunk::<4>()?;
        let (colors, rest) = rest.split_first_chunk::<57>()?;
        let (codes, family) = rest.split_first_chunk::<{ 2 * ACTIONS }>()?;
        let mut keys = Keys([None; ACTIONS]);
        for (action, pair) in Action::ALL.into_iter().zip(codes.as_chunks::<2>().0) {
            keys.set(
                action,
                if *pair == [0, 0] {
                    None
                } else {
                    Some(decoded(*pair)?)
                },
            );
        }
        let color = |index: usize| {
            Rgb::new(
                colors[3 * index],
                colors[3 * index + 1],
                colors[3 * index + 2],
            )
        };
        let config = Config {
            font_family: str::from_utf8(family).ok()?.to_owned(),
            font_size: f64::from_le_bytes(*font_size),
            columns: u16::from_le_bytes(*columns),
            lines: u16::from_le_bytes(*lines),
            padding: f64::from_le_bytes(*padding),
            scrollback: u32::from_le_bytes(*scrollback),
            foreground: color(0),
            background: color(1),
            cursor: color(2),
            ansi: std::array::from_fn(|index| color(index + 3)),
            keys,
        };
        (magic == MAGIC
            && Action::ALL
                .into_iter()
                .all(|action| keys.holder(action).is_none())
            && FONT_SIZE.contains(&config.font_size)
            && PADDING.contains(&config.padding)
            && COLUMNS.contains(&i64::from(config.columns))
            && LINES.contains(&i64::from(config.lines))
            && SCROLLBACK.contains(&i64::from(config.scrollback)))
        .then_some(config)
    }

    #[must_use]
    pub fn palette(&self) -> Palette {
        let mut palette = Palette::default();
        for (index, rgb) in (0..).zip(self.ansi) {
            palette.set_indexed(index, rgb);
        }
        palette.foreground = self.foreground;
        palette.background = self.background;
        palette.cursor = self.cursor;
        palette
    }
}

impl Locations {
    #[must_use]
    pub fn resolve(
        home: Option<OsString>,
        config_home: Option<OsString>,
        state_home: Option<OsString>,
    ) -> Option<Locations> {
        let home = home.filter(|home| !home.is_empty()).map(PathBuf::from);
        let base = |explicit: Option<OsString>, fallback: &str| {
            explicit
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .or_else(|| home.as_ref().map(|home| home.join(fallback)))
        };
        Some(Locations {
            file: base(config_home, ".config")?.join("dartel/dartel.toml"),
            state: base(state_home, ".local/state")?.join("dartel/settings.bin"),
        })
    }

    #[must_use]
    pub fn from_env() -> Option<Locations> {
        Locations::resolve(
            env::var_os("HOME"),
            env::var_os("XDG_CONFIG_HOME"),
            env::var_os("XDG_STATE_HOME"),
        )
    }
}

#[must_use]
pub fn expand(text: &str, home: Option<&Path>) -> PathBuf {
    match (text.strip_prefix('~'), home) {
        (Some(""), Some(home)) => home.to_path_buf(),
        (Some(rest), Some(home)) if rest.starts_with('/') => home.join(&rest[1..]),
        _ => PathBuf::from(text),
    }
}

fn write(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

#[must_use]
pub fn active(state: &Path) -> Config {
    fs::read(state)
        .ok()
        .and_then(|bytes| Config::decode(&bytes))
        .unwrap_or_default()
}

pub fn save(config: &Config, file: &Path) -> io::Result<()> {
    write(file, config.to_toml().as_bytes())
}

pub fn load(file: &Path, state: &Path) -> Result<Config, Error> {
    let config = Config::from_toml(&fs::read_to_string(file)?)?;
    write(state, &config.encode())?;
    Ok(config)
}
