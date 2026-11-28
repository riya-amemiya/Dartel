use std::collections::HashMap;
use std::sync::Arc;

use dartel_term::{
    Attrs, Color, CursorShape, Modes, Palette, Picture, Rgb, Selection, Span, Style, StyleId, Term,
    Tile,
};
use unicode_width::UnicodeWidthChar;

use crate::font::{Fonts, Metrics, REGULAR};
use crate::gpu::{ATLAS_SIZE, Gpu, Layer, Quad};

const SELECTION_ALPHA: u8 = 80;
const MATCH_ALPHA: u8 = 90;
const CURRENT_ALPHA: u8 = 170;
const SCROLL_ALPHA: u8 = 128;
const TAB_CELLS: u32 = 24;

#[derive(Clone, Copy, Debug, Default)]
struct Glyph {
    origin: [u16; 2],
    size: [u16; 2],
    left: i16,
    top: i16,
    advance: i16,
    colored: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pattern {
    Curly,
    Dotted,
    Dashed,
}

#[derive(Debug, Default)]
struct Row {
    backgrounds: Vec<Quad>,
    glyphs: Vec<Quad>,
    pictures: Vec<(Arc<Picture>, Quad)>,
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
    pub caret: bool,
    pub preedit: &'a str,
    pub selection: Option<Selection>,
    pub matches: &'a [Span],
    pub current: Option<Span>,
    pub banner: Option<&'a str>,
    pub tabs: &'a [String],
    pub active: usize,
    pub width: u32,
}

struct Label<'a> {
    line: usize,
    column: usize,
    text: &'a str,
    ink: Rgb,
    paper: Rgb,
    underlined: bool,
}

pub struct Scene {
    fonts: Fonts,
    ascii: [[Option<Glyph>; 128]; 4],
    glyphs: HashMap<(char, usize), Glyph>,
    patterns: [Option<Glyph>; 3],
    pen: [u32; 2],
    shelf: u32,
    rows: Vec<Row>,
    quads: Vec<Quad>,
    layers: Vec<Layer>,
    padding: i32,
    bar: bool,
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

fn tint(paper: Rgb, ink: Rgb) -> Rgb {
    let mix = |paper: u8, ink: u8| ((u16::from(paper) * 7 + u16::from(ink)) / 8) as u8;
    Rgb::new(
        mix(paper.r, ink.r),
        mix(paper.g, ink.g),
        mix(paper.b, ink.b),
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
    let mut attrs = style.attrs;
    if style.link != 0 && !attrs.contains(Attrs::ANY_UNDERLINE) {
        attrs.set(Attrs::UNDERLINE, true);
    }
    Look {
        foreground: ink,
        background: (inverse || style.background != Color::Default).then_some(paper),
        underline: palette.resolve(style.underline, ink),
        attrs,
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
        source: [
            glyph.origin[0],
            glyph.origin[1],
            glyph.size[0],
            glyph.size[1],
        ],
        color: if glyph.colored {
            [u8::MAX; 4]
        } else {
            [color.r, color.g, color.b, u8::MAX]
        },
    }
}

fn pictured(tile: &Tile, origin: [i32; 2], cell: [i32; 2]) -> Quad {
    let edge = |offset: u32, axis: usize| {
        origin[axis]
            + (i64::from(offset) * i64::from(cell[axis]) / i64::from(tile.cell[axis])) as i32
    };
    let (left, top) = (edge(u32::from(tile.left), 0), edge(u32::from(tile.top), 1));
    Quad {
        rect: [
            left as i16,
            top as i16,
            (edge(u32::from(tile.left) + tile.width, 0) - left) as i16,
            (edge(u32::from(tile.top) + tile.height, 1) - top) as i16,
        ],
        source: [
            tile.x as u16,
            tile.y as u16,
            tile.width as u16,
            tile.height as u16,
        ],
        color: [u8::MAX; 4],
    }
}

fn tail(text: &str, cells: usize) -> &str {
    let mut used = 0;
    text.char_indices()
        .rev()
        .take_while(|(_, c)| {
            used += c.width().unwrap_or(0);
            used <= cells
        })
        .last()
        .map_or("", |(index, _)| &text[index..])
}

fn pattern_of(attrs: Attrs) -> Option<Pattern> {
    if attrs.contains(Attrs::CURLY_UNDERLINE) {
        Some(Pattern::Curly)
    } else if attrs.contains(Attrs::DOTTED_UNDERLINE) {
        Some(Pattern::Dotted)
    } else if attrs.contains(Attrs::DASHED_UNDERLINE) {
        Some(Pattern::Dashed)
    } else {
        None
    }
}

fn band(pattern: Pattern, metrics: Metrics) -> (i32, u32, u32) {
    let thickness = metrics.stroke.min(metrics.height);
    let size = match pattern {
        Pattern::Curly => (3 * thickness).max(metrics.width / 4).min(metrics.height),
        Pattern::Dotted | Pattern::Dashed => thickness,
    };
    (
        metrics.underline.min((metrics.height - size) as i32).max(0),
        size,
        thickness,
    )
}

fn coverage(pattern: Pattern, x: u32, y: u32, size: [u32; 2], thickness: u32) -> u8 {
    let [width, height] = size;
    match pattern {
        Pattern::Curly => {
            let (span, stroke) = (width as f32, thickness as f32);
            let swing = (height as f32 - stroke).max(0.0) / 2.0;
            let phase = std::f32::consts::TAU * (x as f32 + 0.5) / span;
            let center = stroke / 2.0 + swing * (1.0 - phase.cos());
            let slope = swing * phase.sin() * std::f32::consts::TAU / span;
            let distance = (y as f32 + 0.5 - center).abs() / slope.hypot(1.0);
            ((stroke / 2.0 + 0.5 - distance).clamp(0.0, 1.0) * f32::from(u8::MAX)).round() as u8
        }
        Pattern::Dotted => {
            let segments = 2 * (width / (2 * thickness)).max(1);
            if (x * segments / width).is_multiple_of(2) {
                u8::MAX
            } else {
                0
            }
        }
        Pattern::Dashed => {
            let gap = width / 3;
            if (gap / 2..width - (gap - gap / 2)).contains(&x) {
                u8::MAX
            } else {
                0
            }
        }
    }
}

fn mask(pattern: Pattern, size: [u32; 2], thickness: u32) -> Vec<u8> {
    (0..size[1])
        .flat_map(|y| (0..size[0]).map(move |x| coverage(pattern, x, y, size, thickness)))
        .collect()
}

fn thumb(track: i32, cell: i32, lines: usize, history: usize, first: usize) -> (i32, i32) {
    let length = ((i64::from(track) * lines as i64 / (history + lines) as i64) as i32)
        .clamp(cell.min(track), track);
    (
        (i64::from(track - length) * first as i64 / history.max(1) as i64) as i32,
        length,
    )
}

fn centering(span: i32, glyph: Glyph) -> i32 {
    if (span - i32::from(glyph.advance)).abs() <= 1 {
        0
    } else {
        (span - i32::from(glyph.advance)) / 2
    }
}

impl Scene {
    #[must_use]
    pub fn new(font_family: &str, font_size: f32, padding: i32) -> Option<Scene> {
        Some(Scene {
            fonts: Fonts::new(font_family, font_size)?,
            ascii: [[None; 128]; 4],
            glyphs: HashMap::new(),
            patterns: [None; 3],
            pen: [0; 2],
            shelf: 0,
            rows: Vec::new(),
            quads: Vec::new(),
            layers: Vec::new(),
            padding,
            bar: false,
            stale: true,
        })
    }

    pub fn show_tabs(&mut self, visible: bool) {
        self.stale |= self.bar != visible;
        self.bar = visible;
    }

    pub fn invalidate(&mut self) {
        self.stale = true;
    }

    fn header(&self) -> i32 {
        if self.bar {
            self.fonts.metrics().height as i32 + self.padding
        } else {
            0
        }
    }

    fn top(&self) -> i32 {
        self.padding + self.header()
    }

    fn tab_span(&self, count: usize, width: u32) -> i32 {
        (width / count.max(1) as u32).clamp(1, TAB_CELLS * self.fonts.metrics().width) as i32
    }

    #[must_use]
    pub fn over_tabs(&self, y: f64) -> bool {
        self.bar && (y as i32) < self.header()
    }

    #[must_use]
    pub fn tab_at(&self, x: f64, count: usize, width: u32) -> Option<usize> {
        Some(x.max(0.0) as usize / self.tab_span(count, width) as usize)
            .filter(|&index| index < count)
    }

    #[must_use]
    pub fn metrics(&self) -> Metrics {
        self.fonts.metrics()
    }

    pub fn rescale(&mut self, font_size: f32, padding: i32) {
        self.fonts.set_size(font_size);
        self.padding = padding;
        self.forget_glyphs();
    }

    #[must_use]
    pub fn grid_size(&self, width: u32, height: u32) -> (usize, usize) {
        let metrics = self.fonts.metrics();
        let cells = |pixels: i32, cell: u32| {
            ((pixels - 2 * self.padding).max(0) as u32 / cell).max(1) as usize
        };
        (
            cells(height as i32 - self.header(), metrics.height),
            cells(width as i32, metrics.width),
        )
    }

    #[must_use]
    pub fn cell_at(&self, x: f64, y: f64) -> (usize, usize) {
        let metrics = self.fonts.metrics();
        let cell = |pixels: f64, origin: i32, cell: u32| {
            ((pixels as i32 - origin).max(0) as u32 / cell) as usize
        };
        (
            cell(y, self.top(), metrics.height),
            cell(x, self.padding, metrics.width),
        )
    }

    fn forget_glyphs(&mut self) {
        self.ascii = [[None; 128]; 4];
        self.glyphs.clear();
        self.patterns = [None; 3];
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
        Some(Glyph {
            origin: self.place(gpu, [raster.width, raster.height], &raster.pixels)?,
            size: [raster.width as u16, raster.height as u16],
            left: raster.left as i16,
            top: raster.top as i16,
            advance: raster.advance.round() as i16,
            colored: raster.colored,
        })
    }

    fn place(&mut self, gpu: &Gpu, size: [u32; 2], pixels: &[u8]) -> Option<[u16; 2]> {
        if self.pen[0] + size[0] > ATLAS_SIZE {
            self.pen = [0, self.pen[1] + self.shelf];
            self.shelf = 0;
        }
        if self.pen[1] + size[1] > ATLAS_SIZE {
            return None;
        }
        gpu.upload(self.pen, size, pixels);
        let origin = [self.pen[0] as u16, self.pen[1] as u16];
        self.pen[0] += size[0];
        self.shelf = self.shelf.max(size[1]);
        Some(origin)
    }

    fn pattern(&mut self, gpu: &Gpu, pattern: Pattern) -> Option<Glyph> {
        if let Some(cached) = self.patterns[pattern as usize] {
            return Some(cached);
        }
        let metrics = self.fonts.metrics();
        let (top, height, thickness) = band(pattern, metrics);
        let size = [metrics.width, height];
        let glyph = if size.iter().all(|side| (1..=ATLAS_SIZE).contains(side)) {
            let pixels: Vec<u8> = mask(pattern, size, thickness)
                .into_iter()
                .flat_map(|alpha| [alpha; 4])
                .collect();
            Glyph {
                origin: self.place(gpu, size, &pixels)?,
                size: [size[0] as u16, size[1] as u16],
                top: top as i16,
                ..Glyph::default()
            }
        } else {
            Glyph::default()
        };
        self.patterns[pattern as usize] = Some(glyph);
        Some(glyph)
    }

    #[must_use]
    pub fn quads(&self) -> &[Quad] {
        &self.quads
    }

    #[must_use]
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    fn compose_pictures(&mut self) {
        let mut pictures: Vec<&(Arc<Picture>, Quad)> =
            self.rows.iter().flat_map(|row| &row.pictures).collect();
        if pictures.is_empty() {
            return;
        }
        pictures.sort_by_key(|(picture, _)| picture.serial);
        self.layers.push(Layer {
            picture: None,
            count: self.quads.len() as u32,
        });
        for (picture, quad) in pictures {
            match self.layers.last_mut() {
                Some(Layer {
                    picture: Some(last),
                    count,
                }) if Arc::ptr_eq(last, picture) => *count += 1,
                _ => self.layers.push(Layer {
                    picture: Some(Arc::clone(picture)),
                    count: 1,
                }),
            }
            self.quads.push(*quad);
        }
    }

    #[must_use]
    pub fn cell_origin(&self, line: usize, column: usize) -> (i32, i32) {
        (
            self.padding + column as i32 * self.fonts.metrics().width as i32,
            self.top() + line as i32 * self.fonts.metrics().height as i32,
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
        self.layers.clear();
        for row in &self.rows {
            self.quads.extend_from_slice(&row.backgrounds);
        }
        for row in &self.rows {
            self.quads.extend_from_slice(&row.glyphs);
        }
        self.compose_pictures();
        let first = term.grid().history() - term.display_offset();
        for span in overlay.matches[overlay
            .matches
            .partition_point(|span| span.end.line < first)..]
            .iter()
            .take_while(|span| span.start.line < first + term.lines())
        {
            self.highlight(term, *span, term.palette().indexed(3), MATCH_ALPHA);
        }
        if let Some(span) = overlay.current {
            self.highlight(term, span, term.palette().indexed(3), CURRENT_ALPHA);
        }
        match overlay.selection {
            Some(Selection::Stream(span)) => {
                self.highlight(term, span, term.palette().foreground, SELECTION_ALPHA);
            }
            Some(Selection::Block(span)) => {
                self.highlight_block(term, span, term.palette().foreground, SELECTION_ALPHA);
            }
            None => {}
        }
        if term.display_offset() > 0 {
            let metrics = self.fonts.metrics();
            let (offset, length) = thumb(
                (term.lines() as u32 * metrics.height) as i32,
                metrics.height as i32,
                term.lines(),
                term.grid().history(),
                first,
            );
            self.quads.push(solid(
                overlay.width as i32 - 2 * metrics.stroke as i32,
                self.top() + offset,
                2 * metrics.stroke as i32,
                length,
                term.palette().foreground,
                SCROLL_ALPHA,
            ));
        }
        if overlay.caret && overlay.preedit.is_empty() {
            self.compose_cursor(gpu, term, overlay.focused);
        }
        if let Some(text) = overlay.banner {
            self.quads.push(solid(
                self.padding,
                self.top() + (term.lines() - 1) as i32 * self.fonts.metrics().height as i32,
                term.cols() as i32 * self.fonts.metrics().width as i32,
                self.fonts.metrics().height as i32,
                term.palette().foreground,
                u8::MAX,
            ));
            self.label(
                gpu,
                term,
                &Label {
                    line: term.lines() - 1,
                    column: 0,
                    text: tail(text, term.cols()),
                    ink: term.palette().background,
                    paper: term.palette().foreground,
                    underlined: false,
                },
            );
        } else if !overlay.preedit.is_empty() {
            self.label(
                gpu,
                term,
                &Label {
                    line: term.cursor().line.min(term.lines() - 1),
                    column: term.cursor().column,
                    text: overlay.preedit,
                    ink: term.palette().foreground,
                    paper: term.palette().background,
                    underlined: true,
                },
            );
        }
        if self.bar {
            self.compose_tabs(gpu, term.palette(), overlay);
        }
        self.layers.push(Layer {
            picture: None,
            count: self.quads.len() as u32
                - self.layers.iter().map(|layer| layer.count).sum::<u32>(),
        });
    }

    fn compose_tabs(&mut self, gpu: &Gpu, palette: &Palette, overlay: &Overlay<'_>) {
        let metrics = self.fonts.metrics();
        let (width, stroke) = (metrics.width as i32, metrics.stroke as i32);
        let header = self.header();
        let span = self.tab_span(overlay.tabs.len(), overlay.width);
        let strip = tint(palette.background, palette.foreground);
        self.quads
            .push(solid(0, 0, overlay.width as i32, header, strip, u8::MAX));
        for (index, title) in overlay.tabs.iter().enumerate() {
            let left = index as i32 * span;
            let ink = if index == overlay.active {
                self.quads
                    .push(solid(left, 0, span, header, palette.background, u8::MAX));
                palette.foreground
            } else {
                blend(palette.foreground, strip)
            };
            self.quads.push(solid(
                left + span - stroke,
                0,
                stroke,
                header,
                palette.background,
                u8::MAX,
            ));
            let mut pen = left + width;
            for c in title.chars() {
                let advance = c.width().unwrap_or(0) as i32 * width;
                if pen + advance > left + span - width {
                    break;
                }
                if advance > 0
                    && let Some(glyph) = self.glyph(gpu, c, REGULAR)
                    && glyph.size[0] > 0
                {
                    self.quads.push(textured(
                        pen + centering(advance, glyph) + i32::from(glyph.left),
                        (header - metrics.height as i32) / 2 + metrics.ascent
                            - i32::from(glyph.top),
                        glyph,
                        ink,
                    ));
                }
                pen += advance;
            }
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
        let top = self.top() + line as i32 * height;
        let reverse = term.modes().contains(Modes::REVERSE_VIDEO);
        let source = term.visible_row(line);
        let mut row = std::mem::take(&mut self.rows[line]);
        row.backgrounds.clear();
        row.glyphs.clear();
        row.pictures.clear();
        row.pictures.extend(source.tiles().iter().map(|tile| {
            (
                Arc::clone(&tile.picture),
                pictured(
                    tile,
                    [self.padding + i32::from(tile.column) * width, top],
                    [width, height],
                ),
            )
        }));
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
                if let Some(pattern) = pattern_of(look.attrs) {
                    let Some(glyph) = self.pattern(gpu, pattern) else {
                        complete = false;
                        break 'cells;
                    };
                    if glyph.size[0] > 0 {
                        row.glyphs.push(textured(
                            left,
                            top + i32::from(glyph.top),
                            glyph,
                            look.underline,
                        ));
                    }
                } else {
                    row.glyphs.push(solid(
                        left,
                        top + metrics.underline,
                        width,
                        metrics.stroke as i32,
                        look.underline,
                        u8::MAX,
                    ));
                }
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
        if source.is_failed() {
            row.backgrounds.push(solid(
                (self.padding - 2 * metrics.stroke as i32).max(0),
                top,
                2 * metrics.stroke as i32,
                height,
                term.palette().indexed(9),
                u8::MAX,
            ));
        }
        self.rows[line] = row;
        complete
    }

    fn highlight(&mut self, term: &Term, span: Span, color: Rgb, alpha: u8) {
        let metrics = self.fonts.metrics();
        let (width, height) = (metrics.width as i32, metrics.height as i32);
        let first = term.grid().history() - term.display_offset();
        for line in span.start.line.max(first)..=span.end.line.min(first + term.lines() - 1) {
            let start = if line == span.start.line {
                span.start.column
            } else {
                0
            };
            let end = if line == span.end.line {
                (span.end.column
                    + 1
                    + usize::from(
                        term.grid()
                            .absolute(line)
                            .cells()
                            .get(span.end.column)
                            .is_some_and(|cell| cell.is_wide()),
                    ))
                .min(term.cols())
            } else {
                term.cols()
            };
            if start < end {
                self.quads.push(solid(
                    self.padding + start as i32 * width,
                    self.top() + (line - first) as i32 * height,
                    (end - start) as i32 * width,
                    height,
                    color,
                    alpha,
                ));
            }
        }
    }

    fn highlight_block(&mut self, term: &Term, span: Span, color: Rgb, alpha: u8) {
        let metrics = self.fonts.metrics();
        let (width, height) = (metrics.width as i32, metrics.height as i32);
        let first = term.grid().history() - term.display_offset();
        for line in span.start.line.max(first)..=span.end.line.min(first + term.lines() - 1) {
            let columns = term.block_columns(line, span);
            if columns.start < columns.end {
                self.quads.push(solid(
                    self.padding + columns.start as i32 * width,
                    self.top() + (line - first) as i32 * height,
                    columns.len() as i32 * width,
                    height,
                    color,
                    alpha,
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
            self.top() + cursor.line as i32 * height,
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

    fn label(&mut self, gpu: &Gpu, term: &Term, label: &Label<'_>) {
        let metrics = self.fonts.metrics();
        let (width, height) = (metrics.width as i32, metrics.height as i32);
        let top = self.top() + label.line as i32 * height;
        let mut column = label.column;
        for c in label.text.chars() {
            let cells = c.width().unwrap_or(0);
            if cells == 0 || column + cells > term.cols() {
                continue;
            }
            let left = self.padding + column as i32 * width;
            let span = cells as i32 * width;
            self.quads
                .push(solid(left, top, span, height, label.paper, u8::MAX));
            if let Some(glyph) = self.glyph(gpu, c, REGULAR)
                && glyph.size[0] > 0
            {
                self.quads.push(textured(
                    left + centering(span, glyph) + i32::from(glyph.left),
                    top + metrics.ascent - i32::from(glyph.top),
                    glyph,
                    label.ink,
                ));
            }
            if label.underlined {
                self.quads.push(solid(
                    left,
                    top + metrics.underline,
                    span,
                    metrics.stroke as i32,
                    label.ink,
                    u8::MAX,
                ));
            }
            column += cells;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Metrics, Pattern, band, mask, thumb};

    fn metrics(width: u32, height: u32, underline: i32, stroke: u32) -> Metrics {
        Metrics {
            width,
            height,
            ascent: underline - 2,
            underline,
            strikeout: underline / 2,
            stroke,
        }
    }

    fn column(pixels: &[u8], width: usize, x: usize) -> Vec<u8> {
        pixels.iter().skip(x).step_by(width).copied().collect()
    }

    #[test]
    fn curly_band_stays_inside_a_short_cell() {
        let (top, size, _) = band(Pattern::Curly, metrics(6, 7, 7, 1));
        assert!(top >= 0 && top as u32 + size <= 7);
    }

    #[test]
    fn curly_band_starts_at_the_underline_when_it_fits() {
        assert_eq!(band(Pattern::Curly, metrics(10, 20, 15, 1)), (15, 3, 1));
    }

    #[test]
    fn dotted_band_is_one_stroke_tall() {
        assert_eq!(band(Pattern::Dotted, metrics(10, 20, 15, 2)), (15, 2, 2));
    }

    #[test]
    fn curly_mask_matches_at_both_cell_edges() {
        let pixels = mask(Pattern::Curly, [10, 4], 1);
        assert_eq!(column(&pixels, 10, 0), column(&pixels, 10, 9));
    }

    #[test]
    fn curly_mask_reaches_the_bottom_in_the_middle_of_the_cell() {
        let pixels = mask(Pattern::Curly, [10, 4], 1);
        assert!(pixels[3 * 10 + 4] > 0 && pixels[4] == 0);
    }

    #[test]
    fn dotted_mask_alternates_across_the_cell_boundary() {
        let pixels = mask(Pattern::Dotted, [10, 1], 1);
        assert_eq!(pixels, [255, 0, 255, 0, 255, 0, 255, 0, 255, 0]);
    }

    #[test]
    fn dashed_mask_leaves_a_gap_on_both_edges() {
        let pixels = mask(Pattern::Dashed, [9, 1], 1);
        assert_eq!(pixels, [0, 255, 255, 255, 255, 255, 255, 0, 0]);
    }

    #[test]
    fn thumb_starts_at_the_top_on_the_oldest_row() {
        assert_eq!(thumb(240, 10, 24, 76, 0), (0, 57));
    }

    #[test]
    fn thumb_moves_down_with_the_first_visible_row() {
        assert_eq!(thumb(240, 10, 24, 76, 38), (91, 57));
    }

    #[test]
    fn thumb_touches_the_bottom_on_the_newest_row() {
        assert_eq!(thumb(240, 10, 24, 76, 76), (183, 57));
    }

    #[test]
    fn thumb_is_at_least_one_cell_long() {
        assert_eq!(thumb(240, 10, 24, 100_000, 50_000), (115, 10));
    }

    #[test]
    fn thumb_fills_the_track_without_history() {
        assert_eq!(thumb(240, 10, 24, 0, 0), (0, 240));
    }
}
