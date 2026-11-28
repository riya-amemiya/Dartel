use umt_rust::color::umt_hexa_to_rgba;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    #[must_use]
    pub fn parse(spec: &str) -> Option<Rgb> {
        match spec.strip_prefix("rgb:") {
            Some(channels) => {
                let mut parts = channels.split('/').map(scaled);
                let rgb = Rgb::new(parts.next()??, parts.next()??, parts.next()??);
                parts.next().is_none().then_some(rgb)
            }
            None => umt_hexa_to_rgba(spec)
                .ok()
                .map(|rgba| Rgb::new(rgba.r as u8, rgba.g as u8, rgba.b as u8)),
        }
    }

    #[must_use]
    pub fn to_spec(self) -> String {
        format!(
            "rgb:{0:02x}{0:02x}/{1:02x}{1:02x}/{2:02x}{2:02x}",
            self.r, self.g, self.b
        )
    }
}

fn scaled(part: &str) -> Option<u8> {
    if !(1..=4).contains(&part.len()) || !part.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(part, 16)
        .ok()
        .map(|value| (value * 255 / ((1 << (4 * part.len())) - 1)) as u8)
}

const KIND: u32 = 24;
const INDEXED: u32 = 1;
const DIRECT: u32 = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Color(u32);

impl Color {
    pub const DEFAULT: Color = Color(0);

    #[inline]
    #[must_use]
    pub fn indexed(index: u8) -> Color {
        Color(INDEXED << KIND | u32::from(index))
    }

    #[inline]
    #[must_use]
    pub fn rgb(rgb: Rgb) -> Color {
        Color(DIRECT << KIND | u32::from(rgb.r) << 16 | u32::from(rgb.g) << 8 | u32::from(rgb.b))
    }

    #[inline]
    pub(super) fn bits(self) -> u32 {
        self.0
    }
}

const ANSI: [Rgb; 16] = [
    Rgb::new(0x1D, 0x1F, 0x21),
    Rgb::new(0xCC, 0x66, 0x66),
    Rgb::new(0xB5, 0xBD, 0x68),
    Rgb::new(0xF0, 0xC6, 0x74),
    Rgb::new(0x81, 0xA2, 0xBE),
    Rgb::new(0xBD, 0x7B, 0xCD),
    Rgb::new(0x8A, 0xBE, 0xB7),
    Rgb::new(0xC5, 0xC8, 0xC6),
    Rgb::new(0x66, 0x66, 0x66),
    Rgb::new(0xDF, 0x6C, 0x5A),
    Rgb::new(0xB9, 0xCA, 0x4A),
    Rgb::new(0xE7, 0xC5, 0x47),
    Rgb::new(0x7A, 0xA6, 0xDA),
    Rgb::new(0xC3, 0x97, 0xD8),
    Rgb::new(0x70, 0xC0, 0xB1),
    Rgb::new(0xEA, 0xEA, 0xEA),
];

const FOREGROUND: Rgb = Rgb::new(0xC5, 0xC8, 0xC6);
const BACKGROUND: Rgb = Rgb::new(0x21, 0x27, 0x33);

const fn cube(step: usize) -> u8 {
    if step == 0 { 0 } else { (55 + 40 * step) as u8 }
}

const fn default_table() -> [Rgb; 256] {
    let mut table = [Rgb::new(0, 0, 0); 256];
    let mut index = 0;
    while index < 256 {
        table[index] = if index < 16 {
            ANSI[index]
        } else if index < 232 {
            Rgb::new(
                cube((index - 16) / 36),
                cube((index - 16) / 6 % 6),
                cube((index - 16) % 6),
            )
        } else {
            Rgb::new(
                (8 + 10 * (index - 232)) as u8,
                (8 + 10 * (index - 232)) as u8,
                (8 + 10 * (index - 232)) as u8,
            )
        };
        index += 1;
    }
    table
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    table: [Rgb; 256],
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
}

impl Default for Palette {
    fn default() -> Palette {
        Palette {
            table: const { default_table() },
            foreground: FOREGROUND,
            background: BACKGROUND,
            cursor: FOREGROUND,
        }
    }
}

impl Palette {
    #[inline]
    #[must_use]
    pub fn indexed(&self, index: u8) -> Rgb {
        self.table[usize::from(index)]
    }

    pub fn set_indexed(&mut self, index: u8, rgb: Rgb) {
        self.table[usize::from(index)] = rgb;
    }

    #[inline]
    #[must_use]
    pub fn resolve(&self, color: Color, fallback: Rgb) -> Rgb {
        match color.0 >> KIND {
            INDEXED => self.indexed(color.0 as u8),
            DIRECT => Rgb::new((color.0 >> 16) as u8, (color.0 >> 8) as u8, color.0 as u8),
            _ => fallback,
        }
    }
}
