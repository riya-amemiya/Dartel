#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Charset {
    #[default]
    Ascii,
    DecSpecial,
}

const SPECIAL: [char; 32] = [
    ' ', '\u{25C6}', '\u{2592}', '\u{2409}', '\u{240C}', '\u{240D}', '\u{240A}', '\u{00B0}',
    '\u{00B1}', '\u{2424}', '\u{240B}', '\u{2518}', '\u{2510}', '\u{250C}', '\u{2514}', '\u{253C}',
    '\u{23BA}', '\u{23BB}', '\u{2500}', '\u{23BC}', '\u{23BD}', '\u{251C}', '\u{2524}', '\u{2534}',
    '\u{252C}', '\u{2502}', '\u{2264}', '\u{2265}', '\u{03C0}', '\u{2260}', '\u{00A3}', '\u{00B7}',
];

impl Charset {
    pub(super) fn designate(designator: u8) -> Charset {
        if designator == b'0' {
            Charset::DecSpecial
        } else {
            Charset::Ascii
        }
    }

    #[inline]
    pub(super) fn translate(self, c: char) -> char {
        match (self, c) {
            (Charset::DecSpecial, '_'..='~') => SPECIAL[c as usize - 0x5F],
            _ => c,
        }
    }
}
