//! The network (precedence) diagram (`web/src/project/NetworkView.tsx`): one card per activity, one arrow per
//! link, phases drawn as frames around their activities, on a free workspace with pan and zoom.
//!
//! Everything here is geometry in *world* units (the web's CSS pixels before the viewport's transform):
//!
//! * [`auto_layout`] — a column per precedence rank, rows handed out in WBS bands (`autoLayout`);
//! * [`time_layout`] — the time-scaled mode, X = early start, width = calendar span (`timeLayout`);
//! * [`hulls`] — the phase frames (`computeHulls`);
//! * [`orth_route`], [`rounded_path`], [`mid_point`] — the links' orthogonal routes with rounded bends;
//! * [`Viewport`], [`fit`], [`month_ticks`] — pan, zoom, « Ajuster à l'écran » and the time ruler;
//! * [`Network`] — the scene of a project (cards, links, frames) and its hit testing.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::date::{self, Date, DateFormat, Lang};
use crate::model::{Dependency, ProjectData, Task};

pub const NODE_W: f64 = 190.0;
pub const NODE_H: f64 = 78.0;
pub const COL_W: f64 = 250.0;
pub const ROW_H: f64 = 108.0;
pub const PAD: f64 = 40.0;
pub const PORT_R: f64 = 6.0;
/// Pixels of travel before a press on a card becomes a drag (a sloppy click selects).
pub const DRAG_THRESHOLD: f64 = 4.0;
/// Grid step for snap-to-grid — the background dots' spacing.
pub const GRID: f64 = 24.0;
/// Time-scaled mode: pixels per calendar day, minimum card width, ruler height.
pub const DAY_W: f64 = 16.0;
pub const MIN_BAR_W: f64 = 96.0;
pub const RULER_H: f64 = 30.0;
/// Rows left free after a phase's band, so stacked frames never touch.
pub const GROUP_GAP: f64 = 0.5;
pub const HULL_PAD: f64 = 10.0;
/// Room kept above a frame's content for the phase name.
pub const HULL_HEAD: f64 = 26.0;
pub const MIN_SCALE: f64 = 0.25;
pub const MAX_SCALE: f64 = 2.5;
/// Half the width of a link's invisible hit stroke (16 px on the web).
pub const EDGE_HIT: f64 = 8.0;
/// The radius of the cut button shown at a hovered link's middle.
pub const CUT_R: f64 = 9.0;

/// A point in world units.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

impl Pt {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// The hand-placed cards: task id → top-left corner (kept per project).
pub type Pos = BTreeMap<String, Pt>;

/// `snapTo`.
pub fn snap_to(v: f64) -> f64 {
    (v / GRID).round() * GRID
}

fn dist(p: Pt, q: Pt) -> f64 {
    (q.x - p.x).hypot(q.y - p.y)
}

/// `parentIds`: any task that has children is a phase, whatever its type says; so is every summary.
pub fn parent_ids(all: &[&Task]) -> HashSet<String> {
    let mut ids: HashSet<String> = all.iter().filter_map(|t| t.parent_id.clone()).collect();
    ids.extend(all.iter().filter(|t| t.is_summary_type()).map(|t| t.id.clone()));
    ids
}

fn children_of<'a>(all: &[&'a Task]) -> HashMap<Option<String>, Vec<&'a Task>> {
    let mut m: HashMap<Option<String>, Vec<&'a Task>> = HashMap::new();
    for t in all {
        m.entry(t.parent_id.clone()).or_default().push(t);
    }
    m
}

/// `localeCompare` closely enough for task names: case-insensitive first.
fn name_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b))
}

/// `autoLayout`: a column per precedence rank; rows in WBS bands, leaves of one parent ordered by schedule.
pub fn auto_layout(all: &[&Task], nodes: &[&Task], edges: &[&Dependency]) -> HashMap<String, Pt> {
    let node_ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let parents = parent_ids(all);
    let mut level: HashMap<&str, i32> = nodes.iter().map(|n| (n.id.as_str(), 0)).collect();
    let (mut changed, mut iter) = (true, 0usize);
    while changed && iter < nodes.len() + 2 {
        iter += 1;
        changed = false;
        for e in edges {
            let nl = level.get(e.from_task_id.as_str()).copied().unwrap_or(0) + 1;
            if nl > level.get(e.to_task_id.as_str()).copied().unwrap_or(0) {
                level.insert(e.to_task_id.as_str(), nl);
                changed = true;
            }
        }
    }
    let kids = children_of(all);
    let mut pos = HashMap::new();
    fn place(items: &[&Task], row0: f64, kids: &HashMap<Option<String>, Vec<&Task>>, parents: &HashSet<String>, node_ids: &HashSet<&str>, level: &HashMap<&str, i32>, pos: &mut HashMap<String, Pt>) -> f64 {
        let (mut row, mut i) = (row0, 0);
        while i < items.len() {
            if parents.contains(&items[i].id) {
                let sub = kids.get(&Some(items[i].id.clone())).map(Vec::as_slice).unwrap_or(&[]);
                let used = place(sub, row, kids, parents, node_ids, level, pos);
                if used > 0.0 {
                    row += used + GROUP_GAP;
                }
                i += 1;
                continue;
            }
            let mut run: Vec<&Task> = Vec::new();
            while i < items.len() && !parents.contains(&items[i].id) {
                run.push(items[i]);
                i += 1;
            }
            run.sort_by(|a, b| a.es().cmp(&b.es()).then_with(|| name_cmp(&a.name, &b.name)));
            let mut counters: HashMap<i32, i32> = HashMap::new();
            for leaf in run {
                if !node_ids.contains(leaf.id.as_str()) {
                    continue;
                }
                let l = level.get(leaf.id.as_str()).copied().unwrap_or(0);
                let r = counters.get(&l).copied().unwrap_or(0);
                pos.insert(leaf.id.clone(), Pt::new(PAD + l as f64 * COL_W, PAD + (row + r as f64) * ROW_H));
                counters.insert(l, r + 1);
            }
            row += counters.values().copied().max().unwrap_or(0).max(0) as f64;
        }
        (row - row0).max(0.0)
    }
    let roots = kids.get(&None).map(Vec::as_slice).unwrap_or(&[]);
    place(roots, 0.0, &kids, &parents, &node_ids, &level, &mut pos);
    pos
}

/// The time-scaled layout (`timeLayout`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TimeLayout {
    pub pos: HashMap<String, Pt>,
    pub width: HashMap<String, f64>,
    pub min_day: i32,
    pub max_day: i32,
}

fn cal_days(t: &Task) -> i32 {
    if t.is_milestone() {
        0
    } else {
        (t.cal_end() - t.es()).max(0)
    }
}

/// `timeLayout`: X = early start, width = calendar span; leaves of a band packed into lanes.
pub fn time_layout(all: &[&Task], nodes: &[&Task]) -> TimeLayout {
    let node_ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let parents = parent_ids(all);
    let min_day = nodes.iter().map(|t| t.es()).min().unwrap_or(0);
    let kids = children_of(all);
    let mut out = TimeLayout { min_day, max_day: min_day, ..TimeLayout::default() };
    fn place(items: &[&Task], row0: f64, kids: &HashMap<Option<String>, Vec<&Task>>, parents: &HashSet<String>, node_ids: &HashSet<&str>, out: &mut TimeLayout) -> f64 {
        let (mut row, mut i) = (row0, 0);
        while i < items.len() {
            if parents.contains(&items[i].id) {
                let sub = kids.get(&Some(items[i].id.clone())).map(Vec::as_slice).unwrap_or(&[]);
                let used = place(sub, row, kids, parents, node_ids, out);
                if used > 0.0 {
                    row += used + GROUP_GAP;
                }
                i += 1;
                continue;
            }
            let mut run: Vec<&Task> = Vec::new();
            while i < items.len() && !parents.contains(&items[i].id) {
                run.push(items[i]);
                i += 1;
            }
            run.sort_by_key(|t| t.es());
            let mut lane_end: Vec<f64> = Vec::new();
            for leaf in run {
                if !node_ids.contains(leaf.id.as_str()) {
                    continue;
                }
                let x = PAD + (leaf.es() - out.min_day) as f64 * DAY_W;
                let w = MIN_BAR_W.max(cal_days(leaf) as f64 * DAY_W);
                let lane = match lane_end.iter().position(|&end| end <= x - 8.0) {
                    Some(l) => l,
                    None => {
                        lane_end.push(0.0);
                        lane_end.len() - 1
                    }
                };
                lane_end[lane] = x + w;
                out.pos.insert(leaf.id.clone(), Pt::new(x, PAD + RULER_H + (row + lane as f64) * ROW_H));
                out.width.insert(leaf.id.clone(), w);
                out.max_day = out.max_day.max(leaf.es() + cal_days(leaf));
            }
            row += lane_end.len() as f64;
        }
        (row - row0).max(0.0)
    }
    let roots = kids.get(&None).map(Vec::as_slice).unwrap_or(&[]);
    place(roots, 0.0, &kids, &parents, &node_ids, &mut out);
    out
}

/// A phase's frame (`Hull`).
#[derive(Debug, Clone, PartialEq)]
pub struct Hull {
    pub id: String,
    pub name: String,
    pub depth: usize,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// The cards inside it (nested phases' included): grabbing the header moves them all.
    pub ids: Vec<String>,
}

#[derive(Clone)]
struct BoxF {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    ids: Vec<String>,
}

/// `computeHulls`: a frame around each phase's cards and nested frames, outer frames first.
pub fn hulls(all: &[&Task], node_ids: &HashSet<String>, pos_of: &dyn Fn(&str) -> Pt, width_of: &dyn Fn(&str) -> f64) -> Vec<Hull> {
    let kids = children_of(all);
    let parents = parent_ids(all);
    let mut out = Vec::new();
    #[allow(clippy::too_many_arguments)]
    fn visit(summary: &Task, depth: usize, kids: &HashMap<Option<String>, Vec<&Task>>, parents: &HashSet<String>, node_ids: &HashSet<String>, pos_of: &dyn Fn(&str) -> Pt, width_of: &dyn Fn(&str) -> f64, out: &mut Vec<Hull>) -> Option<BoxF> {
        let mut bx: Option<BoxF> = None;
        let mut grow = |b: BoxF| {
            bx = Some(match bx.take() {
                Some(a) => BoxF { x1: a.x1.min(b.x1), y1: a.y1.min(b.y1), x2: a.x2.max(b.x2), y2: a.y2.max(b.y2), ids: [a.ids, b.ids].concat() },
                None => b,
            })
        };
        for ch in kids.get(&Some(summary.id.clone())).map(Vec::as_slice).unwrap_or(&[]) {
            if parents.contains(&ch.id) {
                if let Some(inner) = visit(ch, depth + 1, kids, parents, node_ids, pos_of, width_of, out) {
                    grow(BoxF { x1: inner.x1 - HULL_PAD, y1: inner.y1 - HULL_HEAD, x2: inner.x2 + HULL_PAD, y2: inner.y2 + HULL_PAD, ids: inner.ids });
                }
            } else if node_ids.contains(&ch.id) {
                let p = pos_of(&ch.id);
                grow(BoxF { x1: p.x, y1: p.y, x2: p.x + width_of(&ch.id), y2: p.y + NODE_H, ids: vec![ch.id.clone()] });
            }
        }
        let b = bx?;
        out.push(Hull { id: summary.id.clone(), name: summary.name.clone(), depth, x: b.x1 - HULL_PAD, y: b.y1 - HULL_HEAD, w: b.x2 - b.x1 + 2.0 * HULL_PAD, h: b.y2 - b.y1 + HULL_HEAD + HULL_PAD, ids: b.ids.clone() });
        Some(b)
    }
    for it in kids.get(&None).map(Vec::as_slice).unwrap_or(&[]) {
        if parents.contains(&it.id) {
            visit(it, 0, &kids, &parents, node_ids, pos_of, width_of, &mut out);
        }
    }
    // Outer frames first (a stable sort, as the web's), so a nested frame paints on top of its parent.
    out.sort_by_key(|h| h.depth);
    out
}

/// `orthRoute`: right port → left port, around the source when the target is behind it.
pub fn orth_route(src: Pt, dst: Pt) -> Vec<Pt> {
    const P: f64 = 24.0;
    if dst.x >= src.x + P * 2.0 {
        if (src.y - dst.y).abs() < 1.0 {
            return vec![src, dst];
        }
        let mx = ((src.x + dst.x) / 2.0).round();
        return vec![src, Pt::new(mx, src.y), Pt::new(mx, dst.y), dst];
    }
    let my = ((src.y + dst.y) / 2.0).round();
    vec![src, Pt::new(src.x + P, src.y), Pt::new(src.x + P, my), Pt::new(dst.x - P, my), Pt::new(dst.x - P, dst.y), dst]
}

/// One piece of a path after its start point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    Line(Pt),
    /// A quadratic curve: control point, end point.
    Quad(Pt, Pt),
}

/// `roundedPath`: the polyline with its bends rounded (radius 10, at most half of each leg).
pub fn rounded_path(pts: &[Pt]) -> Vec<Seg> {
    if pts.len() < 2 {
        return Vec::new();
    }
    const R: f64 = 10.0;
    let mut rad = vec![0.0; pts.len()];
    for i in 1..pts.len() - 1 {
        rad[i] = R.min(dist(pts[i - 1], pts[i]) / 2.0).min(dist(pts[i], pts[i + 1]) / 2.0);
    }
    let mut out = Vec::new();
    for s in 0..pts.len() - 1 {
        let (p0, p1) = (pts[s], pts[s + 1]);
        let len = match dist(p0, p1) {
            l if l == 0.0 => 1.0,
            l => l,
        };
        let (dx, dy) = ((p1.x - p0.x) / len, (p1.y - p0.y) / len);
        let inner = s + 1 < pts.len() - 1;
        let cut = if inner { rad[s + 1] } else { 0.0 };
        out.push(Seg::Line(Pt::new(p1.x - dx * cut, p1.y - dy * cut)));
        if inner {
            let p2 = pts[s + 2];
            let l2 = match dist(p1, p2) {
                l if l == 0.0 => 1.0,
                l => l,
            };
            out.push(Seg::Quad(p1, Pt::new(p1.x + (p2.x - p1.x) / l2 * rad[s + 1], p1.y + (p2.y - p1.y) / l2 * rad[s + 1])));
        }
    }
    out
}

/// `midPoint`: the point halfway along the polyline (the label and the cut button).
pub fn mid_point(pts: &[Pt]) -> Pt {
    let total: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let l = dist(w[0], w[1]);
        if acc + l >= total / 2.0 {
            let k = if l > 0.0 { (total / 2.0 - acc) / l } else { 0.0 };
            return Pt::new(w[0].x + (w[1].x - w[0].x) * k, w[0].y + (w[1].y - w[0].y) * k);
        }
        acc += l;
    }
    pts.last().copied().unwrap_or_default()
}

/// The distance from `p` to the polyline.
pub fn distance_to_polyline(p: Pt, pts: &[Pt]) -> f64 {
    let mut best = f64::INFINITY;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (vx, vy) = (b.x - a.x, b.y - a.y);
        let l2 = vx * vx + vy * vy;
        let k = if l2 > 0.0 { (((p.x - a.x) * vx + (p.y - a.y) * vy) / l2).clamp(0.0, 1.0) } else { 0.0 };
        best = best.min(dist(p, Pt::new(a.x + vx * k, a.y + vy * k)));
    }
    best
}

/// `edgeLabel`: « SS », « +2 j », « SS +2 j », « −1 j »; empty for a plain finish-to-start link.
pub fn edge_label(d: &Dependency) -> String {
    let ty = if d.dep_type != "FS" { d.dep_type.as_str() } else { "" };
    let lag = if d.lag_days != 0 { format!("{}{}{} j", if ty.is_empty() { "" } else { " " }, if d.lag_days > 0 { '+' } else { '−' }, d.lag_days.abs()) } else { String::new() };
    format!("{ty}{lag}")
}

/// The pan and zoom (`Viewport`): screen = world × scale + (tx, ty).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub tx: f64,
    pub ty: f64,
    pub scale: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self { tx: 0.0, ty: 0.0, scale: 1.0 }
    }
}

impl Viewport {
    /// A stored viewport, if it is sane (the web's validation of its local copy).
    pub fn valid(self) -> Option<Self> {
        (self.tx.is_finite() && self.ty.is_finite() && (MIN_SCALE..=MAX_SCALE).contains(&self.scale)).then_some(self)
    }

    pub fn to_world(&self, sx: f64, sy: f64) -> Pt {
        Pt::new((sx - self.tx) / self.scale, (sy - self.ty) / self.scale)
    }

    pub fn to_screen(&self, p: Pt) -> Pt {
        Pt::new(p.x * self.scale + self.tx, p.y * self.scale + self.ty)
    }

    /// `zoomBy`: the scale only (the zoom pill's − and +).
    pub fn zoom_by(&mut self, f: f64) {
        self.scale = (self.scale * f).clamp(MIN_SCALE, MAX_SCALE);
    }

    /// Ctrl + wheel: zoom around the pointer (`cx`, `cy` in screen units).
    pub fn zoom_at(&mut self, cx: f64, cy: f64, f: f64) {
        let ns = (self.scale * f).clamp(MIN_SCALE, MAX_SCALE);
        let k = ns / self.scale;
        *self = Self { scale: ns, tx: cx - (cx - self.tx) * k, ty: cy - (cy - self.ty) * k };
    }
}

/// A rectangle in world units (`x`, `y`, `w`, `h`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BoxR {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl BoxR {
    pub fn contains(&self, p: Pt) -> bool {
        p.x >= self.x && p.x <= self.x + self.w && p.y >= self.y && p.y <= self.y + self.h
    }
}

/// `fitToContent`: the whole diagram centred in a `w` × `h` view (scale between 0.25 and 1.5).
pub fn fit(boxes: &[BoxR], w: f64, h: f64) -> Viewport {
    if boxes.is_empty() {
        return Viewport::default();
    }
    let min_x = boxes.iter().map(|b| b.x).fold(f64::INFINITY, f64::min) - PAD;
    let min_y = boxes.iter().map(|b| b.y).fold(f64::INFINITY, f64::min) - PAD;
    let max_x = boxes.iter().map(|b| b.x + b.w).fold(f64::NEG_INFINITY, f64::max) + PAD;
    let max_y = boxes.iter().map(|b| b.y + b.h).fold(f64::NEG_INFINITY, f64::max) + PAD;
    let bw = if max_x - min_x == 0.0 { 1.0 } else { max_x - min_x };
    let bh = if max_y - min_y == 0.0 { 1.0 } else { max_y - min_y };
    let scale = (w / bw).min(h / bh).clamp(MIN_SCALE, 1.5);
    Viewport { tx: (w - bw * scale) / 2.0 - min_x * scale, ty: (h - bh * scale) / 2.0 - min_y * scale, scale }
}

/// A month of the time ruler: its world X (the gridline) and screen X (the pinned label).
#[derive(Debug, Clone, PartialEq)]
pub struct MonthTick {
    pub world_x: f64,
    pub screen_x: f64,
    pub label: String,
}

/// `monthTicks`: the months across the visible width (the ruler seems endless as you pan), labels
/// `{ month: 'short', year: '2-digit' }` capitalised (« Oct. 26 »).
pub fn month_ticks(min_day: i32, project_start: Date, vp: &Viewport, width: f64, lang: Lang) -> Vec<MonthTick> {
    if width <= 0.0 {
        return Vec::new();
    }
    let day_at = |sx: f64| min_day as f64 + ((sx - vp.tx) / vp.scale - PAD) / DAY_W;
    let d_left = day_at(0.0).floor() as i64 - 2;
    let d_right = day_at(width).ceil() as i64 + 2;
    let first = project_start.add_days(d_left);
    let last = project_start.add_days(d_right);
    let mut cur = first.first_of_month();
    let mut out = Vec::new();
    let mut guard = 0;
    while cur <= last && guard < 600 {
        guard += 1;
        let off = cur.days_since(project_start) as f64;
        let world_x = PAD + (off - min_day as f64) * DAY_W;
        out.push(MonthTick { world_x, screen_x: vp.tx + world_x * vp.scale, label: capitalize(&date::format(cur, DateFormat::MonthShortYear2, lang)) });
        cur = cur.next_month();
    }
    out
}

/// CSS `text-transform: capitalize`: the first letter of each word.
fn capitalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut start = true;
    for c in s.chars() {
        if start && c.is_alphabetic() {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
        start = c.is_whitespace();
    }
    out
}

/// A card of the scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Index in `data.tasks`.
    pub index: usize,
    pub id: String,
    pub rect: BoxR,
    pub has_in: bool,
    pub has_out: bool,
}

/// A link of the scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub dep_id: String,
    pub from: String,
    pub to: String,
    pub route: Vec<Pt>,
    /// Both ends on the critical path.
    pub critical: bool,
    pub label: String,
}

/// What lies under a point (top first: cards and their ports, links, frame headers, the background).
#[derive(Debug, Clone, PartialEq)]
pub enum Hit {
    /// A card's right port (starts a link).
    OutPort(String),
    Node(String),
    /// The cut button of the hovered link.
    Cut(String),
    Edge(String),
    /// A frame's header (moves its cards).
    HullHead(String),
    Background,
}

/// The scene of a project.
#[derive(Debug, Clone, Default)]
pub struct Network {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub hulls: Vec<Hull>,
    pub timescale: bool,
    pub min_day: i32,
}

impl Network {
    /// The scene of `shown` (the filtered tasks, `displayTasks`), with the hand-placed cards of `manual`.
    pub fn build(data: &ProjectData, shown: &[usize], manual: &Pos, timescale: bool) -> Self {
        let all: Vec<&Task> = shown.iter().filter_map(|&i| data.tasks.get(i)).collect();
        let parents = parent_ids(&all);
        let nodes: Vec<&Task> = all.iter().copied().filter(|t| !parents.contains(&t.id)).collect();
        let ids: HashSet<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let edges: Vec<&Dependency> = data.dependencies.iter().filter(|d| ids.contains(&d.from_task_id) && ids.contains(&d.to_task_id)).collect();
        let (pos, width, min_day) = if timescale {
            let t = time_layout(&all, &nodes);
            (t.pos, t.width, t.min_day)
        } else {
            let mut p = auto_layout(&all, &nodes, &edges);
            for (id, q) in manual {
                if ids.contains(id) {
                    p.insert(id.clone(), *q);
                }
            }
            (p, HashMap::new(), 0)
        };
        let pos_of = |id: &str| pos.get(id).copied().unwrap_or(Pt::new(PAD, PAD));
        let width_of = |id: &str| if timescale { width.get(id).copied().unwrap_or(MIN_BAR_W) } else { NODE_W };
        let index: HashMap<&str, usize> = data.tasks.iter().enumerate().map(|(i, t)| (t.id.as_str(), i)).collect();
        let crit: HashMap<&str, bool> = nodes.iter().map(|n| (n.id.as_str(), n.is_critical)).collect();
        let scene_nodes = nodes
            .iter()
            .map(|n| {
                let p = pos_of(&n.id);
                Node {
                    index: index.get(n.id.as_str()).copied().unwrap_or(0),
                    id: n.id.clone(),
                    rect: BoxR { x: p.x, y: p.y, w: width_of(&n.id), h: NODE_H },
                    has_in: edges.iter().any(|d| d.to_task_id == n.id),
                    has_out: edges.iter().any(|d| d.from_task_id == n.id),
                }
            })
            .collect();
        let scene_edges = edges
            .iter()
            .map(|d| {
                let (a, b) = (pos_of(&d.from_task_id), pos_of(&d.to_task_id));
                Edge {
                    dep_id: d.id.clone(),
                    from: d.from_task_id.clone(),
                    to: d.to_task_id.clone(),
                    route: orth_route(Pt::new(a.x + width_of(&d.from_task_id), a.y + NODE_H / 2.0), Pt::new(b.x, b.y + NODE_H / 2.0)),
                    critical: crit.get(d.from_task_id.as_str()).copied().unwrap_or(false) && crit.get(d.to_task_id.as_str()).copied().unwrap_or(false),
                    label: edge_label(d),
                }
            })
            .collect();
        let hulls = hulls(&all, &ids, &pos_of, &width_of);
        Self { nodes: scene_nodes, edges: scene_edges, hulls, timescale, min_day }
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// The card's top-left corner.
    pub fn pos_of(&self, id: &str) -> Pt {
        self.node(id).map(|n| Pt::new(n.rect.x, n.rect.y)).unwrap_or(Pt::new(PAD, PAD))
    }

    /// The boxes `fitToContent` frames: the cards and the frames.
    pub fn content_boxes(&self) -> Vec<BoxR> {
        self.nodes.iter().map(|n| n.rect).chain(self.hulls.iter().map(|h| BoxR { x: h.x, y: h.y, w: h.w, h: h.h })).collect()
    }

    /// `gridBottom`: how far down the time-scaled mode's month gridlines go.
    pub fn grid_bottom(&self) -> f64 {
        let mut m = PAD + RULER_H + ROW_H;
        for n in &self.nodes {
            m = m.max(n.rect.y + NODE_H);
        }
        for h in &self.hulls {
            m = m.max(h.y + h.h);
        }
        m + PAD
    }

    /// What lies under the world point `p`; `hovered` is the link whose cut button shows.
    pub fn hit(&self, p: Pt, hovered: Option<&str>) -> Hit {
        for n in self.nodes.iter().rev() {
            let port = Pt::new(n.rect.x + n.rect.w, n.rect.y + NODE_H / 2.0);
            if dist(p, port) <= PORT_R + 1.0 {
                return Hit::OutPort(n.id.clone());
            }
            if n.rect.contains(p) {
                return Hit::Node(n.id.clone());
            }
        }
        if let Some(h) = hovered {
            if let Some(e) = self.edges.iter().find(|e| e.dep_id == h) {
                if dist(p, mid_point(&e.route)) <= CUT_R {
                    return Hit::Cut(e.dep_id.clone());
                }
            }
        }
        for e in self.edges.iter().rev() {
            if distance_to_polyline(p, &e.route) <= EDGE_HIT {
                return Hit::Edge(e.dep_id.clone());
            }
        }
        for h in self.hulls.iter().rev() {
            if (BoxR { x: h.x, y: h.y, w: h.w, h: HULL_HEAD }).contains(p) {
                return Hit::HullHead(h.id.clone());
            }
        }
        Hit::Background
    }

    /// The card under `p` other than `source` (the link gesture's target).
    pub fn target_at(&self, p: Pt, source: &str) -> Option<String> {
        self.nodes.iter().rev().find(|n| n.id != source && (n.rect.contains(p) || dist(p, Pt::new(n.rect.x + n.rect.w, n.rect.y + NODE_H / 2.0)) <= PORT_R)).map(|n| n.id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Task;

    fn task(id: &str, parent: Option<&str>, es: i32, dur: i32) -> Task {
        let mut t = Task::new(id, "p");
        t.name = id.to_uppercase();
        t.parent_id = parent.map(str::to_string);
        t.early_start = Some(es);
        t.early_finish = Some(es + dur);
        t.duration_days = dur;
        t
    }

    fn dep(id: &str, a: &str, b: &str) -> Dependency {
        Dependency { id: id.into(), project_id: "p".into(), from_task_id: a.into(), to_task_id: b.into(), dep_type: "FS".into(), lag_days: 0, rest: Default::default() }
    }

    fn data() -> ProjectData {
        let mut d = ProjectData::default();
        d.project.id = "p".into();
        let mut ph = task("ph", None, 0, 8);
        ph.task_type = "summary".into();
        d.tasks = vec![ph, task("a", Some("ph"), 0, 3), task("b", Some("ph"), 0, 4), task("c", Some("ph"), 3, 5), task("d", None, 8, 2)];
        d.dependencies = vec![dep("1", "a", "c"), dep("2", "c", "d"), dep("3", "b", "d")];
        d
    }

    #[test]
    fn auto_layout_ranks_columns_and_bands_rows() {
        let d = data();
        let all: Vec<&Task> = d.tasks.iter().collect();
        let nodes: Vec<&Task> = all.iter().copied().filter(|t| t.id != "ph").collect();
        let edges: Vec<&Dependency> = d.dependencies.iter().collect();
        let p = auto_layout(&all, &nodes, &edges);
        // a and b: rank 0, rows 0 and 1 (schedule order, then name); c: rank 1 row 0; d: rank 2 after the band.
        assert_eq!(p["a"], Pt::new(PAD, PAD));
        assert_eq!(p["b"], Pt::new(PAD, PAD + ROW_H));
        assert_eq!(p["c"], Pt::new(PAD + COL_W, PAD));
        // The phase used 2 rows, then GROUP_GAP.
        assert_eq!(p["d"], Pt::new(PAD + 2.0 * COL_W, PAD + 2.5 * ROW_H));
    }

    #[test]
    fn time_layout_packs_lanes() {
        let d = data();
        let all: Vec<&Task> = d.tasks.iter().collect();
        let nodes: Vec<&Task> = all.iter().copied().filter(|t| t.id != "ph").collect();
        let t = time_layout(&all, &nodes);
        assert_eq!(t.min_day, 0);
        assert_eq!(t.pos["a"], Pt::new(PAD, PAD + RULER_H));
        // b starts with a: a second lane.
        assert_eq!(t.pos["b"].y, PAD + RULER_H + ROW_H);
        assert_eq!(t.width["a"], MIN_BAR_W);
        // A short task keeps the minimum width.
        assert_eq!(t.width["c"], MIN_BAR_W);
        assert_eq!(t.max_day, 10);
    }

    #[test]
    fn scene_hulls_routes_and_hits() {
        let d = data();
        let shown: Vec<usize> = (0..d.tasks.len()).collect();
        let n = Network::build(&d, &shown, &Pos::new(), false);
        assert_eq!(n.nodes.len(), 4);
        assert_eq!(n.edges.len(), 3);
        assert_eq!(n.hulls.len(), 1);
        let h = &n.hulls[0];
        assert_eq!((h.x, h.y), (PAD - HULL_PAD, PAD - HULL_HEAD));
        assert_eq!(h.w, COL_W + NODE_W + 2.0 * HULL_PAD);
        // The card's middle, its out port, a link's middle, the frame's header, the background.
        assert_eq!(n.hit(Pt::new(PAD + 50.0, PAD + 30.0), None), Hit::Node("a".into()));
        assert_eq!(n.hit(Pt::new(PAD + NODE_W, PAD + NODE_H / 2.0), None), Hit::OutPort("a".into()));
        let e = &n.edges[0];
        let m = mid_point(&e.route);
        assert_eq!(n.hit(m, None), Hit::Edge("1".into()));
        assert_eq!(n.hit(m, Some("1")), Hit::Cut("1".into()));
        assert_eq!(n.hit(Pt::new(h.x + 5.0, h.y + 5.0), None), Hit::HullHead("ph".into()));
        assert_eq!(n.hit(Pt::new(-500.0, -500.0), None), Hit::Background);
        // A hand-placed card wins over the auto-layout.
        let mut manual = Pos::new();
        manual.insert("d".into(), Pt::new(1000.0, 10.0));
        assert_eq!(Network::build(&d, &shown, &manual, false).pos_of("d"), Pt::new(1000.0, 10.0));
        // …but not in the time-scaled mode.
        assert_ne!(Network::build(&d, &shown, &manual, true).pos_of("d"), Pt::new(1000.0, 10.0));
    }

    #[test]
    fn routes_and_paths() {
        assert_eq!(orth_route(Pt::new(0.0, 0.0), Pt::new(100.0, 0.0)).len(), 2);
        let r = orth_route(Pt::new(0.0, 0.0), Pt::new(100.0, 50.0));
        assert_eq!(r, vec![Pt::new(0.0, 0.0), Pt::new(50.0, 0.0), Pt::new(50.0, 50.0), Pt::new(100.0, 50.0)]);
        // Behind the source: out, around and back in from the left.
        assert_eq!(orth_route(Pt::new(100.0, 0.0), Pt::new(50.0, 100.0)).len(), 6);
        let segs = rounded_path(&r);
        assert_eq!(segs.len(), 5);
        assert_eq!(segs[0], Seg::Line(Pt::new(40.0, 0.0)));
        assert_eq!(segs[1], Seg::Quad(Pt::new(50.0, 0.0), Pt::new(50.0, 10.0)));
        assert_eq!(mid_point(&r), Pt::new(50.0, 25.0));
    }

    #[test]
    fn labels_viewport_and_ruler() {
        let mut d = dep("x", "a", "b");
        assert_eq!(edge_label(&d), "");
        d.lag_days = 2;
        assert_eq!(edge_label(&d), "+2 j");
        d.dep_type = "SS".into();
        d.lag_days = -1;
        assert_eq!(edge_label(&d), "SS −1 j");
        let mut vp = Viewport::default();
        vp.zoom_at(100.0, 100.0, 2.0);
        assert_eq!(vp, Viewport { tx: -100.0, ty: -100.0, scale: 2.0 });
        assert_eq!(vp.to_world(100.0, 100.0), Pt::new(100.0, 100.0));
        vp.zoom_by(10.0);
        assert_eq!(vp.scale, MAX_SCALE);
        let f = fit(&[BoxR { x: 40.0, y: 40.0, w: 190.0, h: 78.0 }], 1000.0, 600.0);
        assert_eq!(f.scale, 1.5);
        assert_eq!(snap_to(37.0), 48.0);
        let start = Date::new(2026, 10, 5).expect("d");
        let ticks = month_ticks(0, start, &Viewport::default(), 800.0, Lang::Fr);
        assert_eq!(ticks[0].label, "Sept. 26");
        assert_eq!(ticks[1].label, "Oct. 26");
        assert_eq!(ticks[1].world_x, PAD - 4.0 * DAY_W);
    }
}
