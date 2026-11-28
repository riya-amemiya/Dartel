use dartel_vt::{Params, ParamsIter};

use super::{Attrs, Color, Rgb, Style, Term};

fn channel(value: u16) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}

fn extended(group: &[u16], rest: &mut ParamsIter<'_>) -> Option<Color> {
    match *group {
        [_] => match rest.next()?[0] {
            5 => Some(Color::Indexed(channel(rest.next()?[0]))),
            2 => Some(Color::Rgb(Rgb::new(
                channel(rest.next()?[0]),
                channel(rest.next()?[0]),
                channel(rest.next()?[0]),
            ))),
            _ => None,
        },
        [_, 5, index, ..] => Some(Color::Indexed(channel(index))),
        [_, 2, red, green, blue] | [_, 2, _, red, green, blue, ..] => Some(Color::Rgb(Rgb::new(
            channel(red),
            channel(green),
            channel(blue),
        ))),
        _ => None,
    }
}

impl Term {
    pub(super) fn select_graphic_rendition(&mut self, params: &Params) {
        let plain = Style {
            link: self.pen.style.link,
            ..Style::default()
        };
        if params.is_empty() {
            self.pen.style = plain;
            return;
        }
        let mut groups = params.iter();
        while let Some(group) = groups.next() {
            match group[0] {
                0 => self.pen.style = plain,
                1 => self.pen.style.attrs.set(Attrs::BOLD, true),
                2 => self.pen.style.attrs.set(Attrs::DIM, true),
                3 => self.pen.style.attrs.set(Attrs::ITALIC, true),
                4 => {
                    self.pen.style.attrs.set(Attrs::ANY_UNDERLINE, false);
                    match group.get(1).copied().unwrap_or(1) {
                        0 => {}
                        2 => self.pen.style.attrs.set(Attrs::DOUBLE_UNDERLINE, true),
                        3 => self.pen.style.attrs.set(Attrs::CURLY_UNDERLINE, true),
                        4 => self.pen.style.attrs.set(Attrs::DOTTED_UNDERLINE, true),
                        5 => self.pen.style.attrs.set(Attrs::DASHED_UNDERLINE, true),
                        _ => self.pen.style.attrs.set(Attrs::UNDERLINE, true),
                    }
                }
                5 | 6 => self.pen.style.attrs.set(Attrs::BLINK, true),
                7 => self.pen.style.attrs.set(Attrs::INVERSE, true),
                8 => self.pen.style.attrs.set(Attrs::HIDDEN, true),
                9 => self.pen.style.attrs.set(Attrs::STRIKETHROUGH, true),
                21 => {
                    self.pen.style.attrs.set(Attrs::ANY_UNDERLINE, false);
                    self.pen.style.attrs.set(Attrs::DOUBLE_UNDERLINE, true);
                }
                22 => {
                    self.pen.style.attrs.set(Attrs::BOLD, false);
                    self.pen.style.attrs.set(Attrs::DIM, false);
                }
                23 => self.pen.style.attrs.set(Attrs::ITALIC, false),
                24 => self.pen.style.attrs.set(Attrs::ANY_UNDERLINE, false),
                25 => self.pen.style.attrs.set(Attrs::BLINK, false),
                27 => self.pen.style.attrs.set(Attrs::INVERSE, false),
                28 => self.pen.style.attrs.set(Attrs::HIDDEN, false),
                29 => self.pen.style.attrs.set(Attrs::STRIKETHROUGH, false),
                code @ 30..=37 => self.pen.style.foreground = Color::Indexed((code - 30) as u8),
                38 => {
                    if let Some(color) = extended(group, &mut groups) {
                        self.pen.style.foreground = color;
                    }
                }
                39 => self.pen.style.foreground = Color::Default,
                code @ 40..=47 => self.pen.style.background = Color::Indexed((code - 40) as u8),
                48 => {
                    if let Some(color) = extended(group, &mut groups) {
                        self.pen.style.background = color;
                    }
                }
                49 => self.pen.style.background = Color::Default,
                58 => {
                    if let Some(color) = extended(group, &mut groups) {
                        self.pen.style.underline = color;
                    }
                }
                59 => self.pen.style.underline = Color::Default,
                code @ 90..=97 => self.pen.style.foreground = Color::Indexed((code - 90 + 8) as u8),
                code @ 100..=107 => {
                    self.pen.style.background = Color::Indexed((code - 100 + 8) as u8);
                }
                _ => {}
            }
        }
    }
}
