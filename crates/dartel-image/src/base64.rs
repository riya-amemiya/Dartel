fn sextet(byte: u8) -> Option<u32> {
    Some(u32::from(match byte {
        b'A'..=b'Z' => byte - b'A',
        b'a'..=b'z' => byte - b'a' + 26,
        b'0'..=b'9' => byte - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    }))
}

#[must_use]
pub fn decode(text: &[u8]) -> Option<Vec<u8>> {
    let text = match text {
        [rest @ .., b'=', b'='] | [rest @ .., b'='] => rest,
        _ => text,
    };
    let mut bytes = Vec::with_capacity(text.len() / 4 * 3 + 2);
    for chunk in text.chunks(4) {
        let value = chunk
            .iter()
            .try_fold(0, |value, &byte| Some(value << 6 | sextet(byte)?))?;
        match chunk.len() {
            4 => bytes.extend_from_slice(&value.to_be_bytes()[1..]),
            3 => bytes.extend_from_slice(&(value << 6).to_be_bytes()[1..3]),
            2 => bytes.push((value >> 4) as u8),
            _ => return None,
        }
    }
    Some(bytes)
}
