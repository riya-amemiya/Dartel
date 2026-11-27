use std::collections::VecDeque;

use super::cell::Cell;
use super::row::Row;

#[derive(Clone, Debug)]
pub struct Grid {
    rows: VecDeque<Row>,
    lines: usize,
    cols: usize,
    history_limit: usize,
}

impl Grid {
    pub(super) fn new(lines: usize, cols: usize, history_limit: usize) -> Grid {
        Grid {
            rows: (0..lines)
                .map(|_| Row::new(cols, Cell::default()))
                .collect(),
            lines,
            cols,
            history_limit,
        }
    }

    #[inline]
    #[must_use]
    pub fn lines(&self) -> usize {
        self.lines
    }

    #[inline]
    #[must_use]
    pub fn cols(&self) -> usize {
        self.cols
    }

    #[inline]
    #[must_use]
    pub fn history(&self) -> usize {
        self.rows.len() - self.lines
    }

    #[inline]
    #[must_use]
    pub fn total(&self) -> usize {
        self.rows.len()
    }

    #[inline]
    #[must_use]
    pub fn row(&self, line: usize) -> &Row {
        &self.rows[self.rows.len() - self.lines + line]
    }

    #[inline]
    #[must_use]
    pub fn absolute(&self, index: usize) -> &Row {
        &self.rows[index]
    }

    pub fn rows(&self) -> impl Iterator<Item = &Row> {
        self.rows.iter()
    }

    #[inline]
    pub(super) fn row_mut(&mut self, line: usize) -> &mut Row {
        let index = self.rows.len() - self.lines + line;
        &mut self.rows[index]
    }

    pub(super) fn rows_mut(&mut self) -> impl Iterator<Item = &mut Row> {
        self.rows.iter_mut()
    }

    pub(super) fn scroll_up(
        &mut self,
        top: usize,
        bottom: usize,
        count: usize,
        blank: Cell,
        keep: bool,
    ) -> usize {
        let count = count.min(bottom + 1 - top);
        if keep && top == 0 && self.history_limit > 0 {
            for _ in 0..count {
                let row = if self.history() == self.history_limit {
                    self.rows.pop_front().map_or_else(
                        || Row::new(self.cols, blank),
                        |mut row| {
                            row.reset(self.cols, blank);
                            row
                        },
                    )
                } else {
                    Row::new(self.cols, blank)
                };
                self.rows
                    .insert(self.rows.len() + 1 + bottom - self.lines, row);
            }
            return count;
        }
        let base = self.history();
        for line in top..bottom + 1 - count {
            self.rows.swap(base + line, base + line + count);
        }
        for line in bottom + 1 - count..=bottom {
            self.rows[base + line].reset(self.cols, blank);
        }
        0
    }

    pub(super) fn scroll_down(&mut self, top: usize, bottom: usize, count: usize, blank: Cell) {
        let count = count.min(bottom + 1 - top);
        let base = self.history();
        for line in (top + count..=bottom).rev() {
            self.rows.swap(base + line, base + line - count);
        }
        for line in top..top + count {
            self.rows[base + line].reset(self.cols, blank);
        }
    }

    pub(super) fn clear(&mut self, blank: Cell) {
        let base = self.history();
        for row in self.rows.range_mut(base..) {
            row.reset(self.cols, blank);
        }
    }

    pub(super) fn clear_history(&mut self) {
        self.rows.drain(..self.rows.len() - self.lines);
    }

    pub(super) fn resize(
        &mut self,
        lines: usize,
        cols: usize,
        cursor: usize,
        blank: Cell,
    ) -> usize {
        let cursor = if lines < self.lines {
            let trimmed = (self.lines - lines).min(self.lines - 1 - cursor);
            self.rows.truncate(self.rows.len() - trimmed);
            cursor + lines + trimmed - self.lines
        } else {
            let restored = (lines - self.lines).min(self.history());
            self.rows
                .extend((0..lines - self.lines - restored).map(|_| Row::new(cols, blank)));
            cursor + restored
        };
        self.lines = lines;
        self.cols = cols;
        self.rows
            .drain(..self.history().saturating_sub(self.history_limit));
        let base = self.history();
        for row in self.rows.range_mut(base..) {
            row.resize(cols, blank);
        }
        cursor
    }
}
