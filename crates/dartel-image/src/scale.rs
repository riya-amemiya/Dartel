use std::ops::Range;

use super::Image;

fn span(index: u32, from: u32, to: u32) -> Range<usize> {
    let start = (u64::from(index) * u64::from(from) / u64::from(to)) as usize;
    let end = (u64::from(index + 1) * u64::from(from) / u64::from(to)) as usize;
    start..end.max(start + 1)
}

impl Image {
    #[must_use]
    pub fn premultiplied(
        &self,
        [left, top, across, down]: [u32; 4],
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let columns: Vec<Range<usize>> =
            (0..width).map(|index| span(index, across, width)).collect();
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for line in 0..height {
            let rows = span(line, down, height);
            for column in &columns {
                let mut sum = [0_u64; 4];
                for row in rows.clone() {
                    let base = ((top as usize + row) * self.width as usize + left as usize) * 4;
                    for pixel in self.pixels[base + column.start * 4..base + column.end * 4]
                        .as_chunks::<4>()
                        .0
                    {
                        let alpha = u64::from(pixel[3]);
                        sum[0] += u64::from(pixel[0]) * alpha;
                        sum[1] += u64::from(pixel[1]) * alpha;
                        sum[2] += u64::from(pixel[2]) * alpha;
                        sum[3] += alpha;
                    }
                }
                let count = (rows.len() * column.len()) as u64;
                pixels.extend_from_slice(&[
                    ((sum[0] + 255 * count / 2) / (255 * count)) as u8,
                    ((sum[1] + 255 * count / 2) / (255 * count)) as u8,
                    ((sum[2] + 255 * count / 2) / (255 * count)) as u8,
                    ((sum[3] + count / 2) / count) as u8,
                ]);
            }
        }
        pixels
    }
}
