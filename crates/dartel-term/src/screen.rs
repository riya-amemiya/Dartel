use unicode_width::UnicodeWidthChar;

use super::charset::Charset;
use super::graphics::Graphics;
use super::{Cell, Cursor, Modes, Outbox, Saved, Style, StyleId, Term};

impl Term {
    #[inline]
    pub(super) fn pen_id(&mut self) -> StyleId {
        if self.pen.interned != self.pen.style {
            self.pen.id = self.intern(self.pen.style);
            self.pen.interned = self.pen.style;
        }
        self.pen.id
    }

    #[inline]
    pub(super) fn blank(&mut self) -> Cell {
        if self.pen.blank_background != self.pen.style.background {
            self.pen.blank = self.intern(Style {
                background: self.pen.style.background,
                ..Style::default()
            });
            self.pen.blank_background = self.pen.style.background;
        }
        Cell::blank(self.pen.blank)
    }

    fn intern(&mut self, style: Style) -> StyleId {
        if let Some(id) = self.styles.intern(style) {
            return id;
        }
        self.compact_styles();
        self.styles.intern(style).unwrap_or(StyleId::DEFAULT)
    }

    fn compact_styles(&mut self) {
        let mut used = vec![false; self.styles.len()];
        used[self.pen.id.index()] = true;
        used[self.pen.blank.index()] = true;
        for row in self.grid.rows().chain(self.inactive.rows()) {
            for cell in row.cells() {
                used[cell.style().index()] = true;
            }
        }
        let (styles, remap) = self.styles.compacted(&used);
        for row in self.grid.rows_mut().chain(self.inactive.rows_mut()) {
            for cell in row.cells_mut() {
                cell.set_style(remap[cell.style().index()]);
            }
        }
        self.pen.id = remap[self.pen.id.index()];
        self.pen.blank = remap[self.pen.blank.index()];
        self.styles = styles;
    }

    #[inline]
    fn touch(&mut self, line: usize) {
        if let Some(dirty) = self.damage.lines.get_mut(line + self.display_offset) {
            *dirty = true;
        }
    }

    pub(super) fn refresh_fast_print(&mut self) {
        self.fast_print =
            !self.modes.contains(Modes::INSERT) && self.charsets[self.shift] == Charset::Ascii;
    }

    #[inline]
    pub(super) fn write_ascii(&mut self, bytes: &[u8]) {
        if !self.fast_print {
            for &byte in bytes {
                self.write_char(char::from(byte));
            }
            return;
        }
        let style = self.pen_id();
        let cols = self.cols();
        let mut rest = bytes;
        while !rest.is_empty() {
            if self.cursor.pending_wrap {
                self.wrap();
            }
            let Cursor { line, column, .. } = self.cursor;
            let count = rest.len().min(cols - column);
            self.grid
                .row_mut(line)
                .write_ascii(column, &rest[..count], style);
            self.touch(line);
            rest = &rest[count..];
            if column + count < cols {
                self.cursor.column = column + count;
            } else if self.modes.contains(Modes::AUTOWRAP) {
                self.cursor.column = cols - 1;
                self.cursor.pending_wrap = true;
            } else {
                self.cursor.column = cols - 1;
                if let Some(last) = rest.last() {
                    self.grid.row_mut(line).write_ascii(
                        cols - 1,
                        std::slice::from_ref(last),
                        style,
                    );
                }
                break;
            }
        }
        self.last_char = bytes
            .last()
            .map(|&byte| char::from(byte))
            .or(self.last_char);
    }

    pub(super) fn write_char(&mut self, c: char) {
        let c = self.charsets[self.shift].translate(c);
        let Some(width) = c.width() else {
            return;
        };
        if width == 0 {
            self.combine(c);
            return;
        }
        if self.cursor.pending_wrap {
            self.wrap();
        }
        let cols = self.cols();
        if self.cursor.column + width > cols {
            if !self.modes.contains(Modes::AUTOWRAP) {
                return;
            }
            let blank = self.blank();
            let Cursor { line, column, .. } = self.cursor;
            self.grid.row_mut(line).erase(column..cols, blank);
            self.touch(line);
            self.wrap();
        }
        let style = self.pen_id();
        let Cursor { line, column, .. } = self.cursor;
        if self.modes.contains(Modes::INSERT) {
            let blank = self.blank();
            self.grid.row_mut(line).insert(column, width, blank);
        }
        self.grid.row_mut(line).write(column, c, width, style);
        self.touch(line);
        self.last_char = Some(c);
        if column + width < cols {
            self.cursor.column = column + width;
        } else {
            self.cursor.column = cols - 1;
            self.cursor.pending_wrap = self.modes.contains(Modes::AUTOWRAP);
        }
    }

    fn combine(&mut self, c: char) {
        let Cursor {
            line,
            column,
            pending_wrap,
        } = self.cursor;
        if pending_wrap || column > 0 {
            self.grid
                .row_mut(line)
                .combine(column - usize::from(!pending_wrap), c);
            self.touch(line);
        }
    }

    fn wrap(&mut self) {
        self.grid.row_mut(self.cursor.line).set_wrapped(true);
        self.cursor.column = 0;
        self.linefeed();
    }

    pub(super) fn repeat(&mut self, count: usize) {
        if let Some(c) = self.last_char {
            for _ in 0..count {
                self.write_char(c);
            }
        }
    }

    pub(super) fn linefeed(&mut self) {
        self.cursor.pending_wrap = false;
        if self.cursor.line == self.bottom {
            self.scroll_up(1);
        } else if self.cursor.line + 1 < self.lines() {
            self.cursor.line += 1;
        }
    }

    pub(super) fn reverse_index(&mut self) {
        self.cursor.pending_wrap = false;
        if self.cursor.line == self.top {
            self.scroll_down(1);
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
        }
    }

    pub(super) fn carriage_return(&mut self) {
        self.cursor.column = 0;
        self.cursor.pending_wrap = false;
    }

    pub(super) fn backspace(&mut self) {
        self.cursor.column = self.cursor.column.saturating_sub(1);
        self.cursor.pending_wrap = false;
    }

    pub(super) fn tab(&mut self, count: usize) {
        for _ in 0..count {
            self.cursor.column = (self.cursor.column + 1..self.cols())
                .find(|&column| self.tabs[column])
                .unwrap_or(self.cols() - 1);
            if self.cursor.column + 1 == self.cols() {
                break;
            }
        }
    }

    pub(super) fn back_tab(&mut self, count: usize) {
        self.cursor.pending_wrap = false;
        for _ in 0..count {
            self.cursor.column = (0..self.cursor.column)
                .rev()
                .find(|&column| self.tabs[column])
                .unwrap_or(0);
            if self.cursor.column == 0 {
                break;
            }
        }
    }

    pub(super) fn set_tab(&mut self) {
        self.tabs[self.cursor.column] = true;
    }

    pub(super) fn clear_tabs(&mut self, mode: u16) {
        match mode {
            0 => self.tabs[self.cursor.column] = false,
            3 => self.tabs.fill(false),
            _ => {}
        }
    }

    pub(super) fn move_to(&mut self, line: usize, column: usize) {
        let (first, last) = if self.modes.contains(Modes::ORIGIN) {
            (self.top, self.bottom)
        } else {
            (0, self.lines() - 1)
        };
        self.cursor.line = first.saturating_add(line).min(last);
        self.move_to_column(column);
    }

    pub(super) fn move_to_line(&mut self, line: usize) {
        self.move_to(line, self.cursor.column);
    }

    pub(super) fn move_to_column(&mut self, column: usize) {
        self.cursor.column = column.min(self.cols() - 1);
        self.cursor.pending_wrap = false;
    }

    pub(super) fn move_up(&mut self, count: usize) {
        self.cursor.line =
            self.cursor
                .line
                .saturating_sub(count)
                .max(if self.cursor.line >= self.top {
                    self.top
                } else {
                    0
                });
        self.cursor.pending_wrap = false;
    }

    pub(super) fn move_down(&mut self, count: usize) {
        self.cursor.line =
            self.cursor
                .line
                .saturating_add(count)
                .min(if self.cursor.line <= self.bottom {
                    self.bottom
                } else {
                    self.lines() - 1
                });
        self.cursor.pending_wrap = false;
    }

    pub(super) fn move_left(&mut self, count: usize) {
        self.move_to_column(self.cursor.column.saturating_sub(count));
    }

    pub(super) fn move_right(&mut self, count: usize) {
        self.move_to_column(self.cursor.column.saturating_add(count));
    }

    pub(super) fn cursor_position(&self) -> (usize, usize) {
        (
            self.cursor.line
                - if self.modes.contains(Modes::ORIGIN) {
                    self.top.min(self.cursor.line)
                } else {
                    0
                },
            self.cursor.column,
        )
    }

    pub(super) fn scroll_up(&mut self, count: usize) {
        let blank = self.blank();
        let kept = self
            .grid
            .scroll_up(self.top, self.bottom, count, blank, true);
        if self.display_offset > 0 {
            self.display_offset = (self.display_offset + kept).min(self.grid.history());
        }
        self.damage.full = true;
    }

    pub(super) fn scroll_down(&mut self, count: usize) {
        let blank = self.blank();
        self.grid.scroll_down(self.top, self.bottom, count, blank);
        self.damage.full = true;
    }

    pub(super) fn insert_lines(&mut self, count: usize) {
        if (self.top..=self.bottom).contains(&self.cursor.line) {
            let blank = self.blank();
            self.grid
                .scroll_down(self.cursor.line, self.bottom, count, blank);
            self.carriage_return();
            self.damage.full = true;
        }
    }

    pub(super) fn delete_lines(&mut self, count: usize) {
        if (self.top..=self.bottom).contains(&self.cursor.line) {
            let blank = self.blank();
            self.grid
                .scroll_up(self.cursor.line, self.bottom, count, blank, false);
            self.carriage_return();
            self.damage.full = true;
        }
    }

    pub(super) fn insert_chars(&mut self, count: usize) {
        let blank = self.blank();
        let Cursor { line, column, .. } = self.cursor;
        self.grid.row_mut(line).insert(column, count, blank);
        self.cursor.pending_wrap = false;
        self.touch(line);
    }

    pub(super) fn delete_chars(&mut self, count: usize) {
        let blank = self.blank();
        let Cursor { line, column, .. } = self.cursor;
        self.grid.row_mut(line).delete(column, count, blank);
        self.cursor.pending_wrap = false;
        self.touch(line);
    }

    pub(super) fn erase_chars(&mut self, count: usize) {
        let blank = self.blank();
        let Cursor { line, column, .. } = self.cursor;
        let end = column.saturating_add(count).min(self.cols());
        self.grid.row_mut(line).erase(column..end, blank);
        self.cursor.pending_wrap = false;
        self.touch(line);
    }

    pub(super) fn erase_line(&mut self, mode: u16) {
        let blank = self.blank();
        let Cursor { line, column, .. } = self.cursor;
        let cols = self.cols();
        let range = match mode {
            0 => column..cols,
            1 => 0..column + 1,
            2 => 0..cols,
            _ => return,
        };
        if range.end == cols {
            self.grid.row_mut(line).set_wrapped(false);
        }
        self.grid.row_mut(line).erase(range, blank);
        self.cursor.pending_wrap = false;
        self.touch(line);
    }

    pub(super) fn erase_display(&mut self, mode: u16) {
        let blank = self.blank();
        let Cursor { line, column, .. } = self.cursor;
        let (lines, cols) = (self.lines(), self.cols());
        match mode {
            0 => {
                self.grid.row_mut(line).set_wrapped(false);
                self.grid.row_mut(line).erase(column..cols, blank);
                for below in line + 1..lines {
                    self.grid.row_mut(below).clear(cols, blank);
                }
            }
            1 => {
                for above in 0..line {
                    self.grid.row_mut(above).clear(cols, blank);
                }
                self.grid.row_mut(line).erase(0..column + 1, blank);
            }
            2 => self.grid.clear(blank),
            3 => {
                self.grid.clear_history();
                self.display_offset = 0;
            }
            _ => return,
        }
        self.cursor.pending_wrap = false;
        self.damage.full = true;
    }

    pub(super) fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        let top = top.max(1) - 1;
        let bottom = if bottom == 0 {
            self.lines()
        } else {
            bottom.min(self.lines())
        } - 1;
        if top < bottom {
            self.top = top;
            self.bottom = bottom;
            self.move_to(0, 0);
        }
    }

    pub(super) fn save_cursor(&mut self) {
        self.saved = Saved {
            cursor: self.cursor,
            pen: self.pen.style,
            charsets: self.charsets,
            shift: self.shift,
            origin: self.modes.contains(Modes::ORIGIN),
        };
    }

    pub(super) fn restore_cursor(&mut self) {
        self.cursor = Cursor {
            line: self.saved.cursor.line.min(self.lines() - 1),
            column: self.saved.cursor.column.min(self.cols() - 1),
            pending_wrap: self.saved.cursor.pending_wrap,
        };
        self.pen.style = self.saved.pen;
        self.charsets = self.saved.charsets;
        self.shift = self.saved.shift;
        self.modes.set(Modes::ORIGIN, self.saved.origin);
        self.refresh_fast_print();
    }

    pub(super) fn set_alternate(&mut self, enable: bool, clear: bool) {
        if enable == self.modes.contains(Modes::ALTERNATE) {
            return;
        }
        std::mem::swap(&mut self.grid, &mut self.inactive);
        std::mem::swap(&mut self.saved, &mut self.inactive_saved);
        std::mem::swap(&mut self.keyboard, &mut self.inactive_keyboard);
        self.modes.set(Modes::ALTERNATE, enable);
        if enable && clear {
            let blank = self.blank();
            self.grid.clear(blank);
        }
        self.display_offset = 0;
        self.damage.full = true;
    }

    pub(super) fn alignment_test(&mut self) {
        let (lines, cols) = (self.lines(), self.cols());
        for line in 0..lines {
            self.grid
                .row_mut(line)
                .reset(cols, Cell::ascii(b'E', StyleId::DEFAULT));
        }
        self.top = 0;
        self.bottom = lines - 1;
        self.cursor = Cursor::default();
        self.damage.full = true;
    }

    pub(super) fn soft_reset(&mut self) {
        self.modes.set(Modes::CURSOR_VISIBLE, true);
        self.modes.set(Modes::AUTOWRAP, true);
        self.modes.set(Modes::ORIGIN, false);
        self.modes.set(Modes::INSERT, false);
        self.modes.set(Modes::APP_CURSOR, false);
        self.modes.set(Modes::APP_KEYPAD, false);
        self.top = 0;
        self.bottom = self.lines() - 1;
        self.pen.style = Style::default();
        self.charsets = [Charset::default(); 4];
        self.shift = 0;
        self.saved = Saved::default();
        self.cursor.pending_wrap = false;
        self.refresh_fast_print();
    }

    pub(super) fn reset(&mut self) {
        *self = Term {
            outbox: Outbox {
                title_changed: !self.title.is_empty(),
                ..std::mem::take(&mut self.outbox)
            },
            graphics: Graphics::new(self.graphics.cell()),
            ..Term::with_palette(
                self.lines(),
                self.cols(),
                self.history_limit,
                self.base_palette.clone(),
            )
        };
    }
}
