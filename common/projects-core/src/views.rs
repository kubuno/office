//! The data of the plan's other views, laid out as the web lays them out:
//!
//! * the board (`project/BoardView.tsx`): the leaves grouped by status;
//! * the month calendar (`project/CalendarView.tsx`): six weeks from the Monday before the 1st, each leaf on
//!   the days it spans;
//! * the resource load (`project/ResourceLoadView.tsx`): units per resource per day;
//! * the timeline band (`project/TimelineBand.tsx`): the phases and milestones packed on lanes.
//! * the roadmap (`project/RoadmapView.tsx`): the versions with their tasks and progress.

use std::collections::HashMap;

use crate::date::Date;
use crate::model::{ProjectData, Task};
use crate::view::{sched_end, sched_start};

/// The board's columns: status, French label, colour (`BOARD_COLS`).
pub const BOARD_COLUMNS: [(&str, &str, &str); 5] = [
    ("not_started", "À faire", "#9aa0a6"),
    ("in_progress", "En cours", "#1a73e8"),
    ("on_hold", "En attente", "#fbbc04"),
    ("completed", "Terminé", "#34a853"),
    ("cancelled", "Annulé", "#d93025"),
];

/// The priority colours (`PRIO_CLR`, `PRIO_COLOR`).
pub fn priority_color(p: &str) -> &'static str {
    match p {
        "low" => "#34a853",
        "medium" => "#fbbc04",
        "high" => "#ea4335",
        "critical" => "#b80672",
        _ => "#9aa0a6",
    }
}

/// The board: for each column of [`BOARD_COLUMNS`], the indexes (into `data.tasks`) of its cards — every task
/// of `shown` that is not of type summary, in order.
pub fn board(data: &ProjectData, shown: &[usize]) -> Vec<Vec<usize>> {
    BOARD_COLUMNS
        .iter()
        .map(|(st, _, _)| shown.iter().copied().filter(|&i| !data.tasks[i].is_summary_type() && data.tasks[i].status == *st).collect())
        .collect()
}

/// One day of the month grid.
#[derive(Debug, Clone, PartialEq)]
pub struct CalendarDay {
    pub date: Date,
    pub in_month: bool,
    /// The leaves on that day (indexes into `data.tasks`), in order; the web shows three and « +n ».
    pub items: Vec<usize>,
}

/// The month `month_offset` months after the project start's month: 42 days from the Monday on or before its
/// first day.
pub fn month_grid(data: &ProjectData, shown: &[usize], project_start: Date, month_offset: i32) -> (Date, Vec<CalendarDay>) {
    let first = project_start.first_of_month();
    let total = first.year * 12 + (first.month as i32 - 1) + month_offset;
    let month = Date { year: total.div_euclid(12), month: (total.rem_euclid(12) + 1) as u32, day: 1 };
    let grid_start = month.add_days(-(month.weekday_mon0() as i64));
    let spans: Vec<(usize, Date, Date)> = shown.iter().copied().filter(|&i| !data.tasks[i].is_summary_type()).map(|i| (i, sched_start(&data.tasks[i], project_start), sched_end(&data.tasks[i], project_start))).collect();
    let days = (0..42)
        .map(|k| {
            let date = grid_start.add_days(k);
            CalendarDay { date, in_month: date.year == month.year && date.month == month.month, items: spans.iter().filter(|(_, s, e)| date >= *s && date <= *e).map(|(i, _, _)| *i).collect() }
        })
        .collect();
    (month, days)
}

/// Units allocated per resource per day over `[0, total_days)` (summaries excluded; each assignment counts
/// its units on every calendar day of its task's bar).
pub fn resource_load(data: &ProjectData, total_days: i32) -> HashMap<String, Vec<f64>> {
    let n = total_days.max(0) as usize;
    let mut m: HashMap<String, Vec<f64>> = data.resources.iter().map(|r| (r.id.clone(), vec![0.0; n])).collect();
    for a in &data.assignments {
        let (Some(t), Some(arr)) = (data.task(&a.task_id), m.get_mut(&a.resource_id)) else { continue };
        if t.is_summary_type() {
            continue;
        }
        let (s, e) = (t.es(), t.cal_end());
        for d in s.max(0)..e.min(total_days) {
            arr[d as usize] += a.units;
        }
    }
    m
}

/// The timeline band's constants (`LANE_H`, `LANE_GAP`, `PAD_Y`, `MAX_LANES`, `MIN_TRACK_H`, `DATE_GUTTER`).
pub const LANE_H: f64 = 20.0;
pub const LANE_GAP: f64 = 3.0;
pub const PAD_Y: f64 = 6.0;
pub const MAX_LANES: usize = 6;
pub const MIN_TRACK_H: f64 = 48.0;
pub const DATE_GUTTER: f64 = 64.0;

/// One item of the timeline band.
#[derive(Debug, Clone, PartialEq)]
pub struct BandItem {
    pub index: usize,
    pub off: i32,
    /// x within the inner track (the date gutters excluded), width, lane.
    pub x: f64,
    pub w: f64,
    pub lane: usize,
    pub milestone: bool,
}

/// `layoutItems`: the top-level tasks and every milestone, packed greedily on lanes (earliest first, the
/// widest of a tie first, the first lane free at its x).
pub fn band_layout(data: &ProjectData, project_start: Date, span: i32, inner_w: f64) -> (Vec<BandItem>, usize) {
    let span = span.max(1) as f64;
    let mut placed: Vec<BandItem> = Vec::new();
    for (i, t) in data.tasks.iter().enumerate() {
        if !(t.parent_id.is_none() || t.is_milestone()) {
            continue;
        }
        let off = t.early_start.unwrap_or_else(|| {
            let d = t.start_date.as_deref().and_then(Date::parse).unwrap_or(project_start);
            d.days_since(project_start).max(0) as i32
        });
        let milestone = t.is_milestone();
        let cal_days = if milestone { 0 } else { (t.early_finish.unwrap_or(off + t.duration_days) - off).max(0) };
        let x = (off as f64 / span) * inner_w;
        let w = if milestone { 20.0 } else { ((cal_days as f64 / span) * inner_w).max(4.0) };
        placed.push(BandItem { index: i, off, x: if milestone { x - 10.0 } else { x }, w, lane: 0, milestone });
    }
    placed.sort_by(|a, b| a.off.cmp(&b.off).then(b.w.partial_cmp(&a.w).unwrap_or(std::cmp::Ordering::Equal)));
    let mut lane_end: Vec<f64> = Vec::new();
    for it in placed.iter_mut() {
        let lane = match lane_end.iter().position(|&end| end <= it.x - 2.0) {
            Some(l) => l,
            None if lane_end.len() < MAX_LANES => {
                lane_end.push(0.0);
                lane_end.len() - 1
            }
            None => {
                let min = lane_end.iter().copied().fold(f64::INFINITY, f64::min);
                lane_end.iter().position(|&e| e == min).unwrap_or(0)
            }
        };
        it.lane = lane;
        lane_end[lane] = it.x + it.w;
    }
    let lanes = lane_end.len().max(1);
    (placed, lanes)
}

/// The band's track height for `lanes` lanes, and the y of a lane.
pub fn band_track_h(lanes: usize) -> f64 {
    MIN_TRACK_H.max(2.0 * PAD_Y + lanes as f64 * LANE_H + (lanes as f64 - 1.0) * LANE_GAP)
}

pub fn band_lane_y(lanes: usize, lane: usize) -> f64 {
    let track = band_track_h(lanes);
    let top = (track - (lanes as f64 * LANE_H + (lanes as f64 - 1.0) * LANE_GAP)) / 2.0;
    top + lane as f64 * (LANE_H + LANE_GAP)
}

/// The resources of a task (avatars on the bar and in the table: initial + colour).
pub fn assignees<'a>(data: &'a ProjectData, task: &Task) -> Vec<&'a crate::model::Resource> {
    data.assignments.iter().filter(|a| a.task_id == task.id).filter_map(|a| data.resource(&a.resource_id)).collect()
}

/// The first letter of a name, upper-cased (`r.name[0]?.toUpperCase()`), « ? » when empty.
pub fn initial(name: &str) -> String {
    name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "?".into())
}

/// A task under a roadmap version.
#[derive(Debug, Clone, PartialEq)]
pub struct RoadmapTask {
    pub id: String,
    pub name: String,
    pub milestone: bool,
    /// `completed` or `cancelled` (`CLOSED_STATUS`): struck through.
    pub done: bool,
}

/// A version card of the roadmap.
#[derive(Debug, Clone, PartialEq)]
pub struct RoadmapCard {
    pub id: String,
    pub name: String,
    /// `open`, `locked` or `closed`.
    pub status: String,
    pub due_date: Option<String>,
    pub tasks: Vec<RoadmapTask>,
    pub done: usize,
    /// `round(done / total * 100)`, 0 without tasks.
    pub pct: i32,
}

/// The roadmap's cards in the server's order (`position`, `name`), the closed versions only with
/// « Versions terminées »; each version's tasks in the project's order.
pub fn roadmap(versions: &[crate::model::Version], data: &ProjectData, show_closed: bool) -> Vec<RoadmapCard> {
    let s = |v: &crate::model::Version, k: &str| v.rest.get(k).and_then(serde_json::Value::as_str).map(str::to_string);
    versions
        .iter()
        .filter(|v| show_closed || s(v, "status").as_deref() != Some("closed"))
        .map(|v| {
            let tasks: Vec<RoadmapTask> = data
                .tasks
                .iter()
                .filter(|t| t.version_id.as_deref() == Some(v.id.as_str()))
                .map(|t| RoadmapTask { id: t.id.clone(), name: t.name.clone(), milestone: t.task_type == "milestone", done: t.status == "completed" || t.status == "cancelled" })
                .collect();
            let done = tasks.iter().filter(|t| t.done).count();
            let pct = if tasks.is_empty() { 0 } else { crate::rollup::js_round(done as f64 / tasks.len() as f64 * 100.0) as i32 };
            RoadmapCard { id: v.id.clone(), name: v.name.clone(), status: s(v, "status").unwrap_or_else(|| "open".into()), due_date: s(v, "due_date").filter(|d| !d.is_empty()), tasks, done, pct }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Assignment, Resource};

    fn task(id: &str, parent: Option<&str>, es: i32, ef: i32, ty: &str) -> Task {
        let mut t = Task::new(id, "p");
        t.parent_id = parent.map(str::to_string);
        t.early_start = Some(es);
        t.early_finish = Some(ef);
        t.duration_days = ef - es;
        t.task_type = ty.into();
        t
    }

    fn data() -> ProjectData {
        let mut d = ProjectData { tasks: vec![task("ph", None, 0, 10, "summary"), task("a", Some("ph"), 0, 4, "task"), task("m", Some("ph"), 4, 4, "milestone"), task("b", None, 2, 6, "task")], ..Default::default() };
        d.tasks[1].status = "in_progress".into();
        d.resources.push(Resource { id: "r".into(), project_id: "p".into(), name: "ana".into(), role: String::new(), color: "#000".into(), capacity: 1.0, hourly_rate: None, user_id: None, avatar_url: None, kind: "person".into(), unit_label: None, overtime_rate: None, cost_per_use: None, skills: vec![], created_at: String::new(), rest: Default::default() });
        d.assignments.push(Assignment { id: "x".into(), task_id: "a".into(), resource_id: "r".into(), units: 1.0, rest: Default::default() });
        d.assignments.push(Assignment { id: "y".into(), task_id: "b".into(), resource_id: "r".into(), units: 0.5, rest: Default::default() });
        d
    }

    #[test]
    fn board_and_calendar() {
        let d = data();
        let cols = board(&d, &[0, 1, 2, 3]);
        assert_eq!(cols[0], vec![2, 3]);
        assert_eq!(cols[1], vec![1]);
        // 2026-10-07 (a Wednesday): October's grid starts Monday 2026-09-28.
        let start = Date::new(2026, 10, 7).expect("d");
        let (month, days) = month_grid(&d, &[0, 1, 2, 3], start, 0);
        assert_eq!(month, Date::new(2026, 10, 1).expect("d"));
        assert_eq!(days[0].date, Date::new(2026, 9, 28).expect("d"));
        let oct9 = days.iter().find(|x| x.date == Date::new(2026, 10, 9).expect("d")).expect("day");
        assert_eq!(oct9.items, vec![1, 3]);
        assert_eq!(month_grid(&d, &[], start, -10).0, Date::new(2025, 12, 1).expect("d"));
    }

    #[test]
    fn load_and_band() {
        let d = data();
        let load = resource_load(&d, 8);
        assert_eq!(load["r"], vec![1.0, 1.0, 1.5, 1.5, 0.5, 0.5, 0.0, 0.0]);
        let (items, lanes) = band_layout(&d, Date::new(2026, 10, 7).expect("d"), 10, 1000.0);
        // ph (0, wide) takes lane 0, b overlaps it → lane 1 (ends at 600), the milestone at 390 → lane 2.
        assert_eq!(lanes, 3);
        assert_eq!(items.iter().find(|i| i.index == 0).map(|i| i.lane), Some(0));
        assert_eq!(items.iter().find(|i| i.index == 3).map(|i| i.lane), Some(1));
        assert_eq!(items.iter().find(|i| i.index == 2).map(|i| i.lane), Some(2));
        assert_eq!(band_track_h(1), 48.0);
        assert_eq!(initial("ana"), "A");
    }

    #[test]
    fn roadmap_cards() {
        let mut d = data();
        d.tasks[1].version_id = Some("v".into());
        d.tasks[1].status = "completed".into();
        d.tasks[2].version_id = Some("v".into());
        let mut rest = serde_json::Map::new();
        rest.insert("status".into(), serde_json::json!("closed"));
        let vs = vec![crate::model::Version { id: "v".into(), name: "1.0".into(), rest }];
        assert!(roadmap(&vs, &d, false).is_empty());
        let c = &roadmap(&vs, &d, true)[0];
        assert_eq!((c.done, c.tasks.len(), c.pct, c.due_date.clone()), (1, 2, 50, None));
        assert!(c.tasks[0].done);
    }
}
