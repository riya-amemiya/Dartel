#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Decoded {
    Char(char, usize),
    Invalid(usize),
    Incomplete,
}

#[inline]
pub(super) fn decode(bytes: &[u8]) -> Decoded {
    match *bytes {
        [
            lead @ (0xE1..=0xEC | 0xEE..=0xEF),
            second @ 0x80..=0xBF,
            third @ 0x80..=0xBF,
            ..,
        ] => Decoded::Char(
            char::from_u32(
                u32::from(lead & 0x0F) << 12
                    | u32::from(second & 0x3F) << 6
                    | u32::from(third & 0x3F),
            )
            .unwrap_or(char::REPLACEMENT_CHARACTER),
            3,
        ),
        [lead @ 0xC2..=0xDF, second @ 0x80..=0xBF, ..] => Decoded::Char(
            char::from_u32(u32::from(lead & 0x1F) << 6 | u32::from(second & 0x3F))
                .unwrap_or(char::REPLACEMENT_CHARACTER),
            2,
        ),
        _ => general(bytes),
    }
}

fn general(bytes: &[u8]) -> Decoded {
    let Some(&lead) = bytes.first() else {
        return Decoded::Incomplete;
    };
    let (need, low, high) = match lead {
        0xC2..=0xDF => (1, 0x80, 0xBF),
        0xE0 => (2, 0xA0, 0xBF),
        0xE1..=0xEC | 0xEE..=0xEF => (2, 0x80, 0xBF),
        0xED => (2, 0x80, 0x9F),
        0xF0 => (3, 0x90, 0xBF),
        0xF1..=0xF3 => (3, 0x80, 0xBF),
        0xF4 => (3, 0x80, 0x8F),
        _ => return Decoded::Invalid(1),
    };
    let mut code = u32::from(lead) & (0x7F >> (need + 1));
    for index in 1..=need {
        let Some(&byte) = bytes.get(index) else {
            return Decoded::Incomplete;
        };
        let valid = if index == 1 {
            (low..=high).contains(&byte)
        } else {
            (0x80..=0xBF).contains(&byte)
        };
        if !valid {
            return Decoded::Invalid(index);
        }
        code = (code << 6) | u32::from(byte & 0x3F);
    }
    Decoded::Char(
        char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER),
        need + 1,
    )
}

#[cfg(test)]
mod tests {
    use super::{Decoded, decode};

    #[test]
    fn decodes_every_scalar_value() {
        for c in (0x80..=0x0010_FFFF).filter_map(char::from_u32) {
            let mut buffer = [0; 4];
            let encoded = c.encode_utf8(&mut buffer).as_bytes();
            assert_eq!(decode(encoded), Decoded::Char(c, encoded.len()));
        }
    }

    #[test]
    fn reports_truncated_sequences_as_incomplete() {
        assert_eq!(decode(b""), Decoded::Incomplete);
        assert_eq!(decode(b"\xE3"), Decoded::Incomplete);
        assert_eq!(decode(b"\xE3\x81"), Decoded::Incomplete);
        assert_eq!(decode(b"\xF0\x9F\x98"), Decoded::Incomplete);
    }

    #[test]
    fn stops_at_the_maximal_subpart() {
        assert_eq!(decode(b"\xE2\x98x"), Decoded::Invalid(2));
        assert_eq!(decode(b"\xE3\x1B"), Decoded::Invalid(1));
        assert_eq!(decode(b"\xF0\x9F\x98A"), Decoded::Invalid(3));
    }

    #[test]
    fn rejects_bytes_that_never_start_a_sequence() {
        assert_eq!(decode(b"\x80"), Decoded::Invalid(1));
        assert_eq!(decode(b"\xC0\xAF"), Decoded::Invalid(1));
        assert_eq!(decode(b"\xC1\xBF"), Decoded::Invalid(1));
        assert_eq!(decode(b"\xF5\x80\x80\x80"), Decoded::Invalid(1));
        assert_eq!(decode(b"\xFF"), Decoded::Invalid(1));
    }

    #[test]
    fn rejects_overlong_surrogate_and_out_of_range_forms() {
        assert_eq!(decode(b"\xE0\x80\x80"), Decoded::Invalid(1));
        assert_eq!(decode(b"\xED\xA0\x80"), Decoded::Invalid(1));
        assert_eq!(decode(b"\xF0\x80\x80\x80"), Decoded::Invalid(1));
        assert_eq!(decode(b"\xF4\x90\x80\x80"), Decoded::Invalid(1));
    }
}
