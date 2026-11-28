use super::{Image, MAX_SIDE};

const REGISTERS: usize = 256;
const SIDE: usize = MAX_SIDE as usize;
const VT340: [[usize; 3]; 16] = [
    [0, 0, 0],
    [20, 20, 80],
    [80, 13, 13],
    [20, 80, 20],
    [80, 20, 80],
    [20, 80, 80],
    [80, 80, 20],
    [53, 53, 53],
    [26, 26, 26],
    [33, 33, 60],
    [60, 26, 26],
    [33, 60, 33],
    [60, 33, 60],
    [33, 60, 60],
    [60, 60, 33],
    [80, 80, 80],
];

fn level(percent: usize) -> u8 {
    ((percent.min(100) * 255 + 50) / 100) as u8
}

fn opaque([red, green, blue]: [u8; 3]) -> [u8; 4] {
    [red, green, blue, u8::MAX]
}

fn hls(hue: usize, lightness: usize, saturation: usize) -> [u8; 3] {
    let (hue, lightness) = ((hue % 360 + 240) % 360, lightness.min(100));
    let chroma = (100 - (2 * lightness).abs_diff(100)) * saturation.min(100);
    let second = chroma * (60 - (hue % 120).abs_diff(60)) / 60;
    let base = lightness * 100 - chroma / 2;
    match hue / 60 {
        0 => [chroma, second, 0],
        1 => [second, chroma, 0],
        2 => [0, chroma, second],
        3 => [0, second, chroma],
        4 => [second, 0, chroma],
        _ => [chroma, 0, second],
    }
    .map(|part| (((part + base) * 255 + 5000) / 10000) as u8)
}

fn numbers(data: &[u8]) -> ([usize; 5], usize, &[u8]) {
    let mut values = [0_usize; 5];
    let mut count = 0;
    for (at, &byte) in data.iter().enumerate() {
        match byte {
            b'0'..=b'9' => {
                if let Some(value) = values.get_mut(count) {
                    *value = value
                        .saturating_mul(10)
                        .saturating_add(usize::from(byte - b'0'));
                }
            }
            b';' => count += 1,
            _ => return (values, count + 1, &data[at..]),
        }
    }
    (values, count + 1, &[])
}

#[must_use]
pub fn decode(data: &[u8], backdrop: Option<[u8; 3]>) -> Option<Image> {
    let mut registers: [[u8; 4]; REGISTERS] =
        std::array::from_fn(|index| opaque(VT340[index % VT340.len()].map(level)));
    let mut rows: Vec<Vec<[u8; 4]>> = Vec::new();
    let (mut width, mut height) = (0, 0);
    let (mut column, mut band, mut color, mut repeat) = (0_usize, 0_usize, 0, 1);
    let mut rest = data;
    while let Some((&byte, tail)) = rest.split_first() {
        rest = tail;
        match byte {
            b'!' => {
                let (values, _, tail) = numbers(rest);
                repeat = values[0].max(1);
                rest = tail;
            }
            b'"' => {
                let (values, _, tail) = numbers(rest);
                width = width.max(values[2].min(SIDE));
                height = height.max(values[3].min(SIDE));
                rest = tail;
            }
            b'#' => {
                let (values, count, tail) = numbers(rest);
                color = values[0] % REGISTERS;
                if count == 5 {
                    registers[color] = match values[1] {
                        1 => opaque(hls(values[2], values[3], values[4])),
                        2 => opaque([level(values[2]), level(values[3]), level(values[4])]),
                        _ => registers[color],
                    };
                }
                rest = tail;
            }
            b'$' => column = 0,
            b'-' => {
                column = 0;
                band += 1;
            }
            b'?'..=b'~' => {
                let end = column.saturating_add(repeat).min(SIDE);
                let bits = byte - b'?';
                if bits != 0 && column < end {
                    for line in (band.saturating_mul(6)..SIDE)
                        .take(6)
                        .enumerate()
                        .filter(|&(offset, _)| bits >> offset & 1 == 1)
                        .map(|(_, line)| line)
                    {
                        if rows.len() <= line {
                            rows.resize_with(line + 1, Vec::new);
                        }
                        if rows[line].len() < end {
                            rows[line].resize(end, [0; 4]);
                        }
                        rows[line][column..end].fill(registers[color]);
                    }
                    width = width.max(end);
                }
                column = column.saturating_add(repeat);
                repeat = 1;
            }
            _ => {}
        }
    }
    let height = height.max(rows.len());
    let fill = backdrop.map_or([0; 4], opaque);
    let mut pixels = Vec::with_capacity(width * height * 4);
    for line in 0..height {
        let row = rows.get(line).map_or(&[][..], Vec::as_slice);
        for column in 0..width {
            pixels.extend_from_slice(
                &row.get(column)
                    .copied()
                    .filter(|pixel| pixel[3] != 0)
                    .unwrap_or(fill),
            );
        }
    }
    Image::from_rgba(width as u32, height as u32, pixels)
}
