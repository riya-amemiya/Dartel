use std::fmt::Write as _;

pub const DISAMBIGUATE: u8 = 1;
pub const EVENT_TYPES: u8 = 1 << 1;
pub const ALTERNATES: u8 = 1 << 2;
pub const ALL_KEYS: u8 = 1 << 3;
pub const TEXT: u8 = 1 << 4;

pub const SHIFT: u8 = 1;
pub const ALT: u8 = 1 << 1;
pub const CTRL: u8 = 1 << 2;
pub const SUPER: u8 = 1 << 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Press,
    Repeat,
    Release,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keypad {
    Digit(u8),
    Decimal,
    Divide,
    Multiply,
    Subtract,
    Add,
    Enter,
    Equal,
    Separator,
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Insert,
    Delete,
    Begin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Text(char),
    Escape,
    Enter,
    Tab,
    Backspace,
    Insert,
    Delete,
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    CapsLock,
    ScrollLock,
    NumLock,
    PrintScreen,
    Pause,
    Menu,
    F(u8),
    Keypad(Keypad),
    Shift(Side),
    Control(Side),
    Alt(Side),
    Super(Side),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stroke<'a> {
    pub key: Key,
    pub shifted: Option<char>,
    pub base: Option<char>,
    pub text: &'a str,
    pub modifiers: u8,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Encoded {
    Bytes(Vec<u8>),
    Legacy,
    Nothing,
}

fn side(side: Side) -> u32 {
    match side {
        Side::Left => 0,
        Side::Right => 6,
    }
}

fn keypad(key: Keypad) -> (u32, u8) {
    match key {
        Keypad::Digit(digit) => (57399 + u32::from(digit.min(9)), b'u'),
        Keypad::Decimal => (57409, b'u'),
        Keypad::Divide => (57410, b'u'),
        Keypad::Multiply => (57411, b'u'),
        Keypad::Subtract => (57412, b'u'),
        Keypad::Add => (57413, b'u'),
        Keypad::Enter => (57414, b'u'),
        Keypad::Equal => (57415, b'u'),
        Keypad::Separator => (57416, b'u'),
        Keypad::Left => (57417, b'u'),
        Keypad::Right => (57418, b'u'),
        Keypad::Up => (57419, b'u'),
        Keypad::Down => (57420, b'u'),
        Keypad::PageUp => (57421, b'u'),
        Keypad::PageDown => (57422, b'u'),
        Keypad::Home => (57423, b'u'),
        Keypad::End => (57424, b'u'),
        Keypad::Insert => (57425, b'u'),
        Keypad::Delete => (57426, b'u'),
        Keypad::Begin => (1, b'E'),
    }
}

fn code(key: Key) -> (u32, u8) {
    match key {
        Key::Text(c) => (u32::from(c), b'u'),
        Key::Escape => (27, b'u'),
        Key::Enter => (13, b'u'),
        Key::Tab => (9, b'u'),
        Key::Backspace => (127, b'u'),
        Key::Insert => (2, b'~'),
        Key::Delete => (3, b'~'),
        Key::Left => (1, b'D'),
        Key::Right => (1, b'C'),
        Key::Up => (1, b'A'),
        Key::Down => (1, b'B'),
        Key::PageUp => (5, b'~'),
        Key::PageDown => (6, b'~'),
        Key::Home => (1, b'H'),
        Key::End => (1, b'F'),
        Key::CapsLock => (57358, b'u'),
        Key::ScrollLock => (57359, b'u'),
        Key::NumLock => (57360, b'u'),
        Key::PrintScreen => (57361, b'u'),
        Key::Pause => (57362, b'u'),
        Key::Menu => (57363, b'u'),
        Key::F(1) => (1, b'P'),
        Key::F(2) => (1, b'Q'),
        Key::F(3) => (13, b'~'),
        Key::F(4) => (1, b'S'),
        Key::F(number @ 5..=12) => (
            [15, 17, 18, 19, 20, 21, 23, 24][usize::from(number) - 5],
            b'~',
        ),
        Key::F(number) => (57363 + u32::from(number.clamp(13, 35)), b'u'),
        Key::Keypad(key) => keypad(key),
        Key::Shift(at) => (57441 + side(at), b'u'),
        Key::Control(at) => (57442 + side(at), b'u'),
        Key::Alt(at) => (57443 + side(at), b'u'),
        Key::Super(at) => (57444 + side(at), b'u'),
    }
}

fn alternates(stroke: &Stroke<'_>, number: u32) -> String {
    let shifted = stroke
        .shifted
        .filter(|&c| stroke.modifiers & SHIFT != 0 && u32::from(c) != number)
        .map(u32::from);
    let base = stroke
        .base
        .filter(|&c| u32::from(c) != number)
        .map(u32::from);
    match (shifted, base) {
        (Some(shifted), Some(base)) => format!(":{shifted}:{base}"),
        (Some(shifted), None) => format!(":{shifted}"),
        (None, Some(base)) => format!("::{base}"),
        (None, None) => String::new(),
    }
}

#[must_use]
pub fn encode(stroke: &Stroke<'_>, flags: u8) -> Encoded {
    let all = flags & ALL_KEYS != 0;
    let disambiguate = flags & (DISAMBIGUATE | ALL_KEYS) != 0;
    let event = match stroke.action {
        Action::Repeat if flags & EVENT_TYPES != 0 => 2,
        Action::Press | Action::Repeat => 1,
        Action::Release if flags & EVENT_TYPES != 0 => 3,
        Action::Release => return Encoded::Nothing,
    };
    let legacy = match stroke.key {
        Key::Shift(_)
        | Key::Control(_)
        | Key::Alt(_)
        | Key::Super(_)
        | Key::CapsLock
        | Key::ScrollLock
        | Key::NumLock => {
            return if all {
                csi(stroke, flags, event)
            } else {
                Encoded::Nothing
            };
        }
        Key::Escape => !disambiguate,
        Key::Enter | Key::Tab | Key::Backspace => {
            !disambiguate || (!all && (event == 3 || stroke.modifiers == 0))
        }
        Key::Text(_)
        | Key::Keypad(
            Keypad::Digit(_)
            | Keypad::Decimal
            | Keypad::Divide
            | Keypad::Multiply
            | Keypad::Subtract
            | Keypad::Add
            | Keypad::Equal
            | Keypad::Separator,
        ) => !disambiguate || (!all && (event == 3 || stroke.modifiers & !SHIFT == 0)),
        _ => false,
    };
    match (legacy, event) {
        (true, 3) => Encoded::Nothing,
        (true, _) => Encoded::Legacy,
        (false, _) => csi(stroke, flags, event),
    }
}

fn csi(stroke: &Stroke<'_>, flags: u8, event: u8) -> Encoded {
    let (number, trailer) = code(stroke.key);
    let text = Some(stroke.text).filter(|text| {
        flags & TEXT != 0
            && flags & ALL_KEYS != 0
            && event != 3
            && !text.is_empty()
            && !text.chars().any(char::is_control)
    });
    let modifiers = 1 + u32::from(stroke.modifiers);
    let mut fields = match (modifiers, event, text) {
        (1, 1, None) => String::new(),
        (1, 1, Some(_)) => ";".to_owned(),
        (_, 1, _) => format!(";{modifiers}"),
        _ => format!(";{modifiers}:{event}"),
    };
    if let Some(text) = text {
        for (index, c) in text.chars().enumerate() {
            let _ = write!(
                fields,
                "{}{}",
                if index == 0 { ';' } else { ':' },
                u32::from(c)
            );
        }
    }
    let head = if number == 1 && trailer.is_ascii_uppercase() && fields.is_empty() {
        String::new()
    } else if flags & ALTERNATES != 0 && matches!(stroke.key, Key::Text(_)) {
        format!("{number}{}", alternates(stroke, number))
    } else {
        number.to_string()
    };
    Encoded::Bytes(format!("\x1b[{head}{fields}{}", char::from(trailer)).into_bytes())
}
