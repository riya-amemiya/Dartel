use dartel_core::scan::printable_run;

fn naive(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .take_while(|&&byte| (0x20..=0x7E).contains(&byte))
        .count()
}

#[test]
fn empty_input_has_no_run() {
    assert_eq!(printable_run(b""), 0);
}

#[test]
fn printable_text_runs_to_the_end_at_every_length() {
    let text: Vec<u8> = (0..200).map(|index| b' ' + (index % 95) as u8).collect();
    for length in 0..=text.len() {
        assert_eq!(printable_run(&text[..length]), length, "length {length}");
    }
}

#[test]
fn every_byte_value_at_every_position_matches_the_naive_scan() {
    for length in 1..70 {
        for position in 0..length {
            for byte in 0..=u8::MAX {
                let mut bytes = vec![b'a'; length];
                bytes[position] = byte;
                assert_eq!(
                    printable_run(&bytes),
                    naive(&bytes),
                    "length {length} position {position} byte {byte:#04x}"
                );
            }
        }
    }
}

#[test]
fn stops_at_the_first_of_several_rejected_bytes() {
    let mut bytes = vec![b'x'; 64];
    bytes[40] = 0x1B;
    bytes[20] = b'\n';
    bytes[50] = 0xE3;
    assert_eq!(printable_run(&bytes), 20);
}

#[test]
fn unaligned_slices_match_the_naive_scan() {
    let bytes: Vec<u8> = (0..256)
        .map(|index| if index % 37 == 36 { 0x7F } else { b'~' })
        .collect();
    for start in 0..40 {
        assert_eq!(
            printable_run(&bytes[start..]),
            naive(&bytes[start..]),
            "start {start}"
        );
    }
}
