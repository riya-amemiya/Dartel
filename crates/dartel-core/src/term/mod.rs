mod cell;
mod charset;
mod color;
mod find;
mod grid;
mod modes;
mod osc;
mod perform;
mod row;
mod screen;
mod sgr;
mod style;

pub use cell::Cell;
pub use color::{Color, Palette, Rgb};
pub use find::{Point, Span};
pub use grid::Grid;
pub use modes::Modes;
pub use row::Row;
pub use style::{Attrs, Style, StyleId};

use charset::Charset;
use style::StyleTable;

const MAX_SIZE: usize = (u16::MAX / 2) as usize;
const TAB_STOP: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorShape {
    #[default]
    Block,
    Underline,
    Bar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorView {
    pub line: usize,
    pub column: usize,
    pub shape: CursorShape,
    pub visible: bool,
    pub blinking: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct Cursor {
    line: usize,
    column: usize,
    pending_wrap: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct Saved {
    cursor: Cursor,
    pen: Style,
    charsets: [Charset; 4],
    shift: usize,
    origin: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct Pen {
    style: Style,
    interned: Style,
    id: StyleId,
    blank: StyleId,
    blank_background: Color,
}

#[derive(Clone, Debug, Default)]
struct Outbox {
    reply: Vec<u8>,
    clipboard: Option<String>,
    title_changed: bool,
    bell: bool,
}

#[derive(Clone, Debug)]
pub struct Damage {
    lines: Vec<bool>,
    full: bool,
}

impl Damage {
    fn new(lines: usize) -> Damage {
        Damage {
            lines: vec![false; lines],
            full: true,
        }
    }

    #[inline]
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.full
    }

    #[inline]
    #[must_use]
    pub fn line(&self, line: usize) -> bool {
        self.full || self.lines[line]
    }

    #[must_use]
    pub fn any(&self) -> bool {
        self.full || self.lines.contains(&true)
    }
}

#[derive(Clone, Debug)]
pub struct Term {
    grid: Grid,
    inactive: Grid,
    cursor: Cursor,
    saved: Saved,
    inactive_saved: Saved,
    styles: StyleTable,
    pen: Pen,
    palette: Palette,
    base_palette: Palette,
    modes: Modes,
    top: usize,
    bottom: usize,
    tabs: Vec<bool>,
    charsets: [Charset; 4],
    shift: usize,
    fast_print: bool,
    last_char: Option<char>,
    cursor_shape: CursorShape,
    title: String,
    outbox: Outbox,
    damage: Damage,
    display_offset: usize,
    history_limit: usize,
}

fn default_tabs(cols: usize) -> Vec<bool> {
    (0..cols).map(|column| column % TAB_STOP == 0).collect()
}

impl Term {
    #[must_use]
    pub fn new(lines: usize, cols: usize, history_limit: usize) -> Term {
        Term::with_palette(lines, cols, history_limit, Palette::default())
    }

    #[must_use]
    pub fn with_palette(lines: usize, cols: usize, history_limit: usize, palette: Palette) -> Term {
        let (lines, cols) = (lines.clamp(1, MAX_SIZE), cols.clamp(2, MAX_SIZE));
        Term {
            grid: Grid::new(lines, cols, history_limit),
            inactive: Grid::new(lines, cols, 0),
            cursor: Cursor::default(),
            saved: Saved::default(),
            inactive_saved: Saved::default(),
            styles: StyleTable::new(),
            pen: Pen::default(),
            palette: palette.clone(),
            base_palette: palette,
            modes: Modes::default(),
            top: 0,
            bottom: lines - 1,
            tabs: default_tabs(cols),
            charsets: [Charset::default(); 4],
            shift: 0,
            fast_print: true,
            last_char: None,
            cursor_shape: CursorShape::default(),
            title: String::new(),
            outbox: Outbox::default(),
            damage: Damage::new(lines),
            display_offset: 0,
            history_limit,
        }
    }

    pub fn resize(&mut self, lines: usize, cols: usize) {
        let (lines, cols) = (lines.clamp(1, MAX_SIZE), cols.clamp(2, MAX_SIZE));
        if (lines, cols) == (self.lines(), self.cols()) {
            return;
        }
        self.cursor.line = self
            .grid
            .resize(lines, cols, self.cursor.line, Cell::default());
        self.cursor.column = self.cursor.column.min(cols - 1);
        self.cursor.pending_wrap = false;
        self.inactive_saved.cursor.line = self.inactive.resize(
            lines,
            cols,
            self.inactive_saved
                .cursor
                .line
                .min(self.inactive.lines() - 1),
            Cell::default(),
        );
        self.inactive_saved.cursor.column = self.inactive_saved.cursor.column.min(cols - 1);
        self.saved.cursor.line = self.saved.cursor.line.min(lines - 1);
        self.saved.cursor.column = self.saved.cursor.column.min(cols - 1);
        self.top = 0;
        self.bottom = lines - 1;
        self.tabs = default_tabs(cols);
        self.display_offset = self.display_offset.min(self.grid.history());
        self.damage = Damage::new(lines);
    }

    #[inline]
    #[must_use]
    pub fn lines(&self) -> usize {
        self.grid.lines()
    }

    #[inline]
    #[must_use]
    pub fn cols(&self) -> usize {
        self.grid.cols()
    }

    #[inline]
    #[must_use]
    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    #[inline]
    #[must_use]
    pub fn visible_row(&self, line: usize) -> &Row {
        self.grid
            .absolute(self.grid.history() - self.display_offset + line)
    }

    #[inline]
    #[must_use]
    pub fn style(&self, id: StyleId) -> Style {
        self.styles.get(id)
    }

    #[inline]
    #[must_use]
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    #[inline]
    #[must_use]
    pub fn modes(&self) -> Modes {
        self.modes
    }

    #[must_use]
    pub fn cursor(&self) -> CursorView {
        CursorView {
            line: self.cursor.line + self.display_offset,
            column: self.cursor.column,
            shape: self.cursor_shape,
            visible: self.modes.contains(Modes::CURSOR_VISIBLE)
                && self.cursor.line + self.display_offset < self.lines(),
            blinking: self.modes.contains(Modes::CURSOR_BLINK),
        }
    }

    #[inline]
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn take_title_change(&mut self) -> bool {
        std::mem::take(&mut self.outbox.title_changed)
    }

    pub fn take_bell(&mut self) -> bool {
        std::mem::take(&mut self.outbox.bell)
    }

    pub fn take_clipboard(&mut self) -> Option<String> {
        self.outbox.clipboard.take()
    }

    pub fn take_reply(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.outbox.reply)
    }

    #[inline]
    #[must_use]
    pub fn damage(&self) -> &Damage {
        &self.damage
    }

    pub fn reset_damage(&mut self) {
        self.damage.lines.fill(false);
        self.damage.full = false;
    }

    #[inline]
    #[must_use]
    pub fn display_offset(&self) -> usize {
        self.display_offset
    }

    pub fn scroll_display(&mut self, delta: isize) {
        self.set_display_offset(self.display_offset.saturating_add_signed(delta));
    }

    pub fn reset_display(&mut self) {
        self.set_display_offset(0);
    }

    fn set_display_offset(&mut self, offset: usize) {
        if offset.min(self.grid.history()) != self.display_offset {
            self.display_offset = offset.min(self.grid.history());
            self.damage.full = true;
        }
    }
}
