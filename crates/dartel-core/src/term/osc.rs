use std::io::Write;

use umt_rust::string::umt_from_base64;

use super::{Rgb, Term};

fn number(bytes: &[u8]) -> Option<u8> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

fn color(bytes: &[u8]) -> Option<Rgb> {
    Rgb::parse(std::str::from_utf8(bytes).ok()?)
}

fn padded(data: &[u8]) -> Option<String> {
    std::str::from_utf8(data)
        .ok()
        .map(|text| format!("{text}{}", "=".repeat((4 - text.len() % 4) % 4)))
}

impl Term {
    pub(super) fn operating_system_command(&mut self, params: &[&[u8]], bell_terminated: bool) {
        let terminator = if bell_terminated { "\x07" } else { "\x1b\\" };
        match params {
            [b"0" | b"2", title @ ..] if !title.is_empty() => {
                self.title = String::from_utf8_lossy(&title.join(&b';')).into_owned();
                self.outbox.title_changed = true;
            }
            [b"4", pairs @ ..] => {
                for &[index, spec] in pairs.as_chunks::<2>().0 {
                    self.palette_command(index, spec, terminator);
                }
            }
            [b"10", spec, ..] => {
                self.palette.foreground =
                    self.dynamic_color(10, self.palette.foreground, spec, terminator);
            }
            [b"11", spec, ..] => {
                self.palette.background =
                    self.dynamic_color(11, self.palette.background, spec, terminator);
            }
            [b"12", spec, ..] => {
                self.palette.cursor = self.dynamic_color(12, self.palette.cursor, spec, terminator);
            }
            [b"52", _, data] if *data != b"?" => {
                if let Some(text) = padded(data).and_then(|encoded| umt_from_base64(&encoded).ok())
                {
                    self.outbox.clipboard = Some(text);
                }
            }
            [b"104"] | [b"104", b""] => {
                for index in 0..=u8::MAX {
                    self.palette
                        .set_indexed(index, self.base_palette.indexed(index));
                }
                self.damage.full = true;
            }
            [b"104", indexes @ ..] => {
                for index in indexes.iter().filter_map(|index| number(index)) {
                    self.palette
                        .set_indexed(index, self.base_palette.indexed(index));
                }
                self.damage.full = true;
            }
            [b"110", ..] => {
                self.palette.foreground = self.base_palette.foreground;
                self.damage.full = true;
            }
            [b"111", ..] => {
                self.palette.background = self.base_palette.background;
                self.damage.full = true;
            }
            [b"112", ..] => {
                self.palette.cursor = self.base_palette.cursor;
                self.damage.full = true;
            }
            _ => {}
        }
    }

    fn palette_command(&mut self, index: &[u8], spec: &[u8], terminator: &str) {
        let Some(index) = number(index) else {
            return;
        };
        if spec == b"?" {
            let _ = write!(
                self.outbox.reply,
                "\x1b]4;{index};{}{terminator}",
                self.palette.indexed(index).to_spec()
            );
        } else if let Some(rgb) = color(spec) {
            self.palette.set_indexed(index, rgb);
            self.damage.full = true;
        }
    }

    fn dynamic_color(&mut self, code: u8, current: Rgb, spec: &[u8], terminator: &str) -> Rgb {
        if spec == b"?" {
            let _ = write!(
                self.outbox.reply,
                "\x1b]{code};{}{terminator}",
                current.to_spec()
            );
            return current;
        }
        self.damage.full = true;
        color(spec).unwrap_or(current)
    }
}
