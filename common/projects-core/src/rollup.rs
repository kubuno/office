//! Completion rolled up from children (`web/src/project/rollup.ts`).
//!
//! A leaf keeps its stored `progress`. A parent (any task with children, whatever its stored type) shows
//! the duration-weighted average of its **direct** children (each already rolled up), or their plain average
//! when every child weighs nothing (all milestones). The stored value of a parent is never changed: this is
//! a view-only derivation.

use std::collections::{HashMap, HashSet};

use crate::model::Task;

/// JavaScript's `Math.round` (halves round up, towards +∞).
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// `effectiveProgress(tasks)`: task id → completion shown.
pub fn effective_progress(tasks: &[Task]) -> HashMap<String, i32> {
    let mut children: HashMap<&str, Vec<&Task>> = HashMap::new();
    for t in tasks {
        if let Some(p) = t.parent_id.as_deref() {
            children.entry(p).or_default().push(t);
        }
    }
    let mut out: HashMap<String, i32> = HashMap::new();
    let mut in_progress: HashSet<&str> = HashSet::new();

    fn visit<'a>(t: &'a Task, children: &HashMap<&str, Vec<&'a Task>>, out: &mut HashMap<String, i32>, in_progress: &mut HashSet<&'a str>) -> i32 {
        if let Some(&v) = out.get(&t.id) {
            return v;
        }
        if in_progress.contains(t.id.as_str()) {
            return t.progress;
        }
        let Some(kids) = children.get(t.id.as_str()).filter(|k| !k.is_empty()) else {
            out.insert(t.id.clone(), t.progress);
            return t.progress;
        };
        in_progress.insert(t.id.as_str());
        let (mut weighted, mut weight, mut plain) = (0f64, 0f64, 0f64);
        for c in kids {
            let cp = visit(c, children, out, in_progress) as f64;
            let w = c.duration_days.max(0) as f64;
            weighted += cp * w;
            weight += w;
            plain += cp;
        }
        in_progress.remove(t.id.as_str());
        let val = if weight > 0.0 { js_round(weighted / weight) } else { js_round(plain / kids.len() as f64) } as i32;
        out.insert(t.id.clone(), val);
        val
    }

    for t in tasks {
        visit(t, &children, &mut out, &mut in_progress);
    }
    out
}

/// `parentIds(tasks)`: every task that has at least one child.
pub fn parent_ids(tasks: &[Task]) -> HashSet<String> {
    tasks.iter().filter_map(|t| t.parent_id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, parent: Option<&str>, dur: i32, progress: i32) -> Task {
        let mut t = Task::new(id, "p");
        t.parent_id = parent.map(str::to_string);
        t.duration_days = dur;
        t.progress = progress;
        t
    }

    #[test]
    fn weights_by_duration_and_recurses() {
        let tasks = vec![task("p", None, 0, 7), task("a", Some("p"), 3, 100), task("b", Some("p"), 1, 0), task("q", Some("p"), 4, 0), task("q1", Some("q"), 2, 50)];
        let m = effective_progress(&tasks);
        // q = 50 (its only child); p = (100·3 + 0·1 + 50·4) / 8 = 62.5 → 63 (JavaScript rounds halves up).
        assert_eq!(m["q"], 50);
        assert_eq!(m["p"], 63);
        assert_eq!(m["a"], 100);
        assert!(parent_ids(&tasks).contains("p"));
    }

    #[test]
    fn milestones_only_average_plainly() {
        let tasks = vec![task("p", None, 0, 0), task("m1", Some("p"), 0, 100), task("m2", Some("p"), 0, 0)];
        assert_eq!(effective_progress(&tasks)["p"], 50);
    }
}
