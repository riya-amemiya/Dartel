use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use super::color::Color;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct StyleId(u16);

impl StyleId {
    pub const DEFAULT: StyleId = StyleId(0);

    #[inline]
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    #[inline]
    pub(super) const fn raw(self) -> u16 {
        self.0
    }

    #[inline]
    pub(super) const fn from_raw(raw: u16) -> StyleId {
        StyleId(raw)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Attrs(u16);

impl Attrs {
    pub const BOLD: Attrs = Attrs(1);
    pub const DIM: Attrs = Attrs(1 << 1);
    pub const ITALIC: Attrs = Attrs(1 << 2);
    pub const UNDERLINE: Attrs = Attrs(1 << 3);
    pub const DOUBLE_UNDERLINE: Attrs = Attrs(1 << 4);
    pub const CURLY_UNDERLINE: Attrs = Attrs(1 << 5);
    pub const DOTTED_UNDERLINE: Attrs = Attrs(1 << 6);
    pub const DASHED_UNDERLINE: Attrs = Attrs(1 << 7);
    pub const BLINK: Attrs = Attrs(1 << 8);
    pub const INVERSE: Attrs = Attrs(1 << 9);
    pub const HIDDEN: Attrs = Attrs(1 << 10);
    pub const STRIKETHROUGH: Attrs = Attrs(1 << 11);
    pub const ANY_UNDERLINE: Attrs = Attrs(0b1111_1000);

    #[inline]
    #[must_use]
    pub const fn contains(self, other: Attrs) -> bool {
        self.0 & other.0 != 0
    }

    #[inline]
    pub fn set(&mut self, other: Attrs, yes: bool) {
        if yes {
            self.0 |= other.0;
        } else {
            self.0 &= !other.0;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub foreground: Color,
    pub background: Color,
    pub underline: Color,
    pub attrs: Attrs,
    pub link: u16,
}

impl Style {
    #[inline]
    fn key(self) -> u128 {
        u128::from(self.foreground.pack())
            | u128::from(self.background.pack()) << 32
            | u128::from(self.underline.pack()) << 64
            | u128::from(self.attrs.0) << 96
            | u128::from(self.link) << 112
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Mixer(u64);

impl Hasher for Mixer {
    #[inline]
    fn finish(&self) -> u64 {
        self.0 ^ (self.0 >> 32)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_u64(u64::from(byte));
        }
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(26) ^ value).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }

    #[inline]
    fn write_u128(&mut self, value: u128) {
        self.write_u64(value as u64);
        self.write_u64((value >> 64) as u64);
    }
}

const RECENT: usize = 16;

#[derive(Clone, Debug)]
pub(super) struct StyleTable {
    styles: Vec<Style>,
    ids: HashMap<u128, StyleId, BuildHasherDefault<Mixer>>,
    recent: [(u128, StyleId); RECENT],
}

impl StyleTable {
    pub(super) fn new() -> StyleTable {
        StyleTable {
            styles: vec![Style::default()],
            ids: HashMap::from_iter([(Style::default().key(), StyleId::DEFAULT)]),
            recent: [(Style::default().key(), StyleId::DEFAULT); RECENT],
        }
    }

    #[inline]
    pub(super) fn get(&self, id: StyleId) -> Style {
        self.styles[id.index()]
    }

    pub(super) fn len(&self) -> usize {
        self.styles.len()
    }

    pub(super) fn intern(&mut self, style: Style) -> Option<StyleId> {
        let key = style.key();
        let slot = (key ^ key >> 29 ^ key >> 61 ^ key >> 93) as usize % RECENT;
        if self.recent[slot].0 == key {
            return Some(self.recent[slot].1);
        }
        let id = if let Some(&id) = self.ids.get(&key) {
            id
        } else {
            let id = StyleId(u16::try_from(self.styles.len()).ok()?);
            self.styles.push(style);
            self.ids.insert(key, id);
            id
        };
        self.recent[slot] = (key, id);
        Some(id)
    }

    pub(super) fn compacted(&self, used: &[bool]) -> (StyleTable, Vec<StyleId>) {
        let mut table = StyleTable::new();
        let remap = self
            .styles
            .iter()
            .zip(used)
            .map(|(&style, &keep)| {
                if keep {
                    table.intern(style).unwrap_or(StyleId::DEFAULT)
                } else {
                    StyleId::DEFAULT
                }
            })
            .collect();
        (table, remap)
    }
}
