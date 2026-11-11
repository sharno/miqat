//! Rain radar from RainViewer over an OpenStreetMap basemap, as one grayscale image.
//!
//! Both layers share one Web Mercator pixel space: RainViewer 512 px tiles at zoom `z`
//! line up with OSM 256 px tiles at `z + 1`.

mod palette;

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::config::{Location, Zoom};
use crate::image::{Gray, Rgba, luma_over_white};

#[derive(Debug, Clone, PartialEq)]
pub struct RadarImage {
    pub observed_at: DateTime<Utc>,
    pub map: Gray,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Top-left corner in world pixels.
    left: f64,
    top: f64,
    pub width: u32,
    pub height: u32,
    zoom: Zoom,
}

/// Radar tiles are 512 px at the configured zoom.
const RADAR_TILE: u32 = 512;
/// Basemap tiles are 256 px one zoom deeper: the same world size, sharper labels.
const BASEMAP_TILE: u32 = 256;

impl Viewport {
    pub fn centered(location: &Location, zoom: Zoom, width: u32, height: u32) -> Self {
        let world = world_size(zoom);
        let lat = location.latitude.degrees().to_radians();
        let x = (location.longitude.degrees() + 180.0) / 360.0 * world;
        let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * world;
        Self {
            left: (x - f64::from(width) / 2.0).floor(),
            top: (y - f64::from(height) / 2.0).floor(),
            width,
            height,
            zoom,
        }
    }

    /// Tiles of `tile_px` that cover the viewport, and where each lands in it.
    fn placements(&self, tile_px: u32) -> Vec<Placement> {
        let size = f64::from(tile_px);
        let per_axis = (world_size(self.zoom) / size) as i64;
        let first = |start: f64| (start / size).floor() as i64;
        let last = |start: f64, len: u32| ((start + f64::from(len) - 1.0) / size).floor() as i64;
        let columns = first(self.left)..=last(self.left, self.width);
        let rows =
            (first(self.top)..=last(self.top, self.height)).filter(|y| (0..per_axis).contains(y));
        rows.flat_map(|y| {
            columns.clone().map(move |x| Placement {
                x: x.rem_euclid(per_axis) as u32,
                y: y as u32,
                left: x * i64::from(tile_px) - self.left as i64,
                top: y * i64::from(tile_px) - self.top as i64,
            })
        })
        .collect()
    }
}

fn world_size(zoom: Zoom) -> f64 {
    f64::from(RADAR_TILE) * f64::from(1u32 << zoom.level())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Placement {
    x: u32,
    y: u32,
    left: i64,
    top: i64,
}

/// OSM's colorful style pushed toward white, so it reads as context under the rain.
fn faded(pixel: [u8; 4]) -> u8 {
    255 - ((255 - u32::from(luma_over_white(pixel))) * 11 / 20) as u8
}

/// Rain intensity (upper dBZ bound, gray): darker is heavier. Below 10 dBZ is mostly
/// virga and clutter that would wash out the map, so it stays transparent.
const INTENSITY_GRAYS: [(i8, u8); 6] = [
    (10, 255),
    (20, 195),
    (30, 150),
    (40, 105),
    (50, 55),
    (i8::MAX, 0),
];

fn rain_gray(pixel: [u8; 4]) -> u8 {
    let rgba = u32::from_be_bytes(pixel);
    palette::UNIVERSAL_BLUE
        .binary_search_by_key(&rgba, |&(color, _)| color)
        .ok()
        .map(|i| palette::UNIVERSAL_BLUE[i].1)
        .and_then(|dbz| INTENSITY_GRAYS.iter().find(|(below, _)| dbz < *below))
        .map_or(255, |&(_, gray)| gray)
}

pub struct RadarClient {
    agent: ureq::Agent,
}

#[derive(Deserialize)]
struct WeatherMaps {
    host: String,
    radar: RadarFrames,
}

#[derive(Deserialize)]
struct RadarFrames {
    past: Vec<RadarFrame>,
}

#[derive(Deserialize)]
struct RadarFrame {
    time: i64,
    path: String,
}

impl RadarClient {
    pub fn new(agent: ureq::Agent) -> Self {
        Self { agent }
    }

    pub fn basemap(&self, viewport: &Viewport) -> anyhow::Result<Gray> {
        let zoom = viewport.zoom.level() + 1;
        // Fetched once per process (a handful of tiles), within OSM's tile usage policy.
        self.composite(viewport, BASEMAP_TILE, faded, |p| {
            format!("https://tile.openstreetmap.org/{zoom}/{}/{}.png", p.x, p.y)
        })
    }

    pub fn latest(&self, viewport: &Viewport) -> anyhow::Result<RadarImage> {
        let maps: WeatherMaps = self
            .agent
            .get("https://api.rainviewer.com/public/weather-maps.json")
            .call()?
            .body_mut()
            .read_json()
            .context("decoding RainViewer weather-maps.json")?;
        let frame = maps
            .radar
            .past
            .last()
            .context("RainViewer returned no radar frames")?;
        let observed_at =
            DateTime::from_timestamp(frame.time, 0).context("radar frame time out of range")?;
        let zoom = viewport.zoom.level();
        // Options `0_0`: unsmoothed, so pixels match the palette exactly; no separate snow colors.
        let map = self.composite(viewport, RADAR_TILE, rain_gray, |p| {
            format!(
                "{}{}/{RADAR_TILE}/{zoom}/{}/{}/2/0_0.png",
                maps.host, frame.path, p.x, p.y
            )
        })?;
        Ok(RadarImage { observed_at, map })
    }

    fn composite(
        &self,
        viewport: &Viewport,
        tile_px: u32,
        to_gray: impl Fn([u8; 4]) -> u8,
        url: impl Fn(&Placement) -> String,
    ) -> anyhow::Result<Gray> {
        viewport.placements(tile_px).iter().try_fold(
            Gray::filled(viewport.width, viewport.height, 255),
            |mut canvas, placement| {
                let url = url(placement);
                let bytes = self
                    .agent
                    .get(&url)
                    .call()
                    .and_then(|mut r| r.body_mut().read_to_vec())
                    .with_context(|| format!("fetching {url}"))?;
                let tile = Rgba::decode_png(&bytes).with_context(|| format!("decoding {url}"))?;
                canvas.blit(&tile.map_to_gray(&to_gray), placement.left, placement.top);
                Ok(canvas)
            },
        )
    }
}

/// Radar drawn over the basemap; the darker value wins so labels stay visible in rain.
pub fn overlay(basemap: &Gray, radar: &Gray) -> Gray {
    basemap.zip_with(radar, u8::min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn location(lat: f64, lon: f64) -> Location {
        let text = format!(
            "[location]\nlatitude = {lat:?}\nlongitude = {lon:?}\ntimezone = \"UTC\"\n[prayer]\nmethod = \"egyptian\"\n"
        );
        toml::from_str::<Config>(&text).unwrap().location
    }

    #[test]
    fn viewport_centers_on_location() {
        let zoom = Zoom::try_from(7).unwrap();
        let v = Viewport::centered(&location(0.0, 0.0), zoom, 100, 50);
        let world = world_size(zoom);
        assert_eq!((v.left, v.top), (world / 2.0 - 50.0, world / 2.0 - 25.0));
    }

    #[test]
    fn placements_cover_viewport_and_wrap_longitude() {
        let zoom = Zoom::try_from(3).unwrap();
        let v = Viewport::centered(&location(0.0, 179.9), zoom, 758, 280);
        let tiles = v.placements(RADAR_TILE);
        let per_axis = (world_size(zoom) / f64::from(RADAR_TILE)) as u32;
        assert!(tiles.iter().all(|p| p.x < per_axis));
        assert!(
            tiles.iter().any(|p| p.x == 0),
            "wraps past the antimeridian: {tiles:?}"
        );
        // Every viewport pixel is covered by exactly one tile.
        let covered = |x: i64, y: i64| {
            tiles
                .iter()
                .filter(|p| {
                    (p.left..p.left + 512).contains(&x) && (p.top..p.top + 512).contains(&y)
                })
                .count()
        };
        assert!(
            [(0, 0), (757, 0), (0, 279), (757, 279), (379, 140)]
                .iter()
                .all(|&(x, y)| covered(x, y) == 1)
        );
    }

    #[test]
    fn rain_intensity_maps_to_darker_grays() {
        let color = |dbz: i8| {
            let (rgba, _) = palette::UNIVERSAL_BLUE
                .iter()
                .find(|(_, d)| *d == dbz)
                .unwrap();
            rain_gray(rgba.to_be_bytes())
        };
        assert_eq!(color(5), 255);
        assert!(color(15) > color(35));
        assert!(color(35) > color(55));
        assert_eq!(rain_gray([0, 0, 0, 0]), 255);
    }
}
