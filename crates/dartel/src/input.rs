use dartel_term::Modes;
use dartel_term::keys::{self, ALT, Action, CTRL, Encoded, Keypad, SHIFT, SUPER, Side, Stroke};
use winit::event::{ElementState, KeyEvent, Modifiers};
use winit::keyboard::{Key, KeyCode, KeyLocation, NamedKey, PhysicalKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

const LAYOUT: [(KeyCode, char); 47] = [
    (KeyCode::KeyA, 'a'),
    (KeyCode::KeyB, 'b'),
    (KeyCode::KeyC, 'c'),
    (KeyCode::KeyD, 'd'),
    (KeyCode::KeyE, 'e'),
    (KeyCode::KeyF, 'f'),
    (KeyCode::KeyG, 'g'),
    (KeyCode::KeyH, 'h'),
    (KeyCode::KeyI, 'i'),
    (KeyCode::KeyJ, 'j'),
    (KeyCode::KeyK, 'k'),
    (KeyCode::KeyL, 'l'),
    (KeyCode::KeyM, 'm'),
    (KeyCode::KeyN, 'n'),
    (KeyCode::KeyO, 'o'),
    (KeyCode::KeyP, 'p'),
    (KeyCode::KeyQ, 'q'),
    (KeyCode::KeyR, 'r'),
    (KeyCode::KeyS, 's'),
    (KeyCode::KeyT, 't'),
    (KeyCode::KeyU, 'u'),
    (KeyCode::KeyV, 'v'),
    (KeyCode::KeyW, 'w'),
    (KeyCode::KeyX, 'x'),
    (KeyCode::KeyY, 'y'),
    (KeyCode::KeyZ, 'z'),
    (KeyCode::Digit0, '0'),
    (KeyCode::Digit1, '1'),
    (KeyCode::Digit2, '2'),
    (KeyCode::Digit3, '3'),
    (KeyCode::Digit4, '4'),
    (KeyCode::Digit5, '5'),
    (KeyCode::Digit6, '6'),
    (KeyCode::Digit7, '7'),
    (KeyCode::Digit8, '8'),
    (KeyCode::Digit9, '9'),
    (KeyCode::Minus, '-'),
    (KeyCode::Equal, '='),
    (KeyCode::BracketLeft, '['),
    (KeyCode::BracketRight, ']'),
    (KeyCode::Backslash, '\\'),
    (KeyCode::Semicolon, ';'),
    (KeyCode::Quote, '\''),
    (KeyCode::Comma, ','),
    (KeyCode::Period, '.'),
    (KeyCode::Slash, '/'),
    (KeyCode::Backquote, '`'),
];

#[cfg(target_os = "macos")]
fn meta(modifiers: Modifiers) -> bool {
    modifiers.lalt_state() == winit::keyboard::ModifiersKeyState::Pressed
}

#[cfg(not(target_os = "macos"))]
fn meta(modifiers: Modifiers) -> bool {
    modifiers.state().alt_key()
}

fn is_control(text: &str) -> bool {
    text.bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_control())
}

fn control_code(key: &Key) -> Option<u8> {
    match key {
        Key::Character(text) => match text.chars().next()? {
            c @ ('@'..='_' | 'a'..='z') => Some(c as u8 & 0x1F),
            '?' => Some(0x7F),
            _ => None,
        },
        _ => None,
    }
}

fn prefixed(alt: bool, bytes: &[u8]) -> Vec<u8> {
    if alt {
        [b"\x1b", bytes].concat()
    } else {
        bytes.to_vec()
    }
}

fn letter(level: u8, application: bool, action: u8) -> Vec<u8> {
    match (level, application) {
        (1, true) => vec![0x1B, b'O', action],
        (1, false) => vec![0x1B, b'[', action],
        _ => format!("\x1b[1;{level}{}", char::from(action)).into_bytes(),
    }
}

fn tilde(level: u8, code: u8) -> Vec<u8> {
    if level == 1 {
        format!("\x1b[{code}~").into_bytes()
    } else {
        format!("\x1b[{code};{level}~").into_bytes()
    }
}

fn named(key: NamedKey, modifiers: Modifiers, modes: Modes) -> Option<Vec<u8>> {
    let state = modifiers.state();
    let alt = meta(modifiers);
    let level =
        1 + u8::from(state.shift_key()) + 2 * u8::from(alt) + 4 * u8::from(state.control_key());
    let application = modes.contains(Modes::APP_CURSOR);
    Some(match key {
        NamedKey::Enter => prefixed(alt, b"\r"),
        NamedKey::Tab if state.shift_key() => b"\x1b[Z".to_vec(),
        NamedKey::Tab => prefixed(alt, b"\t"),
        NamedKey::Backspace if state.control_key() => prefixed(alt, b"\x08"),
        NamedKey::Backspace => prefixed(alt, b"\x7f"),
        NamedKey::Escape => prefixed(alt, b"\x1b"),
        NamedKey::Space if state.control_key() => prefixed(alt, b"\0"),
        NamedKey::Space => prefixed(alt, b" "),
        NamedKey::ArrowUp => letter(level, application, b'A'),
        NamedKey::ArrowDown => letter(level, application, b'B'),
        NamedKey::ArrowRight => letter(level, application, b'C'),
        NamedKey::ArrowLeft => letter(level, application, b'D'),
        NamedKey::Home => letter(level, application, b'H'),
        NamedKey::End => letter(level, application, b'F'),
        NamedKey::Insert => tilde(level, 2),
        NamedKey::Delete => tilde(level, 3),
        NamedKey::PageUp => tilde(level, 5),
        NamedKey::PageDown => tilde(level, 6),
        NamedKey::F1 => letter(level, true, b'P'),
        NamedKey::F2 => letter(level, true, b'Q'),
        NamedKey::F3 => letter(level, true, b'R'),
        NamedKey::F4 => letter(level, true, b'S'),
        NamedKey::F5 => tilde(level, 15),
        NamedKey::F6 => tilde(level, 17),
        NamedKey::F7 => tilde(level, 18),
        NamedKey::F8 => tilde(level, 19),
        NamedKey::F9 => tilde(level, 20),
        NamedKey::F10 => tilde(level, 21),
        NamedKey::F11 => tilde(level, 23),
        NamedKey::F12 => tilde(level, 24),
        _ => return None,
    })
}

fn character(event: &KeyEvent, modifiers: Modifiers) -> Option<Vec<u8>> {
    let text = event
        .text_with_all_modifiers()
        .or(event.text.as_deref())
        .unwrap_or_default();
    if modifiers.state().control_key()
        && !is_control(text)
        && let Some(code) = control_code(&event.key_without_modifiers())
    {
        return Some(prefixed(meta(modifiers), &[code]));
    }
    (!text.is_empty()).then(|| prefixed(meta(modifiers), text.as_bytes()))
}

fn single(text: &str) -> Option<char> {
    let mut chars = text.chars();
    chars.next().filter(|_| chars.next().is_none())
}

fn side(location: KeyLocation) -> Side {
    if location == KeyLocation::Right {
        Side::Right
    } else {
        Side::Left
    }
}

fn keypad(key: &Key) -> Option<Keypad> {
    Some(match key {
        Key::Named(NamedKey::Enter) => Keypad::Enter,
        Key::Named(NamedKey::ArrowLeft) => Keypad::Left,
        Key::Named(NamedKey::ArrowRight) => Keypad::Right,
        Key::Named(NamedKey::ArrowUp) => Keypad::Up,
        Key::Named(NamedKey::ArrowDown) => Keypad::Down,
        Key::Named(NamedKey::PageUp) => Keypad::PageUp,
        Key::Named(NamedKey::PageDown) => Keypad::PageDown,
        Key::Named(NamedKey::Home) => Keypad::Home,
        Key::Named(NamedKey::End) => Keypad::End,
        Key::Named(NamedKey::Insert) => Keypad::Insert,
        Key::Named(NamedKey::Delete) => Keypad::Delete,
        Key::Named(NamedKey::Clear) => Keypad::Begin,
        Key::Character(text) => match single(text)? {
            digit @ '0'..='9' => Keypad::Digit(digit as u8 - b'0'),
            '.' => Keypad::Decimal,
            '/' => Keypad::Divide,
            '*' => Keypad::Multiply,
            '-' => Keypad::Subtract,
            '+' => Keypad::Add,
            '=' => Keypad::Equal,
            ',' => Keypad::Separator,
            _ => return None,
        },
        _ => return None,
    })
}

fn function(key: NamedKey) -> Option<u8> {
    [
        NamedKey::F1,
        NamedKey::F2,
        NamedKey::F3,
        NamedKey::F4,
        NamedKey::F5,
        NamedKey::F6,
        NamedKey::F7,
        NamedKey::F8,
        NamedKey::F9,
        NamedKey::F10,
        NamedKey::F11,
        NamedKey::F12,
        NamedKey::F13,
        NamedKey::F14,
        NamedKey::F15,
        NamedKey::F16,
        NamedKey::F17,
        NamedKey::F18,
        NamedKey::F19,
        NamedKey::F20,
        NamedKey::F21,
        NamedKey::F22,
        NamedKey::F23,
        NamedKey::F24,
        NamedKey::F25,
        NamedKey::F26,
        NamedKey::F27,
        NamedKey::F28,
        NamedKey::F29,
        NamedKey::F30,
        NamedKey::F31,
        NamedKey::F32,
        NamedKey::F33,
        NamedKey::F34,
        NamedKey::F35,
    ]
    .iter()
    .position(|candidate| *candidate == key)
    .map(|index| index as u8 + 1)
}

fn identified(event: &KeyEvent) -> Option<keys::Key> {
    if event.location == KeyLocation::Numpad
        && let Some(key) = keypad(&event.logical_key)
    {
        return Some(keys::Key::Keypad(key));
    }
    Some(match &event.logical_key {
        Key::Named(NamedKey::Escape) => keys::Key::Escape,
        Key::Named(NamedKey::Enter) => keys::Key::Enter,
        Key::Named(NamedKey::Tab) => keys::Key::Tab,
        Key::Named(NamedKey::Backspace) => keys::Key::Backspace,
        Key::Named(NamedKey::Space) => keys::Key::Text(' '),
        Key::Named(NamedKey::Insert) => keys::Key::Insert,
        Key::Named(NamedKey::Delete) => keys::Key::Delete,
        Key::Named(NamedKey::ArrowLeft) => keys::Key::Left,
        Key::Named(NamedKey::ArrowRight) => keys::Key::Right,
        Key::Named(NamedKey::ArrowUp) => keys::Key::Up,
        Key::Named(NamedKey::ArrowDown) => keys::Key::Down,
        Key::Named(NamedKey::PageUp) => keys::Key::PageUp,
        Key::Named(NamedKey::PageDown) => keys::Key::PageDown,
        Key::Named(NamedKey::Home) => keys::Key::Home,
        Key::Named(NamedKey::End) => keys::Key::End,
        Key::Named(NamedKey::CapsLock) => keys::Key::CapsLock,
        Key::Named(NamedKey::ScrollLock) => keys::Key::ScrollLock,
        Key::Named(NamedKey::NumLock) => keys::Key::NumLock,
        Key::Named(NamedKey::PrintScreen) => keys::Key::PrintScreen,
        Key::Named(NamedKey::Pause) => keys::Key::Pause,
        Key::Named(NamedKey::ContextMenu) => keys::Key::Menu,
        Key::Named(NamedKey::Shift) => keys::Key::Shift(side(event.location)),
        Key::Named(NamedKey::Control) => keys::Key::Control(side(event.location)),
        Key::Named(NamedKey::Alt) => keys::Key::Alt(side(event.location)),
        Key::Named(NamedKey::Super) => keys::Key::Super(side(event.location)),
        Key::Named(key) => keys::Key::F(function(*key)?),
        Key::Character(_) => match event.key_without_modifiers() {
            Key::Character(text) => {
                keys::Key::Text(single(&text)?.to_lowercase().next().unwrap_or_default())
            }
            _ => return None,
        },
        _ => return None,
    })
}

fn stroke(event: &KeyEvent, modifiers: Modifiers) -> Option<Stroke<'_>> {
    let state = modifiers.state();
    let key = identified(event)?;
    let pressed = event.state == ElementState::Pressed;
    Some(Stroke {
        key,
        shifted: match &event.logical_key {
            Key::Character(text) => single(text),
            _ => None,
        },
        base: match event.physical_key {
            PhysicalKey::Code(code) => LAYOUT
                .iter()
                .find(|(candidate, _)| *candidate == code)
                .map(|&(_, c)| c),
            PhysicalKey::Unidentified(_) => None,
        },
        text: event.text.as_deref().unwrap_or_default(),
        modifiers: [
            (SHIFT, matches!(key, keys::Key::Shift(_)), state.shift_key()),
            (ALT, matches!(key, keys::Key::Alt(_)), meta(modifiers)),
            (
                CTRL,
                matches!(key, keys::Key::Control(_)),
                state.control_key(),
            ),
            (SUPER, matches!(key, keys::Key::Super(_)), state.super_key()),
        ]
        .into_iter()
        .filter(|&(_, own, down)| down || (own && pressed))
        .fold(0, |bits, (bit, _, _)| bits | bit),
        action: match (event.state, event.repeat) {
            (ElementState::Released, _) => Action::Release,
            (ElementState::Pressed, true) => Action::Repeat,
            (ElementState::Pressed, false) => Action::Press,
        },
    })
}

pub fn encode(
    event: &KeyEvent,
    modifiers: Modifiers,
    modes: Modes,
    keyboard: u8,
) -> Option<Vec<u8>> {
    if keyboard != 0
        && let Some(stroke) = stroke(event, modifiers)
    {
        match keys::encode(&stroke, keyboard) {
            Encoded::Bytes(bytes) => return Some(bytes),
            Encoded::Nothing => return None,
            Encoded::Legacy => {}
        }
    }
    if event.state == ElementState::Released {
        return None;
    }
    match &event.logical_key {
        Key::Named(key) => named(*key, modifiers, modes).or_else(|| character(event, modifiers)),
        _ => character(event, modifiers),
    }
}
