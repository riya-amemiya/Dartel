use super::cell::Cell;
use super::row::{Logical, PROMPT, Row};

#[derive(Clone, Debug)]
pub struct Grid {
    rows: Vec<Row>,
    head: usize,
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
            head: 0,
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
    fn slot(&self, index: usize) -> usize {
        assert!(index < self.rows.len());
        let slot = self.head + index;
        if slot < self.rows.len() {
            slot
        } else {
            slot - self.rows.len()
        }
    }

    #[inline]
    #[must_use]
    pub fn row(&self, line: usize) -> &Row {
        &self.rows[self.slot(self.history() + line)]
    }

    #[inline]
    #[must_use]
    pub fn absolute(&self, index: usize) -> &Row {
        &self.rows[self.slot(index)]
    }

    pub fn rows(&self) -> impl Iterator<Item = &Row> {
        self.rows[self.head..].iter().chain(&self.rows[..self.head])
    }

    #[inline]
    pub(super) fn row_mut(&mut self, line: usize) -> &mut Row {
        let slot = self.slot(self.history() + line);
        &mut self.rows[slot]
    }

    pub(super) fn rows_mut(&mut self) -> impl Iterator<Item = &mut Row> {
        let (wrapped, leading) = self.rows.split_at_mut(self.head);
        leading.iter_mut().chain(wrapped)
    }

    fn settle(&mut self) {
        self.rows.rotate_left(self.head);
        self.head = 0;
    }

    fn swap(&mut self, first: usize, second: usize) {
        let (first, second) = (self.slot(first), self.slot(second));
        self.rows.swap(first, second);
    }

    #[inline]
    fn recycle(&mut self, blank: Cell) {
        self.rows[self.head].reset(self.cols, blank);
        self.head = if self.head + 1 == self.rows.len() {
            0
        } else {
            self.head + 1
        };
    }

    fn feed(&mut self, blank: Cell) {
        if self.history() >= self.history_limit {
            self.recycle(blank);
        } else {
            self.settle();
            self.rows.push(Row::new(self.cols, blank));
        }
    }

    fn roll(&mut self, top: usize, bottom: usize, count: usize, down: bool) {
        let base = self.history();
        let (first, last) = (self.slot(base + top), self.slot(base + bottom));
        if first <= last {
            if down {
                self.rows[first..=last].rotate_right(count);
            } else {
                self.rows[first..=last].rotate_left(count);
            }
        } else if down {
            for line in (top + count..=bottom).rev() {
                self.swap(base + line, base + line - count);
            }
        } else {
            for line in top..bottom + 1 - count {
                self.swap(base + line, base + line + count);
            }
        }
    }

    #[inline]
    pub(super) fn scroll_up(
        &mut self,
        top: usize,
        bottom: usize,
        count: usize,
        blank: Cell,
        keep: bool,
    ) -> usize {
        if count == 1
            && top == 0
            && bottom + 1 == self.lines
            && self.history() == self.history_limit
            && (keep || self.history_limit == 0)
        {
            self.recycle(blank);
            return usize::from(self.history_limit > 0);
        }
        self.shift_up(top, bottom, count, blank, keep)
    }

    fn shift_up(
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
                self.feed(blank);
                for index in (self.history() + bottom..self.rows.len() - 1).rev() {
                    self.swap(index, index + 1);
                }
            }
            return count;
        }
        if top == 0 && bottom + 1 == self.lines && self.history() == 0 {
            for _ in 0..count {
                self.recycle(blank);
            }
            return 0;
        }
        self.roll(top, bottom, count, false);
        for line in bottom + 1 - count..=bottom {
            self.wipe(line, blank);
        }
        0
    }

    fn wipe(&mut self, line: usize, blank: Cell) {
        let slot = self.slot(self.history() + line);
        self.rows[slot].reset(self.cols, blank);
    }

    pub(super) fn scroll_down(&mut self, top: usize, bottom: usize, count: usize, blank: Cell) {
        let count = count.min(bottom + 1 - top);
        self.roll(top, bottom, count, true);
        for line in top..top + count {
            self.wipe(line, blank);
        }
    }

    pub(super) fn tag_prompt(&mut self, line: usize, tags: u8) -> bool {
        let found = (0..=self.history() + line)
            .rev()
            .find(|&index| self.absolute(index).tags() & PROMPT != 0);
        if let Some(index) = found {
            let slot = self.slot(index);
            self.rows[slot].tag(tags);
        }
        found.is_some()
    }

    pub(super) fn clear(&mut self, blank: Cell) {
        for line in 0..self.lines {
            self.wipe(line, blank);
        }
    }

    pub(super) fn clear_history(&mut self) {
        self.settle();
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
        self.settle();
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
        for row in &mut self.rows[base..] {
            row.resize(cols, blank);
        }
        (line, column.min(cols - 1))
    }

    fn reflow(&mut self, cols: usize, cursor: (usize, usize), blank: Cell) -> (usize, usize) {
        let target = self.history() + cursor.0;
        while self.rows.len() > target + 1
            && self.rows.last().is_some_and(|row| {
                row.occupied() == 0 && !row.is_wrapped() && row.tiles().is_empty()
            })
        {
            self.rows.pop();
        }
        let mut rows = Vec::with_capacity(self.rows.len());
        let mut logical = Logical::default();
        let mut offset = None;
        let mut placed = (0, 0);
        for (index, row) in std::mem::take(&mut self.rows).into_iter().enumerate() {
            let wrapped = row.is_wrapped();
            let keep = if index == target {
                offset = Some(logical.cells.len() + cursor.1);
                row.occupied().max(cursor.1)
            } else {
                row.occupied()
            };
            row.unload(keep, &mut logical);
            if !wrapped {
                placed =
                    split(&mut rows, &mut logical, cols, blank, offset.take()).unwrap_or(placed);
                logical.clear();
            }
        }
        if !logical.cells.is_empty() || !logical.tiles.is_empty() || offset.is_some() {
            placed = split(&mut rows, &mut logical, cols, blank, offset).unwrap_or(placed);
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
    rows: &mut Vec<Row>,
    logical: &mut Logical,
    cols: usize,
    blank: Cell,
    cursor: Option<usize>,
) -> Option<(usize, usize)> {
    let Logical {
        cells,
        marks,
        tiles,
        tags,
    } = logical;
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
        rows.push(Row::assemble(
            &cells[start..end],
            marks
                .iter()
                .filter(|&&(at, _)| (start..end).contains(&at))
                .map(|&(at, c)| ((at - start) as u16, c)),
            tiles
                .extract_if(.., |(at, _)| *at < end || end == cells.len())
                .map(|(_, tile)| tile)
                .collect(),
            cols,
            blank,
            end < cells.len(),
            if start == 0 { *tags } else { 0 },
        ));
        if end == cells.len() {
            return placed;
        }
        start = end;
    }
}
