use dartel_image::base64::decode;

#[test]
fn empty_input_decodes_to_nothing() {
    assert_eq!(decode(b""), Some(Vec::new()));
}

#[test]
fn four_characters_decode_to_three_bytes() {
    assert_eq!(decode(b"Zm9v").as_deref(), Some(&b"foo"[..]));
}

#[test]
fn two_padding_characters_leave_one_byte() {
    assert_eq!(decode(b"Zg==").as_deref(), Some(&b"f"[..]));
}

#[test]
fn one_padding_character_leaves_two_bytes() {
    assert_eq!(decode(b"Zm8=").as_deref(), Some(&b"fo"[..]));
}

#[test]
fn two_trailing_characters_without_padding_leave_one_byte() {
    assert_eq!(decode(b"Zg").as_deref(), Some(&b"f"[..]));
}

#[test]
fn three_trailing_characters_without_padding_leave_two_bytes() {
    assert_eq!(decode(b"Zm8").as_deref(), Some(&b"fo"[..]));
}

#[test]
fn several_groups_decode_in_order() {
    assert_eq!(decode(b"Zm9vYmFy").as_deref(), Some(&b"foobar"[..]));
}

#[test]
fn plus_and_slash_carry_the_two_highest_values() {
    assert_eq!(decode(b"+/+/").as_deref(), Some(&[0xFB, 0xFF, 0xBF][..]));
}

#[test]
fn character_outside_the_alphabet_is_rejected() {
    assert_eq!(decode(b"Zm9!"), None);
}

#[test]
fn single_trailing_character_is_rejected() {
    assert_eq!(decode(b"Zm9vY"), None);
}

#[test]
fn padding_inside_the_text_is_rejected() {
    assert_eq!(decode(b"Zg==Zg=="), None);
}
