#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Modes(u32);

impl Modes {
    pub const AUTOWRAP: Modes = Modes(1);
    pub const ORIGIN: Modes = Modes(1 << 1);
    pub const INSERT: Modes = Modes(1 << 2);
    pub const NEWLINE: Modes = Modes(1 << 3);
    pub const CURSOR_VISIBLE: Modes = Modes(1 << 4);
    pub const CURSOR_BLINK: Modes = Modes(1 << 5);
    pub const APP_CURSOR: Modes = Modes(1 << 6);
    pub const APP_KEYPAD: Modes = Modes(1 << 7);
    pub const ALTERNATE: Modes = Modes(1 << 8);
    pub const BRACKETED_PASTE: Modes = Modes(1 << 9);
    pub const FOCUS: Modes = Modes(1 << 10);
    pub const MOUSE_PRESS: Modes = Modes(1 << 11);
    pub const MOUSE_DRAG: Modes = Modes(1 << 12);
    pub const MOUSE_MOTION: Modes = Modes(1 << 13);
    pub const MOUSE_UTF8: Modes = Modes(1 << 14);
    pub const MOUSE_SGR: Modes = Modes(1 << 15);
    pub const ALTERNATE_SCROLL: Modes = Modes(1 << 16);
    pub const REVERSE_VIDEO: Modes = Modes(1 << 17);
    pub const SYNC_UPDATE: Modes = Modes(1 << 18);
    pub const ANY_MOUSE: Modes =
        Modes(Modes::MOUSE_PRESS.0 | Modes::MOUSE_DRAG.0 | Modes::MOUSE_MOTION.0);

    #[inline]
    #[must_use]
    pub const fn contains(self, other: Modes) -> bool {
        self.0 & other.0 != 0
    }

    #[inline]
    pub(super) fn set(&mut self, other: Modes, yes: bool) {
        if yes {
            self.0 |= other.0;
        } else {
            self.0 &= !other.0;
        }
    }

    pub(super) fn private(mode: u16) -> Option<Modes> {
        Some(match mode {
            1 => Modes::APP_CURSOR,
            5 => Modes::REVERSE_VIDEO,
            6 => Modes::ORIGIN,
            7 => Modes::AUTOWRAP,
            12 => Modes::CURSOR_BLINK,
            25 => Modes::CURSOR_VISIBLE,
            47 | 1047 | 1049 => Modes::ALTERNATE,
            1000 => Modes::MOUSE_PRESS,
            1002 => Modes::MOUSE_DRAG,
            1003 => Modes::MOUSE_MOTION,
            1004 => Modes::FOCUS,
            1005 => Modes::MOUSE_UTF8,
            1006 => Modes::MOUSE_SGR,
            1007 => Modes::ALTERNATE_SCROLL,
            2004 => Modes::BRACKETED_PASTE,
            2026 => Modes::SYNC_UPDATE,
            _ => return None,
        })
    }
}

impl Default for Modes {
    fn default() -> Modes {
        Modes(Modes::AUTOWRAP.0 | Modes::CURSOR_VISIBLE.0 | Modes::ALTERNATE_SCROLL.0)
    }
}
