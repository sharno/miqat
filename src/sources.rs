//! Remote data with its fetch time; refreshed lazily when a frame is requested.

use chrono::{DateTime, Duration, Utc};

use crate::config::Config;
use crate::image::Gray;
use crate::layout;
use crate::radar::{RadarClient, RadarImage, Viewport};
use crate::weather::{self, Weather};

#[derive(Debug, Clone)]
pub struct Snapshot<T> {
    pub data: T,
    pub fetched_at: DateTime<Utc>,
}

/// Open-Meteo updates current conditions every 15 minutes.
pub const WEATHER_EVERY: Duration = Duration::minutes(15);
/// RainViewer publishes a frame every 10 minutes.
pub const RADAR_EVERY: Duration = Duration::minutes(10);

fn is_due<T>(snapshot: &Option<Snapshot<T>>, now: DateTime<Utc>, every: Duration) -> bool {
    snapshot
        .as_ref()
        .is_none_or(|s| now - s.fetched_at >= every)
}

pub struct Sources {
    agent: ureq::Agent,
    radar: RadarClient,
    viewport: Viewport,
    pub weather: Option<Snapshot<Weather>>,
    /// Radar already composited over the basemap.
    pub radar_map: Option<Snapshot<RadarImage>>,
    /// Static; fetched once per process.
    basemap: Option<Gray>,
}

impl Sources {
    pub fn new(config: &Config) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(20)))
            .user_agent(concat!("miqat/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        let (width, height) = layout::RADAR_SIZE;
        Self {
            radar: RadarClient::new(agent.clone()),
            agent,
            viewport: Viewport::centered(&config.location, config.radar.zoom, width, height),
            weather: None,
            radar_map: None,
            basemap: None,
        }
    }

    /// Refetches whatever is due. A failed fetch keeps the previous snapshot, so the
    /// dashboard shows aging data rather than nothing; it is retried on the next frame.
    pub fn refresh(&mut self, config: &Config, now: DateTime<Utc>) {
        if is_due(&self.weather, now, WEATHER_EVERY) {
            let url = weather::request_url(&config.location, &config.weather);
            let fetched = self
                .agent
                .get(&url)
                .call()
                .and_then(|mut r| r.body_mut().read_to_string())
                .map_err(anyhow::Error::from)
                .and_then(|body| weather::decode(&body, config.location.timezone));
            match fetched {
                Ok(data) => {
                    self.weather = Some(Snapshot {
                        data,
                        fetched_at: now,
                    })
                }
                Err(e) => eprintln!("weather: {e:#}"),
            }
        }
        if self.basemap.is_none() {
            match self.radar.basemap(&self.viewport) {
                Ok(map) => self.basemap = Some(map),
                Err(e) => eprintln!("basemap: {e:#}"),
            }
        }
        if let Some(basemap) = &self.basemap
            && is_due(&self.radar_map, now, RADAR_EVERY)
        {
            match self.radar.latest(&self.viewport) {
                Ok(radar) => {
                    let map = crate::radar::overlay(basemap, &radar.map);
                    self.radar_map = Some(Snapshot {
                        data: RadarImage { map, ..radar },
                        fetched_at: now,
                    });
                }
                Err(e) => eprintln!("radar: {e:#}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_when_missing_or_old() {
        let now = Utc::now();
        assert!(is_due::<()>(&None, now, WEATHER_EVERY));
        let fresh = Some(Snapshot {
            data: (),
            fetched_at: now - Duration::minutes(5),
        });
        assert!(!is_due(&fresh, now, WEATHER_EVERY));
        assert!(is_due(&fresh, now + Duration::minutes(10), WEATHER_EVERY));
    }
}
