use dartel_core::term::Modes;
use winit::event::{KeyEvent, Modifiers};
use winit::keyboard::{Key, NamedKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

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

pub fn encode(event: &KeyEvent, modifiers: Modifiers, modes: Modes) -> Option<Vec<u8>> {
    match &event.logical_key {
        Key::Named(key) => named(*key, modifiers, modes).or_else(|| character(event, modifiers)),
        _ => character(event, modifiers),
    }
}
