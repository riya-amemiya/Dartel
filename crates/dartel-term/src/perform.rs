use std::io::Write;

use dartel_vt::{Params, Perform};

use super::charset::Charset;
use super::{CursorShape, Modes, Term};

fn raw(params: &Params, index: usize) -> u16 {
    params.iter().nth(index).map_or(0, |group| group[0])
}

fn count(params: &Params, index: usize) -> usize {
    usize::from(raw(params, index)).max(1)
}

impl Perform for Term {
    #[inline]
    fn print(&mut self, c: char) {
        self.write_char(c);
    }

    #[inline]
    fn print_ascii(&mut self, bytes: &[u8]) {
        self.write_ascii(bytes);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => self.outbox.bell = true,
            0x08 => self.backspace(),
            0x09 => self.tab(1),
            0x0A..=0x0C => {
                self.linefeed();
                if self.modes.contains(Modes::NEWLINE) {
                    self.carriage_return();
                }
            }
            0x0D => self.carriage_return(),
            0x0E | 0x0F => {
                self.shift = usize::from(byte == 0x0E);
                self.refresh_fast_print();
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }
        match (intermediates, byte) {
            ([], b'7') => self.save_cursor(),
            ([], b'8') => self.restore_cursor(),
            ([], b'D') => self.linefeed(),
            ([], b'E') => {
                self.linefeed();
                self.carriage_return();
            }
            ([], b'H') => self.set_tab(),
            ([], b'M') => self.reverse_index(),
            ([], b'c') => self.reset(),
            ([], b'=' | b'>') => self.modes.set(Modes::APP_KEYPAD, byte == b'='),
            ([slot @ (b'(' | b')' | b'*' | b'+')], designator) => {
                self.charsets[usize::from(slot - b'(')] = Charset::designate(designator);
                self.refresh_fast_print();
            }
            ([b'#'], b'8') => self.alignment_test(),
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8) {
        if ignore {
            return;
        }
        match (intermediates, action) {
            ([], b'@') => self.insert_chars(count(params, 0)),
            ([], b'A') => self.move_up(count(params, 0)),
            ([], b'B' | b'e') => self.move_down(count(params, 0)),
            ([], b'C' | b'a') => self.move_right(count(params, 0)),
            ([], b'D') => self.move_left(count(params, 0)),
            ([], b'E') => {
                self.move_down(count(params, 0));
                self.carriage_return();
            }
            ([], b'F') => {
                self.move_up(count(params, 0));
                self.carriage_return();
            }
            ([], b'G' | b'`') => self.move_to_column(count(params, 0) - 1),
            ([], b'H' | b'f') => self.move_to(count(params, 0) - 1, count(params, 1) - 1),
            ([], b'I') => self.tab(count(params, 0)),
            ([] | [b'?'], b'J') => self.erase_display(raw(params, 0)),
            ([] | [b'?'], b'K') => self.erase_line(raw(params, 0)),
            ([], b'L') => self.insert_lines(count(params, 0)),
            ([], b'M') => self.delete_lines(count(params, 0)),
            ([], b'P') => self.delete_chars(count(params, 0)),
            ([], b'S') => self.scroll_up(count(params, 0)),
            ([], b'T') => self.scroll_down(count(params, 0)),
            ([], b'X') => self.erase_chars(count(params, 0)),
            ([], b'Z') => self.back_tab(count(params, 0)),
            ([], b'b') => self.repeat(count(params, 0)),
            ([], b'c') => {
                if raw(params, 0) == 0 {
                    self.outbox.reply.extend_from_slice(b"\x1b[?62;22c");
                }
            }
            ([b'>'], b'c') => {
                if raw(params, 0) == 0 {
                    self.outbox.reply.extend_from_slice(b"\x1b[>1;10;0c");
                }
            }
            ([], b'd') => self.move_to_line(count(params, 0) - 1),
            ([], b'g') => self.clear_tabs(raw(params, 0)),
            ([], b'h' | b'l') => {
                for group in params {
                    self.set_ansi_mode(group[0], action == b'h');
                }
            }
            ([b'?'], b'h' | b'l') => {
                for group in params {
                    self.set_private_mode(group[0], action == b'h');
                }
            }
            ([], b'm') => self.select_graphic_rendition(params),
            ([], b'n') => self.report_status(raw(params, 0)),
            ([b'?', b'$'], b'p') => self.report_private_mode(raw(params, 0)),
            ([b'!'], b'p') => self.soft_reset(),
            ([b' '], b'q') => self.set_cursor_style(raw(params, 0)),
            ([], b'r') => {
                self.set_scroll_region(usize::from(raw(params, 0)), usize::from(raw(params, 1)));
            }
            ([], b's') => self.save_cursor(),
            ([], b't') => {
                if raw(params, 0) == 18 {
                    let _ = write!(
                        self.outbox.reply,
                        "\x1b[8;{};{}t",
                        self.lines(),
                        self.cols()
                    );
                }
            }
            ([], b'u') => self.restore_cursor(),
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        self.operating_system_command(params, bell_terminated);
    }
}

impl Term {
    fn set_ansi_mode(&mut self, mode: u16, enable: bool) {
        match mode {
            4 => {
                self.modes.set(Modes::INSERT, enable);
                self.refresh_fast_print();
            }
            20 => self.modes.set(Modes::NEWLINE, enable),
            _ => {}
        }
    }

    fn set_private_mode(&mut self, mode: u16, enable: bool) {
        match mode {
            5 => {
                self.modes.set(Modes::REVERSE_VIDEO, enable);
                self.damage.full = true;
            }
            6 => {
                self.modes.set(Modes::ORIGIN, enable);
                self.move_to(0, 0);
            }
            7 => {
                self.modes.set(Modes::AUTOWRAP, enable);
                self.cursor.pending_wrap = false;
            }
            47 | 1047 => self.set_alternate(enable, false),
            1048 => {
                if enable {
                    self.save_cursor();
                } else {
                    self.restore_cursor();
                }
            }
            1049 => {
                if enable {
                    self.save_cursor();
                    self.set_alternate(true, true);
                } else if self.modes.contains(Modes::ALTERNATE) {
                    self.set_alternate(false, false);
                    self.restore_cursor();
                }
            }
            _ => {
                if let Some(flag) = Modes::private(mode) {
                    self.modes.set(flag, enable);
                }
            }
        }
    }

    fn report_status(&mut self, kind: u16) {
        match kind {
            5 => self.outbox.reply.extend_from_slice(b"\x1b[0n"),
            6 => {
                let (line, column) = self.cursor_position();
                let _ = write!(self.outbox.reply, "\x1b[{};{}R", line + 1, column + 1);
            }
            _ => {}
        }
    }

    fn report_private_mode(&mut self, mode: u16) {
        let _ = write!(
            self.outbox.reply,
            "\x1b[?{mode};{}$y",
            Modes::private(mode).map_or(0, |flag| 2 - u8::from(self.modes.contains(flag)))
        );
    }

    fn set_cursor_style(&mut self, style: u16) {
        self.cursor_shape = match style {
            0..=2 => CursorShape::Block,
            3 | 4 => CursorShape::Underline,
            5 | 6 => CursorShape::Bar,
            _ => return,
        };
        self.modes
            .set(Modes::CURSOR_BLINK, style == 0 || style % 2 == 1);
    }
}
