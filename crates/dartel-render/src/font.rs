use std::collections::HashMap;
use std::sync::Arc;

use fontdb::{Database, Family, ID, Query, Stretch, Style, Weight};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::Format;
use swash::{CacheKey, FontRef};

#[cfg(target_os = "macos")]
const FAMILIES: &[Family<'static>] = &[
    Family::Name("Menlo"),
    Family::Name("Monaco"),
    Family::Monospace,
];
#[cfg(not(target_os = "macos"))]
const FAMILIES: &[Family<'static>] = &[
    Family::Name("DejaVu Sans Mono"),
    Family::Name("Liberation Mono"),
    Family::Monospace,
];

#[cfg(target_os = "macos")]
const FALLBACKS: &[&str] = &["Hiragino Sans", "Apple Color Emoji", "Apple Symbols"];
#[cfg(not(target_os = "macos"))]
const FALLBACKS: &[&str] = &[
    "Noto Sans Mono CJK JP",
    "Noto Sans CJK JP",
    "IPAGothic",
    "Noto Color Emoji",
    "DejaVu Sans Mono",
];

pub const REGULAR: usize = 0;

struct Face {
    id: ID,
    data: Arc<dyn AsRef<[u8]> + Send + Sync>,
    offset: u32,
    key: CacheKey,
}

impl Face {
    fn font(&self) -> FontRef<'_> {
        FontRef {
            data: (*self.data).as_ref(),
            offset: self.offset,
            key: self.key,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metrics {
    pub width: u32,
    pub height: u32,
    pub ascent: i32,
    pub underline: i32,
    pub strikeout: i32,
    pub stroke: u32,
}

pub struct Raster {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub advance: f32,
    pub colored: bool,
    pub pixels: Vec<u8>,
}

pub struct Library(Database);

impl Library {
    #[must_use]
    pub fn load() -> Library {
        let mut database = Database::new();
        database.load_system_fonts();
        Library(database)
    }
}

pub struct Fonts {
    database: Database,
    faces: Vec<Face>,
    styles: [usize; 4],
    preferred: Vec<ID>,
    fallbacks: HashMap<char, Option<usize>>,
    context: ScaleContext,
    size: f32,
    metrics: Metrics,
}

fn premultiplied(mut pixels: Vec<u8>) -> Vec<u8> {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        for channel in 0..3 {
            pixel[channel] = (u16::from(pixel[channel]) * u16::from(pixel[3]) / 255) as u8;
        }
    }
    pixels
}

impl Fonts {
    pub fn new(
        Library(database): Library,
        family: &str,
        fallback: &[String],
        size: f32,
    ) -> Option<Fonts> {
        let regular = database
            .query(&Query {
                families: &[Family::Name(family)],
                ..Query::default()
            })
            .or_else(|| {
                database.query(&Query {
                    families: FAMILIES,
                    ..Query::default()
                })
            })
            .or_else(|| {
                database
                    .faces()
                    .find(|face| face.monospaced)
                    .map(|face| face.id)
            })
            .or_else(|| database.faces().next().map(|face| face.id))?;
        let family = database.face(regular)?.families.first()?.0.clone();
        let variant = |weight, style| {
            database
                .query(&Query {
                    families: &[Family::Name(&family)],
                    weight,
                    stretch: Stretch::Normal,
                    style,
                })
                .unwrap_or(regular)
        };
        let ids = [
            regular,
            variant(Weight::BOLD, Style::Normal),
            variant(Weight::NORMAL, Style::Italic),
            variant(Weight::BOLD, Style::Italic),
        ];
        let preferred = fallback
            .iter()
            .filter_map(|name| {
                database.query(&Query {
                    families: &[Family::Name(name)],
                    ..Query::default()
                })
            })
            .collect();
        let mut fonts = Fonts {
            database,
            faces: Vec::new(),
            styles: [0; 4],
            preferred,
            fallbacks: HashMap::new(),
            context: ScaleContext::new(),
            size,
            metrics: Metrics {
                width: 1,
                height: 1,
                ascent: 0,
                underline: 0,
                strikeout: 0,
                stroke: 1,
            },
        };
        fonts.styles = [
            fonts.load(ids[0])?,
            fonts.load(ids[1])?,
            fonts.load(ids[2])?,
            fonts.load(ids[3])?,
        ];
        fonts.metrics = fonts.measure();
        Some(fonts)
    }

    pub fn metrics(&self) -> Metrics {
        self.metrics
    }

    pub fn set_size(&mut self, size: f32) {
        self.size = size;
        self.metrics = self.measure();
    }

    fn load(&mut self, id: ID) -> Option<usize> {
        if let Some(index) = self.faces.iter().position(|face| face.id == id) {
            return Some(index);
        }
        let (data, index) = unsafe { self.database.make_shared_face_data(id) }?;
        let (offset, key) = FontRef::from_index((*data).as_ref(), index as usize)
            .map(|font| (font.offset, font.key))?;
        self.faces.push(Face {
            id,
            data,
            offset,
            key,
        });
        Some(self.faces.len() - 1)
    }

    fn measure(&self) -> Metrics {
        let font = self.faces[self.styles[REGULAR]].font();
        let metrics = font.metrics(&[]).scale(self.size);
        Metrics {
            width: font
                .glyph_metrics(&[])
                .scale(self.size)
                .advance_width(font.charmap().map('M'))
                .round()
                .max(1.0) as u32,
            height: (metrics.ascent + metrics.descent + metrics.leading)
                .ceil()
                .max(1.0) as u32,
            ascent: metrics.ascent.round() as i32,
            underline: (metrics.ascent - metrics.underline_offset).round() as i32,
            strikeout: (metrics.ascent - metrics.strikeout_offset).round() as i32,
            stroke: metrics.stroke_size.round().max(1.0) as u32,
        }
    }

    fn resolve(&mut self, c: char, style: usize) -> Option<(usize, u16)> {
        for index in [self.styles[style], self.styles[REGULAR]] {
            let glyph = self.faces[index].font().charmap().map(c);
            if glyph != 0 {
                return Some((index, glyph));
            }
        }
        let index = if let Some(&cached) = self.fallbacks.get(&c) {
            cached
        } else {
            let found = self.discover(c);
            self.fallbacks.insert(c, found);
            found
        }?;
        Some((index, self.faces[index].font().charmap().map(c)))
    }

    fn discover(&mut self, c: char) -> Option<usize> {
        let id = self
            .preferred
            .iter()
            .copied()
            .chain(FALLBACKS.iter().filter_map(|name| {
                self.database.query(&Query {
                    families: &[Family::Name(name)],
                    ..Query::default()
                })
            }))
            .chain(
                self.database
                    .faces()
                    .filter(|face| face.monospaced)
                    .map(|face| face.id),
            )
            .chain(self.database.faces().map(|face| face.id))
            .find(|&id| {
                self.database
                    .with_face_data(id, |data, index| {
                        FontRef::from_index(data, index as usize)
                            .is_some_and(|font| font.charmap().map(c) != 0)
                    })
                    .unwrap_or(false)
            })?;
        self.load(id)
    }

    pub fn rasterize(&mut self, c: char, style: usize) -> Option<Raster> {
        let (index, glyph) = self.resolve(c, style)?;
        let font = self.faces[index].font();
        let advance = font
            .glyph_metrics(&[])
            .scale(self.size)
            .advance_width(glyph);
        let image = Render::new(&[
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ])
        .format(Format::Alpha)
        .render(
            &mut self
                .context
                .builder(font)
                .size(self.size)
                .hint(false)
                .build(),
            glyph,
        )?;
        Some(Raster {
            left: image.placement.left,
            top: image.placement.top,
            width: image.placement.width,
            height: image.placement.height,
            advance,
            colored: image.content == Content::Color,
            pixels: match image.content {
                Content::Color => premultiplied(image.data),
                Content::Mask | Content::SubpixelMask => {
                    image.data.iter().flat_map(|&alpha| [alpha; 4]).collect()
                }
            },
        })
    }
}
