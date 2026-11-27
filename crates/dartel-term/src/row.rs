use std::ops::Range;

use super::cell::Cell;
use super::style::StyleId;

const MAX_MARKS: usize = 8;

#[derive(Clone, Debug)]
pub struct Row {
    cells: Vec<Cell>,
    marks: Vec<(u16, char)>,
    wrapped: bool,
}

impl Row {
    pub(super) fn new(cols: usize, blank: Cell) -> Row {
        Row {
            cells: vec![blank; cols],
            marks: Vec::new(),
            wrapped: false,
        }
    }

    pub(super) fn assemble(
        cells: &[Cell],
        marks: impl Iterator<Item = (u16, char)>,
        cols: usize,
        blank: Cell,
        wrapped: bool,
    ) -> Row {
        let mut row = Row {
            cells: cells.to_vec(),
            marks: marks.collect(),
            wrapped,
        };
        row.cells.resize(cols, blank);
        row
    }

    pub(super) fn unload(self, keep: usize, cells: &mut Vec<Cell>, marks: &mut Vec<(usize, char)>) {
        marks.extend(
            self.marks
                .iter()
                .filter(|&&(column, _)| usize::from(column) < keep)
                .map(|&(column, c)| (cells.len() + usize::from(column), c)),
        );
        cells.extend(
            self.cells
                .into_iter()
                .chain(std::iter::repeat(Cell::default()))
                .take(keep),
        );
    }

    #[inline]
    #[must_use]
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    #[inline]
    pub(super) fn cells_mut(&mut self) -> &mut [Cell] {
        &mut self.cells
    }

    #[inline]
    #[must_use]
    pub fn is_wrapped(&self) -> bool {
        self.wrapped
    }

    #[inline]
    pub(super) fn set_wrapped(&mut self, wrapped: bool) {
        self.wrapped = wrapped;
    }

    pub fn marks(&self, column: usize) -> impl Iterator<Item = char> + '_ {
        self.marks
            .iter()
            .filter(move |&&(mark, _)| usize::from(mark) == column)
            .map(|&(_, c)| c)
    }

    #[must_use]
    pub fn occupied(&self) -> usize {
        self.cells
            .iter()
            .rposition(|cell| !cell.is_blank())
            .map_or(0, |last| last + 1)
    }

    pub(super) fn reset(&mut self, cols: usize, blank: Cell) {
        self.cells.truncate(cols);
        self.cells.fill(blank);
        self.cells.resize(cols, blank);
        self.marks.clear();
        self.wrapped = false;
    }

    pub(super) fn resize(&mut self, cols: usize, blank: Cell) {
        if cols < self.cells.len() {
            self.release(cols..self.cells.len());
        }
        self.cells.resize(cols, blank);
    }

    #[inline]
    pub(super) fn write_ascii(&mut self, column: usize, bytes: &[u8], style: StyleId) {
        self.release(column..column + bytes.len());
        for (cell, &byte) in self.cells[column..column + bytes.len()]
            .iter_mut()
            .zip(bytes)
        {
            *cell = Cell::ascii(byte, style);
        }
    }

    pub(super) fn write(&mut self, column: usize, c: char, width: usize, style: StyleId) {
        self.release(column..column + width);
        if width == 2 {
            self.cells[column] = Cell::wide(c, style);
            self.cells[column + 1] = Cell::spacer(style);
        } else {
            self.cells[column] = Cell::narrow(c, style);
        }
    }

    pub(super) fn erase(&mut self, range: Range<usize>, blank: Cell) {
        self.release(range.clone());
        self.cells[range].fill(blank);
    }

    pub(super) fn insert(&mut self, column: usize, count: usize, blank: Cell) {
        let cols = self.cells.len();
        let count = count.min(cols - column);
        if self.cells[column].is_spacer() {
            self.release(column..column + 1);
            self.cells[column] = blank;
        }
        self.cells[column..].rotate_right(count);
        self.cells[column..column + count].fill(blank);
        if self.cells[cols - 1].is_wide() {
            self.cells[cols - 1] = Cell::blank(self.cells[cols - 1].style());
        }
        self.marks.retain_mut(|(mark, _)| {
            if usize::from(*mark) >= column {
                *mark += count as u16;
            }
            usize::from(*mark) < cols
        });
    }

    pub(super) fn delete(&mut self, column: usize, count: usize, blank: Cell) {
        let cols = self.cells.len();
        let count = count.min(cols - column);
        self.release(column..column + count);
        self.cells[column..].rotate_left(count);
        self.cells[cols - count..].fill(blank);
        for (mark, _) in &mut self.marks {
            if usize::from(*mark) >= column + count {
                *mark -= count as u16;
            }
        }
    }

    pub(super) fn combine(&mut self, column: usize, c: char) {
        let target = column - usize::from(column > 0 && self.cells[column].is_spacer());
        if self.cells[target].is_blank() || self.marks(target).count() == MAX_MARKS {
            return;
        }
        self.cells[target].mark();
        self.marks.push((target as u16, c));
    }

    #[inline]
    fn release(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        let start =
            range.start - usize::from(range.start > 0 && self.cells[range.start].is_spacer());
        let end = range.end
            + usize::from(
                self.cells
                    .get(range.end)
                    .is_some_and(|cell| cell.is_spacer()),
            );
        if start < range.start {
            self.cells[start] = Cell::blank(self.cells[start].style());
        }
        if end > range.end {
            self.cells[range.end] = Cell::blank(self.cells[range.end].style());
        }
        if !self.marks.is_empty() {
            self.marks
                .retain(|&(mark, _)| !(start..end).contains(&usize::from(mark)));
        }
    }
}
