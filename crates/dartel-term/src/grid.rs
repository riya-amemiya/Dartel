use std::collections::VecDeque;

use super::cell::Cell;
use super::row::{PROMPT, Row};

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

    pub(super) fn tag_prompt(&mut self, line: usize, tags: u8) -> bool {
        let found = (0..=self.history() + line)
            .rev()
            .find(|&index| self.rows[index].tags() & PROMPT != 0);
        if let Some(index) = found {
            self.rows[index].tag(tags);
        }
        found.is_some()
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
        cursor: (usize, usize),
        blank: Cell,
        reflow: bool,
    ) -> (usize, usize) {
        let (line, column) = if reflow && cols != self.cols {
            self.reflow(cols, cursor, blank)
        } else {
            cursor
        };
        let line = if lines < self.lines {
            let trimmed = (self.lines - lines).min(self.lines - 1 - line);
            self.rows.truncate(self.rows.len() - trimmed);
            line + lines + trimmed - self.lines
        } else {
            let restored = (lines - self.lines).min(self.history());
            self.rows
                .extend((0..lines - self.lines - restored).map(|_| Row::new(cols, blank)));
            line + restored
        };
        self.lines = lines;
        self.cols = cols;
        self.rows
            .drain(..self.history().saturating_sub(self.history_limit));
        let base = self.history();
        for row in self.rows.range_mut(base..) {
            row.resize(cols, blank);
        }
        (line, column.min(cols - 1))
    }

    fn reflow(&mut self, cols: usize, cursor: (usize, usize), blank: Cell) -> (usize, usize) {
        let target = self.history() + cursor.0;
        while self.rows.len() > target + 1
            && self
                .rows
                .back()
                .is_some_and(|row| row.occupied() == 0 && !row.is_wrapped())
        {
            self.rows.pop_back();
        }
        let mut rows = VecDeque::with_capacity(self.rows.len());
        let mut cells = Vec::new();
        let mut marks = Vec::new();
        let mut offset = None;
        let mut placed = (0, 0);
        let mut tags = 0;
        for (index, row) in std::mem::take(&mut self.rows).into_iter().enumerate() {
            let wrapped = row.is_wrapped();
            tags |= row.tags();
            let keep = if index == target {
                offset = Some(cells.len() + cursor.1);
                row.occupied().max(cursor.1)
            } else {
                row.occupied()
            };
            row.unload(keep, &mut cells, &mut marks);
            if !wrapped {
                placed = split(&mut rows, &cells, &marks, cols, blank, offset.take(), tags)
                    .unwrap_or(placed);
                cells.clear();
                marks.clear();
                tags = 0;
            }
        }
        if !cells.is_empty() || offset.is_some() {
            placed = split(&mut rows, &cells, &marks, cols, blank, offset, tags).unwrap_or(placed);
        }
        rows.extend((rows.len()..self.lines).map(|_| Row::new(cols, blank)));
        let dropped = (rows.len() - self.lines).saturating_sub(self.history_limit);
        rows.drain(..dropped);
        self.rows = rows;
        self.cols = cols;
        (
            placed
                .0
                .saturating_sub(dropped)
                .saturating_sub(self.history())
                .min(self.lines - 1),
            placed.1,
        )
    }
}

fn split(
    rows: &mut VecDeque<Row>,
    cells: &[Cell],
    marks: &[(usize, char)],
    cols: usize,
    blank: Cell,
    cursor: Option<usize>,
    tags: u8,
) -> Option<(usize, usize)> {
    let mut start = 0;
    let mut placed = None;
    loop {
        let limit = (start + cols).min(cells.len());
        let end = limit - usize::from(cells.get(limit).is_some_and(|cell| cell.is_spacer()));
        if placed.is_none()
            && let Some(offset) = cursor.filter(|&offset| offset < end || end == cells.len())
        {
            placed = Some((rows.len(), (offset - start).min(cols - 1)));
        }
        rows.push_back(Row::assemble(
            &cells[start..end],
            marks
                .iter()
                .filter(|&&(at, _)| (start..end).contains(&at))
                .map(|&(at, c)| ((at - start) as u16, c)),
            cols,
            blank,
            end < cells.len(),
            if start == 0 { tags } else { 0 },
        ));
        if end == cells.len() {
            return placed;
        }
        start = end;
    }
}
