//! JavaScript's `Date`, as the web's engine uses it, without a time zone.
//!
//! The web builds every date with the local constructor (`new Date(y, m, d)`) and reads it back with the
//! local getters, and converts to an Excel serial from the local components through `Date.UTC`
//! (`excelSerial`), so its results do not depend on the zone except across a daylight-saving change.
//! The core has no zone: a time value here is the local wall-clock time counted in milliseconds as if it
//! were UTC, which is exactly what the web computes outside those transition hours.
//!
//! Everything follows ECMA-262: `MakeDay`/`MakeTime` with overflowing months and days, `TimeClip` (a time
//! value beyond ±8.64e15 ms is invalid), the constructor's and `Date.UTC`'s years 0–99 meaning 1900–1999.

/// `Date.UTC(1899, 11, 30)`: day 0 of the Excel serial.
pub const EXCEL_EPOCH_MS: f64 = -2_209_161_600_000.0;
/// Milliseconds per day.
pub const DAY_MS: f64 = 86_400_000.0;

/// The local date and time the caller hands to the engine (`new Date()`): it never reads a clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LocalNow {
    pub year: i32,
    /// 1–12.
    pub month: u32,
    /// 1–31.
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millis: u32,
}

impl LocalNow {
    /// The time value of this instant (`new Date()` → `getTime()` in the zone-less model).
    pub fn time_value(&self) -> f64 {
        make_date(
            make_day(self.year as f64, self.month as f64 - 1.0, self.day as f64),
            make_time(self.hour as f64, self.minute as f64, self.second as f64, self.millis as f64),
        )
    }
}

fn to_integer(x: f64) -> f64 {
    if x.is_nan() {
        0.0
    } else {
        x.trunc()
    }
}

/// Days from 1970-01-01 to `year`-`month`-01 (proleptic Gregorian; month 1–12).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// (year, month 1–12, day 1–31) of a day number counted from 1970-01-01.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `MakeDay(year, month, date)` (month 0-based, may overflow either way).
pub fn make_day(year: f64, month: f64, date: f64) -> f64 {
    if !year.is_finite() || !month.is_finite() || !date.is_finite() {
        return f64::NAN;
    }
    let (y, m, dt) = (to_integer(year), to_integer(month), to_integer(date));
    let ym = y + (m / 12.0).floor();
    if ym.abs() > 400_000.0 {
        return f64::NAN;
    }
    let mn = m.rem_euclid(12.0);
    let day = days_from_civil(ym as i64, mn as i64 + 1, 1) as f64;
    day + dt - 1.0
}

/// `MakeTime(hour, min, sec, ms)`.
pub fn make_time(h: f64, m: f64, s: f64, ms: f64) -> f64 {
    if !h.is_finite() || !m.is_finite() || !s.is_finite() || !ms.is_finite() {
        return f64::NAN;
    }
    to_integer(h) * 3_600_000.0 + to_integer(m) * 60_000.0 + to_integer(s) * 1000.0 + to_integer(ms)
}

/// `MakeDate(day, time)`.
pub fn make_date(day: f64, time: f64) -> f64 {
    if !day.is_finite() || !time.is_finite() {
        return f64::NAN;
    }
    day * DAY_MS + time
}

/// `TimeClip`.
pub fn time_clip(t: f64) -> f64 {
    if !t.is_finite() || t.abs() > 8.64e15 {
        f64::NAN
    } else {
        to_integer(t) + 0.0
    }
}

/// Years 0–99 mean 1900–1999 in the constructor and `Date.UTC`.
fn full_year(y: f64) -> f64 {
    if !y.is_nan() {
        let yi = to_integer(y);
        if (0.0..=99.0).contains(&yi) {
            return 1900.0 + yi;
        }
    }
    y
}

/// `new Date(y, m, d, h, mi, s, ms)` (local) — and `Date.UTC(...)`, the same in the zone-less model.
pub fn new_date(y: f64, m: f64, d: f64, h: f64, mi: f64, s: f64, ms: f64) -> f64 {
    time_clip(make_date(make_day(full_year(y), m, d), make_time(h, mi, s, ms)))
}

/// `new Date(y, m, d)`.
pub fn ymd(y: f64, m: f64, d: f64) -> f64 {
    new_date(y, m, d, 0.0, 0.0, 0.0, 0.0)
}

/// `new Date(t)` for a time value.
pub fn from_time(t: f64) -> f64 {
    time_clip(t)
}

/// The components of a valid time value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parts {
    pub year: i64,
    /// 0-based, like `getMonth()`.
    pub month: i64,
    pub date: i64,
    /// 0 = Sunday, like `getDay()`.
    pub weekday: i64,
    pub hours: i64,
    pub minutes: i64,
    pub seconds: i64,
    pub millis: i64,
}

/// `getFullYear()`, `getMonth()`, … of time value `t` (`None` for an invalid date).
pub fn parts(t: f64) -> Option<Parts> {
    if t.is_nan() {
        return None;
    }
    let day = (t / DAY_MS).floor();
    let ms_in_day = t - day * DAY_MS;
    let (y, m, d) = civil_from_days(day as i64);
    let weekday = ((day as i64 % 7) + 7 + 4) % 7; // 1970-01-01 was a Thursday
    let ms = ms_in_day as i64;
    Some(Parts {
        year: y,
        month: m - 1,
        date: d,
        weekday,
        hours: ms / 3_600_000,
        minutes: (ms / 60_000) % 60,
        seconds: (ms / 1000) % 60,
        millis: ms % 1000,
    })
}

/// `excelSerial(d)`: days since 1899-12-30 from the date's components (milliseconds dropped), with
/// Excel's phantom 29 February 1900 (serials below 61 are one lower).
pub fn excel_serial(t: f64) -> f64 {
    let Some(p) = parts(t) else { return f64::NAN };
    let utc = new_date(p.year as f64, p.month as f64, p.date as f64, p.hours as f64, p.minutes as f64, p.seconds as f64, 0.0);
    let raw = (utc - EXCEL_EPOCH_MS) / DAY_MS;
    if raw < 61.0 {
        raw - 1.0
    } else {
        raw
    }
}

/// `serialToDate(n)`: the date of an Excel serial (`None` when it is not a valid date).
pub fn serial_to_date(n: f64) -> Option<f64> {
    if n.is_nan() {
        return None;
    }
    let days = if n < 60.0 { n + 1.0 } else { n };
    let u = from_time(EXCEL_EPOCH_MS + days * DAY_MS);
    let p = parts(u)?;
    let t = new_date(p.year as f64, p.month as f64, p.date as f64, p.hours as f64, p.minutes as f64, p.seconds as f64, 0.0);
    if t.is_nan() {
        None
    } else {
        Some(t)
    }
}

/// The serial of today at midnight (`TODAY()`).
pub fn today_serial(now: &LocalNow) -> f64 {
    excel_serial(ymd(now.year as f64, now.month as f64 - 1.0, now.day as f64))
}

/// The serial of now (`NOW()`).
pub fn now_serial(now: &LocalNow) -> f64 {
    excel_serial(now.time_value())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serials_match_excel() {
        assert_eq!(excel_serial(ymd(1900.0, 0.0, 1.0)), 1.0);
        assert_eq!(excel_serial(ymd(1900.0, 1.0, 28.0)), 59.0);
        assert_eq!(excel_serial(ymd(1900.0, 2.0, 1.0)), 61.0);
        assert_eq!(excel_serial(ymd(2024.0, 0.0, 1.0)), 45292.0);
        let t = serial_to_date(45292.5).unwrap();
        let p = parts(t).unwrap();
        assert_eq!((p.year, p.month, p.date, p.hours, p.weekday), (2024, 0, 1, 12, 1));
        let p = parts(serial_to_date(1.0).unwrap()).unwrap();
        assert_eq!((p.year, p.month, p.date), (1900, 0, 1));
    }

    #[test]
    fn the_constructor_overflows_and_maps_two_digit_years() {
        let p = parts(ymd(2024.0, 13.0, 1.0)).unwrap();
        assert_eq!((p.year, p.month), (2025, 1));
        let p = parts(ymd(2024.0, 0.0, 0.0)).unwrap();
        assert_eq!((p.year, p.month, p.date), (2023, 11, 31));
        assert_eq!(parts(ymd(24.0, 0.0, 1.0)).unwrap().year, 1924);
        assert!(ymd(f64::NAN, 0.0, 1.0).is_nan());
        assert!(serial_to_date(1e12).is_none());
    }

    #[test]
    fn weekdays() {
        assert_eq!(parts(ymd(1970.0, 0.0, 1.0)).unwrap().weekday, 4);
        assert_eq!(parts(ymd(1969.0, 11.0, 31.0)).unwrap().weekday, 3);
        assert_eq!(parts(ymd(2026.0, 9.0, 7.0)).unwrap().weekday, 3);
    }
}
