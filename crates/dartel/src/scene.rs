use std::collections::HashMap;

use dartel_core::term::{
    Attrs, Color, CursorShape, Modes, Palette, Rgb, Span, Style, StyleId, Term,
};
use unicode_width::UnicodeWidthChar;

use crate::font::{Fonts, Metrics, REGULAR};
use crate::gpu::{ATLAS_SIZE, Gpu, Quad};

const SELECTION_ALPHA: u8 = 80;

#[derive(Clone, Copy, Debug, Default)]
struct Glyph {
    origin: [u16; 2],
    size: [u16; 2],
    left: i16,
    top: i16,
    advance: i16,
    colored: bool,
}

#[derive(Debug, Default)]
struct Row {
    backgrounds: Vec<Quad>,
    glyphs: Vec<Quad>,
}

#[derive(Clone, Copy, Debug)]
struct Look {
    foreground: Rgb,
    background: Option<Rgb>,
    underline: Rgb,
    attrs: Attrs,
    face: usize,
}

pub struct Overlay<'a> {
    pub focused: bool,
    pub preedit: &'a str,
    pub selection: Option<Span>,
}

pub struct Scene {
    fonts: Fonts,
    ascii: [[Option<Glyph>; 128]; 4],
    glyphs: HashMap<(char, usize), Glyph>,
    pen: [u32; 2],
    shelf: u32,
    rows: Vec<Row>,
    quads: Vec<Quad>,
    padding: i32,
    stale: bool,
}

fn blend(ink: Rgb, paper: Rgb) -> Rgb {
    let mix = |ink: u8, paper: u8| ((u16::from(ink) * 2 + u16::from(paper)) / 3) as u8;
    Rgb::new(
        mix(ink.r, paper.r),
        mix(ink.g, paper.g),
        mix(ink.b, paper.b),
    )
}

fn look(style: Style, palette: &Palette, reverse: bool) -> Look {
    let inverse = style.attrs.contains(Attrs::INVERSE) != reverse;
    let foreground = palette.resolve(style.foreground, palette.foreground);
    let background = palette.resolve(style.background, palette.background);
    let (ink, paper) = if inverse {
        (background, foreground)
    } else {
        (foreground, background)
    };
    let ink = if style.attrs.contains(Attrs::HIDDEN) {
        paper
    } else if style.attrs.contains(Attrs::DIM) {
        blend(ink, paper)
    } else {
        ink
    };
    Look {
        foreground: ink,
        background: (inverse || style.background != Color::Default).then_some(paper),
        underline: palette.resolve(style.underline, ink),
        attrs: style.attrs,
        face: usize::from(style.attrs.contains(Attrs::BOLD))
            + 2 * usize::from(style.attrs.contains(Attrs::ITALIC)),
    }
}

fn solid(x: i32, y: i32, width: i32, height: i32, color: Rgb, alpha: u8) -> Quad {
    Quad {
        rect: [x as i16, y as i16, width as i16, height as i16],
        source: [0; 4],
        color: [color.r, color.g, color.b, alpha],
    }
}

fn textured(x: i32, y: i32, glyph: Glyph, color: Rgb) -> Quad {
    Quad {
        rect: [
            x as i16,
            y as i16,
            glyph.size[0] as i16,
            glyph.size[1] as i16,
        ],
        source: [glyph.origin[0], glyph.origin[1], 1, 0],
        color: if glyph.colored {
            [u8::MAX; 4]
        } else {
            [color.r, color.g, color.b, u8::MAX]
        },
    }
}

fn centering(span: i32, glyph: Glyph) -> i32 {
    if (span - i32::from(glyph.advance)).abs() <= 1 {
        0
    } else {
        (span - i32::from(glyph.advance)) / 2
    }
}

impl Scene {
    pub fn new(font_size: f32, padding: i32) -> Option<Scene> {
        Some(Scene {
            fonts: Fonts::new(font_size)?,
            ascii: [[None; 128]; 4],
            glyphs: HashMap::new(),
            pen: [0; 2],
            shelf: 0,
            rows: Vec::new(),
            quads: Vec::new(),
            padding,
            stale: true,
        })
    }

    pub fn metrics(&self) -> Metrics {
        self.fonts.metrics()
    }

    pub fn rescale(&mut self, font_size: f32, padding: i32) {
        self.fonts.set_size(font_size);
        self.padding = padding;
        self.forget_glyphs();
    }

    pub fn grid_size(&self, width: u32, height: u32) -> (usize, usize) {
        let metrics = self.fonts.metrics();
        let cells = |pixels: u32, cell: u32| {
            ((pixels as i32 - 2 * self.padding).max(0) as u32 / cell).max(1) as usize
        };
        (cells(height, metrics.height), cells(width, metrics.width))
    }

    pub fn cell_at(&self, x: f64, y: f64) -> (usize, usize) {
        let metrics = self.fonts.metrics();
        let cell =
            |pixels: f64, cell: u32| ((pixels as i32 - self.padding).max(0) as u32 / cell) as usize;
        (cell(y, metrics.height), cell(x, metrics.width))
    }

    fn forget_glyphs(&mut self) {
        self.ascii = [[None; 128]; 4];
        self.glyphs.clear();
        self.pen = [0; 2];
        self.shelf = 0;
        self.rows.clear();
        self.stale = true;
    }

    fn glyph(&mut self, gpu: &Gpu, c: char, face: usize) -> Option<Glyph> {
        if let Some(cached) = self.ascii[face]
            .get(c as usize)
            .map_or_else(|| self.glyphs.get(&(c, face)).copied(), |slot| *slot)
        {
            return Some(cached);
        }
        let glyph = self.rasterize(gpu, c, face)?;
        match self.ascii[face].get_mut(c as usize) {
            Some(slot) => *slot = Some(glyph),
            None => {
                self.glyphs.insert((c, face), glyph);
            }
        }
        Some(glyph)
    }

    fn rasterize(&mut self, gpu: &Gpu, c: char, face: usize) -> Option<Glyph> {
        let Some(raster) = self.fonts.rasterize(c, face).filter(|raster| {
            (1..=ATLAS_SIZE).contains(&raster.width) && (1..=ATLAS_SIZE).contains(&raster.height)
        }) else {
            return Some(Glyph::default());
        };
        if self.pen[0] + raster.width > ATLAS_SIZE {
            self.pen = [0, self.pen[1] + self.shelf];
            self.shelf = 0;
        }
        if self.pen[1] + raster.height > ATLAS_SIZE {
            return None;
        }
        gpu.upload(self.pen, [raster.width, raster.height], &raster.pixels);
        let glyph = Glyph {
            origin: [self.pen[0] as u16, self.pen[1] as u16],
            size: [raster.width as u16, raster.height as u16],
            left: raster.left as i16,
            top: raster.top as i16,
            advance: raster.advance.round() as i16,
            colored: raster.colored,
        };
        self.pen[0] += raster.width;
        self.shelf = self.shelf.max(raster.height);
        Some(glyph)
    }

    pub fn quads(&self) -> &[Quad] {
        &self.quads
    }

    pub fn cell_origin(&self, line: usize, column: usize) -> (i32, i32) {
        (
            self.padding + column as i32 * self.fonts.metrics().width as i32,
            self.padding + line as i32 * self.fonts.metrics().height as i32,
        )
    }

    pub fn compose(&mut self, gpu: &Gpu, term: &Term, overlay: &Overlay<'_>) {
        if self.rows.len() != term.lines() {
            self.rows.clear();
            self.rows.resize_with(term.lines(), Row::default);
            self.stale = true;
        }
        if !self.compose_rows(gpu, term) {
            self.forget_glyphs();
            self.rows.resize_with(term.lines(), Row::default);
            self.compose_rows(gpu, term);
        }
        self.stale = false;
        self.quads.clear();
        for row in &self.rows {
            self.quads.extend_from_slice(&row.backgrounds);
        }
        for row in &self.rows {
            self.quads.extend_from_slice(&row.glyphs);
        }
        self.compose_selection(term, overlay);
        if overlay.preedit.is_empty() {
            self.compose_cursor(gpu, term, overlay.focused);
        } else {
            self.compose_preedit(gpu, term, overlay.preedit);
        }
    }

    fn compose_rows(&mut self, gpu: &Gpu, term: &Term) -> bool {
        (0..term.lines()).all(|line| {
            !(self.stale || term.damage().line(line)) || self.compose_row(gpu, term, line)
        })
    }

    fn compose_row(&mut self, gpu: &Gpu, term: &Term, line: usize) -> bool {
        let metrics = self.fonts.metrics();
        let (width, height) = (metrics.width as i32, metrics.height as i32);
        let top = self.padding + line as i32 * height;
        let reverse = term.modes().contains(Modes::REVERSE_VIDEO);
        let source = term.visible_row(line);
        let mut row = std::mem::take(&mut self.rows[line]);
        row.backgrounds.clear();
        row.glyphs.clear();
        let mut current = (
            StyleId::DEFAULT,
            look(term.style(StyleId::DEFAULT), term.palette(), reverse),
        );
        let mut fill: Option<(usize, Rgb)> = None;
        let mut complete = true;
        'cells: for (column, cell) in source.cells().iter().take(term.cols()).enumerate() {
            if cell.style() != current.0 {
                current = (
                    cell.style(),
                    look(term.style(cell.style()), term.palette(), reverse),
                );
            }
            let look = current.1;
            let left = self.padding + column as i32 * width;
            if fill.map(|(_, color)| color) != look.background {
                if let Some((start, color)) = fill {
                    row.backgrounds.push(solid(
                        self.padding + start as i32 * width,
                        top,
                        (column - start) as i32 * width,
                        height,
                        color,
                        u8::MAX,
                    ));
                }
                fill = look.background.map(|color| (column, color));
            }
            if let Some(base) = cell.ch() {
                let span = if cell.is_wide() { 2 * width } else { width };
                let mut shift = None;
                for c in std::iter::once(base).chain(
                    cell.is_marked()
                        .then(|| source.marks(column))
                        .into_iter()
                        .flatten(),
                ) {
                    let Some(glyph) = self.glyph(gpu, c, look.face) else {
                        complete = false;
                        break 'cells;
                    };
                    let shift = *shift.get_or_insert_with(|| centering(span, glyph));
                    if glyph.size[0] > 0 {
                        row.glyphs.push(textured(
                            left + shift + i32::from(glyph.left),
                            top + metrics.ascent - i32::from(glyph.top),
                            glyph,
                            look.foreground,
                        ));
                    }
                }
            }
            if look.attrs.contains(Attrs::ANY_UNDERLINE) {
                row.glyphs.push(solid(
                    left,
                    top + metrics.underline,
                    width,
                    metrics.stroke as i32,
                    look.underline,
                    u8::MAX,
                ));
            }
            if look.attrs.contains(Attrs::DOUBLE_UNDERLINE) {
                row.glyphs.push(solid(
                    left,
                    top + metrics.underline + 2 * metrics.stroke as i32,
                    width,
                    metrics.stroke as i32,
                    look.underline,
                    u8::MAX,
                ));
            }
            if look.attrs.contains(Attrs::STRIKETHROUGH) {
                row.glyphs.push(solid(
                    left,
                    top + metrics.strikeout,
                    width,
                    metrics.stroke as i32,
                    look.foreground,
                    u8::MAX,
                ));
            }
        }
        if let Some((start, color)) = fill {
            row.backgrounds.push(solid(
                self.padding + start as i32 * width,
                top,
                (term.cols() - start) as i32 * width,
                height,
                color,
                u8::MAX,
            ));
        }
        self.rows[line] = row;
        complete
    }

    fn compose_selection(&mut self, term: &Term, overlay: &Overlay<'_>) {
        let Some(span) = overlay.selection else {
            return;
        };
        let metrics = self.fonts.metrics();
        let (width, height) = (metrics.width as i32, metrics.height as i32);
        let first = term.grid().history() - term.display_offset();
        for line in 0..term.lines() {
            let absolute = first + line;
            if !(span.start.line..=span.end.line).contains(&absolute) {
                continue;
            }
            let start = if absolute == span.start.line {
                span.start.column
            } else {
                0
            };
            let end = if absolute == span.end.line {
                (span.end.column + 1).min(term.cols())
            } else {
                term.cols()
            };
            if start < end {
                self.quads.push(solid(
                    self.padding + start as i32 * width,
                    self.padding + line as i32 * height,
                    (end - start) as i32 * width,
                    height,
                    term.palette().foreground,
                    SELECTION_ALPHA,
                ));
            }
        }
    }

    fn compose_cursor(&mut self, gpu: &Gpu, term: &Term, focused: bool) {
        let cursor = term.cursor();
        if !cursor.visible {
            return;
        }
        let metrics = self.fonts.metrics();
        let (width, height, stroke) = (
            metrics.width as i32,
            metrics.height as i32,
            metrics.stroke as i32,
        );
        let cells = term.visible_row(cursor.line).cells();
        let column = cursor.column
            - usize::from(
                cells
                    .get(cursor.column)
                    .is_some_and(|cell| cell.is_spacer()),
            );
        let Some(cell) = cells.get(column) else {
            return;
        };
        let span = if cell.is_wide() { 2 * width } else { width };
        let (x, y) = (
            self.padding + column as i32 * width,
            self.padding + cursor.line as i32 * height,
        );
        let color = term.palette().cursor;
        match (focused, cursor.shape) {
            (false, _) => {
                for (left, top, wide, tall) in [
                    (x, y, span, stroke),
                    (x, y + height - stroke, span, stroke),
                    (x, y, stroke, height),
                    (x + span - stroke, y, stroke, height),
                ] {
                    self.quads
                        .push(solid(left, top, wide, tall, color, u8::MAX));
                }
            }
            (true, CursorShape::Block) => {
                self.quads.push(solid(x, y, span, height, color, u8::MAX));
                if let Some(glyph) = cell.ch().and_then(|c| {
                    self.glyph(
                        gpu,
                        c,
                        look(term.style(cell.style()), term.palette(), false).face,
                    )
                }) && glyph.size[0] > 0
                {
                    self.quads.push(textured(
                        x + centering(span, glyph) + i32::from(glyph.left),
                        y + metrics.ascent - i32::from(glyph.top),
                        glyph,
                        term.palette().background,
                    ));
                }
            }
            (true, CursorShape::Underline) => self.quads.push(solid(
                x,
                y + height - 2 * stroke,
                span,
                2 * stroke,
                color,
                u8::MAX,
            )),
            (true, CursorShape::Bar) => {
                self.quads
                    .push(solid(x, y, 2 * stroke, height, color, u8::MAX));
            }
        }
    }

    fn compose_preedit(&mut self, gpu: &Gpu, term: &Term, preedit: &str) {
        let metrics = self.fonts.metrics();
        let (width, height) = (metrics.width as i32, metrics.height as i32);
        let cursor = term.cursor();
        let top = self.padding + cursor.line.min(term.lines() - 1) as i32 * height;
        let mut column = cursor.column;
        for c in preedit.chars() {
            let cells = c.width().unwrap_or(0);
            if cells == 0 || column + cells > term.cols() {
                continue;
            }
            let left = self.padding + column as i32 * width;
            let span = cells as i32 * width;
            self.quads.push(solid(
                left,
                top,
                span,
                height,
                term.palette().background,
                u8::MAX,
            ));
            if let Some(glyph) = self.glyph(gpu, c, REGULAR)
                && glyph.size[0] > 0
            {
                self.quads.push(textured(
                    left + centering(span, glyph) + i32::from(glyph.left),
                    top + metrics.ascent - i32::from(glyph.top),
                    glyph,
                    term.palette().foreground,
                ));
            }
            self.quads.push(solid(
                left,
                top + metrics.underline,
                span,
                metrics.stroke as i32,
                term.palette().foreground,
                u8::MAX,
            ));
            column += cells;
        }
    }
}
