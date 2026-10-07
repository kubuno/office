//! Working calendars — which days are worked, and how to move across them, in **day offsets** from a reference
//! date (the project start).
//!
//! The port of the office server's `services/working_calendar.rs` (the scheduler's calendar), which the web
//! mirrors in `web/src/project/workingCalendar.ts` to predict where a dropped Gantt bar will settle. Two
//! differences between the two are kept, each where it belongs:
//!
//! * the scan bound: 4 000 days on the server, 3 660 on the web ([`WorkingCalendar::max_scan`]);
//! * which calendar a project uses: the server takes the project's calendar (`projects.calendar_id`) and,
//!   without one, the instance default ([`for_schedule`]); the web takes the project's calendar, else the
//!   **first** calendar of the list, else the default ([`for_prediction`], QUIRK kept: the prediction only
//!   differs from the server's schedule for a project that has calendars but none selected).

use std::collections::HashMap;

use crate::date::Date;
use crate::model::Calendars;

/// The server's scan bound (`MAX_SCAN`, `working_calendar.rs:130`).
pub const SERVER_MAX_SCAN: i32 = 4000;
/// The web mirror's scan bound (`workingCalendar.ts:8`).
pub const WEB_MAX_SCAN: i32 = 3660;

/// A week's shape plus the days that depart from it.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkingCalendar {
    /// Monday first, Sunday last.
    week: [bool; 7],
    /// Day offset → worked or not, overriding the weekly pattern.
    exceptions: HashMap<i32, bool>,
    /// Weekday of offset 0 (Monday = 0).
    ref_dow: u32,
    /// How far the scanners look for a working day.
    pub max_scan: i32,
}

impl WorkingCalendar {
    pub fn new(week: [bool; 7], exceptions: HashMap<i32, bool>, reference: Date) -> Self {
        Self { week, exceptions, ref_dow: reference.weekday_mon0(), max_scan: SERVER_MAX_SCAN }
    }

    /// `from_parts`: the stored booleans and `(date, is_working)` overrides, dates turned into offsets.
    pub fn from_parts(week: [bool; 7], exceptions: impl IntoIterator<Item = (Date, bool)>, reference: Date) -> Self {
        let map = exceptions.into_iter().map(|(d, w)| (d.days_since(reference) as i32, w)).collect();
        Self::new(week, map, reference)
    }

    /// Monday to Friday, plus the weekend when `include_weekends`: the instance default of a project
    /// without a calendar (`project_default_include_weekends`).
    pub fn default_week(include_weekends: bool, reference: Date) -> Self {
        Self::new([true, true, true, true, true, include_weekends, include_weekends], HashMap::new(), reference)
    }

    /// Every day worked (`WorkingCal.allDays`).
    pub fn all_days(reference: Date) -> Self {
        Self::new([true; 7], HashMap::new(), reference)
    }

    pub fn with_max_scan(mut self, max_scan: i32) -> Self {
        self.max_scan = max_scan;
        self
    }

    /// A calendar with no working day would make every scan run to its bound.
    pub fn has_working_day(&self) -> bool {
        self.week.iter().any(|d| *d) || self.exceptions.values().any(|w| *w)
    }

    pub fn is_working(&self, offset: i32) -> bool {
        if let Some(&forced) = self.exceptions.get(&offset) {
            return forced;
        }
        self.week[((self.ref_dow as i64 + offset as i64).rem_euclid(7)) as usize]
    }

    /// The first worked day at or after `offset`.
    pub fn next_working(&self, offset: i32) -> i32 {
        (offset..offset + self.max_scan).find(|&o| self.is_working(o)).unwrap_or(offset)
    }

    /// The exclusive finish of a task starting at `start` lasting `working_days` worked days.
    pub fn advance(&self, start: i32, working_days: i32) -> i32 {
        if working_days <= 0 {
            // A milestone sits on its day, worked or not.
            return start;
        }
        let mut remaining = working_days;
        let from = self.next_working(start);
        for o in from..from + self.max_scan {
            if self.is_working(o) {
                remaining -= 1;
                if remaining == 0 {
                    return o + 1;
                }
            }
        }
        start + working_days
    }

    /// Working days in the half-open range `[start, finish)`.
    pub fn working_days_between(&self, start: i32, finish: i32) -> i32 {
        if finish <= start {
            return 0;
        }
        (start..finish).filter(|&o| self.is_working(o)).count() as i32
    }

    /// The mirror of [`advance`](Self::advance): where a task must start to finish at `finish`.
    pub fn retreat(&self, finish: i32, working_days: i32) -> i32 {
        if working_days <= 0 {
            return finish;
        }
        let mut remaining = working_days;
        for o in (finish - self.max_scan..finish).rev() {
            if self.is_working(o) {
                remaining -= 1;
                if remaining == 0 {
                    return o;
                }
            }
        }
        finish - working_days
    }
}

fn from_row(data: &Calendars, id: &str, reference: Date) -> Option<WorkingCalendar> {
    let row = data.calendars.iter().find(|c| c.id == id)?;
    let ex = data.exceptions.iter().filter(|e| e.calendar_id == row.id).filter_map(|e| Date::parse(&e.day).map(|d| (d, e.is_working)));
    Some(WorkingCalendar::from_parts(row.week(), ex, reference))
}

/// The calendar the **server's scheduler** uses (`compute_cpm`, `projects.rs:1256-1279`): the project's own
/// calendar, else the instance default.
pub fn for_schedule(data: Option<&Calendars>, reference: Date, include_weekends_default: bool) -> WorkingCalendar {
    data.and_then(|d| d.project_calendar_id.as_deref().and_then(|id| from_row(d, id, reference)))
        .unwrap_or_else(|| WorkingCalendar::default_week(include_weekends_default, reference))
}

/// The calendar the **web's Gantt** predicts drops with (`calendarFromApi`, `workingCalendar.ts:65-88`): the
/// project's calendar, else the first of the list, else the default — with the web's scan bound.
pub fn for_prediction(data: Option<&Calendars>, reference: Date, include_weekends_default: bool) -> WorkingCalendar {
    let cal = data.and_then(|d| {
        let id = d.project_calendar_id.as_deref().filter(|id| d.calendars.iter().any(|c| c.id == *id)).or_else(|| d.calendars.first().map(|c| c.id.as_str()))?;
        from_row(d, id, reference)
    });
    cal.unwrap_or_else(|| WorkingCalendar::default_week(include_weekends_default, reference)).with_max_scan(WEB_MAX_SCAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-09-07 is a Monday (the server's own tests).
    fn monday() -> Date {
        Date::new(2026, 9, 7).expect("date")
    }

    #[test]
    fn skips_the_weekend() {
        let c = WorkingCalendar::default_week(false, monday());
        assert_eq!(c.advance(0, 3), 3);
        assert_eq!(c.advance(0, 5), 5);
        assert_eq!(c.next_working(c.advance(0, 5)), 7);
        assert_eq!(c.advance(5, 1), 8);
    }

    #[test]
    fn honours_exceptions_both_ways() {
        let mut ex = HashMap::new();
        ex.insert(2, false);
        ex.insert(5, true);
        let c = WorkingCalendar::new([true, true, true, true, true, false, false], ex, monday());
        assert_eq!(c.advance(0, 3), 4);
        assert!(c.is_working(5));
    }

    #[test]
    fn retreat_mirrors_advance() {
        let c = WorkingCalendar::default_week(false, monday());
        let finish = c.advance(0, 5);
        assert_eq!(c.retreat(finish, 5), 0);
        assert_eq!(c.advance(5, 0), 5);
    }

    #[test]
    fn negative_offsets_resolve_the_weekday() {
        let c = WorkingCalendar::default_week(false, monday());
        // Offset -1 is the Sunday before, -2 the Saturday, -3 the Friday.
        assert!(!c.is_working(-1));
        assert!(!c.is_working(-2));
        assert!(c.is_working(-3));
    }

    #[test]
    fn prediction_falls_back_to_the_first_calendar() {
        use crate::model::{CalendarRow, Calendars};
        let row = |id: &str, sat: bool| CalendarRow { id: id.into(), project_id: String::new(), name: String::new(), mon: true, tue: true, wed: true, thu: true, fri: true, sat, sun: false, created_at: String::new(), rest: Default::default() };
        let data = Calendars { calendars: vec![row("c1", true), row("c2", false)], exceptions: vec![], project_calendar_id: None, rest: Default::default() };
        // The server ignores the list without a selected calendar; the web takes the first one.
        assert!(!for_schedule(Some(&data), monday(), false).is_working(5));
        assert!(for_prediction(Some(&data), monday(), false).is_working(5));
    }
}
