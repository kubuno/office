//! Working calendars — which days count as work, and how to move across them.
//!
//! Deliberately self-contained: it knows about days, not about projects. A
//! schedule, a resource's availability, a leave register or an instance-wide
//! business calendar can all be expressed with the same two questions — "is this
//! day worked?" and "where do I land after N worked days?" — so the answer lives
//! here rather than inside the scheduler.
//!
//! Everything is expressed in **day offsets** relative to a reference date, which
//! is what the scheduler works in; converting a real date is the caller's job.
use chrono::{Datelike, NaiveDate, Weekday};
use std::collections::HashMap;

/// A week's shape plus the days that depart from it.
#[derive(Debug, Clone)]
pub struct WorkingCalendar {
    /// Monday first, Sunday last.
    week: [bool; 7],
    /// Day offset → worked or not, overriding the weekly pattern. Holidays and
    /// closures sit here as `false`; a worked Saturday sits here as `true`.
    exceptions: HashMap<i32, bool>,
}

impl Default for WorkingCalendar {
    /// Monday to Friday, nothing exceptional — the shape most plans assume.
    fn default() -> Self {
        Self { week: [true, true, true, true, true, false, false], exceptions: HashMap::new() }
    }
}

impl WorkingCalendar {
    pub fn new(week: [bool; 7], exceptions: HashMap<i32, bool>) -> Self {
        Self { week, exceptions }
    }

    /// Build from the stored booleans and a list of `(date, is_working)` overrides,
    /// converting dates to offsets from `reference`.
    pub fn from_parts(
        week: [bool; 7],
        exceptions: impl IntoIterator<Item = (NaiveDate, bool)>,
        reference: NaiveDate,
    ) -> Self {
        let map = exceptions
            .into_iter()
            .map(|(day, working)| ((day - reference).num_days() as i32, working))
            .collect();
        Self { week, exceptions: map }
    }

    /// A calendar with no working day at all would make every scheduling loop
    /// run forever. Callers building one from user input must check this first.
    pub fn has_working_day(&self) -> bool {
        self.week.iter().any(|d| *d) || self.exceptions.values().any(|w| *w)
    }

    pub fn is_working(&self, offset: i32, reference: NaiveDate) -> bool {
        if let Some(&forced) = self.exceptions.get(&offset) {
            return forced;
        }
        let day = reference + chrono::Duration::days(offset as i64);
        self.week[match day.weekday() {
            Weekday::Mon => 0, Weekday::Tue => 1, Weekday::Wed => 2, Weekday::Thu => 3,
            Weekday::Fri => 4, Weekday::Sat => 5, Weekday::Sun => 6,
        }]
    }

    /// The first worked day at or after `offset`. A task cannot start on a closed day.
    pub fn next_working(&self, offset: i32, reference: NaiveDate) -> i32 {
        // Bounded so a calendar that turns out to be empty cannot hang the request.
        (offset..offset + MAX_SCAN)
            .find(|&o| self.is_working(o, reference))
            .unwrap_or(offset)
    }

    /// Where a task starting at `start` and lasting `working_days` finishes.
    ///
    /// The result is the offset of the day AFTER the last worked one, matching the
    /// scheduler's exclusive-finish convention: a one-day task starting Friday
    /// finishes at the following Monday's offset when the weekend is closed.
    pub fn advance(&self, start: i32, working_days: i32, reference: NaiveDate) -> i32 {
        if working_days <= 0 {
            // A milestone has no duration: it sits on its day, worked or not.
            return start;
        }
        let mut remaining = working_days;
        let from = self.next_working(start, reference);
        for o in from..from + MAX_SCAN {
            if self.is_working(o, reference) {
                remaining -= 1;
                if remaining == 0 { return o + 1; }
            }
        }
        start + working_days
    }

    /// Count the working days in the half-open range `[start, finish)` — the worked
    /// duration a summary spans once rolled up from `early_start` to `early_finish`.
    pub fn working_days_between(&self, start: i32, finish: i32, reference: NaiveDate) -> i32 {
        if finish <= start {
            return 0;
        }
        (start..finish).filter(|&o| self.is_working(o, reference)).count() as i32
    }

    /// The mirror of [`advance`]: where a task must start to finish at `finish`.
    pub fn retreat(&self, finish: i32, working_days: i32, reference: NaiveDate) -> i32 {
        if working_days <= 0 {
            return finish;
        }
        let mut remaining = working_days;
        for o in (finish - MAX_SCAN..finish).rev() {
            if self.is_working(o, reference) {
                remaining -= 1;
                if remaining == 0 { return o; }
            }
        }
        finish - working_days
    }
}

/// Upper bound on how far the scanners will look for a working day. Generous
/// enough for any real closure (a year of shutdown), small enough that a
/// misconfigured calendar fails fast instead of spinning.
const MAX_SCAN: i32 = 4000;

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-09-07 is a Monday.
    fn monday() -> NaiveDate { NaiveDate::from_ymd_opt(2026, 9, 7).expect("valid date") }

    #[test]
    fn skips_the_weekend() {
        let c = WorkingCalendar::default();
        // Three days from Monday finish Thursday morning: offset 3.
        assert_eq!(c.advance(0, 3, monday()), 3);
        // Five days fill Mon..Fri; the exclusive finish is Saturday's offset. A
        // successor linked to it lands on Monday, because `next_working` moves it.
        assert_eq!(c.advance(0, 5, monday()), 5);
        assert_eq!(c.next_working(c.advance(0, 5, monday()), monday()), 7);
        // Starting Saturday really starts Monday.
        assert_eq!(c.advance(5, 1, monday()), 8);
    }

    #[test]
    fn honours_exceptions_both_ways() {
        let mut ex = HashMap::new();
        ex.insert(2, false);            // Wednesday closed
        ex.insert(5, true);             // Saturday worked
        let c = WorkingCalendar::new([true; 5].iter().chain([false, false].iter())
            .copied().collect::<Vec<_>>().try_into().expect("7 days"), ex);
        // Mon, Tue, (Wed off), Thu → three days finish at offset 4.
        assert_eq!(c.advance(0, 3, monday()), 4);
        assert!(c.is_working(5, monday()));
    }

    #[test]
    fn retreat_mirrors_advance() {
        let c = WorkingCalendar::default();
        let finish = c.advance(0, 5, monday());
        assert_eq!(c.retreat(finish, 5, monday()), 0);
    }

    #[test]
    fn milestones_have_no_duration() {
        let c = WorkingCalendar::default();
        assert_eq!(c.advance(5, 0, monday()), 5);
    }
}
