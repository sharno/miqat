//! The dashboard as SVG, laid out for the Kindle Paperwhite 1 (758×1024, portrait).
//! Pure: everything it shows comes in through [`Frame`].

use std::fmt::{self, Display, Write as _};

use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use chrono_tz::Tz;

use crate::config::{ClockFormat, Config};
use crate::hijri::HijriDate;
use crate::prayer::{DayTimes, Salah, Upcoming};
use crate::radar::RadarImage;
use crate::sources::Snapshot;
use crate::weather::{Condition, Hour, Light, Quarter, Sky, Weather};

pub const WIDTH: u32 = 758;
pub const HEIGHT: u32 = 1024;
/// Where the radar raster goes; the SVG draws its overlays on top.
pub const RADAR_TOP: u32 = 748;
pub const RADAR_SIZE: (u32, u32) = (WIDTH, HEIGHT - RADAR_TOP);

const MARGIN: f64 = 28.0;
const RIGHT: f64 = WIDTH as f64 - MARGIN;
const MUTED: &str = "#666";

/// The timetable runs from here to the divider above the weather.
const PRAYERS_TOP: f64 = 164.0;
const PRAYERS_BOTTOM: f64 = 466.0;
/// Height of a passed or later row.
const PRAYER_ROW: f64 = 40.0;
/// The upcoming time is the one thing to read from across the room, so it gets a
/// tall card; the other rows tighten up to pay for it.
const NEXT_PRAYER_ROW: f64 = 98.0;
/// The six rows must still clear the divider the weather section starts at.
const _: () = assert!(PRAYERS_TOP + 5.0 * PRAYER_ROW + NEXT_PRAYER_ROW <= PRAYERS_BOTTOM);
/// Data older than this gets an "as of" note.
const STALE_AFTER: Duration = Duration::minutes(45);

pub struct Frame<'a> {
    pub now: DateTime<Utc>,
    pub config: &'a Config,
    pub prayers: &'a DayTimes,
    pub weather: Option<&'a Snapshot<Weather>>,
    pub radar: Option<&'a Snapshot<RadarImage>>,
}

pub fn svg(frame: &Frame) -> String {
    let body = [header(frame), prayers(frame), weather(frame), radar(frame)].concat();
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}" font-family="IBM Plex Sans Arabic">{body}</svg>"#
    )
}

fn header(frame: &Frame) -> String {
    let tz = frame.config.location.timezone;
    let local = frame.now.with_timezone(&tz);
    let hijri = HijriDate::from_gregorian(
        frame.prayers.islamic_date(frame.now),
        frame.config.prayer.hijri_offset_days,
    );
    let (clock, suffix) = clock_parts(frame.now, tz, frame.config.display.clock);
    [
        text(MARGIN - 4.0, 112.0, 104, &clock)
            .weight(700)
            .to_string(),
        suffix.map_or_else(String::new, |s| {
            text(MARGIN + 6.0 + numeric_width(&clock, 104.0), 112.0, 32, s)
                .weight(600)
                .to_string()
        }),
        text(RIGHT, 56.0, 30, &local.format("%A, %-d %B").to_string())
            .weight(600)
            .end()
            .to_string(),
        text(RIGHT, 96.0, 24, &hijri.english()).end().to_string(),
        text(RIGHT, 132.0, 26, &hijri.arabic()).end().to_string(),
        hline(152.0, 2.0),
    ]
    .concat()
}

fn prayers(frame: &Frame) -> String {
    let tz = frame.config.location.timezone;
    let weekday = frame.prayers.date.weekday();
    let upcoming = frame.prayers.upcoming(frame.now);
    let mut top = PRAYERS_TOP;
    let mut rows = String::new();
    for (salah, at) in frame.prayers.board(frame.now) {
        let state = RowState::of(salah, at, upcoming, frame.now);
        let (english, arabic) = salah.names(weekday);
        let time = format_time(at, tz, frame.config.display.clock);
        let weight = if salah == Salah::Sunrise { 400 } else { 600 };
        let row = match state {
            RowState::Next => next_row(top, frame.now, at, (english, arabic, &time), weight),
            RowState::Passed => plain_row(top, (english, arabic, &time), weight, MUTED),
            RowState::Later => plain_row(top, (english, arabic, &time), weight, "#000"),
        };
        rows.push_str(&row);
        top += state.height();
    }
    rows.push_str(&hline(PRAYERS_BOTTOM, 2.0));
    rows
}

/// The upcoming time, as a tall card with much larger type than the rest of the
/// timetable. Its time column sits further right than the plain rows because the
/// larger prayer names would otherwise collide with the time.
fn next_row(
    top: f64,
    now: DateTime<Utc>,
    at: DateTime<Utc>,
    (english, arabic, time): (&str, &str, &str),
    weight: u16,
) -> String {
    const SIZE: f64 = 52.0;
    let baseline = baseline(top, NEXT_PRAYER_ROW, SIZE);
    [
        rect(
            MARGIN - 12.0,
            top + 4.0,
            RIGHT - MARGIN + 24.0,
            NEXT_PRAYER_ROW - 8.0,
            12.0,
        ),
        text(MARGIN, baseline, SIZE as u32, english)
            .weight(weight)
            .fill("#fff")
            .to_string(),
        text(470.0, baseline, SIZE as u32, time)
            .weight(700)
            .end()
            .fill("#fff")
            .to_string(),
        text(484.0, baseline, 24, &countdown(now, at))
            .weight(600)
            .fill("#fff")
            .to_string(),
        text(RIGHT, baseline, 36, arabic)
            .weight(weight)
            .end()
            .fill("#fff")
            .to_string(),
    ]
    .concat()
}

fn plain_row(
    top: f64,
    (english, arabic, time): (&str, &str, &str),
    weight: u16,
    fill: &'static str,
) -> String {
    let baseline = baseline(top, PRAYER_ROW, 32.0);
    [
        text(MARGIN, baseline, 30, english)
            .weight(weight)
            .fill(fill)
            .to_string(),
        text(330.0, baseline, 32, time)
            .weight(700)
            .end()
            .fill(fill)
            .to_string(),
        text(RIGHT, baseline, 28, arabic)
            .weight(weight)
            .end()
            .fill(fill)
            .to_string(),
    ]
    .concat()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowState {
    Next,
    Passed,
    Later,
}

impl RowState {
    fn of(salah: Salah, at: DateTime<Utc>, upcoming: Upcoming, now: DateTime<Utc>) -> Self {
        match (Upcoming { prayer: salah, at } == upcoming, at <= now) {
            (true, _) => RowState::Next,
            (false, true) => RowState::Passed,
            (false, false) => RowState::Later,
        }
    }

    fn height(self) -> f64 {
        match self {
            RowState::Next => NEXT_PRAYER_ROW,
            RowState::Passed | RowState::Later => PRAYER_ROW,
        }
    }
}

fn weather(frame: &Frame) -> String {
    const TOP: f64 = PRAYERS_BOTTOM;
    let Some(snapshot) = frame.weather else {
        return text(WIDTH as f64 / 2.0, TOP + 130.0, 28, "Weather unavailable")
            .middle()
            .fill(MUTED)
            .to_string();
    };
    let w = &snapshot.data;
    let c = &w.current;
    let tz = frame.config.location.timezone;
    let clock = frame.config.display.clock;
    let wind_unit = frame.config.weather.wind_unit.label();
    let today = frame.now.with_timezone(&tz).date_naive();
    let range = w
        .days
        .iter()
        .find(|d| d.date == today)
        .map(|d| format!("H {}  L {}", degrees(d.high), degrees(d.low)));
    let stale = (frame.now - snapshot.fetched_at > STALE_AFTER)
        .then(|| format!("as of {}", format_time(snapshot.fetched_at, tz, clock)));
    let speed = format!("{:.0}", c.wind.speed);
    let feels = format!(
        "Feels {} · {}% humidity",
        degrees(c.feels_like),
        c.humidity_percent.round()
    );
    let hours = next_hours(&w.hours, frame.now, 6)
        .iter()
        .enumerate()
        .map(|(i, hour)| hour_cell(hour, MARGIN + 117.0 * i as f64, TOP + 146.0, tz, clock))
        .collect::<String>();
    [
        icon(c.sky, MARGIN - 6.0, TOP - 2.0, 110.0),
        text(140.0, TOP + 72.0, 76, &degrees(c.temperature))
            .weight(700)
            .to_string(),
        text(140.0, TOP + 108.0, 26, c.sky.condition.label())
            .weight(600)
            .to_string(),
        text(140.0, TOP + 138.0, 21, &feels).fill(MUTED).to_string(),
        wind_arrow(c.wind.from_degrees, 452.0, TOP + 6.0, 64.0),
        text(530.0, TOP + 52.0, 44, &speed).weight(700).to_string(),
        text(
            536.0 + numeric_width(&speed, 44.0),
            TOP + 52.0,
            24,
            wind_unit,
        )
        .to_string(),
        text(
            530.0,
            TOP + 84.0,
            22,
            &format!(
                "gusts {:.0} · from {}",
                c.wind.gust,
                compass(c.wind.from_degrees)
            ),
        )
        .to_string(),
        range.map_or_else(String::new, |r| {
            text(530.0, TOP + 118.0, 22, &r).weight(600).to_string()
        }),
        stale.map_or_else(String::new, |s| {
            text(RIGHT, TOP + 140.0, 18, &s)
                .end()
                .fill(MUTED)
                .to_string()
        }),
        hours,
        text(
            MARGIN,
            RADAR_TOP as f64 - 10.0,
            23,
            &rain_outlook(&w.quarters, frame.now, tz, clock),
        )
        .weight(600)
        .to_string(),
    ]
    .concat()
}

fn hour_cell(hour: &Hour, x: f64, top: f64, tz: Tz, clock: ClockFormat) -> String {
    let label = match clock {
        ClockFormat::TwelveHour => hour.at.with_timezone(&tz).format("%-I%P").to_string(),
        ClockFormat::TwentyFourHour => hour.at.with_timezone(&tz).format("%H:00").to_string(),
    };
    let chance = hour
        .precipitation_chance
        .filter(|&p| p >= 10.0)
        .map(|p| format!("{p:.0}%"));
    let middle = x + 50.0;
    [
        text(middle, top + 20.0, 19, &label)
            .middle()
            .fill(MUTED)
            .to_string(),
        icon(hour.sky, middle - 22.0, top + 24.0, 44.0),
        chance.map_or_else(String::new, |c| {
            text(middle + 27.0, top + 58.0, 17, &c)
                .weight(600)
                .to_string()
        }),
        text(middle, top + 90.0, 22, &degrees(hour.temperature))
            .weight(600)
            .middle()
            .to_string(),
    ]
    .concat()
}

fn radar(frame: &Frame) -> String {
    let top = RADAR_TOP as f64;
    let (width, height) = RADAR_SIZE;
    let (cx, cy) = (f64::from(width) / 2.0, top + f64::from(height) / 2.0);
    let Some(snapshot) = frame.radar else {
        return text(cx, cy, 26, "Radar unavailable")
            .middle()
            .fill(MUTED)
            .to_string();
    };
    let tz = frame.config.location.timezone;
    let observed = format!(
        "Radar {}",
        format_time(snapshot.data.observed_at, tz, frame.config.display.clock)
    );
    [
        hline(top, 2.0),
        format!(r##"<circle cx="{cx}" cy="{cy}" r="9" fill="#fff" stroke="#000" stroke-width="3"/><circle cx="{cx}" cy="{cy}" r="3"/>"##),
        text(MARGIN - 12.0, top + 30.0, 22, &observed).weight(700).halo().to_string(),
        text(WIDTH as f64 - 8.0, HEIGHT as f64 - 8.0, 13, "RainViewer · © OpenStreetMap contributors").end().halo().to_string(),
    ]
    .concat()
}

// ---- formatting (pure, tested) ----

fn clock_parts(t: DateTime<Utc>, tz: Tz, clock: ClockFormat) -> (String, Option<&'static str>) {
    let local = t.with_timezone(&tz);
    match clock {
        ClockFormat::TwelveHour => (
            local.format("%-I:%M").to_string(),
            Some(if local.hour() < 12 { "AM" } else { "PM" }),
        ),
        ClockFormat::TwentyFourHour => (local.format("%H:%M").to_string(), None),
    }
}

/// Plex digits are tabular (0.6 em) and the colon narrower; good enough to place a unit
/// after a number without measuring glyphs.
fn numeric_width(number: &str, size: f64) -> f64 {
    number
        .chars()
        .map(|c| if c == ':' { 0.3 } else { 0.6 })
        .sum::<f64>()
        * size
}

pub fn format_time(t: DateTime<Utc>, tz: Tz, clock: ClockFormat) -> String {
    let local = t.with_timezone(&tz);
    match clock {
        ClockFormat::TwelveHour => local.format("%-I:%M").to_string(),
        ClockFormat::TwentyFourHour => local.format("%H:%M").to_string(),
    }
}

/// Whole minutes remaining, rounded up so "in 1m" shows until the moment arrives.
fn countdown(now: DateTime<Utc>, at: DateTime<Utc>) -> String {
    let minutes = (at - now).num_seconds().max(0).unsigned_abs().div_ceil(60);
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("in {m}m"),
        (h, 0) => format!("in {h}h"),
        (h, m) => format!("in {h}h {m}m"),
    }
}

fn next_hours(hours: &[Hour], now: DateTime<Utc>, count: usize) -> &[Hour] {
    let start = hours.partition_point(|h| h.at <= now);
    &hours[start..(start + count).min(hours.len())]
}

/// Precipitation in mm per 15 minutes that counts as rain.
const WET_MM: f64 = 0.1;

fn rain_outlook(quarters: &[Quarter], now: DateTime<Utc>, tz: Tz, clock: ClockFormat) -> String {
    let start = quarters.partition_point(|q| q.at + Duration::minutes(15) <= now);
    let window = &quarters[start..(start + 8).min(quarters.len())];
    let wet = |q: &Quarter| q.precipitation_mm >= WET_MM;
    let at = |q: &Quarter| format_time(q.at, tz, clock);
    let Some(first) = window.first() else {
        return "No precipitation forecast".to_string();
    };
    match (
        wet(first),
        window.iter().find(|q| wet(q)),
        window.iter().find(|q| !wet(q)),
    ) {
        (true, _, Some(dry)) => format!("Raining now, easing around {}", at(dry)),
        (true, _, None) => "Rain for the next 2 hours".to_string(),
        (false, Some(rain), _) => format!("Rain likely from {}", at(rain)),
        (false, None, _) => "No rain expected in the next 2 hours".to_string(),
    }
}

fn degrees(value: f64) -> String {
    // `-0°` reads oddly; round first so -0.4 shows as 0.
    let rounded = value.round();
    format!("{}°", if rounded == 0.0 { 0.0 } else { rounded })
}

fn compass(from_degrees: f64) -> &'static str {
    const POINTS: [&str; 16] = [
        "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW",
        "NW", "NNW",
    ];
    POINTS[((from_degrees.rem_euclid(360.0) / 22.5).round() as usize) % 16]
}

// ---- drawing primitives ----

/// Baseline that vertically centres a line of `size` type in a box `height` tall.
fn baseline(top: f64, height: f64, size: f64) -> f64 {
    top + (height + size * 0.72) / 2.0
}

struct Text<'a> {
    x: f64,
    y: f64,
    size: u32,
    content: &'a str,
    weight: u16,
    anchor: Anchor,
    fill: &'static str,
    outline: Outline,
}

#[derive(Debug, Clone, Copy)]
enum Anchor {
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, Copy)]
enum Outline {
    None,
    /// A white outline so text stays legible over the map.
    Halo,
}

fn text(x: f64, y: f64, size: u32, content: &str) -> Text<'_> {
    Text {
        x,
        y,
        size,
        content,
        weight: 400,
        anchor: Anchor::Start,
        fill: "#000",
        outline: Outline::None,
    }
}

impl Text<'_> {
    fn weight(self, weight: u16) -> Self {
        Self { weight, ..self }
    }
    fn end(self) -> Self {
        Self {
            anchor: Anchor::End,
            ..self
        }
    }
    fn middle(self) -> Self {
        Self {
            anchor: Anchor::Middle,
            ..self
        }
    }
    fn fill(self, fill: &'static str) -> Self {
        Self { fill, ..self }
    }
    fn halo(self) -> Self {
        Self {
            outline: Outline::Halo,
            ..self
        }
    }
}

impl Display for Text<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let anchor = match self.anchor {
            Anchor::Start => "start",
            Anchor::Middle => "middle",
            Anchor::End => "end",
        };
        let outline = match self.outline {
            Outline::None => "",
            Outline::Halo => {
                r##" stroke="#fff" stroke-width="5" stroke-linejoin="round" paint-order="stroke""##
            }
        };
        write!(
            f,
            r#"<text x="{}" y="{}" font-size="{}" font-weight="{}" text-anchor="{anchor}" fill="{}"{outline}>"#,
            self.x, self.y, self.size, self.weight, self.fill
        )?;
        self.content.chars().try_for_each(|c| match c {
            '&' => f.write_str("&amp;"),
            '<' => f.write_str("&lt;"),
            '>' => f.write_str("&gt;"),
            c => f.write_char(c),
        })?;
        f.write_str("</text>")
    }
}

fn hline(y: f64, width: f64) -> String {
    format!(
        r#"<rect x="0" y="{}" width="{WIDTH}" height="{width}"/>"#,
        y - width / 2.0
    )
}

fn rect(x: f64, y: f64, width: f64, height: f64, radius: f64) -> String {
    format!(r#"<rect x="{x}" y="{y}" width="{width}" height="{height}" rx="{radius}"/>"#)
}

/// An arrow pointing where the wind blows to, in a ring marked N.
fn wind_arrow(from_degrees: f64, x: f64, y: f64, size: f64) -> String {
    let to = (from_degrees + 180.0).rem_euclid(360.0);
    let s = size / 100.0;
    format!(
        r##"<g transform="translate({x} {y}) scale({s})"><circle cx="50" cy="50" r="44" fill="none" stroke="#000" stroke-width="4"/><text x="50" y="22" font-size="16" font-weight="700" text-anchor="middle">N</text><g transform="rotate({to} 50 50)"><path d="M50 20 L66 62 L50 53 L34 62 Z"/><rect x="47" y="52" width="6" height="28"/></g></g>"##
    )
}

/// Line-art weather icons in a 100×100 box.
fn icon(sky: Sky, x: f64, y: f64, size: f64) -> String {
    const SUN: &str = r##"<circle cx="50" cy="50" r="18" fill="#fff" stroke="#000" stroke-width="6"/><g stroke="#000" stroke-width="6" stroke-linecap="round"><path d="M50 12v10M50 78v10M12 50h10M78 50h10M23 23l7 7M70 70l7 7M23 77l7-7M70 30l7-7"/></g>"##;
    const MOON: &str = r##"<path d="M60 18a34 34 0 1 0 24 52a28 28 0 0 1-24-52z" fill="#fff" stroke="#000" stroke-width="6" stroke-linejoin="round"/>"##;
    const CLOUD: &str = r##"<path d="M30 76h44a16 16 0 0 0 0-32a22 22 0 0 0-42-4a18 18 0 0 0-2 36z" fill="#fff" stroke="#000" stroke-width="6" stroke-linejoin="round"/>"##;
    const SMALL_SUN: &str = r##"<g transform="translate(-8 -12) scale(0.7)"><circle cx="50" cy="50" r="18" fill="#fff" stroke="#000" stroke-width="7"/><g stroke="#000" stroke-width="7" stroke-linecap="round"><path d="M50 14v8M14 50h8M24 24l6 6M76 24l-6 6"/></g></g>"##;
    const SMALL_MOON: &str = r##"<g transform="translate(-6 -10) scale(0.65)"><path d="M60 18a34 34 0 1 0 24 52a28 28 0 0 1-24-52z" fill="#fff" stroke="#000" stroke-width="8"/></g>"##;
    const HIGH_CLOUD: &str = r##"<g transform="translate(0 -12)"><path d="M30 76h44a16 16 0 0 0 0-32a22 22 0 0 0-42-4a18 18 0 0 0-2 36z" fill="#fff" stroke="#000" stroke-width="6" stroke-linejoin="round"/></g>"##;
    const RAIN: &str = r##"<g stroke="#000" stroke-width="6" stroke-linecap="round"><path d="M36 74l-6 14M52 74l-6 14M68 74l-6 14"/></g>"##;
    const HEAVY_RAIN: &str = r##"<g stroke="#000" stroke-width="6" stroke-linecap="round"><path d="M30 72l-8 20M44 72l-8 20M58 72l-8 20M72 72l-8 20"/></g>"##;
    const DRIZZLE: &str = r##"<g fill="#000"><circle cx="36" cy="80" r="4"/><circle cx="52" cy="88" r="4"/><circle cx="68" cy="80" r="4"/></g>"##;
    const SNOW: &str = r##"<g stroke="#000" stroke-width="4" stroke-linecap="round"><path d="M34 72v16M27 76l14 8M27 84l14-8M66 72v16M59 76l14 8M59 84l14-8"/></g>"##;
    const BOLT: &str = r##"<path d="M54 64l-12 18h10l-6 16 18-22h-10l6-12z" stroke="#000" stroke-width="2" stroke-linejoin="round"/>"##;
    const FOG: &str = r##"<g stroke="#000" stroke-width="6" stroke-linecap="round"><path d="M18 40h64M12 56h76M22 72h58"/></g>"##;
    let (light, small_light) = match sky.light {
        Light::Day => (SUN, SMALL_SUN),
        Light::Night => (MOON, SMALL_MOON),
    };
    let layers: &[&str] = match sky.condition {
        Condition::Clear | Condition::MainlyClear => &[light],
        Condition::PartlyCloudy => &[small_light, CLOUD],
        Condition::Overcast => &[CLOUD],
        Condition::Fog => &[FOG],
        Condition::Drizzle => &[HIGH_CLOUD, DRIZZLE],
        Condition::LightRain | Condition::Rain => &[HIGH_CLOUD, RAIN],
        Condition::HeavyRain | Condition::FreezingRain => &[HIGH_CLOUD, HEAVY_RAIN],
        Condition::Snow | Condition::HeavySnow => &[HIGH_CLOUD, SNOW],
        Condition::Thunderstorm => &[HIGH_CLOUD, BOLT],
    };
    format!(
        r#"<g transform="translate({x} {y}) scale({})">{}</g>"#,
        size / 100.0,
        layers.concat()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 11, h, m, 0).unwrap()
    }

    fn quarters(mm: &[f64]) -> Vec<Quarter> {
        mm.iter()
            .enumerate()
            .map(|(i, &precipitation_mm)| Quarter {
                at: t(12, 0) + Duration::minutes(15 * i as i64),
                precipitation_mm,
            })
            .collect()
    }

    #[test]
    fn countdown_rounds_up() {
        assert_eq!(
            countdown(t(12, 0), t(12, 0) + Duration::seconds(30)),
            "in 1m"
        );
        assert_eq!(countdown(t(12, 0), t(13, 0)), "in 1h");
        assert_eq!(countdown(t(12, 0), t(14, 5)), "in 2h 5m");
    }

    #[test]
    fn rain_outlook_cases() {
        let utc = chrono_tz::UTC;
        let h24 = ClockFormat::TwentyFourHour;
        let now = t(12, 5);
        assert_eq!(
            rain_outlook(&quarters(&[0.0; 10]), now, utc, h24),
            "No rain expected in the next 2 hours"
        );
        assert_eq!(
            rain_outlook(&quarters(&[0.0, 0.0, 0.5, 1.0]), now, utc, h24),
            "Rain likely from 12:30"
        );
        assert_eq!(
            rain_outlook(&quarters(&[0.4, 0.3, 0.0, 0.0]), now, utc, h24),
            "Raining now, easing around 12:30"
        );
        assert_eq!(
            rain_outlook(&quarters(&[0.4; 8]), now, utc, h24),
            "Rain for the next 2 hours"
        );
        // The 12:00 quarter is current at 12:05; past quarters are ignored.
        assert_eq!(
            rain_outlook(&quarters(&[0.4, 0.0]), t(12, 20), utc, h24),
            "No rain expected in the next 2 hours"
        );
        assert_eq!(
            rain_outlook(&[], now, utc, h24),
            "No precipitation forecast"
        );
    }

    #[test]
    fn compass_points() {
        assert_eq!(compass(0.0), "N");
        assert_eq!(compass(284.0), "WNW");
        assert_eq!(compass(359.0), "N");
    }

    #[test]
    fn twelve_hour_clock() {
        let ams = chrono_tz::Europe::Amsterdam;
        assert_eq!(
            clock_parts(t(13, 5), ams, ClockFormat::TwelveHour),
            ("2:05".into(), Some("PM"))
        );
        assert_eq!(
            format_time(t(13, 5), ams, ClockFormat::TwentyFourHour),
            "14:05"
        );
    }

    #[test]
    fn degrees_avoid_negative_zero() {
        assert_eq!(degrees(-0.4), "0°");
        assert_eq!(degrees(57.3), "57°");
    }

    #[test]
    fn text_is_escaped() {
        assert!(
            text(0.0, 0.0, 10, "a<b&c")
                .to_string()
                .contains("a&lt;b&amp;c")
        );
    }
}
