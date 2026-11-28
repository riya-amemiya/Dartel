use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Write;
use std::ops::Range;
use std::sync::{Arc, Weak};

use dartel_image::{Image, MAX_SIDE, base64, inflate, png, sixel};

use super::{Row, Term};

const QUOTA: usize = 256 << 20;
const MAX_UPLOAD: usize = MAX_SIDE as usize * MAX_SIDE as usize * 4;
const MAX_INFLATED: usize = 64 << 20;
const MAX_SIXEL: usize = 32 << 20;
const DEFAULT_CELL: [u16; 2] = [8, 16];

#[derive(Debug, PartialEq, Eq)]
pub struct Picture {
    pub serial: u64,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Tile {
    pub picture: Arc<Picture>,
    pub column: u16,
    pub left: u16,
    pub top: u16,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub cell: [u16; 2],
    pub(super) image: u32,
    pub(super) placement: u32,
    pub(super) fragile: bool,
}

impl Tile {
    #[must_use]
    pub fn columns(&self) -> Range<usize> {
        let first = usize::from(self.column);
        first
            ..first
                + (usize::from(self.left) + self.width as usize).div_ceil(usize::from(self.cell[0]))
    }

    pub(super) fn slice(&self, columns: Range<usize>) -> Tile {
        let skip = ((columns.start - usize::from(self.column)) * usize::from(self.cell[0])) as u32;
        Tile {
            column: columns.start as u16,
            x: self.x + skip,
            width: (self.width - skip).min((columns.len() * usize::from(self.cell[0])) as u32),
            ..self.clone()
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Command {
    action: u8,
    quiet: u32,
    format: u32,
    medium: u8,
    compression: u8,
    more: bool,
    width: u32,
    height: u32,
    id: u32,
    number: u32,
    placement: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    offset: [u32; 2],
    columns: u32,
    rows: u32,
    stay: bool,
    detached: bool,
    delete: u8,
    depth: i64,
}

impl Command {
    fn parse(control: &[u8]) -> Command {
        let mut command = Command {
            action: b't',
            format: 32,
            medium: b'd',
            delete: b'a',
            ..Command::default()
        };
        for pair in control.split(|&byte| byte == b',') {
            let [key, b'=', value @ ..] = pair else {
                continue;
            };
            let number = std::str::from_utf8(value)
                .ok()
                .and_then(|text| text.parse::<i64>().ok())
                .unwrap_or(0);
            let count = number.clamp(0, i64::from(u32::MAX)) as u32;
            let letter = value.first().copied().unwrap_or(0);
            match key {
                b'a' => command.action = letter,
                b'q' => command.quiet = count,
                b'f' => command.format = count,
                b't' => command.medium = letter,
                b'o' => command.compression = letter,
                b'm' => command.more = count == 1,
                b's' => command.width = count,
                b'v' => command.height = count,
                b'i' => command.id = count,
                b'I' => command.number = count,
                b'p' => command.placement = count,
                b'x' => command.x = count,
                b'y' => command.y = count,
                b'w' => command.w = count,
                b'h' => command.h = count,
                b'X' => command.offset[0] = count,
                b'Y' => command.offset[1] = count,
                b'c' => command.columns = count,
                b'r' => command.rows = count,
                b'C' => command.stay = count == 1,
                b'U' | b'P' => command.detached |= count != 0,
                b'd' => command.delete = letter,
                b'z' => command.depth = number,
                _ => {}
            }
        }
        command
    }
}

struct Spot {
    picture: Arc<Picture>,
    left: u32,
    top: u32,
    rows: u32,
    image: u32,
    placement: u32,
    fragile: bool,
}

#[derive(Clone, Debug)]
pub(super) struct Graphics {
    cell: [u16; 2],
    images: HashMap<u32, Arc<Image>>,
    order: VecDeque<u32>,
    numbers: HashMap<u32, u32>,
    upload: Option<(Command, Option<Vec<u8>>)>,
    sixel: Option<(bool, Vec<u8>)>,
    placed: Vec<Weak<Picture>>,
    serial: u64,
    assigned: u32,
}

impl Default for Graphics {
    fn default() -> Graphics {
        Graphics::new(DEFAULT_CELL)
    }
}

impl Graphics {
    pub(super) fn new(cell: [u16; 2]) -> Graphics {
        Graphics {
            cell,
            images: HashMap::new(),
            order: VecDeque::new(),
            numbers: HashMap::new(),
            upload: None,
            sixel: None,
            placed: Vec::new(),
            serial: 0,
            assigned: 0,
        }
    }

    pub(super) fn cell(&self) -> [u16; 2] {
        self.cell
    }

    fn assign(&mut self) -> u32 {
        loop {
            self.assigned = self
                .assigned
                .checked_sub(1)
                .filter(|&id| id != 0)
                .unwrap_or(u32::MAX);
            if !self.images.contains_key(&self.assigned) {
                return self.assigned;
            }
        }
    }

    fn trim_images(&mut self, quota: usize) {
        while self.order.len() > 1
            && self
                .images
                .values()
                .map(|image| image.pixels.len())
                .sum::<usize>()
                > quota
        {
            for oldest in self.order.drain(..1) {
                self.images.remove(&oldest);
                self.numbers.retain(|_, held| *held != oldest);
            }
        }
    }

    fn trim_pictures(&mut self, quota: usize) -> Vec<u64> {
        self.placed.retain(|held| held.strong_count() > 0);
        let mut dropped = Vec::new();
        while self.placed.len() > 1
            && self
                .placed
                .iter()
                .filter_map(Weak::upgrade)
                .map(|held| held.pixels.len())
                .sum::<usize>()
                > quota
        {
            dropped.extend(self.placed.remove(0).upgrade().map(|oldest| oldest.serial));
        }
        dropped
    }
}

fn load(command: &Command, bytes: Option<Vec<u8>>) -> Result<Image, &'static str> {
    if command.medium != b'd' {
        return Err("EINVAL:only direct transmission is supported");
    }
    let sized = (1..=MAX_SIDE).contains(&command.width) && (1..=MAX_SIDE).contains(&command.height);
    let area = command.width as usize * command.height as usize;
    let limit = match command.format {
        24 if sized => area * 3,
        32 if sized => area * 4,
        24 | 32 => return Err("EINVAL:the image size is missing or too large"),
        100 => MAX_INFLATED,
        _ => return Err("EINVAL:unsupported image format"),
    };
    let bytes = bytes.ok_or("EINVAL:the payload is not base64")?;
    let bytes = if command.compression == b'z' {
        inflate::zlib(&bytes, limit).ok_or("EINVAL:the payload is not zlib data")?
    } else {
        bytes
    };
    match command.format {
        24 => Image::from_rgb(command.width, command.height, &bytes),
        32 => Image::from_rgba(command.width, command.height, bytes),
        _ => png::decode(&bytes),
    }
    .ok_or("EINVAL:the image data is malformed")
}

fn framed(pixels: &[u8], inner: [u32; 2], outer: [u32; 2]) -> Vec<u8> {
    let (left, top) = (
        ((outer[0] - inner[0]) / 2) as usize,
        ((outer[1] - inner[1]) / 2) as usize,
    );
    let mut canvas = vec![0; outer[0] as usize * outer[1] as usize * 4];
    for (line, row) in pixels.chunks_exact(inner[0] as usize * 4).enumerate() {
        let start = ((top + line) * outer[0] as usize + left) * 4;
        canvas[start..start + row.len()].copy_from_slice(row);
    }
    canvas
}

fn append(buffer: &mut Vec<u8>, bytes: &[u8], limit: usize) {
    buffer.extend_from_slice(&bytes[..bytes.len().min(limit.saturating_sub(buffer.len()))]);
}

impl Term {
    pub fn set_cell_size(&mut self, width: u16, height: u16) {
        self.graphics.cell = [width.max(1), height.max(1)];
    }

    pub(super) fn sixel_begin(&mut self, transparent: bool) {
        self.graphics.sixel = Some((transparent, Vec::new()));
    }

    pub(super) fn sixel_feed(&mut self, bytes: &[u8]) {
        if let Some((_, data)) = &mut self.graphics.sixel {
            append(data, bytes, MAX_SIXEL);
        }
    }

    pub(super) fn sixel_end(&mut self) {
        let Some((transparent, data)) = self.graphics.sixel.take() else {
            return;
        };
        let background = self.palette.background;
        let Some(image) = sixel::decode(
            &data,
            (!transparent).then_some([background.r, background.g, background.b]),
        ) else {
            return;
        };
        let picture = self.picture(image.width, image.height, image.pixels);
        let rows = picture.height.div_ceil(u32::from(self.graphics.cell[1]));
        self.lay(
            &Spot {
                picture,
                left: 0,
                top: 0,
                rows,
                image: 0,
                placement: 0,
                fragile: true,
            },
            true,
        );
        self.linefeed();
    }

    pub(super) fn graphics_command(&mut self, payload: &[u8]) {
        let split = payload
            .iter()
            .position(|&byte| byte == b';')
            .unwrap_or(payload.len());
        let command = Command::parse(&payload[..split]);
        let data = payload.get(split + 1..).unwrap_or(&[]);
        if let Some((first, buffer)) = self.graphics.upload.take()
            && command.action != b'd'
        {
            let buffer = buffer.zip(base64::decode(data)).map(|(mut buffer, bytes)| {
                append(&mut buffer, &bytes, MAX_UPLOAD);
                buffer
            });
            if command.more {
                self.graphics.upload = Some((first, buffer));
            } else {
                self.transmit(
                    &Command {
                        quiet: first.quiet.max(command.quiet),
                        ..first
                    },
                    buffer,
                );
            }
            return;
        }
        if command.id != 0 && command.number != 0 {
            self.answer(
                &command,
                command.id,
                Err("EINVAL:an image id and an image number were both given"),
            );
            return;
        }
        match command.action {
            b't' | b'T' | b'q' if command.more => {
                self.graphics.upload = Some((command, base64::decode(data)));
            }
            b't' | b'T' | b'q' => self.transmit(&command, base64::decode(data)),
            b'p' => self.put(&command),
            b'd' => self.delete(&command),
            _ => self.answer(&command, command.id, Err("EINVAL:unsupported action")),
        }
    }

    fn answer(&mut self, command: &Command, id: u32, result: Result<(), &'static str>) {
        if (id == 0 && command.number == 0)
            || command.quiet >= 2
            || (command.quiet == 1 && result.is_ok())
        {
            return;
        }
        let _ = write!(self.outbox.reply, "\x1b_Gi={id}");
        if command.number != 0 {
            let _ = write!(self.outbox.reply, ",I={}", command.number);
        }
        if command.placement != 0 && matches!(command.action, b'T' | b'p') {
            let _ = write!(self.outbox.reply, ",p={}", command.placement);
        }
        let _ = write!(self.outbox.reply, ";{}\x1b\\", result.err().unwrap_or("OK"));
    }

    fn transmit(&mut self, command: &Command, bytes: Option<Vec<u8>>) {
        let image = match load(command, bytes) {
            Ok(image) => Arc::new(image),
            Err(message) => {
                self.answer(command, command.id, Err(message));
                return;
            }
        };
        if command.action == b'q' {
            self.answer(command, command.id, Ok(()));
            return;
        }
        let id = self.store(command, &image);
        let result = if command.action == b'T' {
            self.place(command, id, &image)
        } else {
            Ok(())
        };
        self.answer(command, id, result);
    }

    fn store(&mut self, command: &Command, image: &Arc<Image>) -> u32 {
        let id = if command.number == 0 {
            command.id
        } else {
            self.graphics.assign()
        };
        if id == 0 {
            return 0;
        }
        self.forget(id);
        if command.number != 0 {
            self.graphics.numbers.insert(command.number, id);
        }
        self.graphics.images.insert(id, Arc::clone(image));
        self.graphics.order.push_back(id);
        self.graphics.trim_images(QUOTA);
        id
    }

    fn forget(&mut self, id: u32) {
        if self.graphics.images.remove(&id).is_some() {
            self.graphics.order.retain(|&held| held != id);
            self.graphics.numbers.retain(|_, held| *held != id);
            self.strip(|tile| tile.image == id);
        }
    }

    fn put(&mut self, command: &Command) {
        let id = if command.id == 0 {
            self.graphics
                .numbers
                .get(&command.number)
                .copied()
                .unwrap_or(0)
        } else {
            command.id
        };
        let result = match self.graphics.images.get(&id).cloned() {
            Some(image) => self.place(command, id, &image),
            None => Err("ENOENT:no image with this id"),
        };
        self.answer(command, id, result);
    }

    fn place(&mut self, command: &Command, id: u32, image: &Image) -> Result<(), &'static str> {
        if command.detached {
            return Err("EINVAL:unsupported placement");
        }
        let [cell_w, cell_h] = self.graphics.cell.map(u32::from);
        let (x, y) = (command.x.min(image.width), command.y.min(image.height));
        let span = |wanted: u32, room: u32| if wanted == 0 { room } else { wanted.min(room) };
        let (w, h) = (
            span(command.w, image.width - x),
            span(command.h, image.height - y),
        );
        if w == 0 || h == 0 {
            return Err("EINVAL:the source rectangle is outside the image");
        }
        let columns = command.columns.min(MAX_SIDE / cell_w);
        let rows = command.rows.min(MAX_SIDE / cell_h);
        let fit = |along: u32, from: u32, across: u32| {
            ((u64::from(along) * u64::from(across) / u64::from(from)) as u32).clamp(1, MAX_SIDE)
        };
        let (area, inner) = match (columns * cell_w, rows * cell_h) {
            (0, 0) => ([w, h], [w, h]),
            (across, 0) => ([across, fit(across, w, h)], [across, fit(across, w, h)]),
            (0, down) => ([fit(down, h, w), down], [fit(down, h, w), down]),
            (across, down)
                if u64::from(across) * u64::from(h) <= u64::from(down) * u64::from(w) =>
            {
                ([across, down], [across, fit(across, w, h).min(down)])
            }
            (across, down) => ([across, down], [fit(down, h, w).min(across), down]),
        };
        let scaled = image.premultiplied([x, y, w, h], inner[0], inner[1]);
        let pixels = if area == inner {
            scaled
        } else {
            framed(&scaled, inner, area)
        };
        let (left, top) = (
            command.offset[0].min(cell_w - 1),
            command.offset[1].min(cell_h - 1),
        );
        if id != 0 && command.placement != 0 {
            self.strip(|tile| tile.image == id && tile.placement == command.placement);
        }
        let picture = self.picture(area[0], area[1], pixels);
        self.lay(
            &Spot {
                picture,
                left,
                top,
                rows: if rows == 0 {
                    (top + area[1]).div_ceil(cell_h)
                } else {
                    rows
                },
                image: id,
                placement: command.placement,
                fragile: false,
            },
            !command.stay,
        );
        if !command.stay {
            let column = self.cursor.column
                + if columns == 0 {
                    (left + area[0]).div_ceil(cell_w)
                } else {
                    columns
                } as usize;
            if column < self.cols() {
                self.move_to_column(column);
            } else {
                self.carriage_return();
                self.linefeed();
            }
        }
        Ok(())
    }

    fn picture(&mut self, width: u32, height: u32, pixels: Vec<u8>) -> Arc<Picture> {
        self.graphics.serial += 1;
        let picture = Arc::new(Picture {
            serial: self.graphics.serial,
            width,
            height,
            pixels,
        });
        self.graphics.placed.push(Arc::downgrade(&picture));
        let dropped = self.graphics.trim_pictures(QUOTA);
        if !dropped.is_empty() {
            self.strip(|tile| dropped.contains(&tile.picture.serial));
        }
        picture
    }

    fn lay(&mut self, spot: &Spot, advance: bool) {
        let cell = self.graphics.cell;
        let [cell_w, cell_h] = cell.map(u32::from);
        let (start, column) = (self.cursor.line, self.cursor.column);
        let width = spot
            .picture
            .width
            .min((self.cols() - column) as u32 * cell_w - spot.left);
        let covered = column..column + (spot.left + width).div_ceil(cell_w) as usize;
        for index in 0..spot.rows {
            let line = if advance {
                self.cursor.line
            } else {
                start + index as usize
            };
            if line >= self.lines() {
                break;
            }
            let skip = if index == 0 { spot.top } else { 0 };
            let y = (index * cell_h).saturating_sub(spot.top);
            let row = self.grid.row_mut(line);
            if spot.fragile {
                row.uncover(covered.clone());
            }
            row.place(Tile {
                picture: Arc::clone(&spot.picture),
                column: column as u16,
                left: spot.left as u16,
                top: skip as u16,
                x: 0,
                y,
                width,
                height: (cell_h - skip).min(spot.picture.height - y),
                cell,
                image: spot.image,
                placement: spot.placement,
                fragile: spot.fragile,
            });
            if advance && index + 1 < spot.rows {
                if self.cursor.line != self.bottom && self.cursor.line + 1 >= self.lines() {
                    break;
                }
                self.linefeed();
            }
        }
        self.damage.full = true;
    }

    fn delete(&mut self, command: &Command) {
        let kind = command.delete.to_ascii_lowercase();
        let id = if kind == b'n' {
            self.graphics
                .numbers
                .get(&command.number)
                .copied()
                .unwrap_or(0)
        } else {
            command.id
        };
        let (cursor, column, line) = (
            self.cursor,
            (command.x as usize).wrapping_sub(1),
            (command.y as usize).wrapping_sub(1),
        );
        let touched = |at: usize, tile: &Tile| match kind {
            b'a' => true,
            b'c' => at == cursor.line && tile.columns().contains(&cursor.column),
            b'p' => at == line && tile.columns().contains(&column),
            b'q' => at == line && tile.columns().contains(&column) && command.depth == 0,
            b'x' => tile.columns().contains(&column),
            b'y' => at == line,
            b'z' => command.depth == 0,
            _ => false,
        };
        let hit: HashSet<u64> = (0..self.lines())
            .flat_map(|at| {
                self.grid
                    .row(at)
                    .tiles()
                    .iter()
                    .filter(move |tile| !tile.fragile && touched(at, tile))
                    .map(|tile| tile.picture.serial)
            })
            .collect();
        let named = |image: u32| match kind {
            b'i' | b'n' => id != 0 && image == id,
            b'r' => image != 0 && (command.x..=command.y).contains(&image),
            _ => false,
        };
        let doomed = |tile: &Tile| {
            !tile.fragile
                && (hit.contains(&tile.picture.serial)
                    || (named(tile.image)
                        && (command.placement == 0 || tile.placement == command.placement)))
        };
        let mut freed: HashSet<u32> = if command.delete.is_ascii_uppercase() {
            self.grid
                .rows()
                .chain(self.inactive.rows())
                .flat_map(Row::tiles)
                .filter(|tile| doomed(tile))
                .map(|tile| tile.image)
                .chain(
                    self.graphics
                        .images
                        .keys()
                        .copied()
                        .filter(|&image| named(image)),
                )
                .collect()
        } else {
            HashSet::new()
        };
        self.strip(doomed);
        for tile in self
            .grid
            .rows()
            .chain(self.inactive.rows())
            .flat_map(Row::tiles)
        {
            freed.remove(&tile.image);
        }
        self.graphics
            .images
            .retain(|image, _| !freed.contains(image));
        self.graphics.order.retain(|image| !freed.contains(image));
        self.graphics
            .numbers
            .retain(|_, image| !freed.contains(image));
    }

    fn strip(&mut self, doomed: impl Fn(&Tile) -> bool) {
        for row in self.grid.rows_mut().chain(self.inactive.rows_mut()) {
            row.retain_tiles(|tile| !doomed(tile));
        }
        self.damage.full = true;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dartel_image::Image;

    use super::{Graphics, Picture, append};

    fn keep(graphics: &mut Graphics, id: u32, bytes: usize) {
        graphics.images.insert(
            id,
            Arc::new(Image {
                width: 1,
                height: 1,
                pixels: vec![0; bytes],
            }),
        );
        graphics.order.push_back(id);
    }

    fn pictures(count: u64) -> Vec<Arc<Picture>> {
        (1..=count)
            .map(|serial| {
                Arc::new(Picture {
                    serial,
                    width: 1,
                    height: 2,
                    pixels: vec![0; 8],
                })
            })
            .collect()
    }

    #[test]
    fn append_stops_at_the_limit() {
        let mut buffer = vec![1, 2];
        append(&mut buffer, &[3, 4, 5], 4);
        assert_eq!(buffer, [1, 2, 3, 4]);
    }

    #[test]
    fn append_adds_nothing_past_the_limit() {
        let mut buffer = vec![1, 2, 3];
        append(&mut buffer, &[4], 2);
        assert_eq!(buffer, [1, 2, 3]);
    }

    #[test]
    fn assigned_ids_count_down_from_the_largest() {
        let mut graphics = Graphics::default();
        assert_eq!(
            [graphics.assign(), graphics.assign()],
            [u32::MAX, u32::MAX - 1]
        );
    }

    #[test]
    fn assigned_ids_skip_stored_images() {
        let mut graphics = Graphics::default();
        keep(&mut graphics, u32::MAX, 4);
        assert_eq!(graphics.assign(), u32::MAX - 1);
    }

    #[test]
    fn assigned_ids_start_over_instead_of_reaching_zero() {
        let mut graphics = Graphics {
            assigned: 1,
            ..Graphics::default()
        };
        assert_eq!(graphics.assign(), u32::MAX);
    }

    #[test]
    fn oldest_images_are_dropped_over_the_quota() {
        let mut graphics = Graphics::default();
        keep(&mut graphics, 1, 8);
        keep(&mut graphics, 2, 8);
        keep(&mut graphics, 3, 8);
        graphics.numbers.insert(9, 1);
        graphics.trim_images(16);
        assert_eq!(graphics.order, [2, 3]);
        assert!(!graphics.images.contains_key(&1));
        assert!(graphics.numbers.is_empty());
    }

    #[test]
    fn images_within_the_quota_are_kept() {
        let mut graphics = Graphics::default();
        keep(&mut graphics, 1, 8);
        keep(&mut graphics, 2, 8);
        graphics.trim_images(16);
        assert_eq!(graphics.order, [1, 2]);
    }

    #[test]
    fn newest_image_is_kept_even_over_the_quota() {
        let mut graphics = Graphics::default();
        keep(&mut graphics, 1, 8);
        graphics.trim_images(4);
        assert_eq!(graphics.order, [1]);
    }

    #[test]
    fn oldest_pictures_are_reported_over_the_quota() {
        let mut graphics = Graphics::default();
        let held = pictures(3);
        graphics.placed = held.iter().map(Arc::downgrade).collect();
        assert_eq!(graphics.trim_pictures(16), [1]);
    }

    #[test]
    fn released_pictures_do_not_count_against_the_quota() {
        let mut graphics = Graphics::default();
        let mut held = pictures(3);
        graphics.placed = held.iter().map(Arc::downgrade).collect();
        held.truncate(2);
        assert_eq!(graphics.trim_pictures(16), Vec::<u64>::new());
    }

    #[test]
    fn newest_picture_is_kept_even_over_the_quota() {
        let mut graphics = Graphics::default();
        let held = pictures(1);
        graphics.placed = held.iter().map(Arc::downgrade).collect();
        assert_eq!(graphics.trim_pictures(4), Vec::<u64>::new());
    }
}
