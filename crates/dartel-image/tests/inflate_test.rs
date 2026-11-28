use dartel_image::inflate::{inflate, zlib};

const REPEATED: [u8; 5] = [0x4B, 0x4C, 0xC4, 0x04, 0x00];
const QUICK: [u8; 56] = [
    0x2B, 0xC9, 0x48, 0x55, 0x28, 0x2C, 0xCD, 0x4C, 0xCE, 0x56, 0x48, 0x2A, 0xCA, 0x2F, 0xCF, 0x53,
    0x48, 0xCB, 0xAF, 0x50, 0xC8, 0x2A, 0xCD, 0x2D, 0x28, 0x56, 0xC8, 0x2F, 0x4B, 0x2D, 0x52, 0x28,
    0x01, 0x4A, 0xE7, 0x24, 0x56, 0x55, 0x2A, 0xA4, 0xE4, 0xA7, 0x2B, 0x24, 0xE6, 0xA5, 0x80, 0x05,
    0x88, 0x56, 0x9F, 0x9E, 0x98, 0x99, 0x07, 0x00,
];
const TALE: [u8; 99] = [
    0x75, 0xCD, 0xC1, 0x09, 0x80, 0x30, 0x10, 0x44, 0xD1, 0x56, 0xB6, 0x00, 0xCB, 0xF0, 0x22, 0xD8,
    0x44, 0xD4, 0x49, 0xB2, 0x18, 0xB3, 0x92, 0x5D, 0x09, 0x76, 0x2F, 0x39, 0x89, 0x10, 0xCF, 0xEF,
    0x0F, 0x33, 0x19, 0x55, 0xA7, 0x64, 0x11, 0xB4, 0x40, 0x8D, 0xC4, 0x93, 0xF1, 0x01, 0x1D, 0x88,
    0x5F, 0xA9, 0x52, 0xFE, 0xC8, 0x05, 0x34, 0xA8, 0xAC, 0x9B, 0x1C, 0x3D, 0xF1, 0x22, 0x89, 0x35,
    0x66, 0xE8, 0x77, 0x88, 0x53, 0xD6, 0xD8, 0x82, 0x05, 0x89, 0xE1, 0xFB, 0xC6, 0x79, 0x2D, 0xD8,
    0xAE, 0xC4, 0x76, 0x7F, 0x02, 0x85, 0x53, 0xC9, 0xAD, 0x98, 0x39, 0x44, 0xFB, 0xB1, 0xD1, 0x95,
    0xBD, 0xFD, 0x3E,
];
const SCANLINES: [u8; 26] = [
    0x08, 0xD7, 0x63, 0x64, 0x60, 0x60, 0x30, 0x62, 0x90, 0x84, 0x23, 0x16, 0x06, 0x23, 0x49, 0x06,
    0x06, 0x04, 0x22, 0x95, 0x0F, 0x00, 0xB0, 0x5E, 0x03, 0xFA,
];
const EMPTY: [u8; 8] = [0x78, 0x9C, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01];

fn bits(stream: &str) -> Vec<u8> {
    stream
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<u8>>()
        .chunks(8)
        .map(|chunk| {
            chunk
                .iter()
                .enumerate()
                .fold(0, |byte, (index, &bit)| byte | (bit - b'0') << index)
        })
        .collect()
}

#[test]
fn stored_block_copies_its_bytes() {
    assert_eq!(
        inflate(&[0x01, 0x03, 0x00, 0xFC, 0xFF, b'a', b'b', b'c'], 3).as_deref(),
        Some(&b"abc"[..])
    );
}

#[test]
fn blocks_are_read_until_the_final_one() {
    assert_eq!(
        inflate(
            &[
                0x00, 0x01, 0x00, 0xFE, 0xFF, b'a', 0x01, 0x01, 0x00, 0xFE, 0xFF, b'b'
            ],
            2
        )
        .as_deref(),
        Some(&b"ab"[..])
    );
}

#[test]
fn stored_block_with_a_mismatched_length_check_is_rejected() {
    assert_eq!(inflate(&[0x01, 0x01, 0x00, 0x00, 0x00, b'a'], 1), None);
}

#[test]
fn stored_block_shorter_than_its_length_is_rejected() {
    assert_eq!(inflate(&[0x01, 0x05, 0x00, 0xFA, 0xFF, b'a'], 5), None);
}

#[test]
fn stored_block_with_a_cut_header_is_rejected() {
    assert_eq!(inflate(&[0x01, 0x01], 1), None);
}

#[test]
fn stored_block_larger_than_the_limit_is_rejected() {
    assert_eq!(
        inflate(&[0x01, 0x03, 0x00, 0xFC, 0xFF, b'a', b'b', b'c'], 2),
        None
    );
}

#[test]
fn fixed_block_decodes_a_literal() {
    assert_eq!(inflate(&[0x4B, 0x04, 0x00], 1).as_deref(), Some(&b"a"[..]));
}

#[test]
fn literal_past_the_limit_is_rejected() {
    assert_eq!(inflate(&[0x4B, 0x04, 0x00], 0), None);
}

#[test]
fn match_longer_than_its_distance_repeats_the_copied_bytes() {
    assert_eq!(inflate(&REPEATED, 20), Some(vec![b'a'; 20]));
}

#[test]
fn match_past_the_limit_is_rejected() {
    assert_eq!(inflate(&REPEATED, 10), None);
}

#[test]
fn fixed_block_copies_earlier_text() {
    assert_eq!(
        inflate(&QUICK, 97).as_deref(),
        Some(
            &b"the quick brown fox jumps over the lazy dog and the quick brown fox jumps over the lazy dog again"[..]
        )
    );
}

#[test]
fn dynamic_block_builds_its_own_codes() {
    assert_eq!(
        inflate(&TALE, 228).as_deref(),
        Some(
            &b"It was the best of times, it was the worst of times, it was the age of wisdom, it was the age of foolishness, it was the epoch of belief, it was the epoch of incredulity, it was the season of Light, it was the season of Darkness"[..]
        )
    );
}

#[test]
fn dynamic_block_expands_every_run_length_code() {
    let stream = bits(
        "1 01 00000 00000 0111
         110 110 010 010 000 000 000 000 000 000 000 000 000 110 000 000 000 110
         01 0110101  101  110 00  01 1111111  111 111  111 001  100  00
         100 101 110 111 0",
    );
    assert_eq!(inflate(&stream, 4).as_deref(), Some(&b"abcd"[..]));
}

#[test]
fn reserved_block_type_is_rejected() {
    assert_eq!(inflate(&bits("1 11"), 1), None);
}

#[test]
fn empty_input_is_rejected() {
    assert_eq!(inflate(&[], 1), None);
}

#[test]
fn block_cut_inside_a_code_is_rejected() {
    assert_eq!(inflate(&[0x4B], 1), None);
}

#[test]
fn match_reaching_before_the_output_is_rejected() {
    assert_eq!(inflate(&bits("1 10 0000001 00000"), 3), None);
}

#[test]
fn unassigned_length_code_is_rejected() {
    assert_eq!(inflate(&bits("1 10 11000110"), 3), None);
}

#[test]
fn unassigned_distance_code_is_rejected() {
    assert_eq!(inflate(&bits("1 10 10010001 0000001 11110"), 4), None);
}

#[test]
fn more_literal_codes_than_the_format_allows_are_rejected() {
    assert_eq!(inflate(&bits("1 01 11111 00000 0000"), 1), None);
}

#[test]
fn more_distance_codes_than_the_format_allows_are_rejected() {
    assert_eq!(inflate(&bits("1 01 00000 11111 0000"), 1), None);
}

#[test]
fn oversubscribed_code_lengths_are_rejected() {
    assert_eq!(
        inflate(&bits("1 01 00000 00000 0000 100 100 100 100"), 1),
        None
    );
}

#[test]
fn code_without_an_assigned_symbol_is_rejected() {
    assert_eq!(
        inflate(
            &bits("1 01 00000 00000 0000 000 000 000 100 1111111111111111"),
            1
        ),
        None
    );
}

#[test]
fn repeat_code_without_a_previous_length_is_rejected() {
    assert_eq!(
        inflate(&bits("1 01 00000 00000 0000 100 000 000 100 1 00"), 1),
        None
    );
}

#[test]
fn run_past_the_announced_code_count_is_rejected() {
    assert_eq!(
        inflate(
            &bits("1 01 00000 00000 0000 000 000 100 100 1 1111111 1 1111111"),
            1
        ),
        None
    );
}

#[test]
fn dynamic_block_without_an_end_code_is_rejected() {
    assert_eq!(
        inflate(
            &bits("1 01 00000 00000 0000 000 000 100 100 1 1111111 1 1011011"),
            1
        ),
        None
    );
}

#[test]
fn zlib_stream_yields_the_wrapped_data() {
    let expected = [
        &[1, 0, 0, 0, 50, 0, 25, 50, 0, 25, 50, 0, 25, 50, 0, 25][..],
        &[4, 0, 50, 25, 0, 0, 25, 0, 0, 25, 0, 0, 25, 0, 0, 25].repeat(4),
    ]
    .concat();
    assert_eq!(zlib(&SCANLINES, 80), Some(expected));
}

#[test]
fn zlib_stream_of_no_data_is_accepted() {
    assert_eq!(zlib(&EMPTY, 0), Some(Vec::new()));
}

#[test]
fn zlib_checksum_covers_data_longer_than_one_summing_block() {
    let stream = [
        &[0x78, 0x01, 0x01, 0x70, 0x17, 0x8F, 0xE8][..],
        &[0xFF; 6000],
        &[0xA4, 0x97, 0x59, 0xEA],
    ]
    .concat();
    assert_eq!(zlib(&stream, 6000), Some(vec![0xFF; 6000]));
}

#[test]
fn zlib_stream_cut_inside_its_header_is_rejected() {
    assert_eq!(zlib(&[0x78], 1), None);
}

#[test]
fn zlib_stream_with_another_compression_method_is_rejected() {
    assert_eq!(
        zlib(&[0x79, 0x18, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01], 0),
        None
    );
}

#[test]
fn zlib_stream_with_a_failing_header_check_is_rejected() {
    assert_eq!(
        zlib(&[0x78, 0x9D, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01], 0),
        None
    );
}

#[test]
fn zlib_stream_asking_for_a_preset_dictionary_is_rejected() {
    assert_eq!(
        zlib(&[0x78, 0x20, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01], 0),
        None
    );
}

#[test]
fn zlib_stream_with_broken_data_is_rejected() {
    assert_eq!(zlib(&[0x78, 0x9C, 0x07], 1), None);
}

#[test]
fn zlib_stream_with_a_wrong_checksum_is_rejected() {
    assert_eq!(
        zlib(&[0x78, 0x9C, 0x03, 0x00, 0x00, 0x00, 0x00, 0x02], 0),
        None
    );
}

#[test]
fn zlib_stream_cut_inside_its_checksum_is_rejected() {
    assert_eq!(zlib(&[0x78, 0x9C, 0x03, 0x00, 0x00, 0x00, 0x00], 0), None);
}
