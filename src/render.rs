//! SVG + radar raster → the 16-level grayscale PNG the Kindle displays.

use anyhow::Context;
use resvg::{tiny_skia, usvg};

use crate::image::{Gray, luma_over_white};
use crate::layout::{self, Frame};

const FONTS: [&[u8]; 3] = [
    include_bytes!("../assets/fonts/IBMPlexSansArabic-Regular.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSansArabic-SemiBold.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSansArabic-Bold.ttf"),
];

pub struct Renderer {
    options: usvg::Options<'static>,
}

impl Default for Renderer {
    fn default() -> Self {
        let mut options = usvg::Options::default();
        FONTS
            .iter()
            .for_each(|font| options.fontdb_mut().load_font_data(font.to_vec()));
        Self { options }
    }
}

impl Renderer {
    pub fn png(&self, frame: &Frame) -> anyhow::Result<Vec<u8>> {
        self.gray(frame)?.encode_png()
    }

    pub fn gray(&self, frame: &Frame) -> anyhow::Result<Gray> {
        let tree =
            usvg::Tree::from_str(&layout::svg(frame), &self.options).context("parsing SVG")?;
        let mut pixmap =
            tiny_skia::Pixmap::new(layout::WIDTH, layout::HEIGHT).context("allocating canvas")?;
        pixmap.fill(tiny_skia::Color::WHITE);
        if let Some(radar) = frame.radar {
            draw_gray(&mut pixmap, &radar.data.map, layout::RADAR_TOP);
        }
        resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
        Ok(Gray {
            width: layout::WIDTH,
            height: layout::HEIGHT,
            // The canvas is opaque, so premultiplied and straight RGBA agree.
            pixels: pixmap
                .pixels()
                .iter()
                .map(|p| quantize(luma_over_white([p.red(), p.green(), p.blue(), p.alpha()])))
                .collect(),
        })
    }
}

fn draw_gray(pixmap: &mut tiny_skia::Pixmap, image: &Gray, top: u32) {
    let offset = (top * pixmap.width()) as usize;
    pixmap.pixels_mut()[offset..offset + image.pixels.len()]
        .iter_mut()
        .zip(&image.pixels)
        .for_each(|(dst, &g)| {
            *dst = tiny_skia::PremultipliedColorU8::from_rgba(g, g, g, 255).unwrap_or(*dst)
        });
}

/// The Paperwhite shows 16 gray levels; snapping here keeps the panel from dithering.
fn quantize(value: u8) -> u8 {
    ((u16::from(value) + 8) / 17 * 17) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_hits_16_levels() {
        let levels: std::collections::BTreeSet<u8> = (0..=255).map(quantize).collect();
        assert_eq!(levels.len(), 16);
        assert_eq!((quantize(0), quantize(255), quantize(9)), (0, 255, 17));
    }
}
