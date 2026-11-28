const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DISTANCE_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DISTANCE_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
const ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];
const ADLER: u32 = 65521;

struct Bits<'a> {
    data: &'a [u8],
    at: usize,
    hold: u32,
    count: u32,
}

impl Bits<'_> {
    fn take(&mut self, need: u32) -> Option<usize> {
        while self.count < need {
            self.hold |= u32::from(*self.data.get(self.at)?) << self.count;
            self.at += 1;
            self.count += 8;
        }
        let value = self.hold & ((1 << need) - 1);
        self.hold >>= need;
        self.count -= need;
        Some(value as usize)
    }
}

struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Option<Huffman> {
        let mut counts = [0_u16; 16];
        for &length in lengths {
            counts[usize::from(length)] += 1;
        }
        counts[0] = 0;
        counts[1..].iter().try_fold(1_i32, |left, &count| {
            Some((left << 1) - i32::from(count)).filter(|left| *left >= 0)
        })?;
        let mut offsets = [0_u16; 16];
        for length in 1..15 {
            offsets[length + 1] = offsets[length] + counts[length];
        }
        let mut symbols = vec![0; lengths.len()];
        for (symbol, &length) in lengths.iter().enumerate() {
            if length != 0 {
                symbols[usize::from(offsets[usize::from(length)])] = symbol as u16;
                offsets[usize::from(length)] += 1;
            }
        }
        Some(Huffman { counts, symbols })
    }

    fn decode(&self, bits: &mut Bits<'_>) -> Option<usize> {
        let (mut code, mut first, mut index) = (0, 0, 0);
        for &count in &self.counts[1..] {
            code |= bits.take(1)?;
            if code < first + usize::from(count) {
                return self
                    .symbols
                    .get(index + code - first)
                    .map(|&symbol| usize::from(symbol));
            }
            index += usize::from(count);
            first = (first + usize::from(count)) << 1;
            code <<= 1;
        }
        None
    }
}

fn stored(bits: &mut Bits<'_>, out: &mut Vec<u8>, limit: usize) -> Option<()> {
    bits.hold = 0;
    bits.count = 0;
    let (header, rest) = bits.data.get(bits.at..)?.split_first_chunk::<4>()?;
    let length = usize::from(u16::from_le_bytes([header[0], header[1]]));
    let body = rest
        .get(..length)
        .filter(|_| header[0] == !header[2] && header[1] == !header[3])?;
    if out.len() + length > limit {
        return None;
    }
    out.extend_from_slice(body);
    bits.at += 4 + length;
    Some(())
}

fn block(
    bits: &mut Bits<'_>,
    out: &mut Vec<u8>,
    limit: usize,
    literals: &Huffman,
    distances: &Huffman,
) -> Option<()> {
    loop {
        match literals.decode(bits)? {
            symbol @ 0..=255 => {
                if out.len() == limit {
                    return None;
                }
                out.push(symbol as u8);
            }
            256 => return Some(()),
            symbol @ 257..=285 => {
                let length = usize::from(LENGTH_BASE[symbol - 257])
                    + bits.take(u32::from(LENGTH_EXTRA[symbol - 257]))?;
                let code = distances.decode(bits)?;
                let distance = usize::from(*DISTANCE_BASE.get(code)?)
                    + bits.take(u32::from(DISTANCE_EXTRA[code]))?;
                if distance > out.len() || out.len() + length > limit {
                    return None;
                }
                let start = out.len() - distance;
                if distance >= length {
                    out.extend_from_within(start..start + length);
                } else {
                    for index in start..start + length {
                        out.push(out[index]);
                    }
                }
            }
            _ => return None,
        }
    }
}

fn fixed() -> Option<(Huffman, Huffman)> {
    let mut lengths = [8_u8; 288];
    lengths[144..256].fill(9);
    lengths[256..280].fill(7);
    Some((Huffman::new(&lengths)?, Huffman::new(&[5; 30])?))
}

fn dynamic(bits: &mut Bits<'_>) -> Option<(Huffman, Huffman)> {
    let literals = bits.take(5)? + 257;
    let distances = bits.take(5)? + 1;
    let codes = bits.take(4)? + 4;
    if literals > 286 || distances > 30 {
        return None;
    }
    let mut code_lengths = [0_u8; 19];
    for &index in &ORDER[..codes] {
        code_lengths[index] = bits.take(3)? as u8;
    }
    let code = Huffman::new(&code_lengths)?;
    let mut lengths = [0_u8; 316];
    let mut index = 0;
    while index < literals + distances {
        let (value, repeat) = match code.decode(bits)? {
            symbol @ 0..=15 => (symbol as u8, 1),
            16 => (*lengths.get(index.checked_sub(1)?)?, 3 + bits.take(2)?),
            17 => (0, 3 + bits.take(3)?),
            _ => (0, 11 + bits.take(7)?),
        };
        if index + repeat > literals + distances {
            return None;
        }
        lengths[index..index + repeat].fill(value);
        index += repeat;
    }
    if lengths[256] == 0 {
        return None;
    }
    Some((
        Huffman::new(&lengths[..literals])?,
        Huffman::new(&lengths[literals..literals + distances])?,
    ))
}

fn raw(data: &[u8], limit: usize) -> Option<(Vec<u8>, usize)> {
    let mut bits = Bits {
        data,
        at: 0,
        hold: 0,
        count: 0,
    };
    let mut out = Vec::new();
    loop {
        let last = bits.take(1)? == 1;
        match bits.take(2)? {
            0 => stored(&mut bits, &mut out, limit)?,
            1 => {
                let (literals, distances) = fixed()?;
                block(&mut bits, &mut out, limit, &literals, &distances)?;
            }
            2 => {
                let (literals, distances) = dynamic(&mut bits)?;
                block(&mut bits, &mut out, limit, &literals, &distances)?;
            }
            _ => return None,
        }
        if last {
            return Some((out, bits.at));
        }
    }
}

fn adler32(data: &[u8]) -> u32 {
    let (low, high) = data.chunks(5552).fold((1, 0), |(low, high), chunk| {
        let (low, high) = chunk
            .iter()
            .fold((low, high), |(low, high): (u32, u32), &byte| {
                (low + u32::from(byte), high + low + u32::from(byte))
            });
        (low % ADLER, high % ADLER)
    });
    high << 16 | low
}

#[must_use]
pub fn inflate(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    raw(data, limit).map(|(out, _)| out)
}

#[must_use]
pub fn zlib(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    let (header, rest) = data.split_first_chunk::<2>()?;
    if header[0] & 0x0F != 8 || u16::from_be_bytes(*header) % 31 != 0 || header[1] & 0x20 != 0 {
        return None;
    }
    let (out, used) = raw(rest, limit)?;
    let (check, _) = rest.get(used..)?.split_first_chunk::<4>()?;
    (u32::from_be_bytes(*check) == adler32(&out)).then_some(out)
}
