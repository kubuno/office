//! Civil dates (no time, no zone) — what a project stores (`start_date: "2026-10-07"`) and what the schedule
//! counts in (day offsets from the project start).
//!
//! The web turns the stored `"YYYY-MM-DD"` into a JavaScript `Date` (`new Date(project.start_date)`, UTC
//! midnight) and then reads it in local time (`addDays`, `getDay`, `getDate`). East of UTC — where Kubuno's
//! users are — that is the same civil day; west of UTC the web would show the previous day (QUIRK not
//! ported: the desktop always reads the civil date the server stored).
//!
//! The display formats are the ones `formatDate` asks `Intl.DateTimeFormat` for in the projects editor, in
//! French and English (the module's two languages on the desktop).

use std::fmt;

/// A civil date of the proleptic Gregorian calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    /// 1–12.
    pub month: u32,
    /// 1–31.
    pub day: u32,
}

/// Days from 1970-01-01 to the given civil date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date `z` days after 1970-01-01 (`civil_from_days`).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        _ => 28,
    }
}

impl Date {
    /// A valid date, or `None`.
    pub fn new(year: i32, month: u32, day: u32) -> Option<Self> {
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(Self { year, month, day })
    }

    /// Reads `"YYYY-MM-DD"` (a longer ISO string is cut at the date: `"2026-10-07T00:00:00Z"`).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let s = s.get(..10).unwrap_or(s);
        let mut it = s.split('-');
        let y = it.next()?.parse::<i32>().ok()?;
        let m = it.next()?.parse::<u32>().ok()?;
        let d = it.next()?.parse::<u32>().ok()?;
        if it.next().is_some() {
            return None;
        }
        Self::new(y, m, d)
    }

    /// Days since 1970-01-01.
    pub fn to_days(self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }

    pub fn from_days(days: i64) -> Self {
        let (year, month, day) = civil_from_days(days);
        Self { year, month, day }
    }

    /// `addDays`.
    pub fn add_days(self, n: i64) -> Self {
        Self::from_days(self.to_days() + n)
    }

    /// `differenceInDays(self, other)`: whole days from `other` to `self`.
    pub fn days_since(self, other: Date) -> i64 {
        self.to_days() - other.to_days()
    }

    /// Monday = 0 … Sunday = 6.
    pub fn weekday_mon0(self) -> u32 {
        // 1970-01-01 was a Thursday (3).
        (self.to_days() + 3).rem_euclid(7) as u32
    }

    /// JavaScript's `getDay()`: Sunday = 0 … Saturday = 6.
    pub fn js_day(self) -> u32 {
        (self.weekday_mon0() + 1) % 7
    }

    pub fn is_weekend(self) -> bool {
        self.weekday_mon0() >= 5
    }

    /// The ISO 8601 week number (`isoWeek` of the SDK).
    pub fn iso_week(self) -> u32 {
        // The Thursday of this date's week decides the year the week belongs to.
        let thursday = self.add_days(3 - self.weekday_mon0() as i64);
        let jan1 = Date { year: thursday.year, month: 1, day: 1 };
        (thursday.days_since(jan1) / 7 + 1) as u32
    }

    /// `"YYYY-MM-DD"` (`toISODate`).
    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// The first day of this date's month.
    pub fn first_of_month(self) -> Self {
        Self { day: 1, ..self }
    }

    pub fn days_in_month(self) -> u32 {
        days_in_month(self.year, self.month)
    }

    /// The first day of the next month.
    pub fn next_month(self) -> Self {
        if self.month == 12 {
            Self { year: self.year + 1, month: 1, day: 1 }
        } else {
            Self { year: self.year, month: self.month + 1, day: 1 }
        }
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.iso())
    }
}

/// The display language of the dates (`i18n.language`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    Fr,
    En,
}

impl Lang {
    /// `"fr"`, `"fr-FR"` → French; anything else English.
    pub fn from_tag(tag: &str) -> Self {
        if tag.to_ascii_lowercase().starts_with("fr") {
            Lang::Fr
        } else {
            Lang::En
        }
    }
}

const FR_LONG: [&str; 12] = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];
const FR_SHORT: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
const EN_LONG: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const EN_SHORT: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const FR_WEEKDAY_SHORT: [&str; 7] = ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."];
const EN_WEEKDAY_SHORT: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// The `Intl.DateTimeFormat` shapes the projects editor uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateFormat {
    /// `{ month: 'long', year: 'numeric' }` — « octobre 2026 » / « October 2026 » (the Gantt header).
    MonthYear,
    /// `{ day: 'numeric', month: 'short' }` — « 7 oct. » / « Oct 7 » (the boundary flags).
    DayMonth,
    /// `{ day: 'numeric', month: 'short', year: '2-digit' }` — « 7 oct. 26 » / « Oct 7, 26 » (the table).
    DayMonthYear2,
    /// The `'date'` preset `{ day: 'numeric', month: 'short', year: 'numeric' }` — « 7 oct. 2026 ».
    Date,
    /// `{ month: 'long' }` — « octobre ».
    Month,
    /// `{ weekday: 'short' }` — « lun. » / « Mon ».
    WeekdayShort,
    /// `{ month: 'short', year: '2-digit' }` — « oct. 26 » / « Oct 26 » (the network's time ruler).
    MonthShortYear2,
}

/// `formatDate(date, shape)` in `lang`.
pub fn format(d: Date, shape: DateFormat, lang: Lang) -> String {
    let mi = (d.month - 1) as usize;
    match (shape, lang) {
        (DateFormat::MonthYear, Lang::Fr) => format!("{} {}", FR_LONG[mi], d.year),
        (DateFormat::MonthYear, Lang::En) => format!("{} {}", EN_LONG[mi], d.year),
        (DateFormat::DayMonth, Lang::Fr) => format!("{} {}", d.day, FR_SHORT[mi]),
        (DateFormat::DayMonth, Lang::En) => format!("{} {}", EN_SHORT[mi], d.day),
        (DateFormat::DayMonthYear2, Lang::Fr) => format!("{} {} {:02}", d.day, FR_SHORT[mi], d.year.rem_euclid(100)),
        (DateFormat::DayMonthYear2, Lang::En) => format!("{} {}, {:02}", EN_SHORT[mi], d.day, d.year.rem_euclid(100)),
        (DateFormat::Date, Lang::Fr) => format!("{} {} {}", d.day, FR_SHORT[mi], d.year),
        (DateFormat::Date, Lang::En) => format!("{} {}, {}", EN_SHORT[mi], d.day, d.year),
        (DateFormat::Month, Lang::Fr) => FR_LONG[mi].to_string(),
        (DateFormat::Month, Lang::En) => EN_LONG[mi].to_string(),
        (DateFormat::WeekdayShort, Lang::Fr) => FR_WEEKDAY_SHORT[d.weekday_mon0() as usize].to_string(),
        (DateFormat::WeekdayShort, Lang::En) => EN_WEEKDAY_SHORT[d.weekday_mon0() as usize].to_string(),
        (DateFormat::MonthShortYear2, Lang::Fr) => format!("{} {:02}", FR_SHORT[mi], d.year.rem_euclid(100)),
        (DateFormat::MonthShortYear2, Lang::En) => format!("{} {:02}", EN_SHORT[mi], d.year.rem_euclid(100)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_days() {
        for z in [-800_000i64, -1, 0, 1, 20_000, 20_733, 2_000_000] {
            assert_eq!(Date::from_days(z).to_days(), z);
        }
        assert_eq!(Date::parse("1970-01-01").map(Date::to_days), Some(0));
        assert_eq!(Date::parse("2026-10-07T12:00:00Z").map(|d| d.iso()), Some("2026-10-07".into()));
        assert!(Date::parse("2026-02-30").is_none());
    }

    #[test]
    fn weekdays_and_weeks() {
        // 2026-09-07 is a Monday (the server's test reference).
        let mon = Date::new(2026, 9, 7).expect("date");
        assert_eq!(mon.weekday_mon0(), 0);
        assert_eq!(mon.js_day(), 1);
        assert_eq!(mon.add_days(6).js_day(), 0);
        assert_eq!(Date::new(2026, 1, 1).expect("d").iso_week(), 1);
        assert_eq!(Date::new(2027, 1, 1).expect("d").iso_week(), 53);
        assert_eq!(Date::new(2026, 10, 7).expect("d").iso_week(), 41);
    }

    #[test]
    fn formats_like_intl() {
        let d = Date::new(2026, 10, 7).expect("date");
        assert_eq!(format(d, DateFormat::MonthYear, Lang::Fr), "octobre 2026");
        assert_eq!(format(d, DateFormat::DayMonth, Lang::Fr), "7 oct.");
        assert_eq!(format(d, DateFormat::DayMonthYear2, Lang::Fr), "7 oct. 26");
        assert_eq!(format(d, DateFormat::DayMonthYear2, Lang::En), "Oct 7, 26");
        assert_eq!(format(d, DateFormat::Date, Lang::En), "Oct 7, 2026");
    }
}
