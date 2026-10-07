//! Golden schedules: projects built on a development core (`tools/golden.ps1`,
//! vskubuno docs/PROJECTS-DESKTOP.md §4), read before the schedule, then scheduled by the office server
//! (`POST /projects/:id/cpm`). The core's scheduler must land every task on the server's figures.
//!
//! The cases cover the four link types, positive and negative lags, every constraint (SNET, SNLT, FNET, FNLT,
//! MSO, MFO, ALAP), a missed deadline, nested summaries, a link leaving a summary, the instance's default week
//! (no project calendar) and a six-day calendar with a closed weekday and a worked Sunday.

use kubuno_office_projects_core::calendar;
use kubuno_office_projects_core::cpm;
use kubuno_office_projects_core::model::{Calendars, ProjectData, Task};
use serde_json::Value;

fn case(name: &str) -> (ProjectData, Calendars, Vec<Task>) {
    let path = format!("{}/tests/fixtures/golden-{name}.json", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let v: Value = serde_json::from_str(&text).expect("fixture JSON");
    let before: ProjectData = serde_json::from_value(v["before"].clone()).expect("project");
    let cals: Calendars = serde_json::from_value(v["calendars"].clone()).expect("calendars");
    let scheduled: Vec<Task> = serde_json::from_value(v["scheduled"]["tasks"].clone()).expect("scheduled tasks");
    (before, cals, scheduled)
}

fn check(name: &str) {
    let (before, cals, expected) = case(name);
    let start = before.project.start().expect("the golden projects have a start date");
    // The development core runs with the instance default (weekends not worked).
    let cal = calendar::for_schedule(Some(&cals), start, false);
    let got = cpm::schedule(&before.tasks, &before.dependencies, start, &cal).expect("scheduled");
    assert_eq!(got.len(), expected.len());
    let mut mismatches = Vec::new();
    for (t, s) in before.tasks.iter().zip(&got) {
        let e = expected.iter().find(|x| x.id == t.id).expect("task in the server's answer");
        let server = (e.early_start, e.early_finish, e.late_start, e.late_finish, e.total_float, e.free_float, e.is_critical, e.deadline_missed, e.duration_days);
        let core = (Some(s.early_start), Some(s.early_finish), Some(s.late_start), Some(s.late_finish), Some(s.total_float), Some(s.free_float), s.is_critical, s.deadline_missed, s.duration_days);
        if server != core {
            mismatches.push(format!("{}: server {server:?} core {core:?}", t.name));
        }
    }
    assert!(mismatches.is_empty(), "{name}:\n{}", mismatches.join("\n"));
}

#[test]
fn default_week_matches_the_server() {
    check("default-week");
}

#[test]
fn six_day_calendar_with_exceptions_matches_the_server() {
    check("six-days-holidays");
}

#[test]
fn the_fixtures_round_trip_losslessly() {
    for name in ["default-week", "six-days-holidays"] {
        let path = format!("{}/tests/fixtures/golden-{name}.json", env!("CARGO_MANIFEST_DIR"));
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        let data: ProjectData = serde_json::from_value(v["before"].clone()).expect("project");
        assert_eq!(serde_json::to_value(&data).expect("json"), v["before"], "{name}");
    }
}
