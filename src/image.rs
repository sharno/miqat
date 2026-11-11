//! Minimal 8-bit image buffers and PNG codecs.

use anyhow::{Context, bail};

/// 8-bit grayscale, row-major; 0 is black.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gray {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Gray {
    pub fn filled(width: u32, height: u32, value: u8) -> Self {
        Self {
            width,
            height,
            pixels: vec![value; (width * height) as usize],
        }
    }

    /// Per-pixel `f(self, other)`; both images must share dimensions.
    pub fn zip_with(&self, other: &Gray, f: impl Fn(u8, u8) -> u8) -> Gray {
        assert_eq!((self.width, self.height), (other.width, other.height));
        Gray {
            width: self.width,
            height: self.height,
            pixels: self
                .pixels
                .iter()
                .zip(&other.pixels)
                .map(|(&a, &b)| f(a, b))
                .collect(),
        }
    }

    /// Copies `tile` (already mapped to gray) with its top-left at (left, top), clipped.
    pub fn blit(&mut self, tile: &Gray, left: i64, top: i64) {
        let (w, h) = (i64::from(self.width), i64::from(self.height));
        let x0 = left.max(0);
        let x1 = (left + i64::from(tile.width)).min(w);
        if x0 >= x1 {
            return;
        }
        for y in top.max(0)..(top + i64::from(tile.height)).min(h) {
            let src_row = ((y - top) * i64::from(tile.width)) as usize;
            let dst_row = (y * w) as usize;
            let src = &tile.pixels[src_row + (x0 - left) as usize..src_row + (x1 - left) as usize];
            self.pixels[dst_row + x0 as usize..dst_row + x1 as usize].copy_from_slice(src);
        }
    }

    pub fn encode_png(&self) -> anyhow::Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&self.pixels)?;
        writer.finish()?;
        Ok(out)
    }
}

/// Straight (not premultiplied) RGBA, so palette lookups match exact source colors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

impl Rgba {
    pub fn decode_png(bytes: &[u8]) -> anyhow::Result<Self> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().context("reading PNG header")?;
        let mut buf = vec![0; reader.output_buffer_size().context("PNG too large")?];
        let info = reader.next_frame(&mut buf).context("decoding PNG")?;
        let data = &buf[..info.buffer_size()];
        let pixels = match info.color_type {
            png::ColorType::Rgba => data.as_chunks::<4>().0.to_vec(),
            png::ColorType::Rgb => data
                .as_chunks::<3>()
                .0
                .iter()
                .map(|&[r, g, b]| [r, g, b, 255])
                .collect(),
            png::ColorType::GrayscaleAlpha => data
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[g, a]| [g, g, g, a])
                .collect(),
            png::ColorType::Grayscale => data.iter().map(|&g| [g, g, g, 255]).collect(),
            png::ColorType::Indexed => bail!("palette was not expanded"),
        };
        Ok(Self {
            width: info.width,
            height: info.height,
            pixels,
        })
    }

    pub fn map_to_gray(&self, f: impl Fn([u8; 4]) -> u8) -> Gray {
        Gray {
            width: self.width,
            height: self.height,
            pixels: self.pixels.iter().map(|&p| f(p)).collect(),
        }
    }
}

/// Rec. 601 luma, composited over white.
pub fn luma_over_white([r, g, b, a]: [u8; 4]) -> u8 {
    let luma = (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)) / 1000;
    let alpha = u32::from(a);
    ((luma * alpha + 255 * (255 - alpha)) / 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blit_clips_at_edges() {
        let mut canvas = Gray::filled(4, 3, 255);
        let tile = Gray::filled(3, 3, 0);
        canvas.blit(&tile, -1, 1);
        assert_eq!(
            canvas.pixels,
            vec![255, 255, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255]
        );
        canvas.blit(&tile, 10, 0);
        canvas.blit(&tile, -10, 0);
    }

    #[test]
    fn png_round_trip() {
        let image = Gray {
            width: 2,
            height: 1,
            pixels: vec![0, 200],
        };
        let decoded = Rgba::decode_png(&image.encode_png().unwrap()).unwrap();
        assert_eq!(decoded.pixels, vec![[0, 0, 0, 255], [200, 200, 200, 255]]);
    }

    #[test]
    fn transparent_is_white() {
        assert_eq!(luma_over_white([0, 0, 0, 0]), 255);
        assert_eq!(luma_over_white([0, 0, 0, 255]), 0);
    }
}
