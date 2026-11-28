use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use dartel_term::{
    Attrs, Cell, Color, CursorShape, Modes, PLACEHOLDER, Palette, Picture, Rgb, Selection, Span,
    Style, Term, Tile,
};
use unicode_width::UnicodeWidthChar;

use crate::font::{Fonts, Library, Ligature, Metrics, REGULAR, Raster};
use crate::gpu::{ATLAS_SIZE, Gpu, Layer, Quad};

const SELECTION_ALPHA: u8 = 80;
const MATCH_ALPHA: u8 = 90;
const CURRENT_ALPHA: u8 = 170;
const SCROLL_ALPHA: u8 = 128;
const TAB_CELLS: u32 = 24;
const JOINER: char = '\u{200D}';
const DOUBLE: u8 = 3;
const QUADRANTS: [u8; 10] = [4, 8, 1, 13, 9, 7, 11, 2, 6, 14];
const ARMS: &str = concat!(
    "0101020210102020",
    "0101020210102020",
    "0101020210102020",
    "0110021001200220",
    "0011001200210022",
    "1100120021002200",
    "1001100220012002",
    "1110121021101120",
    "2120221012202220",
    "1011101220111021",
    "2021201210222022",
    "0111011202110212",
    "0121012202210222",
    "1101110212011202",
    "2101210222012202",
    "1111111212111212",
    "2111112121212112",
    "2211112212212212",
    "1222212222212222",
    "0101020210102020",
    "0303303003100130",
    "0330001300310033",
    "1300310033001003",
    "3001300313103130",
    "3330101330313033",
    "0313013103331303",
    "3101330313133131",
    "3333011000111001",
    "1100000000000000",
    "0001100001000010",
    "0002200002000020",
    "0201102001022010",
);

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

struct Canvas {
    size: [u32; 2],
    pixels: Vec<u8>,
}

impl Canvas {
    fn new(size: [u32; 2]) -> Canvas {
        Canvas {
            size,
            pixels: vec![0; (size[0] * size[1]) as usize],
        }
    }

    fn set(&mut self, columns: Range<u32>, rows: Range<u32>, alpha: u8) {
        for y in rows {
            let row = (y * self.size[0]) as usize;
            self.pixels[row + columns.start as usize..row + columns.end as usize].fill(alpha);
        }
    }

    fn set_along(&mut self, axis: usize, along: Range<u32>, across: Range<u32>, alpha: u8) {
        if axis == 0 {
            self.set(along, across, alpha);
        } else {
            self.set(across, along, alpha);
        }
    }

    fn paint(&mut self, coverage: impl Fn(f32, f32) -> f32) {
        let width = self.size[0];
        for (index, pixel) in self.pixels.iter_mut().enumerate() {
            let (x, y) = (index as u32 % width, index as u32 / width);
            let alpha = (coverage(x as f32 + 0.5, y as f32 + 0.5).clamp(0.0, 1.0)
                * f32::from(u8::MAX))
            .round() as u8;
            *pixel = (*pixel).max(alpha);
        }
    }
}

struct Label<'a> {
    line: usize,
    column: usize,
    text: &'a str,
    ink: Rgb,
    paper: Rgb,
    underlined: bool,
}

struct Patch {
    origin: [u32; 2],
    size: [u32; 2],
    pixels: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default)]
struct Seen {
    joiner: bool,
    linker: bool,
}

pub struct Scene {
    fonts: Fonts,
    ascii: [[Option<Glyph>; 128]; 4],
    glyphs: HashMap<(char, usize), Glyph>,
    shaped: HashMap<(u16, usize), Glyph>,
    joins: Vec<Ligature>,
    chains: Vec<Range<usize>>,
    seen: Seen,
    text: String,
    patterns: [Option<Glyph>; 3],
    pen: [u32; 2],
    shelf: u32,
    patches: Vec<Patch>,
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
        background: (inverse || style.background != Color::DEFAULT).then_some(paper),
        underline: palette.resolve(style.underline, ink),
        attrs,
        face: face(style.attrs),
    }
}

fn face(attrs: Attrs) -> usize {
    usize::from(attrs.contains(Attrs::BOLD)) + 2 * usize::from(attrs.contains(Attrs::ITALIC))
}

fn joinable(cell: Cell) -> bool {
    cell.ch().is_some_and(|c| !drawn(u32::from(c)))
        && !cell.is_wide()
        && !cell.is_spacer()
        && !cell.is_marked()
}

fn runs(cells: &[Cell]) -> impl Iterator<Item = (usize, &[Cell])> {
    cells
        .chunk_by(|a, b| a.style() == b.style() && joinable(*a) && joinable(*b))
        .scan(0, |column, run| {
            let start = *column;
            *column += run.len();
            Some((start, run))
        })
        .filter(|(_, run)| run.len() > 1)
}

fn swap(glyphs: &mut Vec<Quad>, from: usize, nominal: Option<Quad>, joined: Option<Quad>) -> usize {
    let found = nominal
        .and_then(|nominal| glyphs[from..].iter().position(|quad| *quad == nominal))
        .map(|position| from + position);
    match (found, joined) {
        (Some(position), Some(quad)) => {
            glyphs[position] = quad;
            position + 1
        }
        (Some(position), None) => {
            glyphs.remove(position);
            position
        }
        (None, Some(quad)) => {
            glyphs.insert(from, quad);
            from + 1
        }
        (None, None) => from,
    }
}

fn linking(c: char) -> bool {
    matches!(c, '\u{1F3FB}'..='\u{1F3FF}' | '\u{1F1E6}'..='\u{1F1FF}')
}

fn candidates(cells: &[Cell]) -> bool {
    cells.iter().fold(false, |found, cell| {
        found | (0x1F1E6..=0x1F3FF).contains(&cell.code())
    })
}

fn characters(marks: &[(u16, char)], column: usize, cell: Cell) -> impl Iterator<Item = char> {
    cell.ch().into_iter().chain(
        marks
            .iter()
            .filter(move |&&(mark, _)| cell.is_marked() && usize::from(mark) == column)
            .map(|&(_, c)| c),
    )
}

fn after(cells: &[Cell], column: usize) -> usize {
    column + 1 + usize::from(cells[column].is_wide())
}

fn linked(marks: &[(u16, char)], cells: &[Cell], first: usize, last: usize, next: usize) -> bool {
    let (Some(previous), Some(c)) = (cells[last].ch(), cells[next].ch()) else {
        return false;
    };
    (cells[last].is_marked()
        && marks
            .iter()
            .rfind(|&&(mark, _)| usize::from(mark) == last)
            .is_some_and(|&(_, mark)| mark == JOINER))
        || matches!(c, '\u{1F3FB}'..='\u{1F3FF}')
        || (first == last
            && matches!(previous, '\u{1F1E6}'..='\u{1F1FF}')
            && matches!(c, '\u{1F1E6}'..='\u{1F1FF}'))
}

fn sequences(marks: &[(u16, char)], cells: &[Cell], found: &mut Vec<Range<usize>>) {
    let mut column = 0;
    while column < cells.len() {
        let mut last = column;
        let mut end = after(cells, column);
        while end < cells.len() && linked(marks, cells, column, last, end) {
            last = end;
            end = after(cells, end);
        }
        if last > column {
            found.push(column..end.min(cells.len()));
        }
        column = end;
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

fn rgba(alpha: Vec<u8>) -> Vec<u8> {
    alpha.into_iter().flat_map(|alpha| [alpha; 4]).collect()
}

fn strokes(size: [u32; 2], stroke: u32) -> [u32; 3] {
    let room = size[0].min(size[1]).max(1);
    let light = stroke.clamp(1, room);
    let heavy = (2 * light).max(size[0] / 5).min(room);
    let gap = light.max(size[0] / 8).min(room.saturating_sub(2 * light));
    [light, heavy, gap]
}

fn lines(weight: u8, span: u32, [light, heavy, gap]: [u32; 3]) -> Vec<Range<u32>> {
    let line = |start: u32, width: u32| start.min(span)..(start + width).min(span);
    match weight {
        0 => Vec::new(),
        DOUBLE => {
            let start = span.saturating_sub(2 * light + gap) / 2;
            vec![line(start, light), line(start + light + gap, light)]
        }
        _ => {
            let width = if weight == 1 { light } else { heavy };
            vec![line(span.saturating_sub(width) / 2, width)]
        }
    }
}

fn arms(code: u32) -> Option<[u8; 4]> {
    let index = code.checked_sub(0x2500)? as usize;
    let digits = ARMS.as_bytes().get(4 * index..4 * index + 4)?;
    Some(std::array::from_fn(|arm| digits[arm] - b'0'))
}

fn reach(perpendicular: &[Range<u32>], positive: bool, near: bool) -> u32 {
    let line = if near == positive {
        perpendicular.last()
    } else {
        perpendicular.first()
    };
    line.map_or(0, |line| if positive { line.start } else { line.end })
}

fn draw_arms(canvas: &mut Canvas, axis: usize, own: [u8; 2], sides: [u8; 2], strokes: [u32; 3]) {
    let side = sides[0].max(sides[1]);
    let perpendicular = lines(
        if side == 0 { own[0].max(own[1]) } else { side },
        canvas.size[axis],
        strokes,
    );
    for (direction, weight) in own.into_iter().enumerate() {
        let positive = direction == 1;
        let stops = own[1 - direction] == 0 && sides.iter().all(|&arm| arm != 0);
        for (index, across) in lines(weight, canvas.size[1 - axis], strokes)
            .into_iter()
            .enumerate()
        {
            let near = side == DOUBLE
                && if weight == DOUBLE {
                    sides[index] != 0
                } else {
                    stops
                };
            let edge = reach(&perpendicular, positive, near);
            let along = if positive {
                edge..canvas.size[axis]
            } else {
                0..edge
            };
            canvas.set_along(axis, along, across, u8::MAX);
        }
    }
}

fn dashes(code: u32) -> Option<(usize, u32)> {
    match code {
        0x2504..=0x250B => Some((
            ((code - 0x2504) / 2 % 2) as usize,
            if code < 0x2508 { 3 } else { 4 },
        )),
        0x254C..=0x254F => Some((((code - 0x254C) / 2) as usize, 2)),
        _ => None,
    }
}

fn draw_dashes(canvas: &mut Canvas, axis: usize, count: u32) {
    let (length, breadth) = (canvas.size[axis], canvas.size[1 - axis]);
    for segment in 0..count {
        let (start, end) = (segment * length / count, (segment + 1) * length / count);
        let gap = (end - start) / 3;
        canvas.set_along(axis, start..start + gap / 2, 0..breadth, 0);
        canvas.set_along(axis, end - (gap - gap / 2)..end, 0..breadth, 0);
    }
}

fn draw_arc(canvas: &mut Canvas, arms: [u8; 4], strokes: [u32; 3]) {
    let [width, height] = canvas.size;
    let column = lines(1, width, strokes).remove(0);
    let row = lines(1, height, strokes).remove(0);
    let half = strokes[0] as f32 / 2.0;
    let center = [column.start as f32 + half, row.start as f32 + half];
    let sign = [arms[1], arms[2]].map(|arm| if arm == 0 { -1.0 } else { 1.0 });
    let room = |axis: usize, span: u32| {
        if sign[axis] > 0.0 {
            span as f32 - center[axis]
        } else {
            center[axis]
        }
    };
    let radius = (room(0, width).min(room(1, height)) - 1.0).max(0.0);
    let corner = [center[0] + sign[0] * radius, center[1] + sign[1] * radius];
    let inside = |value: f32, range: &Range<u32>| {
        if (range.start as f32..range.end as f32).contains(&value) {
            1.0
        } else {
            0.0
        }
    };
    canvas.paint(|x, y| {
        let (dx, dy) = ((x - corner[0]) * sign[0], (y - corner[1]) * sign[1]);
        if dy > 0.0 {
            inside(x, &column)
        } else if dx > 0.0 {
            inside(y, &row)
        } else {
            half + 0.5 - (dx.hypot(dy) - radius).abs()
        }
    });
}

fn draw_diagonals(canvas: &mut Canvas, code: u32, light: u32) {
    let [width, height] = canvas.size.map(|side| side as f32);
    let length = width.hypot(height);
    let half = light as f32 / 2.0 + 0.5;
    canvas.paint(|x, y| {
        let rising = (height * x + width * y - width * height).abs() / length;
        let falling = (height * x - width * y).abs() / length;
        half - match code {
            0x2571 => rising,
            0x2572 => falling,
            _ => rising.min(falling),
        }
    });
}

fn draw_eighths(canvas: &mut Canvas, [left, right, top, bottom]: [u32; 4], alpha: u8) {
    let edges = |from: u32, to: u32, span: u32| {
        let start = from * span / 8;
        start..(to * span / 8).max(start + 1).min(span)
    };
    let (columns, rows) = (
        edges(left, right, canvas.size[0]),
        edges(top, bottom, canvas.size[1]),
    );
    canvas.set(columns, rows, alpha);
}

fn draw_block(canvas: &mut Canvas, code: u32) {
    match code {
        0x2580 => draw_eighths(canvas, [0, 8, 0, 4], u8::MAX),
        0x2581..=0x2588 => draw_eighths(canvas, [0, 8, 0x2588 - code, 8], u8::MAX),
        0x2589..=0x258F => draw_eighths(canvas, [0, 0x2590 - code, 0, 8], u8::MAX),
        0x2590 => draw_eighths(canvas, [4, 8, 0, 8], u8::MAX),
        0x2591..=0x2593 => {
            draw_eighths(canvas, [0, 8, 0, 8], ((code - 0x2590) * 255 / 4) as u8);
        }
        0x2594 => draw_eighths(canvas, [0, 8, 0, 1], u8::MAX),
        0x2595 => draw_eighths(canvas, [7, 8, 0, 8], u8::MAX),
        _ => {
            let quadrants = QUADRANTS[(code - 0x2596) as usize];
            for quadrant in (0..4).filter(|quadrant| quadrants >> quadrant & 1 != 0) {
                let (x, y) = (4 * (quadrant % 2), 4 * (quadrant / 2));
                draw_eighths(canvas, [x, x + 4, y, y + 4], u8::MAX);
            }
        }
    }
}

fn draw_powerline(canvas: &mut Canvas, code: u32, light: u32) {
    let [width, height] = canvas.size.map(|side| side as f32);
    let length = width.hypot(height / 2.0);
    let half = light as f32 / 2.0 + 0.5;
    canvas.paint(|x, y| {
        let x = if code >= 0xE0B2 { width - x } else { x };
        let upper = (width * y - height / 2.0 * x) / length;
        let lower = (width * (height - y) - height / 2.0 * x) / length;
        if code.is_multiple_of(2) {
            upper.min(lower) + 0.5
        } else {
            half - upper.abs().min(lower.abs())
        }
    });
}

fn drawn(code: u32) -> bool {
    matches!(code, 0x2500..=0x259F | 0xE0B0..=0xE0B3)
}

fn drawing(c: char, size: [u32; 2], stroke: u32) -> Option<Vec<u8>> {
    let code = u32::from(c);
    if !drawn(code) {
        return None;
    }
    let strokes = strokes(size, stroke);
    let mut canvas = Canvas::new(size);
    match code {
        0x2571..=0x2573 => draw_diagonals(&mut canvas, code, strokes[0]),
        0x256D..=0x2570 => draw_arc(&mut canvas, arms(code)?, strokes),
        0x2500..=0x257F => {
            let [up, right, down, left] = arms(code)?;
            draw_arms(&mut canvas, 0, [left, right], [up, down], strokes);
            draw_arms(&mut canvas, 1, [up, down], [left, right], strokes);
            if let Some((axis, count)) = dashes(code) {
                draw_dashes(&mut canvas, axis, count);
            }
        }
        0x2580..=0x259F => draw_block(&mut canvas, code),
        _ => draw_powerline(&mut canvas, code, strokes[0]),
    }
    Some(canvas.pixels)
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
    pub fn new(
        library: Library,
        font_family: &str,
        font_fallback: &[String],
        font_size: f32,
        padding: i32,
    ) -> Option<Scene> {
        Some(Scene {
            fonts: Fonts::new(library, font_family, font_fallback, font_size)?,
            ascii: [[None; 128]; 4],
            glyphs: HashMap::new(),
            shaped: HashMap::new(),
            joins: Vec::new(),
            chains: Vec::new(),
            seen: Seen::default(),
            text: String::new(),
            patterns: [None; 3],
            pen: [0; 2],
            shelf: 0,
            patches: Vec::new(),
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
        self.shaped.clear();
        self.seen = Seen::default();
        self.patterns = [None; 3];
        self.pen = [0; 2];
        self.shelf = 0;
        self.patches.clear();
        self.rows.clear();
        self.stale = true;
    }

    pub fn flush(&mut self, gpu: &Gpu) {
        for patch in self.patches.drain(..) {
            gpu.upload(patch.origin, patch.size, &patch.pixels);
        }
    }

    fn glyph(&mut self, c: char, face: usize) -> Option<Glyph> {
        if let Some(cached) = self.ascii[face]
            .get(c as usize)
            .map_or_else(|| self.glyphs.get(&(c, face)).copied(), |slot| *slot)
        {
            return Some(cached);
        }
        let glyph = self.rasterize(c, face)?;
        match self.ascii[face].get_mut(c as usize) {
            Some(slot) => *slot = Some(glyph),
            None => {
                self.glyphs.insert((c, face), glyph);
            }
        }
        Some(glyph)
    }

    fn cached(&self, c: char, face: usize) -> Option<Glyph> {
        self.ascii[face]
            .get(c as usize)
            .map_or_else(|| self.glyphs.get(&(c, face)).copied(), |slot| *slot)
    }

    fn shaped_glyph(&mut self, id: u16, face: usize) -> Option<Glyph> {
        if let Some(cached) = self.shaped.get(&(id, face)) {
            return Some(*cached);
        }
        let raster = self.fonts.rasterize_glyph(id, face);
        let glyph = self.store(raster)?;
        self.shaped.insert((id, face), glyph);
        Some(glyph)
    }

    fn rasterize(&mut self, c: char, face: usize) -> Option<Glyph> {
        self.seen.joiner |= c == JOINER;
        self.seen.linker |= linking(c);
        let metrics = self.fonts.metrics();
        let size = [metrics.width, metrics.height];
        if let Some(alpha) = drawing(c, size, metrics.stroke) {
            if !size.iter().all(|side| (1..=ATLAS_SIZE).contains(side)) {
                return Some(Glyph::default());
            }
            return Some(Glyph {
                origin: self.place(size, rgba(alpha))?,
                size: [size[0] as u16, size[1] as u16],
                top: metrics.ascent as i16,
                advance: size[0] as i16,
                ..Glyph::default()
            });
        }
        let raster = (c != PLACEHOLDER)
            .then(|| self.fonts.rasterize(c, face))
            .flatten();
        self.store(raster)
    }

    fn store(&mut self, raster: Option<Raster>) -> Option<Glyph> {
        let Some(raster) = raster.filter(|raster| {
            (1..=ATLAS_SIZE).contains(&raster.width) && (1..=ATLAS_SIZE).contains(&raster.height)
        }) else {
            return Some(Glyph::default());
        };
        Some(Glyph {
            origin: self.place([raster.width, raster.height], raster.pixels)?,
            size: [raster.width as u16, raster.height as u16],
            left: raster.left as i16,
            top: raster.top as i16,
            advance: raster.advance.round() as i16,
            colored: raster.colored,
        })
    }

    fn place(&mut self, size: [u32; 2], pixels: Vec<u8>) -> Option<[u16; 2]> {
        if self.pen[0] + size[0] > ATLAS_SIZE {
            self.pen = [0, self.pen[1] + self.shelf];
            self.shelf = 0;
        }
        if self.pen[1] + size[1] > ATLAS_SIZE {
            return None;
        }
        self.patches.push(Patch {
            origin: self.pen,
            size,
            pixels,
        });
        let origin = [self.pen[0] as u16, self.pen[1] as u16];
        self.pen[0] += size[0];
        self.shelf = self.shelf.max(size[1]);
        Some(origin)
    }

    fn pattern(&mut self, pattern: Pattern) -> Option<Glyph> {
        if let Some(cached) = self.patterns[pattern as usize] {
            return Some(cached);
        }
        let metrics = self.fonts.metrics();
        let (top, height, thickness) = band(pattern, metrics);
        let size = [metrics.width, height];
        let glyph = if size.iter().all(|side| (1..=ATLAS_SIZE).contains(side)) {
            Glyph {
                origin: self.place(size, rgba(mask(pattern, size, thickness)))?,
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

    fn shift(&mut self, term: &Term) {
        let shift = term.damage().shift();
        if shift == 0 || self.stale || term.damage().is_full() {
            return;
        }
        self.rows.rotate_left(shift);
        let rise = (shift as u32 * self.fonts.metrics().height) as i16;
        let kept = self.rows.len() - shift;
        for row in &mut self.rows[..kept] {
            for quad in row
                .backgrounds
                .iter_mut()
                .chain(&mut row.glyphs)
                .chain(row.pictures.iter_mut().map(|(_, quad)| quad))
            {
                quad.rect[1] -= rise;
            }
        }
    }

    pub fn compose(&mut self, term: &Term, overlay: &Overlay<'_>) {
        if self.rows.len() != term.lines() {
            self.rows.clear();
            self.rows.resize_with(term.lines(), Row::default);
            self.stale = true;
        }
        self.shift(term);
        if !(self.compose_rows(term)
            && (!self.fonts.ligatures() || self.ligate_rows(term))
            && (!(self.seen.joiner || self.seen.linker) || self.unite_rows(term)))
        {
            self.forget_glyphs();
            self.rows.resize_with(term.lines(), Row::default);
            self.compose_rows(term);
            self.ligate_rows(term);
            self.unite_rows(term);
        }
        if term.has_virtual_placements() {
            self.compose_placeholders(term);
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
            self.compose_cursor(term, overlay.focused);
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
            self.compose_tabs(term.palette(), overlay);
        }
        self.layers.push(Layer {
            picture: None,
            count: self.quads.len() as u32
                - self.layers.iter().map(|layer| layer.count).sum::<u32>(),
        });
    }

    fn compose_tabs(&mut self, palette: &Palette, overlay: &Overlay<'_>) {
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
                    && let Some(glyph) = self.glyph(c, REGULAR)
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

    #[cold]
    fn compose_placeholders(&mut self, term: &Term) {
        let metrics = self.fonts.metrics();
        let (width, height) = (metrics.width as i32, metrics.height as i32);
        let (padding, top) = (self.padding, self.top());
        for line in 0..term.lines() {
            if !(self.stale || term.damage().line(line)) {
                continue;
            }
            let origin = top + line as i32 * height;
            self.rows[line]
                .pictures
                .extend(term.placeholder_tiles(line).iter().map(|tile| {
                    (
                        Arc::clone(&tile.picture),
                        pictured(
                            tile,
                            [padding + i32::from(tile.column) * width, origin],
                            [width, height],
                        ),
                    )
                }));
        }
    }

    fn compose_rows(&mut self, term: &Term) -> bool {
        (0..term.lines())
            .all(|line| !(self.stale || term.damage().line(line)) || self.compose_row(term, line))
    }

    fn ligate_rows(&mut self, term: &Term) -> bool {
        !self.fonts.ligatures()
            || (0..term.lines())
                .all(|line| !(self.stale || term.damage().line(line)) || self.ligate(term, line))
    }

    fn ligate(&mut self, term: &Term, line: usize) -> bool {
        let source = term.visible_row(line);
        let cells = &source.cells()[..source.used().min(term.cols())];
        self.joins.clear();
        for (column, run) in runs(cells) {
            self.text.clear();
            self.text.extend(run.iter().filter_map(|cell| cell.ch()));
            self.joins.extend(
                self.fonts
                    .shape(&self.text, face(term.style(run[0].style()).attrs))
                    .iter()
                    .map(|ligature| Ligature {
                        start: column + ligature.start,
                        ..*ligature
                    }),
            );
        }
        if self.joins.is_empty() {
            return true;
        }
        let joins = std::mem::take(&mut self.joins);
        let mut row = std::mem::take(&mut self.rows[line]);
        let complete = self
            .join_glyphs(term, line, cells, &joins, &mut row)
            .is_some();
        self.rows[line] = row;
        self.joins = joins;
        complete
    }

    fn join_glyphs(
        &mut self,
        term: &Term,
        line: usize,
        cells: &[Cell],
        joins: &[Ligature],
        row: &mut Row,
    ) -> Option<()> {
        let metrics = self.fonts.metrics();
        let width = metrics.width as i32;
        let top = self.top() + line as i32 * metrics.height as i32;
        let reverse = term.modes().contains(Modes::REVERSE_VIDEO);
        let mut index = 0;
        for ligature in joins {
            let look = look(
                term.style(cells[ligature.start].style()),
                term.palette(),
                reverse,
            );
            let glyph = self.shaped_glyph(ligature.glyph, self.fonts.face(look.face))?;
            for offset in 0..ligature.cells {
                let left = self.padding + (ligature.start + offset) as i32 * width;
                let nominal = self.glyph(cells[ligature.start + offset].ch()?, look.face)?;
                index = swap(
                    &mut row.glyphs,
                    index,
                    (nominal.size[0] > 0).then(|| {
                        textured(
                            left + centering(width, nominal) + i32::from(nominal.left),
                            top + metrics.ascent - i32::from(nominal.top),
                            nominal,
                            look.foreground,
                        )
                    }),
                    (offset == 0 && glyph.size[0] > 0).then(|| {
                        textured(
                            left + centering(ligature.cells as i32 * width, glyph)
                                + i32::from(glyph.left),
                            top + metrics.ascent - i32::from(glyph.top),
                            glyph,
                            look.foreground,
                        )
                    }),
                );
            }
        }
        Some(())
    }

    fn unite_rows(&mut self, term: &Term) -> bool {
        if !(self.seen.joiner || self.seen.linker) {
            return true;
        }
        let Some(held) = (0..term.lines()).try_fold(false, |held, line| {
            if self.stale || term.damage().line(line) {
                self.unite(term, line).map(|found| held | found)
            } else {
                Some(held)
            }
        }) else {
            return false;
        };
        if !held && (self.stale || term.damage().is_full()) {
            self.seen = Seen::default();
            self.glyphs
                .retain(|&(c, _), _| !(c == JOINER || linking(c)));
        }
        true
    }

    fn unite(&mut self, term: &Term, line: usize) -> Option<bool> {
        let source = term.visible_row(line);
        let cells = &source.cells()[..source.used().min(term.cols())];
        let marks = source.mark_entries();
        if !(marks.iter().any(|&(_, mark)| mark == JOINER) || self.seen.linker && candidates(cells))
        {
            return Some(false);
        }
        self.chains.clear();
        sequences(marks, cells, &mut self.chains);
        if self.chains.is_empty() {
            return Some(true);
        }
        let chains = std::mem::take(&mut self.chains);
        let mut row = std::mem::take(&mut self.rows[line]);
        let complete = self.unite_glyphs(term, line, cells, &chains, &mut row);
        self.rows[line] = row;
        self.chains = chains;
        complete.map(|()| true)
    }

    fn unite_glyphs(
        &mut self,
        term: &Term,
        line: usize,
        cells: &[Cell],
        chains: &[Range<usize>],
        row: &mut Row,
    ) -> Option<()> {
        let metrics = self.fonts.metrics();
        let width = metrics.width as i32;
        let top = self.top() + line as i32 * metrics.height as i32;
        let reverse = term.modes().contains(Modes::REVERSE_VIDEO);
        let marks = term.visible_row(line).mark_entries();
        let mut index = 0;
        for chain in chains {
            self.text.clear();
            for column in chain.clone().filter(|&column| !cells[column].is_spacer()) {
                self.text.extend(characters(marks, column, cells[column]));
            }
            let first = look(
                term.style(cells[chain.start].style()),
                term.palette(),
                reverse,
            );
            let Some((face, id)) = self.fonts.unite(&self.text, first.face) else {
                continue;
            };
            let glyph = self.shaped_glyph(id, face)?;
            let fit = (i32::from(glyph.advance) + width / 2) / width;
            if glyph.size[0] == 0 || fit > chain.len() as i32 {
                continue;
            }
            let mut joined = Some(textured(
                self.padding
                    + chain.start as i32 * width
                    + centering(fit.max(1) * width, glyph)
                    + i32::from(glyph.left),
                top + metrics.ascent - i32::from(glyph.top),
                glyph,
                first.foreground,
            ));
            for column in chain.clone().filter(|&column| !cells[column].is_spacer()) {
                let cell = cells[column];
                let look = look(term.style(cell.style()), term.palette(), reverse);
                let span = if cell.is_wide() { 2 * width } else { width };
                let left = self.padding + column as i32 * width;
                let mut shift = None;
                for c in characters(marks, column, cell) {
                    let nominal = self.cached(c, look.face)?;
                    let shift = *shift.get_or_insert_with(|| centering(span, nominal));
                    index = swap(
                        &mut row.glyphs,
                        index,
                        (nominal.size[0] > 0).then(|| {
                            textured(
                                left + shift + i32::from(nominal.left),
                                top + metrics.ascent - i32::from(nominal.top),
                                nominal,
                                look.foreground,
                            )
                        }),
                        joined.take(),
                    );
                }
            }
        }
        Some(())
    }

    fn compose_row(&mut self, term: &Term, line: usize) -> bool {
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
        let padding = self.padding;
        let strip = |start: usize, end: usize, color: Rgb| {
            solid(
                padding + start as i32 * width,
                top,
                (end - start) as i32 * width,
                height,
                color,
                u8::MAX,
            )
        };
        let cells = &source.cells()[..source.used().min(term.cols())];
        let last = cells
            .last()
            .copied()
            .filter(|_| cells.len() == term.cols())
            .unwrap_or_default();
        let tail = look(term.style(last.style()), term.palette(), reverse);
        let body = if last.is_blank()
            && !tail.attrs.contains(Attrs::ANY_UNDERLINE)
            && !tail.attrs.contains(Attrs::STRIKETHROUGH)
        {
            cells
                .iter()
                .rposition(|cell| *cell != last)
                .map_or(0, |index| index + 1)
        } else {
            cells.len()
        };
        let mut current = (last.style(), tail);
        let mut fill: Option<(usize, Rgb)> = None;
        let mut complete = true;
        'cells: for (column, cell) in cells[..body].iter().enumerate() {
            if cell.style() != current.0 {
                current = (
                    cell.style(),
                    look(term.style(cell.style()), term.palette(), reverse),
                );
            }
            let look = current.1;
            let left = padding + column as i32 * width;
            if fill.map(|(_, color)| color) != look.background {
                if let Some((start, color)) = fill {
                    row.backgrounds.push(strip(start, column, color));
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
                    let Some(glyph) = self.glyph(c, look.face) else {
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
                    let Some(glyph) = self.pattern(pattern) else {
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
        if body < term.cols() && fill.map(|(_, color)| color) != tail.background {
            if let Some((start, color)) = fill {
                row.backgrounds.push(strip(start, body, color));
            }
            fill = tail.background.map(|color| (body, color));
        }
        if let Some((start, color)) = fill {
            row.backgrounds.push(strip(start, term.cols(), color));
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

    fn compose_cursor(&mut self, term: &Term, focused: bool) {
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

    fn label(&mut self, term: &Term, label: &Label<'_>) {
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
            if let Some(glyph) = self.glyph(c, REGULAR)
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
    use dartel_term::{Cell, Rgb, Term};
    use dartel_vt::Parser;

    use super::{
        ARMS, Metrics, Pattern, Quad, band, candidates, drawing, mask, runs, sequences, solid,
        strokes, swap, thumb,
    };

    fn spans(input: &str) -> Vec<(usize, usize)> {
        let mut term = Term::new(1, 20, 0);
        Parser::new().advance(&mut term, input.as_bytes());
        let row = term.visible_row(0);
        let cells: Vec<Cell> = row.cells()[..row.used()].to_vec();
        runs(&cells)
            .map(|(start, run)| (start, run.len()))
            .collect()
    }

    fn chains(input: &str) -> Vec<(usize, usize)> {
        let mut term = Term::new(1, 20, 0);
        Parser::new().advance(&mut term, input.as_bytes());
        let row = term.visible_row(0);
        let mut found = Vec::new();
        sequences(row.mark_entries(), &row.cells()[..row.used()], &mut found);
        found
            .into_iter()
            .map(|chain| (chain.start, chain.end))
            .collect()
    }

    fn candidate(input: &str) -> bool {
        let mut term = Term::new(1, 20, 0);
        Parser::new().advance(&mut term, input.as_bytes());
        let row = term.visible_row(0);
        candidates(&row.cells()[..row.used()])
    }

    #[test]
    fn regional_indicator_makes_a_line_a_candidate() {
        assert!(candidate("a\u{1F1EF}b"));
    }

    #[test]
    fn skin_tone_modifier_makes_a_line_a_candidate() {
        assert!(candidate("\u{1F44D}\u{1F3FD}"));
    }

    #[test]
    fn line_without_a_modifier_or_an_indicator_is_no_candidate() {
        assert!(!candidate("a\u{1F468}\u{4E2D}\u{1F1E5}\u{1F400}"));
    }

    #[test]
    fn joiners_link_every_member_of_a_family() {
        assert_eq!(
            chains("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}x"),
            [(0, 6)]
        );
    }

    #[test]
    fn joiner_after_a_variation_selector_continues_the_sequence() {
        assert_eq!(chains("\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}x"), [(0, 3)]);
    }

    #[test]
    fn skin_tone_modifier_joins_the_emoji_before_it() {
        assert_eq!(chains("\u{1F44D}\u{1F3FD}x"), [(0, 4)]);
    }

    #[test]
    fn joiner_on_a_skin_tone_modifier_continues_the_sequence() {
        assert_eq!(chains("\u{1F469}\u{1F3FD}\u{200D}\u{1F4BB}x"), [(0, 6)]);
    }

    #[test]
    fn two_regional_indicators_form_one_flag() {
        assert_eq!(chains("\u{1F1EF}\u{1F1F5}x"), [(0, 2)]);
    }

    #[test]
    fn third_regional_indicator_stays_alone() {
        assert_eq!(chains("\u{1F1EF}\u{1F1F5}\u{1F1FA}x"), [(0, 2)]);
    }

    #[test]
    fn four_regional_indicators_form_two_flags() {
        assert_eq!(
            chains("\u{1F1EF}\u{1F1F5}\u{1F1FA}\u{1F1F8}x"),
            [(0, 2), (2, 4)]
        );
    }

    #[test]
    fn emoji_without_a_joiner_stay_apart() {
        assert_eq!(chains("\u{1F468}\u{1F469}x"), [(0, 0); 0]);
    }

    #[test]
    fn variation_selector_alone_links_nothing() {
        assert_eq!(chains("\u{2764}\u{FE0F}x"), [(0, 0); 0]);
    }

    #[test]
    fn keycap_links_nothing() {
        assert_eq!(chains("1\u{FE0F}\u{20E3}x"), [(0, 0); 0]);
    }

    #[test]
    fn overwritten_member_ends_the_sequence_after_the_joiner() {
        assert_eq!(
            chains("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\x1b[1;3HX"),
            [(0, 3)]
        );
    }

    fn quad(x: i32) -> Quad {
        solid(x, 0, 1, 1, Rgb::new(1, 2, 3), u8::MAX)
    }

    #[test]
    fn run_covers_a_line_of_one_style() {
        assert_eq!(spans("a->b"), [(0, 4)]);
    }

    #[test]
    fn style_change_splits_a_run() {
        assert_eq!(spans("a-\x1b[31m>b"), [(0, 2), (2, 2)]);
    }

    #[test]
    fn wide_character_ends_a_run() {
        assert_eq!(spans("ab\u{65E5}cd"), [(0, 2), (4, 2)]);
    }

    #[test]
    fn cell_with_a_combining_mark_ends_a_run() {
        assert_eq!(spans("abc\u{0301}de"), [(0, 2), (3, 2)]);
    }

    #[test]
    fn drawn_box_character_ends_a_run() {
        assert_eq!(spans("ab\u{2500}cd"), [(0, 2), (3, 2)]);
    }

    #[test]
    fn default_cell_ends_a_run() {
        assert_eq!(spans("ab\x1b[3Ccd"), [(0, 2), (5, 2)]);
    }

    #[test]
    fn single_cell_is_not_a_run() {
        assert_eq!(spans("a\x1b[31mb\x1b[32mcd"), [(2, 2)]);
    }

    #[test]
    fn swap_replaces_the_nominal_quad_with_the_ligature() {
        let mut glyphs = vec![quad(0), quad(1), quad(2)];
        assert_eq!(swap(&mut glyphs, 0, Some(quad(1)), Some(quad(9))), 2);
        assert_eq!(glyphs, [quad(0), quad(9), quad(2)]);
    }

    #[test]
    fn swap_removes_the_nominal_quad_of_a_covered_cell() {
        let mut glyphs = vec![quad(0), quad(1), quad(2)];
        assert_eq!(swap(&mut glyphs, 0, Some(quad(1)), None), 1);
        assert_eq!(glyphs, [quad(0), quad(2)]);
    }

    #[test]
    fn swap_inserts_the_ligature_where_the_cell_drew_nothing() {
        let mut glyphs = vec![quad(0), quad(2)];
        assert_eq!(swap(&mut glyphs, 1, None, Some(quad(9))), 2);
        assert_eq!(glyphs, [quad(0), quad(9), quad(2)]);
    }

    #[test]
    fn swap_searches_only_after_the_previous_ligature() {
        let mut glyphs = vec![quad(1), quad(2), quad(1)];
        assert_eq!(swap(&mut glyphs, 1, Some(quad(1)), Some(quad(9))), 3);
        assert_eq!(glyphs, [quad(1), quad(2), quad(9)]);
    }

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

    fn draw(c: char, size: [u32; 2]) -> Vec<u8> {
        drawing(c, size, 1).unwrap()
    }

    fn row(pixels: &[u8], width: usize, y: usize) -> Vec<u8> {
        pixels[y * width..(y + 1) * width].to_vec()
    }

    fn edges(pixels: &[u8], [width, height]: [u32; 2]) -> [Vec<u8>; 4] {
        let (width, height) = (width as usize, height as usize);
        [
            row(pixels, width, 0),
            column(pixels, width, width - 1),
            row(pixels, width, height - 1),
            column(pixels, width, 0),
        ]
    }

    #[test]
    fn arm_table_holds_four_weights_for_every_box_drawing_character() {
        assert!(ARMS.len() == 512 && ARMS.bytes().all(|digit| (b'0'..=b'3').contains(&digit)));
    }

    #[test]
    fn heavy_stroke_and_double_gap_grow_with_the_cell_width() {
        assert_eq!(strokes([30, 60], 1), [1, 6, 3]);
    }

    #[test]
    fn heavy_stroke_is_twice_the_light_stroke_in_a_small_cell() {
        assert_eq!(strokes([10, 20], 1), [1, 2, 1]);
    }

    #[test]
    fn drawn_mask_covers_one_cell() {
        assert_eq!(draw('\u{253C}', [10, 20]).len(), 200);
    }

    #[test]
    fn characters_outside_the_drawn_ranges_are_left_to_the_font() {
        assert!(drawing('A', [10, 20], 1).is_none() && drawing('\u{25A0}', [10, 20], 1).is_none());
    }

    #[test]
    fn light_horizontal_line_runs_across_the_vertical_center() {
        let pixels = draw('\u{2500}', [10, 20]);
        assert!(row(&pixels, 10, 9).iter().all(|&alpha| alpha == 255));
    }

    #[test]
    fn light_horizontal_line_is_one_stroke_tall() {
        let pixels = draw('\u{2500}', [10, 20]);
        assert_eq!(pixels.iter().filter(|&&alpha| alpha > 0).count(), 10);
    }

    #[test]
    fn light_vertical_line_runs_down_the_horizontal_center() {
        let pixels = draw('\u{2502}', [10, 20]);
        assert!(column(&pixels, 10, 4).iter().all(|&alpha| alpha == 255));
    }

    #[test]
    fn heavy_line_is_thicker_than_the_light_line() {
        let count = |c| {
            column(&draw(c, [10, 20]), 10, 0)
                .iter()
                .filter(|&&alpha| alpha > 0)
                .count()
        };
        assert!(count('\u{2501}') > count('\u{2500}'));
    }

    #[test]
    fn every_arm_meets_the_cell_edge_where_the_straight_line_of_its_weight_does() {
        let straight = [
            ['\u{2500}', '\u{2501}', '\u{2550}'],
            ['\u{2502}', '\u{2503}', '\u{2551}'],
        ];
        for (size, stroke) in [([10, 20], 1), ([7, 15], 1), ([24, 48], 3), ([5, 9], 1)] {
            for code in (0x2500..=0x257F)
                .filter(|code| !matches!(code, 0x2504..=0x250B | 0x254C..=0x254F | 0x2571..=0x2573))
            {
                let c = char::from_u32(code).unwrap();
                let found = edges(&drawing(c, size, stroke).unwrap(), size);
                let digits = &ARMS.as_bytes()[4 * (code as usize - 0x2500)..];
                for (edge, digit) in digits.iter().take(4).enumerate() {
                    let weight = usize::from(digit - b'0');
                    let expected = if weight == 0 {
                        vec![0; found[edge].len()]
                    } else {
                        let line = straight[1 - edge % 2][weight - 1];
                        edges(&drawing(line, size, stroke).unwrap(), size)[edge].clone()
                    };
                    assert_eq!(found[edge], expected, "{c} {size:?} edge {edge}");
                }
            }
        }
    }

    #[test]
    fn double_cross_leaves_the_middle_of_the_cell_empty() {
        assert_eq!(draw('\u{256C}', [10, 20])[9 * 10 + 4], 0);
    }

    #[test]
    fn double_corner_joins_the_outer_lines() {
        let pixels = draw('\u{2554}', [10, 20]);
        assert!(pixels[8 * 10 + 3] == 255 && pixels[10 * 10 + 5] == 255);
    }

    #[test]
    fn double_corner_leaves_the_gap_between_its_lines_open() {
        assert_eq!(draw('\u{2554}', [10, 20])[9 * 10 + 4], 0);
    }

    #[test]
    fn double_tee_keeps_its_through_line_unbroken() {
        let pixels = draw('\u{2566}', [10, 20]);
        assert!(row(&pixels, 10, 8).iter().all(|&alpha| alpha == 255));
    }

    #[test]
    fn single_stem_under_a_double_line_starts_at_the_lower_line() {
        let pixels = draw('\u{2564}', [10, 20]);
        assert!(pixels[9 * 10 + 4] == 0 && pixels[10 * 10 + 4] == 255);
    }

    #[test]
    fn quadruple_dash_leaves_one_gap_in_each_quarter() {
        let pixels = draw('\u{2508}', [16, 24]);
        assert_eq!(
            row(&pixels, 16, 11),
            [
                255, 255, 255, 0, 255, 255, 255, 0, 255, 255, 255, 0, 255, 255, 255, 0
            ]
        );
    }

    #[test]
    fn double_dash_breaks_a_vertical_line_along_its_rows() {
        let pixels = draw('\u{254E}', [10, 20]);
        let open = [0, 8, 9, 10, 18, 19];
        let expected: Vec<u8> = (0..20)
            .map(|y| if open.contains(&y) { 0 } else { 255 })
            .collect();
        assert_eq!(column(&pixels, 10, 4), expected);
    }

    #[test]
    fn rounded_corner_has_partial_coverage_on_its_curve() {
        assert!(
            draw('\u{256D}', [10, 20])
                .iter()
                .any(|&alpha| alpha > 0 && alpha < 255)
        );
    }

    #[test]
    fn rising_diagonal_touches_the_bottom_left_and_top_right_corners() {
        let pixels = draw('\u{2571}', [10, 20]);
        assert!(pixels[19 * 10] > 0 && pixels[9] > 0 && pixels[0] == 0);
    }

    #[test]
    fn diagonal_cross_is_symmetric_left_to_right() {
        let pixels = draw('\u{2573}', [10, 20]);
        let mirrored: Vec<u8> = pixels
            .chunks(10)
            .flat_map(|line| line.iter().rev().copied())
            .collect();
        assert_eq!(pixels, mirrored);
    }

    #[test]
    fn lower_half_block_fills_exactly_the_bottom_half() {
        let pixels = draw('\u{2584}', [10, 21]);
        assert!(pixels[..100].iter().all(|&alpha| alpha == 0));
        assert!(pixels[100..].iter().all(|&alpha| alpha == 255));
    }

    #[test]
    fn upper_and_lower_half_blocks_cover_the_cell_once() {
        let (upper, lower) = (draw('\u{2580}', [9, 21]), draw('\u{2584}', [9, 21]));
        assert!(
            upper
                .iter()
                .zip(&lower)
                .all(|(&a, &b)| u16::from(a) + u16::from(b) == 255)
        );
    }

    #[test]
    fn left_and_right_half_blocks_cover_the_cell_once() {
        let (left, right) = (draw('\u{258C}', [9, 21]), draw('\u{2590}', [9, 21]));
        assert!(
            left.iter()
                .zip(&right)
                .all(|(&a, &b)| u16::from(a) + u16::from(b) == 255)
        );
    }

    #[test]
    fn lower_eighths_grow_one_eighth_at_a_time() {
        let filled = |c| draw(c, [8, 16]).iter().filter(|&&alpha| alpha > 0).count();
        assert_eq!(
            ['\u{2581}', '\u{2582}', '\u{2587}', '\u{2588}'].map(filled),
            [16, 32, 112, 128]
        );
    }

    #[test]
    fn left_eighths_shrink_one_eighth_at_a_time() {
        let filled = |c| draw(c, [8, 16]).iter().filter(|&&alpha| alpha > 0).count();
        assert_eq!(
            ['\u{2589}', '\u{258A}', '\u{258F}'].map(filled),
            [112, 96, 16]
        );
    }

    #[test]
    fn thin_eighth_block_keeps_one_pixel_in_a_small_cell() {
        let pixels = draw('\u{2594}', [5, 6]);
        assert!(pixels[..5].iter().all(|&alpha| alpha == 255));
        assert!(pixels[5..].iter().all(|&alpha| alpha == 0));
    }

    #[test]
    fn right_eighth_block_sits_on_the_right_edge() {
        let pixels = draw('\u{2595}', [8, 16]);
        assert!(column(&pixels, 8, 7).iter().all(|&alpha| alpha == 255));
        assert_eq!(pixels.iter().filter(|&&alpha| alpha > 0).count(), 16);
    }

    #[test]
    fn quadrant_block_fills_the_upper_left_and_lower_right() {
        let pixels = draw('\u{259A}', [10, 20]);
        assert_eq!(
            [pixels[0], pixels[199], pixels[9], pixels[190]],
            [255, 255, 0, 0]
        );
    }

    #[test]
    fn three_quadrant_block_leaves_only_the_upper_right_empty() {
        let pixels = draw('\u{2599}', [10, 20]);
        assert_eq!(
            [pixels[0], pixels[199], pixels[9], pixels[190]],
            [255, 255, 0, 255]
        );
    }

    #[test]
    fn shades_cover_the_cell_at_a_quarter_a_half_and_three_quarters() {
        assert_eq!(
            ['\u{2591}', '\u{2592}', '\u{2593}'].map(|c| draw(c, [4, 8])),
            [vec![63; 32], vec![127; 32], vec![191; 32]]
        );
    }

    #[test]
    fn right_pointing_triangle_covers_the_left_edge() {
        let pixels = draw('\u{E0B0}', [10, 20]);
        assert!(column(&pixels, 10, 0).iter().all(|&alpha| alpha > 0));
    }

    #[test]
    fn right_pointing_triangle_leaves_the_right_corners_empty() {
        let pixels = draw('\u{E0B0}', [10, 20]);
        assert!(pixels[9] == 0 && pixels[199] == 0 && pixels[10 * 10 + 9] > 0);
    }

    #[test]
    fn left_pointing_triangle_mirrors_the_right_pointing_one() {
        let mirrored: Vec<u8> = draw('\u{E0B0}', [10, 20])
            .chunks(10)
            .flat_map(|line| line.iter().rev().copied())
            .collect();
        assert_eq!(draw('\u{E0B2}', [10, 20]), mirrored);
    }

    #[test]
    fn line_arrow_leaves_the_inside_of_the_triangle_empty() {
        let (solid, line) = (draw('\u{E0B0}', [10, 20]), draw('\u{E0B1}', [10, 20]));
        assert!(solid[10 * 10 + 2] == 255 && line[10 * 10 + 2] == 0);
    }

    #[test]
    fn left_line_arrow_mirrors_the_right_line_arrow() {
        let mirrored: Vec<u8> = draw('\u{E0B1}', [10, 20])
            .chunks(10)
            .flat_map(|line| line.iter().rev().copied())
            .collect();
        assert_eq!(draw('\u{E0B3}', [10, 20]), mirrored);
    }
}
