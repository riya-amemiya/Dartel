use super::style::StyleId;

const WIDE: u16 = 1;
const SPACER: u16 = 1 << 1;
const MARKED: u16 = 1 << 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Cell {
    code: u32,
    style: StyleId,
    flags: u16,
}

impl Cell {
    #[inline]
    pub(crate) const fn blank(style: StyleId) -> Cell {
        Cell {
            code: 0,
            style,
            flags: 0,
        }
    }

    #[inline]
    pub(crate) const fn ascii(byte: u8, style: StyleId) -> Cell {
        Cell {
            code: byte as u32,
            style,
            flags: 0,
        }
    }

    #[inline]
    pub(crate) const fn narrow(c: char, style: StyleId) -> Cell {
        Cell {
            code: c as u32,
            style,
            flags: 0,
        }
    }

    #[inline]
    pub(crate) const fn wide(c: char, style: StyleId) -> Cell {
        Cell {
            code: c as u32,
            style,
            flags: WIDE,
        }
    }

    #[inline]
    pub(crate) const fn spacer(style: StyleId) -> Cell {
        Cell {
            code: 0,
            style,
            flags: SPACER,
        }
    }

    #[inline]
    #[must_use]
    pub fn ch(self) -> Option<char> {
        char::from_u32(self.code).filter(|_| self.code != 0)
    }

    #[inline]
    #[must_use]
    pub const fn is_blank(self) -> bool {
        self.code == 0 && self.flags & SPACER == 0
    }

    #[inline]
    #[must_use]
    pub const fn is_wide(self) -> bool {
        self.flags & WIDE != 0
    }

    #[inline]
    #[must_use]
    pub const fn is_spacer(self) -> bool {
        self.flags & SPACER != 0
    }

    #[inline]
    #[must_use]
    pub const fn is_marked(self) -> bool {
        self.flags & MARKED != 0
    }

    #[inline]
    #[must_use]
    pub const fn style(self) -> StyleId {
        self.style
    }

    #[inline]
    pub(crate) fn set_style(&mut self, style: StyleId) {
        self.style = style;
    }

    #[inline]
    pub(crate) fn mark(&mut self) {
        self.flags |= MARKED;
    }
}
