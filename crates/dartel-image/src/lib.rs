pub mod base64;
pub mod inflate;
pub mod png;
mod scale;
pub mod sixel;

pub const MAX_SIDE: u32 = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Image {
    #[must_use]
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Option<Image> {
        (sized(width, height) && pixels.len() == width as usize * height as usize * 4).then_some(
            Image {
                width,
                height,
                pixels,
            },
        )
    }

    #[must_use]
    pub fn from_rgb(width: u32, height: u32, pixels: &[u8]) -> Option<Image> {
        (sized(width, height) && pixels.len() == width as usize * height as usize * 3).then(|| {
            Image {
                width,
                height,
                pixels: pixels
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .flat_map(|&[red, green, blue]| [red, green, blue, u8::MAX])
                    .collect(),
            }
        })
    }
}

fn sized(width: u32, height: u32) -> bool {
    (1..=MAX_SIDE).contains(&width) && (1..=MAX_SIDE).contains(&height)
}
