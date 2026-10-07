//! What the editor shows of a project, derived from it (`ProjectEditorPage.tsx`): the rows of the task table
//! (filter, sort, group, folded summaries), the row numbers the predecessor column speaks in, the scheduled
//! dates, the chart's extent, the status bar's counts and the CSV export.

use std::collections::{HashMap, HashSet};

use crate::date::Date;
use crate::model::{ProjectData, Task};

/// The Gantt's minimum span in days (`MIN_DAYS`).
pub const MIN_DAYS: i32 = 60;

/// The filter, sort and grouping of the table (`filterText`, `filterStatus`, `filterPriority`, `sortBy`,
/// `groupBy`, `ProjectEditorPage.tsx:301-316`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    pub text: String,
    /// `''` or a status (`not_started`, `in_progress`, `on_hold`, `completed`, `cancelled`).
    pub status: String,
    /// `''` or a priority (`low`, `medium`, `high`, `critical`).
    pub priority: String,
    /// `''` or `name`, `start`, `end`, `duration`, `priority`, `progress`, `status`.
    pub sort_by: String,
    /// `''` or `status`, `priority`, `resource`.
    pub group_by: String,
}

impl Filter {
    /// `filterActive`.
    pub fn active(&self) -> bool {
        !self.text.trim().is_empty() || !self.status.is_empty() || !self.priority.is_empty()
    }

    /// Something narrows or reorders the table (the filter bar stays open).
    pub fn any(&self) -> bool {
        self.active() || !self.sort_by.is_empty() || !self.group_by.is_empty()
    }
}

/// One row of the task table.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// Index into `ProjectData::tasks`.
    pub index: usize,
    pub depth: usize,
    pub has_children: bool,
}

/// `filteredIds`: the matching tasks and their ancestors (the hierarchy stays readable); `None` without a filter.
pub fn filtered_ids(data: &ProjectData, f: &Filter) -> Option<HashSet<String>> {
    if !f.active() {
        return None;
    }
    let q = f.text.trim().to_lowercase();
    let by_id: HashMap<&str, &Task> = data.tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut keep = HashSet::new();
    for t in &data.tasks {
        let ok = (q.is_empty() || t.name.to_lowercase().contains(&q)) && (f.status.is_empty() || t.status == f.status) && (f.priority.is_empty() || t.priority == f.priority);
        if ok {
            let mut cur = Some(t);
            let mut guard = 0;
            while let Some(c) = cur {
                if !keep.insert(c.id.clone()) || guard > 10_000 {
                    break;
                }
                guard += 1;
                cur = c.parent_id.as_deref().and_then(|p| by_id.get(p).copied());
            }
        }
    }
    Some(keep)
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum Key {
    Num(f64),
    Text(String),
}

fn rank(table: &[(&str, f64)], v: &str) -> f64 {
    table.iter().find(|(k, _)| *k == v).map(|(_, r)| *r).unwrap_or(9.0)
}

fn sort_key(data: &ProjectData, t: &Task, k: &str) -> Key {
    match k {
        "name" => Key::Text(t.name.to_lowercase()),
        "start" => Key::Num(t.es() as f64),
        "end" => Key::Num(t.early_finish.unwrap_or(t.es() + t.duration_days) as f64),
        "duration" => Key::Num(t.duration_days as f64),
        "progress" => Key::Num(t.progress as f64),
        "priority" => Key::Num(rank(&[("critical", 0.0), ("high", 1.0), ("medium", 2.0), ("low", 3.0)], &t.priority)),
        "status" => Key::Num(rank(&[("in_progress", 0.0), ("not_started", 1.0), ("on_hold", 2.0), ("completed", 3.0), ("cancelled", 4.0)], &t.status)),
        "resource" => Key::Text(match data.assignments.iter().find(|a| a.task_id == t.id) {
            Some(a) => data.resource(&a.resource_id).map(|r| r.name.clone()).unwrap_or_default(),
            None => "~".into(),
        }),
        _ => Key::Num(0.0),
    }
}

/// `visibleTasks`: the table's rows. Sorting or grouping flattens the outline (depth 0, no folding);
/// otherwise the outline is walked from the roots, skipping the children of folded summaries.
pub fn visible_rows(data: &ProjectData, f: &Filter, collapsed: &HashSet<String>) -> Vec<Row> {
    let ids = filtered_ids(data, f);
    let pass = |t: &Task| ids.as_ref().is_none_or(|s| s.contains(&t.id));
    if !f.sort_by.is_empty() || !f.group_by.is_empty() {
        let keys: Vec<&str> = [f.group_by.as_str(), f.sort_by.as_str()].into_iter().filter(|k| !k.is_empty()).collect();
        let mut base: Vec<usize> = (0..data.tasks.len()).filter(|&i| pass(&data.tasks[i])).collect();
        base.sort_by(|&a, &b| {
            for k in &keys {
                let (ka, kb) = (sort_key(data, &data.tasks[a], k), sort_key(data, &data.tasks[b], k));
                if ka < kb {
                    return std::cmp::Ordering::Less;
                }
                if ka > kb {
                    return std::cmp::Ordering::Greater;
                }
            }
            std::cmp::Ordering::Equal
        });
        return base.into_iter().map(|index| Row { index, depth: 0, has_children: false }).collect();
    }
    let mut kids: HashMap<Option<&str>, Vec<usize>> = HashMap::new();
    for (i, t) in data.tasks.iter().enumerate() {
        if pass(t) {
            kids.entry(t.parent_id.as_deref()).or_default().push(i);
        }
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    fn walk(data: &ProjectData, kids: &HashMap<Option<&str>, Vec<usize>>, pid: Option<&str>, depth: usize, collapsed: &HashSet<String>, out: &mut Vec<Row>, seen: &mut HashSet<usize>) {
        let Some(list) = kids.get(&pid) else { return };
        for &i in list {
            if !seen.insert(i) {
                continue;
            }
            let t = &data.tasks[i];
            let has = kids.get(&Some(t.id.as_str())).is_some_and(|k| !k.is_empty());
            out.push(Row { index: i, depth, has_children: has });
            if has && !collapsed.contains(&t.id) {
                walk(data, kids, Some(t.id.as_str()), depth + 1, collapsed, out, seen);
            }
        }
    }
    walk(data, &kids, None, 0, collapsed, &mut out, &mut seen);
    // No hierarchy found (every parent missing): the original order, flat.
    if out.is_empty() && !data.tasks.is_empty() && ids.is_none() {
        return (0..data.tasks.len()).map(|index| Row { index, depth: 0, has_children: false }).collect();
    }
    out
}

/// `displayTasks`: the board and the calendar apply the filter, flat.
pub fn display_tasks(data: &ProjectData, f: &Filter) -> Vec<usize> {
    let ids = filtered_ids(data, f);
    (0..data.tasks.len()).filter(|&i| ids.as_ref().is_none_or(|s| s.contains(&data.tasks[i].id))).collect()
}

/// `taskNumber`: the 1-based number of each task in the stored order (the predecessor column's language).
pub fn task_numbers(data: &ProjectData) -> HashMap<String, usize> {
    data.tasks.iter().enumerate().map(|(i, t)| (t.id.clone(), i + 1)).collect()
}

/// `predecessorText`: « 1;3 » — the numbers of the task's predecessors, ascending.
pub fn predecessor_text(data: &ProjectData, task_id: &str) -> String {
    let numbers = task_numbers(data);
    let mut n: Vec<usize> = data.dependencies.iter().filter(|d| d.to_task_id == task_id).filter_map(|d| numbers.get(&d.from_task_id).copied()).collect();
    n.sort_unstable();
    n.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(";")
}

/// JavaScript's `parseInt` of a trimmed token: leading digits (a sign allowed), else NaN (`None`).
fn js_parse_int(s: &str) -> Option<i64> {
    let s = s.trim();
    let (neg, digits) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let d: String = digits.chars().take_while(|c| c.is_ascii_digit()).collect();
    let v: i64 = d.parse().ok()?;
    Some(if neg { -v } else { v })
}

/// What `setPredecessors(taskId, text)` would change: the dependencies to delete (ids) and the predecessor
/// task ids to link (FS), from « 1;3 » (separators `;`, `,`, spaces; numbers outside the table and the task
/// itself ignored).
pub fn predecessors_plan(data: &ProjectData, task_id: &str, text: &str) -> (Vec<String>, Vec<String>) {
    let mut wanted: Vec<String> = Vec::new();
    for tok in text.split([';', ',', ' ', '\t', '\n', '\r']) {
        if let Some(n) = js_parse_int(tok).filter(|n| *n >= 1) {
            if let Some(t) = data.tasks.get((n - 1) as usize) {
                if t.id != task_id && !wanted.contains(&t.id) {
                    wanted.push(t.id.clone());
                }
            }
        }
    }
    let current: Vec<_> = data.dependencies.iter().filter(|d| d.to_task_id == task_id).collect();
    let removed = current.iter().filter(|d| !wanted.contains(&d.from_task_id)).map(|d| d.id.clone()).collect();
    let have: HashSet<&str> = current.iter().map(|d| d.from_task_id.as_str()).collect();
    let added = wanted.into_iter().filter(|f| !have.contains(f.as_str())).collect();
    (removed, added)
}

/// `schedStart`: the bar's first day.
pub fn sched_start(t: &Task, project_start: Date) -> Date {
    project_start.add_days(t.es() as i64)
}

/// `schedEnd`: the bar's last day (the exclusive finish minus one, never before day 0).
pub fn sched_end(t: &Task, project_start: Date) -> Date {
    project_start.add_days((t.cal_end() - 1).max(0) as i64)
}

/// `projectFinishDay`: the latest computed finish (`(early_finish ?? 0) || es + duration`).
pub fn project_finish_day(data: &ProjectData) -> i32 {
    data.tasks
        .iter()
        .map(|t| {
            let ef = t.early_finish.unwrap_or(0);
            if ef != 0 {
                ef
            } else {
                t.es() + t.duration_days
            }
        })
        .fold(0, i32::max)
}

/// `targetEndDay`: the target end as an offset from day 0, when it is not before it.
pub fn target_end_day(data: &ProjectData, project_start: Date) -> Option<i32> {
    let end = data.project.end()?;
    let d = end.days_since(project_start) as i32;
    (d >= 0).then_some(d)
}

/// `totalDays`.
pub fn total_days(data: &ProjectData, project_start: Date) -> i32 {
    let finish = project_finish_day(data);
    let target = target_end_day(data, project_start).map(|t| t + 5).unwrap_or(0);
    MIN_DAYS.max(finish + 5).max(target)
}

/// The status bar's figures (`ProjectEditorPage.tsx:2416-2434`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusCounts {
    pub tasks: usize,
    pub critical: usize,
    pub completed: usize,
    /// `(finish day shown, target day, overrun days)` when the project has a target end.
    pub finish_vs_target: Option<(Date, Date, i32)>,
}

pub fn status_counts(data: &ProjectData, project_start: Date) -> StatusCounts {
    let finish = project_finish_day(data);
    let fvt = target_end_day(data, project_start).map(|target| (project_start.add_days((finish - 1).max(0) as i64), project_start.add_days(target as i64), finish - target));
    StatusCounts {
        tasks: data.tasks.len(),
        critical: data.tasks.iter().filter(|t| t.is_critical).count(),
        completed: data.tasks.iter().filter(|t| t.status == "completed").count(),
        finish_vs_target: fvt,
    }
}

/// The CSV headers of `exportCsv`, in the UI language.
pub struct CsvHeaders<'a> {
    pub name: &'a str,
    pub status: &'a str,
    pub priority: &'a str,
    pub start: &'a str,
    pub end: &'a str,
    pub duration: &'a str,
    pub predecessors: &'a str,
    pub resources: &'a str,
}

/// `exportCsv`: BOM, CRLF, every cell quoted (`"` doubled), the stored order; dates are the scheduled ones
/// and parents show their rolled-up completion.
pub fn export_csv(data: &ProjectData, project_start: Date, h: &CsvHeaders<'_>) -> String {
    let esc = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let progress = crate::rollup::effective_progress(&data.tasks);
    let header = ["WBS", h.name, "Type", h.status, h.priority, h.start, h.end, h.duration, "%", h.predecessors, h.resources];
    let mut lines = vec![header.iter().map(|c| esc(c)).collect::<Vec<_>>().join(",")];
    for t in &data.tasks {
        let res: Vec<String> = data.assignments.iter().filter(|a| a.task_id == t.id).filter_map(|a| data.resource(&a.resource_id).map(|r| r.name.clone())).filter(|n| !n.is_empty()).collect();
        let cells = [
            t.wbs.clone(),
            t.name.clone(),
            t.task_type.clone(),
            t.status.clone(),
            t.priority.clone(),
            sched_start(t, project_start).iso(),
            sched_end(t, project_start).iso(),
            t.duration_days.to_string(),
            progress.get(&t.id).copied().unwrap_or(t.progress).to_string(),
            predecessor_text(data, &t.id),
            res.join(", "),
        ];
        lines.push(cells.iter().map(|c| esc(c)).collect::<Vec<_>>().join(","));
    }
    format!("\u{feff}{}", lines.join("\r\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Assignment, Dependency, Resource};

    fn task(id: &str, parent: Option<&str>, name: &str) -> Task {
        let mut t = Task::new(id, "p");
        t.parent_id = parent.map(str::to_string);
        t.name = name.into();
        t
    }

    fn data() -> ProjectData {
        let mut d = ProjectData::default();
        d.project.start_date = Some("2026-09-07".into());
        d.tasks = vec![task("a", None, "Phase"), task("a1", Some("a"), "Dig"), task("a2", Some("a"), "Pour"), task("b", None, "Inspect")];
        d.tasks[2].priority = "high".into();
        d.tasks[3].status = "completed".into();
        d.dependencies.push(Dependency { id: "d1".into(), project_id: "p".into(), from_task_id: "a1".into(), to_task_id: "a2".into(), dep_type: "FS".into(), lag_days: 0, rest: Default::default() });
        d
    }

    #[test]
    fn outline_rows_fold_and_filter() {
        let d = data();
        let rows = visible_rows(&d, &Filter::default(), &HashSet::new());
        assert_eq!(rows.iter().map(|r| (r.index, r.depth, r.has_children)).collect::<Vec<_>>(), vec![(0, 0, true), (1, 1, false), (2, 1, false), (3, 0, false)]);
        let folded: HashSet<String> = ["a".to_string()].into();
        assert_eq!(visible_rows(&d, &Filter::default(), &folded).len(), 2);
        let f = Filter { text: "pou".into(), ..Default::default() };
        // The match and its ancestor.
        assert_eq!(visible_rows(&d, &f, &HashSet::new()).iter().map(|r| r.index).collect::<Vec<_>>(), vec![0, 2]);
    }

    #[test]
    fn sorting_flattens() {
        let d = data();
        let f = Filter { sort_by: "priority".into(), ..Default::default() };
        let rows = visible_rows(&d, &f, &HashSet::new());
        assert_eq!(rows[0].index, 2);
        assert!(rows.iter().all(|r| r.depth == 0));
    }

    #[test]
    fn predecessors_as_numbers() {
        let mut d = data();
        assert_eq!(predecessor_text(&d, "a2"), "2");
        let (removed, added) = predecessors_plan(&d, "a2", "4; 1,3 x 99 0");
        assert_eq!(removed, vec!["d1".to_string()]);
        assert_eq!(added, vec!["b".to_string(), "a".to_string()]);
        d.dependencies.clear();
        assert_eq!(predecessor_text(&d, "a2"), "");
    }

    #[test]
    fn exports_the_web_csv() {
        let mut d = data();
        d.tasks[1].early_start = Some(0);
        d.tasks[1].early_finish = Some(3);
        d.tasks[1].name = "Say \"hi\"".into();
        d.resources.push(Resource { id: "r".into(), project_id: "p".into(), name: "Ana".into(), role: String::new(), color: "#000".into(), capacity: 1.0, hourly_rate: None, user_id: None, avatar_url: None, kind: "person".into(), unit_label: None, overtime_rate: None, cost_per_use: None, skills: vec![], created_at: String::new(), rest: Default::default() });
        d.assignments.push(Assignment { id: "x".into(), task_id: "a1".into(), resource_id: "r".into(), units: 1.0, rest: Default::default() });
        let h = CsvHeaders { name: "Nom", status: "Statut", priority: "Priorité", start: "Début", end: "Fin", duration: "Durée", predecessors: "Préd.", resources: "Ressources" };
        let csv = export_csv(&d, Date::new(2026, 9, 7).expect("d"), &h);
        assert!(csv.starts_with("\u{feff}\"WBS\",\"Nom\""));
        let line = csv.split("\r\n").nth(2).expect("line");
        assert_eq!(line, "\"\",\"Say \"\"hi\"\"\",\"task\",\"not_started\",\"medium\",\"2026-09-07\",\"2026-09-09\",\"1\",\"0\",\"\",\"Ana\"");
    }

    #[test]
    fn extent_and_status() {
        let mut d = data();
        d.tasks[3].early_start = Some(10);
        d.tasks[3].early_finish = Some(12);
        d.project.end_date = Some("2026-09-17".into());
        let start = Date::new(2026, 9, 7).expect("d");
        assert_eq!(project_finish_day(&d), 12);
        assert_eq!(target_end_day(&d, start), Some(10));
        assert_eq!(total_days(&d, start), 60);
        let s = status_counts(&d, start);
        assert_eq!(s.completed, 1);
        assert_eq!(s.finish_vs_target.map(|f| f.2), Some(2));
    }
}
