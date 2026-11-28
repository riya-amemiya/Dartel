use super::inflate::zlib;
use super::{Image, MAX_SIDE};

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const WHOLE: [[usize; 4]; 1] = [[0, 0, 1, 1]];
const ADAM7: [[usize; 4]; 7] = [
    [0, 0, 8, 8],
    [4, 0, 8, 8],
    [0, 4, 4, 8],
    [2, 0, 4, 4],
    [0, 2, 2, 4],
    [1, 0, 2, 2],
    [0, 1, 1, 2],
];
const CRC: [u32; 256] = {
    let mut table = [0; 256];
    let mut index = 0;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 == 1 {
                0xEDB8_8320 ^ (value >> 1)
            } else {
                value >> 1
            };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
};

#[derive(Clone, Copy)]
struct Header {
    width: usize,
    height: usize,
    depth: u8,
    color: u8,
    interlaced: bool,
}

impl Header {
    fn parse(data: &[u8]) -> Option<Header> {
        let &[
            w0,
            w1,
            w2,
            w3,
            h0,
            h1,
            h2,
            h3,
            depth,
            color,
            0,
            0,
            interlace @ (0 | 1),
        ] = data
        else {
            return None;
        };
        let header = Header {
            width: u32::from_be_bytes([w0, w1, w2, w3]) as usize,
            height: u32::from_be_bytes([h0, h1, h2, h3]) as usize,
            depth,
            color,
            interlaced: interlace == 1,
        };
        let sides = 1..=MAX_SIDE as usize;
        (sides.contains(&header.width)
            && sides.contains(&header.height)
            && match color {
                0 => matches!(depth, 1 | 2 | 4 | 8 | 16),
                3 => matches!(depth, 1 | 2 | 4 | 8),
                2 | 4 | 6 => matches!(depth, 8 | 16),
                _ => false,
            })
        .then_some(header)
    }

    fn channels(self) -> usize {
        match self.color {
            0 | 3 => 1,
            2 => 3,
            4 => 2,
            _ => 4,
        }
    }

    fn stride(self, columns: usize) -> usize {
        (columns * self.channels() * usize::from(self.depth)).div_ceil(8)
    }

    fn step(self) -> usize {
        (self.channels() * usize::from(self.depth)).div_ceil(8)
    }

    fn passes(self) -> &'static [[usize; 4]] {
        if self.interlaced { &ADAM7 } else { &WHOLE }
    }

    fn extent(self, &[left, top, across, down]: &[usize; 4]) -> (usize, usize) {
        let columns = self.width.saturating_sub(left).div_ceil(across);
        let rows = self.height.saturating_sub(top).div_ceil(down);
        if columns == 0 {
            (0, 0)
        } else {
            (columns, rows)
        }
    }

    fn sample(self, line: &[u8], index: usize) -> u16 {
        match self.depth {
            8 => u16::from(line[index]),
            16 => u16::from_be_bytes([line[2 * index], line[2 * index + 1]]),
            depth => {
                let bit = index * usize::from(depth);
                u16::from(line[bit / 8] >> (8 - usize::from(depth) - bit % 8) & ((1 << depth) - 1))
            }
        }
    }

    fn narrow(self, value: u16) -> u8 {
        match self.depth {
            8 => value as u8,
            16 => (value >> 8) as u8,
            depth => (value * 255 / ((1 << depth) - 1)) as u8,
        }
    }

    fn pixel(self, line: &[u8], column: usize, palette: &[u8], alpha: &[u8]) -> Option<[u8; 4]> {
        let raw = |channel: usize| self.sample(line, column * self.channels() + channel);
        Some(match self.color {
            0 => {
                let gray = self.narrow(raw(0));
                [gray, gray, gray, opacity(alpha, &[raw(0)])]
            }
            2 => [
                self.narrow(raw(0)),
                self.narrow(raw(1)),
                self.narrow(raw(2)),
                opacity(alpha, &[raw(0), raw(1), raw(2)]),
            ],
            3 => {
                let index = usize::from(raw(0));
                let entry = palette.get(3 * index..3 * index + 3)?;
                [
                    entry[0],
                    entry[1],
                    entry[2],
                    alpha.get(index).copied().unwrap_or(u8::MAX),
                ]
            }
            4 => {
                let gray = self.narrow(raw(0));
                [gray, gray, gray, self.narrow(raw(1))]
            }
            _ => [
                self.narrow(raw(0)),
                self.narrow(raw(1)),
                self.narrow(raw(2)),
                self.narrow(raw(3)),
            ],
        })
    }
}

fn opacity(key: &[u8], samples: &[u16]) -> u8 {
    if key.len() == 2 * samples.len()
        && key
            .as_chunks::<2>()
            .0
            .iter()
            .zip(samples)
            .all(|(key, &sample)| u16::from_be_bytes(*key) == sample)
    {
        0
    } else {
        u8::MAX
    }
}

fn crc(kind: [u8; 4], data: &[u8]) -> u32 {
    !kind.iter().chain(data).fold(u32::MAX, |sum, &byte| {
        CRC[usize::from(sum as u8 ^ byte)] ^ (sum >> 8)
    })
}

fn paeth(left: u8, above: u8, corner: u8) -> u8 {
    let estimate = i16::from(left) + i16::from(above) - i16::from(corner);
    let distance = |value: u8| (estimate - i16::from(value)).abs();
    if distance(left) <= distance(above) && distance(left) <= distance(corner) {
        left
    } else if distance(above) <= distance(corner) {
        above
    } else {
        corner
    }
}

fn unfilter(kind: u8, step: usize, previous: &[u8], line: &mut [u8]) -> Option<()> {
    match kind {
        0 => {}
        1 => {
            for index in step..line.len() {
                line[index] = line[index].wrapping_add(line[index - step]);
            }
        }
        2 => {
            for (byte, &above) in line.iter_mut().zip(previous) {
                *byte = byte.wrapping_add(above);
            }
        }
        3 => {
            for index in 0..line.len() {
                let left = if index < step { 0 } else { line[index - step] };
                line[index] = line[index]
                    .wrapping_add(u16::midpoint(u16::from(left), u16::from(previous[index])) as u8);
            }
        }
        4 => {
            for index in 0..line.len() {
                let (left, corner) = if index < step {
                    (0, 0)
                } else {
                    (line[index - step], previous[index - step])
                };
                line[index] = line[index].wrapping_add(paeth(left, previous[index], corner));
            }
        }
        _ => return None,
    }
    Some(())
}

#[must_use]
pub fn decode(data: &[u8]) -> Option<Image> {
    let mut rest = data.strip_prefix(&SIGNATURE)?;
    let mut header = None;
    let (mut palette, mut alpha): (&[u8], &[u8]) = (&[], &[]);
    let mut body = Vec::new();
    loop {
        let (length, tail) = rest.split_first_chunk::<4>()?;
        let (kind, tail) = tail.split_first_chunk::<4>()?;
        let (payload, tail) = tail.split_at_checked(u32::from_be_bytes(*length) as usize)?;
        let (sum, tail) = tail.split_first_chunk::<4>()?;
        if crc(*kind, payload) != u32::from_be_bytes(*sum) {
            return None;
        }
        rest = tail;
        match kind {
            b"IHDR" => header = Some(Header::parse(payload)?),
            b"PLTE" => palette = payload,
            b"tRNS" => alpha = payload,
            b"IDAT" => body.extend_from_slice(payload),
            b"IEND" => break,
            _ => {}
        }
    }
    let header = header?;
    let mut raw = zlib(
        &body,
        header
            .passes()
            .iter()
            .map(|pass| header.extent(pass))
            .map(|(columns, rows)| rows * (1 + header.stride(columns)))
            .sum(),
    )?;
    let blank = vec![0; header.stride(header.width)];
    let mut pixels = vec![0; header.width * header.height * 4];
    let mut rest = raw.as_mut_slice();
    for pass in header.passes() {
        let (columns, rows) = header.extent(pass);
        let stride = header.stride(columns);
        let mut previous = &blank[..stride];
        for row in 0..rows {
            let (scanline, tail) = std::mem::take(&mut rest).split_at_mut_checked(1 + stride)?;
            let (kind, line) = scanline.split_first_mut()?;
            unfilter(*kind, header.step(), previous, line)?;
            let start = ((pass[1] + row * pass[3]) * header.width + pass[0]) * 4;
            if header.color == 6 && header.depth == 8 && pass[2] == 1 {
                pixels[start..start + line.len()].copy_from_slice(line);
            } else {
                for column in 0..columns {
                    pixels[start + column * pass[2] * 4..][..4]
                        .copy_from_slice(&header.pixel(line, column, palette, alpha)?);
                }
            }
            previous = line;
            rest = tail;
        }
    }
    Image::from_rgba(header.width as u32, header.height as u32, pixels)
}
