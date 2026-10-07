//! Auto-layout — `computeLayout` (`DiagramEditorPage.tsx:428-495`): hierarchical top-down or left-right
//! (longest-path layering), `tree` (the top-down one), circle and grid. Pure: new positions by shape id.

use std::collections::BTreeMap;

use crate::canvas::Point;
use crate::geometry::js_round;
use crate::model::{Connector, Shape};

/// `LayoutKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutKind {
    HierTb,
    HierLr,
    Tree,
    Circle,
    Grid,
}

impl LayoutKind {
    pub fn parse(s: &str) -> Option<LayoutKind> {
        Some(match s {
            "hier_tb" => LayoutKind::HierTb,
            "hier_lr" => LayoutKind::HierLr,
            "tree" => LayoutKind::Tree,
            "circle" => LayoutKind::Circle,
            "grid" => LayoutKind::Grid,
            _ => return None,
        })
    }
}

/// New positions (`x`, `y`) of the shapes laid out, in the order the shapes were given. `ids` limits the
/// layout to a selection.
pub fn compute_layout(kind: LayoutKind, shapes: &[Shape], conns: &[Connector], ids: Option<&[String]>) -> Vec<(String, Point)> {
    let nodes: Vec<&Shape> = shapes.iter().filter(|s| ids.is_none_or(|ids| ids.iter().any(|i| i == s.id()))).collect();
    let mut pos: Vec<(String, Point)> = Vec::new();
    if nodes.is_empty() {
        return pos;
    }
    let (base_x, base_y) = (80.0, 80.0);
    let n = nodes.len() as f64;

    if kind == LayoutKind::Circle {
        let r = (n * 50.0 / (2.0 * std::f64::consts::PI)).max(140.0);
        let (cx, cy) = (base_x + r, base_y + r);
        for (i, s) in nodes.iter().enumerate() {
            let a = (i as f64 / n) * 2.0 * std::f64::consts::PI - std::f64::consts::FRAC_PI_2;
            pos.push((s.id().to_string(), Point::new(js_round(cx + r * a.cos() - s.w() / 2.0), js_round(cy + r * a.sin() - s.h() / 2.0))));
        }
        return pos;
    }

    if kind == LayoutKind::Grid {
        let cols = n.sqrt().ceil().max(1.0) as usize;
        let cw = nodes.iter().map(|s| s.w()).fold(f64::NEG_INFINITY, f64::max) + 40.0;
        let ch = nodes.iter().map(|s| s.h()).fold(f64::NEG_INFINITY, f64::max) + 40.0;
        for (i, s) in nodes.iter().enumerate() {
            pos.push((s.id().to_string(), Point::new(base_x + (i % cols) as f64 * cw, base_y + (i / cols) as f64 * ch)));
        }
        return pos;
    }

    // Hierarchical (layered). `tree` is an alias of top-bottom hierarchical.
    let horizontal = kind == LayoutKind::HierLr;
    let in_set = |id: &str| nodes.iter().any(|s| s.id() == id);
    let edges: Vec<(&str, &str)> = conns
        .iter()
        .filter_map(|c| match (c.source_id(), c.target_id()) {
            (Some(a), Some(b)) if in_set(a) && in_set(b) => Some((a, b)),
            _ => None,
        })
        .collect();
    let mut level: BTreeMap<&str, i64> = nodes.iter().map(|s| (s.id(), 0)).collect();
    // Longest-path layering by relaxation, bounded (cycle-safe).
    let mut changed = true;
    let mut iter = 0usize;
    while changed && {
        iter += 1;
        iter <= nodes.len() + 2
    } {
        changed = false;
        for (a, b) in &edges {
            let nl = level.get(a).copied().unwrap_or(0) + 1;
            if nl > level.get(b).copied().unwrap_or(0) {
                level.insert(b, nl);
                changed = true;
            }
        }
    }
    let mut by_level: BTreeMap<i64, Vec<&Shape>> = BTreeMap::new();
    for s in &nodes {
        by_level.entry(level.get(s.id()).copied().unwrap_or(0)).or_default().push(s);
    }
    let (gap_main, gap_cross) = (90.0, 50.0);
    let mut cursor_main = if horizontal { base_x } else { base_y };
    for arr in by_level.values() {
        let main_size = arr.iter().map(|s| if horizontal { s.w() } else { s.h() }).fold(f64::NEG_INFINITY, f64::max);
        let mut cross = if horizontal { base_y } else { base_x };
        for s in arr {
            pos.push((s.id().to_string(), if horizontal { Point::new(cursor_main, cross) } else { Point::new(cross, cursor_main) }));
            cross += (if horizontal { s.h() } else { s.w() }) + gap_cross;
        }
        cursor_main += main_size + gap_main;
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Obj;

    fn s(id: &str, w: f64, h: f64) -> Shape {
        Shape::new(id, "rect", 0.0, 0.0, w, h, "", Obj::new(), 0.0, "default")
    }

    fn at(pos: &[(String, Point)], id: &str) -> Point {
        pos.iter().find(|(i, _)| i == id).map(|(_, p)| *p).expect("laid out")
    }

    #[test]
    fn hierarchical_layers_follow_the_longest_path() {
        let shapes = vec![s("a", 120.0, 60.0), s("b", 120.0, 60.0), s("c", 120.0, 60.0)];
        let conns = vec![Connector::between("1", "a", "b", "d"), Connector::between("2", "b", "c", "d"), Connector::between("3", "a", "c", "d")];
        let pos = compute_layout(LayoutKind::HierTb, &shapes, &conns, None);
        assert_eq!(at(&pos, "a"), Point::new(80.0, 80.0));
        assert_eq!(at(&pos, "b"), Point::new(80.0, 80.0 + 60.0 + 90.0));
        assert_eq!(at(&pos, "c"), Point::new(80.0, 80.0 + 2.0 * 150.0));
        let lr = compute_layout(LayoutKind::HierLr, &shapes, &conns, None);
        assert_eq!(at(&lr, "b"), Point::new(80.0 + 120.0 + 90.0, 80.0));
    }

    #[test]
    fn a_cycle_terminates() {
        let shapes = vec![s("a", 10.0, 10.0), s("b", 10.0, 10.0)];
        let conns = vec![Connector::between("1", "a", "b", "d"), Connector::between("2", "b", "a", "d")];
        assert_eq!(compute_layout(LayoutKind::Tree, &shapes, &conns, None).len(), 2);
    }

    #[test]
    fn grid_and_circle() {
        let shapes: Vec<Shape> = (0..5).map(|i| s(&format!("s{i}"), 100.0, 50.0)).collect();
        let g = compute_layout(LayoutKind::Grid, &shapes, &[], None);
        // ⌈√5⌉ = 3 columns of 140 × 90: the 4th shape opens the second row, the 5th follows it.
        assert_eq!(at(&g, "s3"), Point::new(80.0, 80.0 + 90.0));
        assert_eq!(at(&g, "s4"), Point::new(80.0 + 140.0, 80.0 + 90.0));
    }

    #[test]
    fn circle_starts_at_the_top() {
        let shapes: Vec<Shape> = (0..4).map(|i| s(&format!("s{i}"), 20.0, 20.0)).collect();
        let c = compute_layout(LayoutKind::Circle, &shapes, &[], None);
        // r = max(140, 200 / 2π) = 140, centre (220, 220): the first node is above it.
        assert_eq!(at(&c, "s0"), Point::new(210.0, 70.0));
    }
}
