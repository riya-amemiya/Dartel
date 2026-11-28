use std::ops::Range;

use super::cell::Cell;
use super::graphics::Tile;
use super::style::StyleId;

const MAX_MARKS: usize = 8;

pub(super) const PROMPT: u8 = 1;
pub(super) const OUTPUT: u8 = 1 << 1;
pub(super) const FAILED: u8 = 1 << 2;

#[derive(Clone, Debug)]
pub struct Row {
    cells: Vec<Cell>,
    marks: Vec<(u16, char)>,
    tiles: Vec<Tile>,
    used: u16,
    wide: bool,
    wrapped: bool,
    tags: u8,
}

#[derive(Default)]
pub(super) struct Logical {
    pub(super) cells: Vec<Cell>,
    pub(super) marks: Vec<(usize, char)>,
    pub(super) tiles: Vec<(usize, Tile)>,
    pub(super) tags: u8,
}

impl Logical {
    pub(super) fn clear(&mut self) {
        self.cells.clear();
        self.marks.clear();
        self.tiles.clear();
        self.tags = 0;
    }
}

#[inline]
fn reach(cols: usize, blank: Cell) -> u16 {
    if blank == Cell::default() {
        0
    } else {
        cols as u16
    }
}

impl Row {
    pub(super) fn new(cols: usize, blank: Cell) -> Row {
        Row {
            cells: vec![blank; cols],
            marks: Vec::new(),
            tiles: Vec::new(),
            used: reach(cols, blank),
            wide: false,
            wrapped: false,
            tags: 0,
        }
    }

    pub(super) fn assemble(
        cells: &[Cell],
        marks: impl Iterator<Item = (u16, char)>,
        tiles: Vec<Tile>,
        cols: usize,
        blank: Cell,
        wrapped: bool,
        tags: u8,
    ) -> Row {
        let mut row = Row {
            cells: cells.to_vec(),
            marks: marks.collect(),
            tiles,
            used: reach(cols, blank).max(cells.len().min(cols) as u16),
            wide: cells.iter().any(|cell| cell.is_wide() || cell.is_spacer()),
            wrapped,
            tags,
        };
        row.cells.resize(cols, blank);
        row
    }

    pub(super) fn unload(self, keep: usize, logical: &mut Logical) {
        let base = logical.cells.len();
        logical.tags |= self.tags;
        logical
            .tiles
            .extend(self.tiles.into_iter().map(|tile| (base, tile)));
        logical.marks.extend(
            self.marks
                .iter()
                .filter(|&&(column, _)| usize::from(column) < keep)
                .map(|&(column, c)| (base + usize::from(column), c)),
        );
        logical.cells.extend(
            self.cells
                .into_iter()
                .chain(std::iter::repeat(Cell::default()))
                .take(keep),
        );
    }

    #[inline]
    #[must_use]
    pub fn tiles(&self) -> &[Tile] {
        &self.tiles
    }

    #[inline]
    pub(super) fn tiles_mut(&mut self) -> &mut Vec<Tile> {
        &mut self.tiles
    }

    pub(super) fn uncover(&mut self, range: Range<usize>) {
        let overlaps = |tile: &Tile| {
            tile.fragile && tile.columns().start < range.end && range.start < tile.columns().end
        };
        if !self.tiles.iter().any(overlaps) {
            return;
        }
        for tile in std::mem::take(&mut self.tiles) {
            if overlaps(&tile) {
                let covered = tile.columns();
                if covered.start < range.start {
                    self.tiles.push(tile.slice(covered.start..range.start));
                }
                if range.end < covered.end {
                    self.tiles.push(tile.slice(range.end..covered.end));
                }
            } else {
                self.tiles.push(tile);
            }
        }
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

    #[inline]
    #[must_use]
    pub fn is_prompt(&self) -> bool {
        self.tags & PROMPT != 0
    }

    #[inline]
    #[must_use]
    pub fn is_output(&self) -> bool {
        self.tags & OUTPUT != 0
    }

    #[inline]
    #[must_use]
    pub fn is_failed(&self) -> bool {
        self.tags & FAILED != 0
    }

    #[inline]
    pub(super) fn tags(&self) -> u8 {
        self.tags
    }

    #[inline]
    pub(super) fn tag(&mut self, tags: u8) {
        self.tags |= tags;
    }

    pub fn marks(&self, column: usize) -> impl Iterator<Item = char> + '_ {
        self.marks
            .iter()
            .filter(move |&&(mark, _)| usize::from(mark) == column)
            .map(|&(_, c)| c)
    }

    #[must_use]
    pub fn occupied(&self) -> usize {
        self.cells[..usize::from(self.used)]
            .iter()
            .rposition(|cell| !cell.is_blank())
            .map_or(0, |last| last + 1)
    }

    #[inline]
    fn wipe(&mut self, cols: usize, blank: Cell) {
        if self.cells.len() == cols && blank == Cell::default() {
            self.cells[..usize::from(self.used)].fill(blank);
        } else {
            self.refill(cols, blank);
        }
        self.used = reach(cols, blank);
        self.wide = false;
        self.marks.clear();
        self.wrapped = false;
        self.tags = 0;
    }

    fn refill(&mut self, cols: usize, blank: Cell) {
        self.cells.truncate(cols);
        self.cells.fill(blank);
        self.cells.resize(cols, blank);
    }

    pub(super) fn clear(&mut self, cols: usize, blank: Cell) {
        self.wipe(cols, blank);
        self.tiles.retain(|tile| !tile.fragile);
    }

    #[inline]
    pub(super) fn reset(&mut self, cols: usize, blank: Cell) {
        self.wipe(cols, blank);
        self.tiles.clear();
    }

    pub(super) fn resize(&mut self, cols: usize, blank: Cell) {
        if cols < self.cells.len() {
            self.release(cols..self.cells.len());
        }
        self.used = if cols > self.cells.len() {
            self.used.max(reach(cols, blank))
        } else {
            self.used.min(cols as u16)
        };
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
        self.used = self.used.max((column + bytes.len()) as u16);
    }

    #[inline]
    pub(super) fn write(&mut self, column: usize, c: char, width: usize, style: StyleId) {
        self.release(column..column + width);
        if width == 2 {
            self.cells[column] = Cell::wide(c, style);
            self.cells[column + 1] = Cell::spacer(style);
            self.wide = true;
        } else {
            self.cells[column] = Cell::narrow(c, style);
        }
        self.used = self.used.max((column + width) as u16);
    }

    pub(super) fn erase(&mut self, range: Range<usize>, blank: Cell) {
        self.release(range.clone());
        let used = usize::from(self.used);
        if blank != Cell::default() {
            self.used = self.used.max(range.end as u16);
            self.cells[range].fill(blank);
        } else if range.start < used {
            if range.end >= used {
                self.used = range.start as u16;
            }
            self.cells[range.start..range.end.min(used)].fill(blank);
        }
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
        self.used = cols as u16;
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
        self.used = cols as u16;
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
        if !range.is_empty()
            && (!self.tiles.is_empty()
                || range.start < usize::from(self.used)
                    && (!self.marks.is_empty()
                        || self.wide
                            && (self.cells[range.start].is_spacer()
                                || self
                                    .cells
                                    .get(range.end)
                                    .is_some_and(|cell| cell.is_spacer()))))
        {
            self.detach(range);
        }
    }

    fn detach(&mut self, range: Range<usize>) {
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
        if !self.tiles.is_empty() {
            self.uncover(start..end);
        }
    }
}
