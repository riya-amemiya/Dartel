use dartel_term::keys::{
    ALL_KEYS, ALT, ALTERNATES, Action, CTRL, DISAMBIGUATE, EVENT_TYPES, Encoded, Key, Keypad,
    SHIFT, Side, Stroke, TEXT, encode,
};

fn stroke(key: Key, modifiers: u8) -> Stroke<'static> {
    Stroke {
        key,
        shifted: None,
        base: None,
        text: "",
        modifiers,
        action: Action::Press,
    }
}

fn bytes(text: &str) -> Encoded {
    Encoded::Bytes(text.as_bytes().to_vec())
}

#[test]
fn plain_text_key_stays_legacy_when_disambiguating() {
    assert_eq!(
        encode(&stroke(Key::Text('a'), 0), DISAMBIGUATE),
        Encoded::Legacy
    );
}

#[test]
fn shifted_text_key_stays_legacy_when_disambiguating() {
    assert_eq!(
        encode(&stroke(Key::Text('a'), SHIFT), DISAMBIGUATE),
        Encoded::Legacy
    );
}

#[test]
fn control_text_key_is_an_escape_code_when_disambiguating() {
    assert_eq!(
        encode(&stroke(Key::Text('i'), CTRL), DISAMBIGUATE),
        bytes("\x1b[105;5u")
    );
}

#[test]
fn alt_text_key_is_an_escape_code_when_disambiguating() {
    assert_eq!(
        encode(&stroke(Key::Text('a'), ALT), DISAMBIGUATE),
        bytes("\x1b[97;3u")
    );
}

#[test]
fn control_shift_text_key_reports_the_unshifted_code() {
    assert_eq!(
        encode(&stroke(Key::Text('a'), CTRL | SHIFT), DISAMBIGUATE),
        bytes("\x1b[97;6u")
    );
}

#[test]
fn unmodified_enter_tab_and_backspace_stay_legacy_when_disambiguating() {
    for key in [Key::Enter, Key::Tab, Key::Backspace] {
        assert_eq!(encode(&stroke(key, 0), DISAMBIGUATE), Encoded::Legacy);
    }
}

#[test]
fn modified_enter_tab_and_backspace_are_escape_codes_when_disambiguating() {
    assert_eq!(
        encode(&stroke(Key::Enter, SHIFT), DISAMBIGUATE),
        bytes("\x1b[13;2u")
    );
    assert_eq!(
        encode(&stroke(Key::Tab, CTRL), DISAMBIGUATE),
        bytes("\x1b[9;5u")
    );
    assert_eq!(
        encode(&stroke(Key::Backspace, ALT), DISAMBIGUATE),
        bytes("\x1b[127;3u")
    );
}

#[test]
fn escape_is_an_escape_code_when_disambiguating() {
    assert_eq!(
        encode(&stroke(Key::Escape, 0), DISAMBIGUATE),
        bytes("\x1b[27u")
    );
}

#[test]
fn text_and_c0_keys_stay_legacy_without_disambiguation() {
    for key in [
        Key::Text('a'),
        Key::Escape,
        Key::Enter,
        Key::Tab,
        Key::Backspace,
    ] {
        assert_eq!(encode(&stroke(key, CTRL), EVENT_TYPES), Encoded::Legacy);
    }
}

#[test]
fn cursor_key_without_modifiers_omits_the_number() {
    assert_eq!(encode(&stroke(Key::Up, 0), DISAMBIGUATE), bytes("\x1b[A"));
}

#[test]
fn cursor_key_with_modifiers_keeps_the_number() {
    assert_eq!(
        encode(&stroke(Key::Left, CTRL), DISAMBIGUATE),
        bytes("\x1b[1;5D")
    );
}

#[test]
fn functional_keys_use_their_numbers_and_final_characters() {
    for (key, expected) in [
        (Key::Insert, "\x1b[2~"),
        (Key::Delete, "\x1b[3~"),
        (Key::Right, "\x1b[C"),
        (Key::Down, "\x1b[B"),
        (Key::PageUp, "\x1b[5~"),
        (Key::PageDown, "\x1b[6~"),
        (Key::Home, "\x1b[H"),
        (Key::End, "\x1b[F"),
        (Key::PrintScreen, "\x1b[57361u"),
        (Key::Pause, "\x1b[57362u"),
        (Key::Menu, "\x1b[57363u"),
        (Key::F(1), "\x1b[P"),
        (Key::F(2), "\x1b[Q"),
        (Key::F(3), "\x1b[13~"),
        (Key::F(4), "\x1b[S"),
        (Key::F(5), "\x1b[15~"),
        (Key::F(6), "\x1b[17~"),
        (Key::F(7), "\x1b[18~"),
        (Key::F(8), "\x1b[19~"),
        (Key::F(9), "\x1b[20~"),
        (Key::F(10), "\x1b[21~"),
        (Key::F(11), "\x1b[23~"),
        (Key::F(12), "\x1b[24~"),
        (Key::F(13), "\x1b[57376u"),
        (Key::F(35), "\x1b[57398u"),
    ] {
        assert_eq!(encode(&stroke(key, 0), DISAMBIGUATE), bytes(expected));
    }
}

#[test]
fn keypad_keys_use_their_own_numbers() {
    for (key, expected) in [
        (Keypad::Digit(0), "\x1b[57399u"),
        (Keypad::Digit(9), "\x1b[57408u"),
        (Keypad::Decimal, "\x1b[57409u"),
        (Keypad::Divide, "\x1b[57410u"),
        (Keypad::Multiply, "\x1b[57411u"),
        (Keypad::Subtract, "\x1b[57412u"),
        (Keypad::Add, "\x1b[57413u"),
        (Keypad::Enter, "\x1b[57414u"),
        (Keypad::Equal, "\x1b[57415u"),
        (Keypad::Separator, "\x1b[57416u"),
        (Keypad::Left, "\x1b[57417u"),
        (Keypad::Right, "\x1b[57418u"),
        (Keypad::Up, "\x1b[57419u"),
        (Keypad::Down, "\x1b[57420u"),
        (Keypad::PageUp, "\x1b[57421u"),
        (Keypad::PageDown, "\x1b[57422u"),
        (Keypad::Home, "\x1b[57423u"),
        (Keypad::End, "\x1b[57424u"),
        (Keypad::Insert, "\x1b[57425u"),
        (Keypad::Delete, "\x1b[57426u"),
        (Keypad::Begin, "\x1b[E"),
    ] {
        assert_eq!(
            encode(&stroke(Key::Keypad(key), 0), ALL_KEYS),
            bytes(expected)
        );
    }
}

#[test]
fn keypad_keys_that_type_text_stay_legacy_when_disambiguating() {
    for key in [
        Keypad::Digit(5),
        Keypad::Decimal,
        Keypad::Divide,
        Keypad::Multiply,
        Keypad::Subtract,
        Keypad::Add,
        Keypad::Equal,
        Keypad::Separator,
    ] {
        assert_eq!(
            encode(&stroke(Key::Keypad(key), SHIFT), DISAMBIGUATE),
            Encoded::Legacy
        );
    }
}

#[test]
fn keypad_keys_with_control_or_without_text_are_escape_codes_when_disambiguating() {
    assert_eq!(
        encode(&stroke(Key::Keypad(Keypad::Digit(5)), CTRL), DISAMBIGUATE),
        bytes("\x1b[57404;5u")
    );
    assert_eq!(
        encode(&stroke(Key::Keypad(Keypad::Enter), 0), DISAMBIGUATE),
        bytes("\x1b[57414u")
    );
    assert_eq!(
        encode(&stroke(Key::Keypad(Keypad::Left), 0), DISAMBIGUATE),
        bytes("\x1b[57417u")
    );
}

#[test]
fn release_is_dropped_without_event_types() {
    let released = Stroke {
        action: Action::Release,
        ..stroke(Key::Up, 0)
    };
    assert_eq!(encode(&released, DISAMBIGUATE), Encoded::Nothing);
}

#[test]
fn repeat_is_a_press_without_event_types() {
    let repeated = Stroke {
        action: Action::Repeat,
        ..stroke(Key::Up, 0)
    };
    assert_eq!(encode(&repeated, DISAMBIGUATE), bytes("\x1b[A"));
}

#[test]
fn repeat_and_release_carry_the_event_type() {
    let repeated = Stroke {
        action: Action::Repeat,
        ..stroke(Key::Up, 0)
    };
    let released = Stroke {
        action: Action::Release,
        ..stroke(Key::Up, CTRL)
    };
    assert_eq!(
        encode(&repeated, DISAMBIGUATE | EVENT_TYPES),
        bytes("\x1b[1;1:2A")
    );
    assert_eq!(
        encode(&released, DISAMBIGUATE | EVENT_TYPES),
        bytes("\x1b[1;5:3A")
    );
}

#[test]
fn release_of_text_enter_tab_and_backspace_needs_all_keys() {
    for key in [Key::Text('a'), Key::Enter, Key::Tab, Key::Backspace] {
        let released = Stroke {
            action: Action::Release,
            ..stroke(key, CTRL)
        };
        assert_eq!(
            encode(&released, DISAMBIGUATE | EVENT_TYPES),
            Encoded::Nothing
        );
    }
}

#[test]
fn all_keys_reports_text_keys_as_escape_codes() {
    assert_eq!(
        encode(&stroke(Key::Text('a'), 0), ALL_KEYS),
        bytes("\x1b[97u")
    );
}

#[test]
fn all_keys_reports_enter_tab_and_backspace_as_escape_codes() {
    assert_eq!(encode(&stroke(Key::Enter, 0), ALL_KEYS), bytes("\x1b[13u"));
    assert_eq!(encode(&stroke(Key::Tab, 0), ALL_KEYS), bytes("\x1b[9u"));
    assert_eq!(
        encode(&stroke(Key::Backspace, 0), ALL_KEYS),
        bytes("\x1b[127u")
    );
}

#[test]
fn all_keys_reports_the_release_of_a_text_key() {
    let released = Stroke {
        action: Action::Release,
        ..stroke(Key::Tab, 0)
    };
    assert_eq!(
        encode(&released, ALL_KEYS | EVENT_TYPES),
        bytes("\x1b[9;1:3u")
    );
}

#[test]
fn modifier_and_lock_keys_are_reported_only_with_all_keys() {
    for (key, expected) in [
        (Key::Shift(Side::Left), "\x1b[57441;2u"),
        (Key::Control(Side::Left), "\x1b[57442;2u"),
        (Key::Alt(Side::Left), "\x1b[57443;2u"),
        (Key::Super(Side::Left), "\x1b[57444;2u"),
        (Key::Shift(Side::Right), "\x1b[57447;2u"),
        (Key::Control(Side::Right), "\x1b[57448;2u"),
        (Key::Alt(Side::Right), "\x1b[57449;2u"),
        (Key::Super(Side::Right), "\x1b[57450;2u"),
        (Key::CapsLock, "\x1b[57358;2u"),
        (Key::ScrollLock, "\x1b[57359;2u"),
        (Key::NumLock, "\x1b[57360;2u"),
    ] {
        assert_eq!(encode(&stroke(key, SHIFT), DISAMBIGUATE), Encoded::Nothing);
        assert_eq!(encode(&stroke(key, SHIFT), ALL_KEYS), bytes(expected));
    }
}

#[test]
fn associated_text_follows_the_modifiers() {
    let typed = Stroke {
        text: "A",
        ..stroke(Key::Text('a'), SHIFT)
    };
    assert_eq!(encode(&typed, ALL_KEYS | TEXT), bytes("\x1b[97;2;65u"));
}

#[test]
fn associated_text_leaves_default_modifiers_empty() {
    let typed = Stroke {
        text: "a\u{301}",
        ..stroke(Key::Text('a'), 0)
    };
    assert_eq!(encode(&typed, ALL_KEYS | TEXT), bytes("\x1b[97;;97:769u"));
}

#[test]
fn associated_text_needs_all_keys_and_the_text_flag() {
    let typed = Stroke {
        text: "a",
        ..stroke(Key::Text('a'), CTRL)
    };
    assert_eq!(encode(&typed, ALL_KEYS), bytes("\x1b[97;5u"));
    assert_eq!(encode(&typed, DISAMBIGUATE | TEXT), bytes("\x1b[97;5u"));
}

#[test]
fn associated_text_is_left_out_for_control_characters_and_releases() {
    let control = Stroke {
        text: "\r",
        ..stroke(Key::Enter, 0)
    };
    let released = Stroke {
        text: "a",
        action: Action::Release,
        ..stroke(Key::Text('a'), 0)
    };
    assert_eq!(encode(&control, ALL_KEYS | TEXT), bytes("\x1b[13u"));
    assert_eq!(
        encode(&released, ALL_KEYS | TEXT | EVENT_TYPES),
        bytes("\x1b[97;1:3u")
    );
}

#[test]
fn alternates_report_the_shifted_and_base_keys() {
    let typed = Stroke {
        shifted: Some('+'),
        base: Some('='),
        ..stroke(Key::Text('='), CTRL | SHIFT)
    };
    let cyrillic = Stroke {
        shifted: Some('\u{421}'),
        base: Some('c'),
        ..stroke(Key::Text('\u{441}'), CTRL | SHIFT)
    };
    assert_eq!(
        encode(&typed, DISAMBIGUATE | ALTERNATES),
        bytes("\x1b[61:43;6u")
    );
    assert_eq!(
        encode(&cyrillic, DISAMBIGUATE | ALTERNATES),
        bytes("\x1b[1089:1057:99;6u")
    );
}

#[test]
fn alternates_report_the_base_key_alone_after_an_empty_field() {
    let typed = Stroke {
        shifted: Some('\u{421}'),
        base: Some('c'),
        ..stroke(Key::Text('\u{441}'), CTRL)
    };
    assert_eq!(
        encode(&typed, DISAMBIGUATE | ALTERNATES),
        bytes("\x1b[1089::99;5u")
    );
}

#[test]
fn alternates_are_left_out_without_the_flag_and_for_functional_keys() {
    let typed = Stroke {
        shifted: Some('A'),
        base: Some('q'),
        ..stroke(Key::Text('a'), CTRL | SHIFT)
    };
    let cursor = Stroke {
        shifted: Some('A'),
        base: Some('q'),
        ..stroke(Key::Up, SHIFT)
    };
    assert_eq!(encode(&typed, DISAMBIGUATE), bytes("\x1b[97;6u"));
    assert_eq!(
        encode(&cursor, DISAMBIGUATE | ALTERNATES),
        bytes("\x1b[1;2A")
    );
}
