use dartel_search::{Query, find_all, url_at};

use super::{Row, Term};

const SEPARATORS: &str = " \t\"'`()[]{}<>|;,";
const SCHEMES: [&str; 5] = ["http://", "https://", "ftp://", "file://", "mailto:"];

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Stream(Span),
    Block(Span),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Url(String),
    Path(String),
}

impl Span {
    #[must_use]
    pub fn block(anchor: Point, head: Point) -> Span {
        Span {
            start: Point {
                line: anchor.line.min(head.line),
                column: anchor.column.min(head.column),
            },
            end: Point {
                line: anchor.line.max(head.line),
                column: anchor.column.max(head.column),
            },
        }
    }
}

#[must_use]
pub fn path_candidate(word: &str) -> &str {
    (0..2).fold(
        word.trim_end_matches([':', '.', ',']),
        |text, _| match text.rsplit_once(':') {
            Some((head, digits))
                if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) =>
            {
                head
            }
            _ => text,
        },
    )
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
    fn logical(&self, line: usize) -> (usize, usize) {
        let line = line.min(self.grid.total() - 1);
        let first = (0..line)
            .rev()
            .find(|&above| !self.grid.absolute(above).is_wrapped())
            .map_or(0, |above| above + 1);
        let last = (line..self.grid.total() - 1)
            .find(|&below| !self.grid.absolute(below).is_wrapped())
            .unwrap_or(self.grid.total() - 1);
        (first, last)
    }

    #[must_use]
    pub fn word_at(&self, point: Point) -> Option<Span> {
        let cols = self.cols();
        let (first, last) = self.logical(point.line);
        let inside = |index: usize| {
            let cell = self.grid.absolute(first + index / cols).cells()[index % cols];
            cell.is_spacer() || cell.ch().is_some_and(|c| !SEPARATORS.contains(c))
        };
        let origin = (point.line.min(last) - first) * cols + point.column.min(cols - 1);
        let limit = (last - first + 1) * cols;
        let start = (0..origin)
            .rev()
            .find(|&index| !inside(index))
            .map_or(0, |index| index + 1);
        let end = (origin + 1..limit)
            .find(|&index| !inside(index))
            .unwrap_or(limit)
            - 1;
        inside(origin).then_some(Span {
            start: Point {
                line: first + start / cols,
                column: start % cols,
            },
            end: Point {
                line: first + end / cols,
                column: end % cols,
            },
        })
    }

    #[must_use]
    pub fn line_at(&self, point: Point) -> Span {
        let (first, last) = self.logical(point.line);
        Span {
            start: Point {
                line: first,
                column: 0,
            },
            end: Point {
                line: last,
                column: self.cols() - 1,
            },
        }
    }

    #[must_use]
    pub fn target_at(&self, point: Point) -> Option<Target> {
        let line = point.line.min(self.grid.total() - 1);
        let row = self.grid.absolute(line);
        let column = point.column.min(self.cols() - 1);
        let column = column - usize::from(column > 0 && row.cells()[column].is_spacer());
        if let Some(link) = self.link(self.style(row.cells()[column].style()).link) {
            return SCHEMES
                .iter()
                .any(|scheme| {
                    link.get(..scheme.len())
                        .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
                })
                .then(|| Target::Url(link.to_owned()));
        }
        let (first, last) = self.logical(line);
        let mut bytes = Vec::new();
        let mut offset = usize::MAX;
        for index in first..=last {
            let row = self.grid.absolute(index);
            let pointed = index == line && column < row.occupied();
            let split = if pointed { column } else { row.occupied() };
            push_cells(&mut bytes, row, 0..split);
            if pointed {
                offset = bytes.len();
            }
            push_cells(&mut bytes, row, split..row.occupied());
        }
        if let Some(range) = url_at(&bytes, offset) {
            return Some(Target::Url(
                String::from_utf8_lossy(&bytes[range]).into_owned(),
            ));
        }
        self.word_at(Point { line, column })
            .map(|span| path_candidate(&self.text(span)).to_owned())
            .filter(|path| !path.is_empty())
            .map(Target::Path)
    }

    #[must_use]
    pub fn prompt_before(&self, line: usize) -> Option<usize> {
        (0..line.min(self.grid.total()))
            .rev()
            .find(|&index| self.grid.absolute(index).is_prompt())
    }

    #[must_use]
    pub fn prompt_after(&self, line: usize) -> Option<usize> {
        (line + 1..self.grid.total()).find(|&index| self.grid.absolute(index).is_prompt())
    }

    #[must_use]
    pub fn last_output(&self) -> Option<Span> {
        let cursor = self.grid.history() + self.cursor.line;
        let start = (0..=cursor)
            .rev()
            .find(|&index| self.grid.absolute(index).is_output())?;
        let end = (start + 1..=cursor)
            .find(|&index| self.grid.absolute(index).is_prompt())
            .map_or(cursor, |prompt| prompt - 1);
        (!self.grid.absolute(start).is_prompt()).then_some(Span {
            start: Point {
                line: start,
                column: 0,
            },
            end: Point {
                line: end,
                column: self.cols() - 1,
            },
        })
    }

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
        find_all(query, &bytes, limit)
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

    #[must_use]
    pub fn block_columns(&self, line: usize, span: Span) -> std::ops::Range<usize> {
        let cells = self.grid.absolute(line.min(self.grid.total() - 1)).cells();
        let start = span.start.column
            - usize::from(
                span.start.column > 0
                    && cells
                        .get(span.start.column)
                        .is_some_and(|cell| cell.is_spacer()),
            );
        let end = (span.end.column
            + 1
            + usize::from(
                cells
                    .get(span.end.column)
                    .is_some_and(|cell| cell.is_wide()),
            ))
        .min(self.cols());
        start..end
    }

    #[must_use]
    pub fn block_text(&self, span: Span) -> String {
        let mut bytes = Vec::new();
        for line in span.start.line..=span.end.line.min(self.grid.total() - 1) {
            if line != span.start.line {
                bytes.push(b'\n');
            }
            let row = self.grid.absolute(line);
            let columns = self.block_columns(line, span);
            let last = columns.end.min(row.occupied());
            if columns.start < last {
                push_cells(&mut bytes, row, columns.start..last);
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }
}
