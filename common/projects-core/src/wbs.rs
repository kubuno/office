//! Outline numbers (`1`, `1.2`, `1.2.3`) - the port of the server's `project_wbs::renumber`
//! (`server/src/handlers/project_wbs.rs:87-139`), which runs after every task creation, move and deletion. The
//! desktop applies it to its local copy so a change shows its numbers before (or without) the server.

use std::collections::{HashMap, HashSet};

use crate::date::Date;
use crate::model::Task;

/// A creation timestamp as a sortable key (`DateTime<Utc>` order): seconds since the epoch, then nanoseconds.
/// RFC 3339 text with a variable number of fraction digits does not sort as text (`…00.5Z` < `…00Z`).
pub fn timestamp_key(s: &str) -> (i64, u32) {
    let date = Date::parse(s).map(Date::to_days).unwrap_or(0);
    let time = s.get(11..).unwrap_or("");
    let num = |a: usize, b: usize| time.get(a..b).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
    let (h, m, sec) = (num(0, 2), num(3, 5), num(6, 8));
    let mut nanos = 0u32;
    let mut rest = time.get(8..).unwrap_or("");
    if let Some(frac) = rest.strip_prefix('.') {
        let digits: String = frac.chars().take_while(|c| c.is_ascii_digit()).collect();
        let mut padded = digits.clone();
        padded.truncate(9);
        while padded.len() < 9 {
            padded.push('0');
        }
        nanos = padded.parse().unwrap_or(0);
        rest = &frac[digits.len()..];
    }
    // An offset (`+02:00`) moves the instant back to UTC.
    let mut offset = 0i64;
    if let Some(sign) = rest.chars().next().filter(|c| *c == '+' || *c == '-') {
        let oh = rest.get(1..3).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
        let om = rest.get(4..6).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
        offset = (oh * 3600 + om * 60) * if sign == '+' { 1 } else { -1 };
    }
    (date * 86_400 + h * 3600 + m * 60 + sec - offset, nanos)
}

/// The outline code of every task, in the order the plan is displayed (pre-order: siblings by position,
/// then creation time). A task whose parent is not in the project counts as a root; a cycle in `parent_id`
/// does not hang (each task is numbered once).
pub fn codes(tasks: &[Task]) -> Vec<(String, String)> {
    if tasks.is_empty() {
        return Vec::new();
    }
    type Node<'a> = (&'a str, i32, (i64, u32));
    let mut children: HashMap<Option<&str>, Vec<Node<'_>>> = HashMap::new();
    for t in tasks {
        children.entry(t.parent_id.as_deref()).or_default().push((t.id.as_str(), t.position, timestamp_key(&t.created_at)));
    }
    for list in children.values_mut() {
        list.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)));
    }
    let known: HashSet<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
    let mut roots: Vec<Node<'_>> = tasks
        .iter()
        .filter(|t| t.parent_id.as_deref().is_none_or(|p| !known.contains(p)))
        .map(|t| (t.id.as_str(), t.position, timestamp_key(&t.created_at)))
        .collect();
    roots.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)));

    let mut out = Vec::with_capacity(tasks.len());
    let mut stack: Vec<(&str, String)> = roots.iter().enumerate().rev().map(|(i, r)| (r.0, (i + 1).to_string())).collect();
    let mut seen = HashSet::new();
    while let Some((id, code)) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(kids) = children.get(&Some(id)) {
            for (i, kid) in kids.iter().enumerate().rev() {
                stack.push((kid.0, format!("{code}.{}", i + 1)));
            }
        }
        out.push((id.to_string(), code));
    }
    out
}

/// Writes the codes onto the tasks.
pub fn renumber(tasks: &mut [Task]) {
    let map: HashMap<String, String> = codes(tasks).into_iter().collect();
    for t in tasks.iter_mut() {
        if let Some(c) = map.get(&t.id) {
            if t.wbs != *c {
                t.wbs = c.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, parent: Option<&str>, pos: i32, created: &str) -> Task {
        let mut t = Task::new(id, "p");
        t.parent_id = parent.map(str::to_string);
        t.position = pos;
        t.created_at = created.into();
        t
    }

    #[test]
    fn numbers_the_outline() {
        let mut tasks = vec![
            task("a", None, 0, "2026-01-01T00:00:00Z"),
            task("a1", Some("a"), 1, "2026-01-01T00:00:01Z"),
            task("a2", Some("a"), 0, "2026-01-01T00:00:02Z"),
            task("b", None, 0, "2026-01-01T00:00:00.5Z"),
            task("orphan", Some("gone"), 5, "2026-01-01T00:00:00Z"),
        ];
        renumber(&mut tasks);
        let code = |id: &str| tasks.iter().find(|t| t.id == id).map(|t| t.wbs.clone()).unwrap_or_default();
        // Equal positions: creation time decides (00Z before 00.5Z).
        assert_eq!(code("a"), "1");
        assert_eq!(code("b"), "2");
        assert_eq!(code("a2"), "1.1");
        assert_eq!(code("a1"), "1.2");
        assert_eq!(code("orphan"), "3");
    }

    #[test]
    fn a_cycle_does_not_hang() {
        let mut tasks = vec![task("x", Some("y"), 0, ""), task("y", Some("x"), 0, "")];
        renumber(&mut tasks);
        assert!(tasks.iter().all(|t| t.wbs.is_empty()));
    }

    #[test]
    fn timestamps_sort_as_instants() {
        assert!(timestamp_key("2026-01-01T00:00:00Z") < timestamp_key("2026-01-01T00:00:00.5Z"));
        assert_eq!(timestamp_key("2026-01-01T02:00:00+02:00"), timestamp_key("2026-01-01T00:00:00Z"));
    }
}
