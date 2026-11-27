use super::{Row, Term};
use crate::search::{self, Query};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: Point,
    pub end: Point,
}

#[inline]
fn push_char(bytes: &mut Vec<u8>, c: char) {
    if c.is_ascii() {
        bytes.push(c as u8);
    } else {
        bytes.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
    }
}

fn push_cells(bytes: &mut Vec<u8>, row: &Row, columns: std::ops::Range<usize>) {
    for (column, cell) in row.cells()[columns.clone()].iter().enumerate() {
        if cell.is_spacer() {
            continue;
        }
        push_char(bytes, cell.ch().unwrap_or(' '));
        if cell.is_marked() {
            for mark in row.marks(columns.start + column) {
                push_char(bytes, mark);
            }
        }
    }
}

fn column_at(row: &Row, offset: usize) -> usize {
    let mut remaining = offset;
    for (column, cell) in row.cells()[..row.occupied()].iter().enumerate() {
        if cell.is_spacer() {
            continue;
        }
        let width = cell.ch().map_or(1, char::len_utf8)
            + if cell.is_marked() {
                row.marks(column).map(char::len_utf8).sum()
            } else {
                0
            };
        if remaining < width {
            return column;
        }
        remaining -= width;
    }
    row.occupied().saturating_sub(1)
}

impl Term {
    #[must_use]
    pub fn search(&self, query: &Query, limit: usize) -> Vec<Span> {
        let mut bytes = Vec::new();
        let mut starts = Vec::with_capacity(self.grid.total());
        for row in self.grid.rows() {
            starts.push(bytes.len());
            push_cells(&mut bytes, row, 0..row.occupied());
            if !row.is_wrapped() {
                bytes.push(b'\n');
            }
        }
        search::find_all(query, &bytes, limit)
            .into_iter()
            .map(|found| Span {
                start: self.locate(&starts, found.start()),
                end: self.locate(&starts, found.end() - 1),
            })
            .collect()
    }

    fn locate(&self, starts: &[usize], offset: usize) -> Point {
        let line = starts.partition_point(|&start| start <= offset) - 1;
        Point {
            line,
            column: column_at(self.grid.absolute(line), offset - starts[line]),
        }
    }

    #[must_use]
    pub fn text(&self, span: Span) -> String {
        let mut bytes = Vec::new();
        for line in span.start.line..=span.end.line.min(self.grid.total() - 1) {
            let row = self.grid.absolute(line);
            let first = if line == span.start.line {
                span.start.column
            } else {
                0
            };
            let last = if line == span.end.line {
                (span.end.column + 1).min(row.occupied())
            } else {
                row.occupied()
            };
            if first < last {
                push_cells(&mut bytes, row, first..last);
            }
            if line != span.end.line && !row.is_wrapped() {
                bytes.push(b'\n');
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }
}
