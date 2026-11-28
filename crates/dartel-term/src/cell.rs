use super::style::StyleId;

const CODE: u64 = 0xFFFF_FFFF;
const STYLE: u32 = 32;
const WIDE: u64 = 1 << 48;
const SPACER: u64 = 1 << 49;
const MARKED: u64 = 1 << 50;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct Cell(u64);

impl Cell {
    #[inline]
    pub(crate) const fn blank(style: StyleId) -> Cell {
        Cell((style.raw() as u64) << STYLE)
    }

    #[inline]
    pub(crate) const fn ascii(byte: u8, style: StyleId) -> Cell {
        Cell(byte as u64 | (style.raw() as u64) << STYLE)
    }

    #[inline]
    pub(crate) const fn narrow(c: char, style: StyleId) -> Cell {
        Cell(c as u64 | (style.raw() as u64) << STYLE)
    }

    #[inline]
    pub(crate) const fn wide(c: char, style: StyleId) -> Cell {
        Cell(c as u64 | (style.raw() as u64) << STYLE | WIDE)
    }

    #[inline]
    pub(crate) const fn spacer(style: StyleId) -> Cell {
        Cell((style.raw() as u64) << STYLE | SPACER)
    }

    #[inline]
    #[must_use]
    pub fn ch(self) -> Option<char> {
        char::from_u32(self.0 as u32).filter(|&c| c != '\0')
    }

    #[inline]
    #[must_use]
    pub const fn is_blank(self) -> bool {
        self.0 & (CODE | SPACER) == 0
    }

    #[inline]
    #[must_use]
    pub const fn is_wide(self) -> bool {
        self.0 & WIDE != 0
    }

    #[inline]
    #[must_use]
    pub const fn is_spacer(self) -> bool {
        self.0 & SPACER != 0
    }

    #[inline]
    #[must_use]
    pub const fn is_marked(self) -> bool {
        self.0 & MARKED != 0
    }

    #[inline]
    #[must_use]
    pub const fn style(self) -> StyleId {
        StyleId::from_raw((self.0 >> STYLE) as u16)
    }

    #[inline]
    pub(crate) fn set_style(&mut self, style: StyleId) {
        self.0 = self.0 & !(0xFFFF << STYLE) | u64::from(style.raw()) << STYLE;
    }

    #[inline]
    pub(crate) fn mark(&mut self) {
        self.0 |= MARKED;
    }
}
