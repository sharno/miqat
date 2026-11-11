//! Tabular (arithmetical) Islamic calendar. It can differ from a sighted or Umm al-Qura
//! date by a day, which `hijri_offset_days` corrects.

use chrono::{Datelike, Duration, NaiveDate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HijriDate {
    pub year: i32,
    /// 1..=12
    pub month: u8,
    pub day: u8,
}

const MONTHS: [(&str, &str); 12] = [
    ("Muharram", "محرم"),
    ("Safar", "صفر"),
    ("Rabi' al-Awwal", "ربيع الأول"),
    ("Rabi' al-Thani", "ربيع الآخر"),
    ("Jumada al-Ula", "جمادى الأولى"),
    ("Jumada al-Akhirah", "جمادى الآخرة"),
    ("Rajab", "رجب"),
    ("Sha'ban", "شعبان"),
    ("Ramadan", "رمضان"),
    ("Shawwal", "شوال"),
    ("Dhu al-Qa'dah", "ذو القعدة"),
    ("Dhu al-Hijjah", "ذو الحجة"),
];

impl HijriDate {
    pub fn from_gregorian(date: NaiveDate, offset_days: i8) -> Self {
        let date = date + Duration::days(offset_days.into());
        // Days since 0001-01-01 (day 1) shifted to the Julian Day Number.
        let jd = i64::from(date.num_days_from_ce()) + 1_721_425;
        // Integer algorithm from the "Kuwaiti" tabular calendar.
        let l = jd - 1_948_440 + 10_632;
        let n = (l - 1) / 10_631;
        let l = l - 10_631 * n + 354;
        let j = ((10_985 - l) / 5_316) * ((50 * l) / 17_719) + (l / 5_670) * ((43 * l) / 15_238);
        let l = l - ((30 - j) / 15) * ((17_719 * j) / 50) - (j / 16) * ((15_238 * j) / 43) + 29;
        let month = (24 * l) / 709;
        let day = l - (709 * month) / 24;
        let year = 30 * n + j - 30;
        Self {
            year: year as i32,
            month: month as u8,
            day: day as u8,
        }
    }

    pub fn english(self) -> String {
        format!(
            "{} {} {} AH",
            self.day,
            MONTHS[usize::from(self.month - 1)].0,
            self.year
        )
    }

    pub fn arabic(self) -> String {
        format!(
            "{} {} {} هـ",
            arabic_digits(self.day.into()),
            MONTHS[usize::from(self.month - 1)].1,
            arabic_digits(self.year)
        )
    }
}

fn arabic_digits(n: i32) -> String {
    n.to_string()
        .chars()
        .map(|c| {
            c.to_digit(10)
                .map_or(c, |d| char::from_u32(0x0660 + d).unwrap_or(c))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hijri(y: i32, m: u32, d: u32) -> HijriDate {
        HijriDate::from_gregorian(NaiveDate::from_ymd_opt(y, m, d).unwrap(), 0)
    }

    #[test]
    fn known_dates() {
        // 1 Ramadan 1445 was 2024-03-11 in Umm al-Qura; the tabular calendar agrees within a day.
        let r = hijri(2024, 3, 11);
        assert_eq!((r.year, r.month), (1445, 9));
        assert!(r.day <= 2, "{r:?}");
        // 1 Muharram 1448 is around 2026-06-16.
        let m = hijri(2026, 6, 17);
        assert_eq!((m.year, m.month), (1448, 1));
    }

    #[test]
    fn offset_shifts_day() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        let base = HijriDate::from_gregorian(date, 0);
        let next = HijriDate::from_gregorian(date, 1);
        assert_ne!(base, next);
    }

    #[test]
    fn renders_names() {
        let h = HijriDate {
            year: 1448,
            month: 6,
            day: 1,
        };
        assert_eq!(h.english(), "1 Jumada al-Akhirah 1448 AH");
        assert_eq!(h.arabic(), "١ جمادى الآخرة ١٤٤٨ هـ");
    }
}
