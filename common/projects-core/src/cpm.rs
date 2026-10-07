//! The critical-path scheduler — the port of the office server's `compute_cpm`
//! (`server/src/handlers/projects.rs:1223-1577`, `POST /projects/:id/cpm`), statement for statement, so a
//! plan scheduled by the desktop (offline, or to show a drop before the server answers) lands on the very
//! offsets the server would store.
//!
//! Everything is in day offsets from the project start; durations are worked days of the project's
//! [`WorkingCalendar`]; finishes are exclusive.

use std::collections::{HashMap, VecDeque};

use crate::calendar::WorkingCalendar;
use crate::date::Date;
use crate::model::{Dependency, Task};

/// What the scheduler writes back on one task (`UPDATE tasks SET …`, `projects.rs:1536-1556`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Scheduled {
    pub early_start: i32,
    pub early_finish: i32,
    pub late_start: i32,
    pub late_finish: i32,
    pub total_float: i32,
    pub free_float: i32,
    pub is_critical: bool,
    pub deadline_missed: bool,
    /// The stored duration: unchanged for a leaf, the worked days of the rolled-up span for a parent.
    pub duration_days: i32,
}

/// Why a plan cannot be scheduled (the server answers 422 with these messages).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpmError {
    /// « Le calendrier du projet n'a aucun jour travaillé. »
    NoWorkingDay,
    /// « Les liaisons forment une boucle : impossible de planifier {names}. » — the first five tasks left out.
    Loop(Vec<String>),
}

impl std::fmt::Display for CpmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CpmError::NoWorkingDay => f.write_str("Le calendrier du projet n'a aucun jour travaillé."),
            CpmError::Loop(names) => write!(f, "Les liaisons forment une boucle : impossible de planifier {}.", names.join(", ")),
        }
    }
}

/// Schedules `tasks` (in the server's order: `ORDER BY position`) linked by `deps`. `project_start` turns the
/// constraint and deadline dates into offsets (a project without a start date is anchored on today, as the
/// server does). Returns one [`Scheduled`] per task, in the same order.
pub fn schedule(tasks: &[Task], deps: &[Dependency], project_start: Date, calendar: &WorkingCalendar) -> Result<Vec<Scheduled>, CpmError> {
    if tasks.is_empty() {
        return Ok(Vec::new());
    }
    if !calendar.has_working_day() {
        return Err(CpmError::NoWorkingDay);
    }
    let to_offset = |s: &str| Date::parse(s).map(|d| d.days_since(project_start) as i32);
    let finish_of = |start: i32, dur: i32| calendar.advance(start, dur);
    let start_of = |finish: i32, dur: i32| calendar.retreat(finish, dur);
    let workable = |offset: i32| calendar.next_working(offset);

    let n = tasks.len();
    let task_idx: HashMap<&str, usize> = tasks.iter().enumerate().map(|(i, t)| (t.id.as_str(), i)).collect();

    let mut in_degree = vec![0usize; n];
    let mut adj: Vec<Vec<(usize, &str, i32)>> = vec![Vec::new(); n];
    for dep in deps {
        if let (Some(&from), Some(&to)) = (task_idx.get(dep.from_task_id.as_str()), task_idx.get(dep.to_task_id.as_str())) {
            adj[from].push((to, dep.dep_type.as_str(), dep.lag_days));
            in_degree[to] += 1;
        }
    }

    // Forward pass (ES/EF): constraints are lower bounds on the early start, like links.
    let mut early_start = vec![0i32; n];
    let mut early_finish = vec![0i32; n];
    for (i, t) in tasks.iter().enumerate() {
        if let Some(at) = t.constraint_date.as_deref().and_then(to_offset) {
            let floor = match t.constraint_type.as_str() {
                "SNET" => Some(at),
                "FNET" => Some(start_of(at, t.duration_days)),
                "MSO" => Some(at),
                "MFO" => Some(start_of(at, t.duration_days)),
                _ => None,
            };
            if let Some(f) = floor {
                early_start[i] = early_start[i].max(f);
            }
        }
        early_start[i] = workable(early_start[i]);
        early_finish[i] = finish_of(early_start[i], t.duration_days);
    }

    let mut queue: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    let mut topo = Vec::with_capacity(n);
    let mut temp_indegree = in_degree.clone();
    while let Some(u) = queue.pop_front() {
        topo.push(u);
        for &(v, dep_type, lag) in &adj[u] {
            let dur_v = tasks[v].duration_days;
            let bound = match dep_type {
                "SS" => early_start[u] + lag,
                "FF" => start_of(early_finish[u] + lag, dur_v),
                "SF" => start_of(early_start[u] + lag, dur_v),
                _ => early_finish[u] + lag,
            };
            if bound > early_start[v] {
                early_start[v] = workable(bound);
                early_finish[v] = finish_of(early_start[v], dur_v);
            }
            temp_indegree[v] -= 1;
            if temp_indegree[v] == 0 {
                queue.push_back(v);
            }
        }
    }

    // Backward pass (LS/LF).
    let project_end = early_finish.iter().copied().max().unwrap_or(0);
    let mut late_finish = vec![project_end; n];
    let mut late_start = vec![0i32; n];
    for (i, t) in tasks.iter().enumerate() {
        if let Some(at) = t.constraint_date.as_deref().and_then(to_offset) {
            let ceiling = match t.constraint_type.as_str() {
                "SNLT" => Some(finish_of(at, t.duration_days)),
                "FNLT" => Some(at),
                "MSO" => Some(finish_of(at, t.duration_days)),
                "MFO" => Some(at),
                _ => None,
            };
            if let Some(c) = ceiling {
                late_finish[i] = late_finish[i].min(c);
            }
        }
    }
    for &u in &topo {
        late_start[u] = start_of(late_finish[u], tasks[u].duration_days);
    }

    if topo.len() < n {
        let stuck = (0..n).filter(|i| !topo.contains(i)).map(|i| tasks[i].name.clone()).take(5).collect();
        return Err(CpmError::Loop(stuck));
    }

    for &u in topo.iter().rev() {
        let dur_u = tasks[u].duration_days;
        for &(v, dep_type, lag) in &adj[u] {
            let bound = match dep_type {
                "SS" => finish_of(late_start[v] - lag, dur_u),
                "FF" => late_finish[v] - lag,
                "SF" => finish_of(late_finish[v] - lag, dur_u),
                _ => late_start[v] - lag,
            };
            if bound < late_finish[u] {
                late_finish[u] = bound;
                late_start[u] = start_of(late_finish[u], dur_u);
            }
        }
    }

    // As Late As Possible consumes the float the backward pass measured.
    for i in 0..n {
        if tasks[i].constraint_type == "ALAP" && late_start[i] > early_start[i] {
            early_start[i] = late_start[i];
            early_finish[i] = finish_of(early_start[i], tasks[i].duration_days);
        }
    }

    let deadline_missed: Vec<bool> = tasks.iter().enumerate().map(|(i, t)| t.deadline_date.as_deref().and_then(to_offset).is_some_and(|d| early_finish[i] > d)).collect();

    let mut free_float = vec![0i32; n];
    for u in 0..n {
        let mut slack: Option<i32> = None;
        for &(v, dep_type, lag) in &adj[u] {
            let dur_v = tasks[v].duration_days;
            let bound = match dep_type {
                "SS" => early_start[u] + lag,
                "FF" => early_finish[u] + lag - dur_v,
                "SF" => early_start[u] + lag - dur_v,
                _ => early_finish[u] + lag,
            };
            let gap = early_start[v] - bound;
            slack = Some(slack.map_or(gap, |s| s.min(gap)));
        }
        free_float[u] = slack.unwrap_or(late_start[u] - early_start[u]);
    }

    // Summary roll-up: a task with sub-tasks spans them (post-order, nested parents first).
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut roots: Vec<usize> = Vec::new();
    for (i, t) in tasks.iter().enumerate() {
        match t.parent_id.as_deref().and_then(|p| task_idx.get(p)) {
            Some(&pi) => children[pi].push(i),
            None => roots.push(i),
        }
    }
    let mut post: Vec<usize> = Vec::with_capacity(n);
    let mut stack: Vec<(usize, bool)> = roots.iter().rev().map(|&r| (r, false)).collect();
    while let Some((u, done)) = stack.pop() {
        if done {
            post.push(u);
        } else {
            stack.push((u, true));
            for &ch in children[u].iter().rev() {
                stack.push((ch, false));
            }
        }
    }
    let mut duration_out: Vec<i32> = tasks.iter().map(|t| t.duration_days).collect();
    let mut roll_es = early_start.clone();
    let mut roll_ef = early_finish.clone();
    let mut roll_ls = late_start.clone();
    let mut roll_lf = late_finish.clone();
    for &u in &post {
        if children[u].is_empty() {
            continue;
        }
        roll_es[u] = children[u].iter().map(|&c| roll_es[c]).min().unwrap_or(roll_es[u]);
        roll_ef[u] = children[u].iter().map(|&c| roll_ef[c]).max().unwrap_or(roll_ef[u]);
        roll_ls[u] = children[u].iter().map(|&c| roll_ls[c]).min().unwrap_or(roll_ls[u]);
        roll_lf[u] = children[u].iter().map(|&c| roll_lf[c]).max().unwrap_or(roll_lf[u]);
    }
    for i in 0..n {
        if !children[i].is_empty() {
            early_start[i] = roll_es[i];
            early_finish[i] = roll_ef[i];
            late_start[i] = roll_ls[i];
            late_finish[i] = roll_lf[i];
            duration_out[i] = calendar.working_days_between(roll_es[i], roll_ef[i]);
        }
    }

    Ok((0..n)
        .map(|i| {
            let tf = late_start[i] - early_start[i];
            Scheduled {
                early_start: early_start[i],
                early_finish: early_finish[i],
                late_start: late_start[i],
                late_finish: late_finish[i],
                total_float: tf,
                free_float: free_float[i],
                is_critical: tf <= 0,
                deadline_missed: deadline_missed[i],
                duration_days: duration_out[i],
            }
        })
        .collect())
}

/// Writes a schedule onto the tasks it was computed for (what the server's `UPDATE` does, `cpm_dirty` cleared).
pub fn apply(tasks: &mut [Task], scheduled: &[Scheduled]) {
    for (t, s) in tasks.iter_mut().zip(scheduled) {
        t.early_start = Some(s.early_start);
        t.early_finish = Some(s.early_finish);
        t.late_start = Some(s.late_start);
        t.late_finish = Some(s.late_finish);
        t.total_float = Some(s.total_float);
        t.free_float = Some(s.free_float);
        t.is_critical = s.is_critical;
        t.deadline_missed = s.deadline_missed;
        t.duration_days = s.duration_days;
        t.cpm_dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monday() -> Date {
        Date::new(2026, 9, 7).expect("date")
    }

    fn task(id: &str, dur: i32) -> Task {
        let mut t = Task::new(id, "p");
        t.name = id.to_uppercase();
        t.duration_days = dur;
        t
    }

    fn dep(from: &str, to: &str, ty: &str, lag: i32) -> Dependency {
        Dependency { id: format!("{from}-{to}"), project_id: "p".into(), from_task_id: from.into(), to_task_id: to.into(), dep_type: ty.into(), lag_days: lag, rest: Default::default() }
    }

    #[test]
    fn finish_to_start_over_a_weekend() {
        let cal = WorkingCalendar::default_week(false, monday());
        let tasks = vec![task("a", 5), task("b", 2), task("c", 1)];
        let deps = vec![dep("a", "b", "FS", 0)];
        let s = schedule(&tasks, &deps, monday(), &cal).expect("scheduled");
        // A fills Mon–Fri; B starts the next Monday (offset 7) and finishes Wednesday (9).
        assert_eq!((s[0].early_start, s[0].early_finish), (0, 5));
        assert_eq!((s[1].early_start, s[1].early_finish), (7, 9));
        assert!(s[0].is_critical && s[1].is_critical);
        // C floats until the project end (offset 9): one day of work, late start Tuesday (8).
        assert_eq!(s[2].late_start, 8);
        assert_eq!(s[2].total_float, 8);
        assert!(!s[2].is_critical);
    }

    #[test]
    fn a_loop_is_refused_with_its_names() {
        let cal = WorkingCalendar::all_days(monday());
        let tasks = vec![task("a", 1), task("b", 1)];
        let deps = vec![dep("a", "b", "FS", 0), dep("b", "a", "FS", 0)];
        assert_eq!(schedule(&tasks, &deps, monday(), &cal), Err(CpmError::Loop(vec!["A".into(), "B".into()])));
    }

    #[test]
    fn constraints_and_alap() {
        let cal = WorkingCalendar::all_days(monday());
        let mut a = task("a", 2);
        a.constraint_type = "SNET".into();
        a.constraint_date = Some("2026-09-10".into());
        let mut b = task("b", 1);
        b.constraint_type = "ALAP".into();
        let c = task("c", 10);
        let s = schedule(&[a, b, c], &[], monday(), &cal).expect("scheduled");
        assert_eq!(s[0].early_start, 3);
        // ALAP: pulled to its late start (project end 10 - 1).
        assert_eq!(s[1].early_start, 9);
    }

    #[test]
    fn parents_span_their_children() {
        let cal = WorkingCalendar::default_week(false, monday());
        let mut p = task("p", 1);
        p.task_type = "summary".into();
        let mut a = task("a", 3);
        a.parent_id = Some("p".into());
        let mut b = task("b", 4);
        b.parent_id = Some("p".into());
        let s = schedule(&[p, a, b], &[dep("a", "b", "FS", 0)], monday(), &cal).expect("scheduled");
        assert_eq!((s[0].early_start, s[0].early_finish), (0, s[2].early_finish));
        // Worked days between Monday and B's finish (Mon–Wed + Thu, Fri, Mon, Tue).
        assert_eq!(s[0].duration_days, 7);
    }

    #[test]
    fn lags_leads_and_relationship_types() {
        let cal = WorkingCalendar::all_days(monday());
        let tasks = vec![task("a", 4), task("b", 2), task("c", 3), task("d", 2)];
        let deps = vec![dep("a", "b", "SS", 1), dep("a", "c", "FF", 0), dep("a", "d", "FS", -1)];
        let s = schedule(&tasks, &deps, monday(), &cal).expect("scheduled");
        assert_eq!(s[1].early_start, 1);
        assert_eq!(s[2].early_finish, 4);
        assert_eq!(s[3].early_start, 3);
    }
}
