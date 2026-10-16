//! The Gantt chart's geometry and gestures - the platform-neutral half of `web/src/project/GanttRenderer.ts`
//! and of the pointer handlers of `ProjectEditorPage.tsx` (`ganttBarHit`, `onGanttDown`, `recomputePreview`,
//! `recomputeMarker`, `onGanttUp`). The painting itself (colours, fonts, Direct2D or Canvas 2D) stays with the
//! platform; everything that decides *where* a bar is and *what* a drag does lives here, so every client draws
//! and edits the same plan the same way.
//!
//! Coordinates are DIP in the chart's **world** (x = 0 at the chart's left edge, before scrolling):
//! `x_of_day(d) = (d + lead_days) · day_w`. Day offsets count from the project start (day 0).

use std::collections::{HashMap, HashSet};

use crate::calendar::WorkingCalendar;
use crate::model::{ProjectData, Task};
use crate::rollup::{effective_progress, js_round};

/// Row height (`ROW_H`).
pub const ROW_H: f64 = 28.0;
/// Header height: month band + day band (`HEADER_H`).
pub const HEADER_H: f64 = 56.0;
/// Days shown before day 0 and after the content at rest (`VISIBLE_LEAD`).
pub const VISIBLE_LEAD: i32 = 7;
/// Days of margin opened on both sides while a boundary marker is dragged (`DRAG_MARGIN`).
pub const DRAG_MARGIN: i32 = 45;
/// The timeline band's height (`TIMELINE_H`).
pub const TIMELINE_H: f64 = 60.0;
/// A task bar's height (`BAR_H`).
pub const BAR_H: f64 = 18.0;
/// Pointer travel (px) before a press becomes a drag.
pub const DRAG_THRESHOLD: f64 = 4.0;
/// The edge band (px) where a drag scrolls the chart, and the scroll step per 16 ms tick.
pub const AUTO_SCROLL_EDGE: f64 = 44.0;
pub const AUTO_SCROLL_STEP: f64 = 16.0;

/// The time scale (`ZoomLevel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Zoom {
    #[default]
    Day,
    Week,
    Month,
}

impl Zoom {
    /// Pixels per day (`ZOOM_DAYW`).
    pub fn day_w(self) -> f64 {
        match self {
            Zoom::Day => 26.0,
            Zoom::Week => 9.0,
            Zoom::Month => 3.2,
        }
    }
    /// « Arrière »: day → week → month.
    pub fn out(self) -> Self {
        match self {
            Zoom::Day => Zoom::Week,
            _ => Zoom::Month,
        }
    }
    /// « Avant »: month → week → day.
    pub fn zoom_in(self) -> Self {
        match self {
            Zoom::Month => Zoom::Week,
            _ => Zoom::Day,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Zoom::Day => "day",
            Zoom::Week => "week",
            Zoom::Month => "month",
        }
    }
}

/// The chart's horizontal frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub day_w: f64,
    /// Days before day 0 (`leadDays`: 7 at rest, 45 while a marker is dragged).
    pub lead_days: i32,
    /// The rightmost day drawn (`drawRight` = content right + margin).
    pub draw_right: i32,
}

impl Frame {
    pub fn x_of_day(&self, d: f64) -> f64 {
        (d + self.lead_days as f64) * self.day_w
    }
    pub fn day_at(&self, x_world: f64) -> f64 {
        x_world / self.day_w - self.lead_days as f64
    }
    /// The chart's world width (`ganttW`).
    pub fn width(&self) -> f64 {
        (self.lead_days + self.draw_right) as f64 * self.day_w
    }
}

/// `contentRight`, `drawRight` and the frame for the current state (`ProjectEditorPage.tsx:977-985`).
pub fn frame(data: &ProjectData, target_end_day: Option<i32>, zoom: Zoom, marker_dragging: bool) -> Frame {
    let content_right = crate::view::project_finish_day(data).max(target_end_day.unwrap_or(0)).max(14);
    let margin = if marker_dragging { DRAG_MARGIN } else { VISIBLE_LEAD };
    Frame { day_w: zoom.day_w(), lead_days: margin, draw_right: content_right + margin }
}

/// A bar being dragged (`barPreview`): its start and exclusive end in fractional calendar days.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preview {
    pub start: f64,
    pub end: f64,
}

/// What a row of the chart draws.
#[derive(Debug, Clone, PartialEq)]
pub enum BarShape {
    /// A diamond centred on its day.
    Milestone { cx: f64 },
    /// A bracket spanning the children (or the task's own extent when none is visible).
    Summary { x: f64, w: f64 },
    /// A pill.
    Task { x: f64, w: f64 },
}

/// One row of the chart.
#[derive(Debug, Clone, PartialEq)]
pub struct Bar {
    /// Index into `ProjectData::tasks`.
    pub index: usize,
    pub shape: BarShape,
    /// The bar's start day as drawn (the dragged preview included) - the baseline slip compares it.
    pub start_day: f64,
    /// Completion drawn on a summary's label (rolled up over the **visible** rows, like the web).
    pub progress: i32,
}

/// The rows' bars, as `GanttRenderer.render` places them over the **visible** rows (`rows`: indexes into
/// `data.tasks`, in display order). A dragged leaf follows `preview`; a dragged parent shifts its not-started
/// descendants by the same amount; brackets re-span over what moved.
pub fn bars(data: &ProjectData, rows: &[usize], frame: &Frame, preview: Option<(&str, Preview)>) -> Vec<Bar> {
    let tasks: Vec<&Task> = rows.iter().map(|&i| &data.tasks[i]).collect();
    let mut children_of: HashMap<&str, Vec<&Task>> = HashMap::new();
    for t in &tasks {
        if let Some(p) = t.parent_id.as_deref() {
            children_of.entry(p).or_default().push(t);
        }
    }
    let by_id: HashMap<&str, &Task> = tasks.iter().map(|t| (t.id.as_str(), *t)).collect();
    let is_parent = |id: &str| children_of.get(id).is_some_and(|c| !c.is_empty());
    // QUIRK kept: the roll-up only sees the visible rows (a folded parent shows the roll-up of nothing, i.e.
    // its own stored %).
    let owned: Vec<Task> = tasks.iter().map(|t| (*t).clone()).collect();
    let eff = effective_progress(&owned);

    let (mut drag_leaf, mut drag_ancestor, mut drag_shift) = (None::<&str>, None::<&str>, 0.0);
    if let Some((id, p)) = preview {
        if is_parent(id) {
            drag_ancestor = Some(id);
            drag_shift = p.start - by_id.get(id).map(|t| t.es()).unwrap_or(0) as f64;
        } else {
            drag_leaf = Some(id);
        }
    }
    let descends = |t: &Task, anc: &str| -> bool {
        let mut p = t.parent_id.as_deref();
        let mut g = 0;
        while let Some(pid) = p {
            if g > 64 {
                break;
            }
            g += 1;
            if pid == anc {
                return true;
            }
            p = by_id.get(pid).and_then(|x| x.parent_id.as_deref());
        }
        false
    };
    let eff_start = |t: &Task| -> f64 {
        if let (Some(l), Some((_, p))) = (drag_leaf, preview) {
            if t.id == l {
                return p.start;
            }
        }
        if let Some(a) = drag_ancestor {
            if descends(t, a) && t.progress == 0 {
                return t.es() as f64 + drag_shift;
            }
        }
        t.es() as f64
    };
    let eff_end = |t: &Task| -> f64 {
        if let (Some(l), Some((_, p))) = (drag_leaf, preview) {
            if t.id == l {
                return p.end;
            }
        }
        if let Some(a) = drag_ancestor {
            if descends(t, a) && t.progress == 0 {
                return t.cal_end() as f64 + drag_shift;
            }
        }
        t.cal_end() as f64
    };

    fn span_of<'a>(id: &'a str, children_of: &HashMap<&'a str, Vec<&'a Task>>, cache: &mut HashMap<String, Option<(f64, f64)>>, eff_start: &dyn Fn(&Task) -> f64, eff_end: &dyn Fn(&Task) -> f64) -> Option<(f64, f64)> {
        if let Some(c) = cache.get(id) {
            return *c;
        }
        cache.insert(id.to_string(), None);
        let (mut es, mut ef) = (f64::INFINITY, f64::NEG_INFINITY);
        for c in children_of.get(id).into_iter().flatten() {
            let parent = c.is_summary_type() || children_of.get(c.id.as_str()).is_some_and(|k| !k.is_empty());
            if parent {
                if let Some((s, f)) = span_of(c.id.as_str(), children_of, cache, eff_start, eff_end) {
                    es = es.min(s);
                    ef = ef.max(f);
                }
            } else {
                let cs = eff_start(c);
                es = es.min(cs);
                ef = ef.max(eff_end(c).max(cs));
            }
        }
        let res = (es <= ef).then_some((es, ef));
        cache.insert(id.to_string(), res);
        res
    }
    let mut cache: HashMap<String, Option<(f64, f64)>> = HashMap::new();

    let mut out = Vec::with_capacity(tasks.len());
    for (row, t) in tasks.iter().enumerate() {
        let s = eff_start(t);
        let e = eff_end(t);
        let dur = (e - s).max(0.0);
        let x = frame.x_of_day(s);
        let shape = if t.is_milestone() {
            BarShape::Milestone { cx: x }
        } else if t.is_summary_type() || is_parent(&t.id) {
            let span = span_of(t.id.as_str(), &children_of, &mut cache, &eff_start, &eff_end);
            let (ss, se) = span.unwrap_or((s, s + dur.max(1.0)));
            BarShape::Summary { x: frame.x_of_day(ss), w: ((se - ss) * frame.day_w).max(6.0) }
        } else {
            BarShape::Task { x, w: (dur * frame.day_w).max(4.0) }
        };
        out.push(Bar { index: rows[row], shape, start_day: s, progress: eff.get(&t.id).copied().unwrap_or(t.progress) });
    }
    out
}

/// What a press on the chart grabbed (`ganttBarHit`).
#[derive(Debug, Clone, PartialEq)]
pub enum HitMode {
    Move,
    MoveSummary,
    Resize,
    ResizeLeft,
    Link,
    /// The task is selected but its dates are pinned (started, or done).
    Locked,
    /// A boundary marker of the project (`start` or `end`).
    Marker(MarkerKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    Start,
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    /// Index into `ProjectData::tasks` (`None` for a marker).
    pub task: Option<usize>,
    pub mode: HitMode,
    /// The pointer's day, fractional.
    pub day_float: f64,
    /// The bar's world x and width (the link handle starts at `bx + bw`).
    pub bx: f64,
    pub bw: f64,
    /// The visible row (`-1` for a marker).
    pub row: i64,
}

/// Does any not-started leaf hang under `pid` (`hasMovableLeaf`)?
fn has_movable_leaf(data: &ProjectData, pid: &str, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    data.tasks.iter().filter(|x| x.parent_id.as_deref() == Some(pid)).any(|x| {
        if data.tasks.iter().any(|y| y.parent_id.as_deref() == Some(x.id.as_str())) {
            has_movable_leaf(data, &x.id, depth + 1)
        } else {
            x.progress == 0
        }
    })
}

/// The flags of the two boundary markers for hit-testing: `(kind, day, flag x, flag width)` in world x. The
/// platform measures the flag's label (`600 9px` + 12 px of padding, `GanttRenderer.flagRect`).
pub fn marker_hit(x_world: f64, frame: &Frame, flags: &[(MarkerKind, i32, f64, f64)]) -> Option<(MarkerKind, f64)> {
    for &(kind, day, fx, fw) in flags {
        let gx = frame.x_of_day(day as f64);
        let on_flag = x_world >= fx - 3.0 && x_world <= fx + fw + 3.0;
        if on_flag || (x_world - gx).abs() <= 9.0 {
            return Some((kind, gx));
        }
    }
    None
}

/// `ganttBarHit` below the header: what the pointer at world `(x, y)` grabs over the visible `rows`.
pub fn hit(data: &ProjectData, rows: &[usize], frame: &Frame, x_world: f64, y: f64) -> Option<Hit> {
    if y < HEADER_H {
        return None;
    }
    let row = ((y - HEADER_H) / ROW_H).floor() as i64;
    let index = *rows.get(usize::try_from(row).ok()?)?;
    let task = &data.tasks[index];
    let es = task.es() as f64;
    let cal_end = task.cal_end() as f64;
    let day_float = frame.day_at(x_world);
    let has_kids = data.tasks.iter().any(|x| x.parent_id.as_deref() == Some(task.id.as_str()));
    if task.is_summary_type() || has_kids {
        let ef = task.early_finish.map(|v| v as f64).unwrap_or(es + 1.0);
        let sbx = frame.x_of_day(es);
        let sbw = ((ef - es) * frame.day_w).max(6.0);
        if x_world < sbx - 2.0 || x_world > sbx + sbw + 3.0 {
            return None;
        }
        let mode = if has_movable_leaf(data, &task.id, 0) { HitMode::MoveSummary } else { HitMode::Locked };
        return Some(Hit { task: Some(index), mode, day_float, bx: sbx, bw: sbw, row });
    }
    if task.is_milestone() {
        let cx = frame.x_of_day(es);
        if (x_world - cx).abs() > 9.0 {
            return None;
        }
        let mode = if task.progress > 0 { HitMode::Locked } else { HitMode::Move };
        return Some(Hit { task: Some(index), mode, day_float, bx: cx, bw: 0.0, row });
    }
    if task.task_type != "task" {
        return None;
    }
    let bx = frame.x_of_day(es);
    let bw = ((cal_end - es) * frame.day_w).max(4.0);
    let end = bx + bw;
    if x_world >= end + 1.0 && x_world <= end + 14.0 {
        return Some(Hit { task: Some(index), mode: HitMode::Link, day_float, bx, bw, row });
    }
    if x_world < bx - 2.0 || x_world > end + 3.0 {
        return None;
    }
    let grip = 8f64.min(bw / 3.0);
    let raw = if x_world <= bx + grip {
        HitMode::ResizeLeft
    } else if x_world >= end - grip {
        HitMode::Resize
    } else {
        HitMode::Move
    };
    let started = task.progress > 0;
    let done = task.progress >= 100;
    let mode = if (started && matches!(raw, HitMode::ResizeLeft | HitMode::Move)) || (done && raw == HitMode::Resize) { HitMode::Locked } else { raw };
    Some(Hit { task: Some(index), mode, day_float, bx, bw, row })
}

/// The cursor a hover shows (`onGanttMove`): `ew-resize`, `not-allowed`, `crosshair`, `grab`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorKind {
    Default,
    ResizeEw,
    NotAllowed,
    Crosshair,
    Grab,
}

pub fn cursor_for(mode: Option<&HitMode>) -> CursorKind {
    match mode {
        None => CursorKind::Default,
        Some(HitMode::Marker(_)) | Some(HitMode::Resize) | Some(HitMode::ResizeLeft) => CursorKind::ResizeEw,
        Some(HitMode::Locked) => CursorKind::NotAllowed,
        Some(HitMode::Link) => CursorKind::Crosshair,
        Some(_) => CursorKind::Grab,
    }
}

/// A bar drag in progress (`barDragRef`).
#[derive(Debug, Clone, PartialEq)]
pub struct BarDrag {
    pub task_id: String,
    pub mode: HitMode,
    pub grab_day_float: f64,
    pub orig_start: i32,
    pub orig_end: i32,
    pub orig_dur: i32,
    pub orig_ctype: String,
    pub orig_cdate: Option<String>,
    /// How far left the gesture may travel, in days (≤ 0).
    pub min_delta: f64,
    pub start_cx: f64,
    pub start_cy: f64,
    pub active: bool,
}

/// The leaves under `pid`, in outline order.
fn leaves_of<'a>(data: &'a ProjectData, pid: &str, out: &mut Vec<&'a Task>, depth: usize) {
    if depth > 64 {
        return;
    }
    for ch in data.tasks.iter().filter(|x| x.parent_id.as_deref() == Some(pid)) {
        if data.tasks.iter().any(|y| y.parent_id.as_deref() == Some(ch.id.as_str())) {
            leaves_of(data, &ch.id, out, depth + 1);
        } else {
            out.push(ch);
        }
    }
}

/// `onGanttDown` for a bar (not a marker, not locked, not the link handle): the drag state.
pub fn start_bar_drag(data: &ProjectData, hit: &Hit, client_x: f64, client_y: f64) -> Option<BarDrag> {
    let t = &data.tasks[hit.task?];
    let es = t.es();
    let parent = t.is_summary_type() || data.tasks.iter().any(|x| x.parent_id.as_deref() == Some(t.id.as_str()));
    let orig_end = if parent { t.early_finish.unwrap_or(es + 1) } else { t.early_finish.unwrap_or(es + t.duration_days) };
    let mut min_delta = -(es as f64);
    if hit.mode == HitMode::MoveSummary {
        let mut leaves = Vec::new();
        leaves_of(data, &t.id, &mut leaves, 0);
        let movable: Vec<i32> = leaves.iter().filter(|l| l.progress == 0).map(|l| l.es()).collect();
        min_delta = if movable.is_empty() { 0.0 } else { -(movable.iter().copied().min().unwrap_or(0) as f64) };
    }
    Some(BarDrag {
        task_id: t.id.clone(),
        mode: hit.mode.clone(),
        grab_day_float: hit.day_float,
        orig_start: es,
        orig_end,
        orig_dur: t.duration_days,
        orig_ctype: t.constraint_type.clone(),
        orig_cdate: t.constraint_date.clone(),
        min_delta,
        start_cx: client_x,
        start_cy: client_y,
        active: false,
    })
}

impl BarDrag {
    /// Whether the pointer has travelled far enough for the press to be a drag (sets `active`).
    pub fn activate(&mut self, client_x: f64, client_y: f64) -> bool {
        if !self.active && (client_x - self.start_cx).hypot(client_y - self.start_cy) >= DRAG_THRESHOLD {
            self.active = true;
        }
        self.active
    }

    /// `recomputePreview`: where the bar sits for the pointer's fractional day.
    pub fn preview(&self, cur_float: f64) -> Preview {
        let delta = cur_float - self.grab_day_float;
        let (os, oe) = (self.orig_start as f64, self.orig_end as f64);
        match self.mode {
            HitMode::Move | HitMode::MoveSummary => {
                let d = delta.max(self.min_delta);
                let start = os + d;
                Preview { start, end: start + (oe - os) }
            }
            HitMode::ResizeLeft => Preview { start: (os + delta).max(0.0).min(oe - 0.4), end: oe },
            _ => Preview { start: os, end: (oe + delta).max(os + 0.4) },
        }
    }
}

/// One leaf of a phase move: its id, the patch, and the patch that restores it.
pub type PhaseChange = (String, serde_json::Map<String, serde_json::Value>, serde_json::Map<String, serde_json::Value>);

/// What releasing a bar drag changes (`onGanttUp`).
#[derive(Debug, Clone, PartialEq)]
pub enum BarCommit {
    /// Below the threshold, or nothing moved.
    Nothing,
    /// A phase slid: each not-started leaf is pinned (SNET) at its shifted, snapped date.
    Phase { changes: Vec<PhaseChange>, held: Preview },
    /// One task moved or resized: `before`/`after` patches and where its bar will settle.
    Task { before: serde_json::Map<String, serde_json::Value>, after: serde_json::Map<String, serde_json::Value>, settled: Preview },
}

fn obj(pairs: Vec<(&str, serde_json::Value)>) -> serde_json::Map<String, serde_json::Value> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

/// `onGanttUp` for a bar drag: the patches the gesture commits, converted back to the scheduler's terms (a
/// start snapped to the next worked day, a duration in worked days) with the prediction calendar, so the bar
/// settles where the recomputed schedule will put it. `day_iso(off)` is `toISODate(addDays(projectStart, off))`.
pub fn commit_bar_drag(data: &ProjectData, drag: &BarDrag, prev: Preview, cal: &WorkingCalendar, day_iso: &dyn Fn(i32) -> String) -> BarCommit {
    use serde_json::{json, Value};
    if !drag.active {
        return BarCommit::Nothing;
    }
    if drag.mode == HitMode::MoveSummary {
        let delta = js_round(prev.start) as i32 - drag.orig_start;
        if delta == 0 {
            return BarCommit::Nothing;
        }
        let mut leaves = Vec::new();
        leaves_of(data, &drag.task_id, &mut leaves, 0);
        let changes: Vec<_> = leaves
            .iter()
            .filter(|l| l.progress == 0)
            .map(|l| {
                let ns = cal.next_working((l.es() + delta).max(0));
                let after = obj(vec![("constraint_type", json!("SNET")), ("constraint_date", json!(day_iso(ns))), ("start_date", json!(day_iso(ns)))]);
                let before = obj(vec![
                    ("constraint_type", json!(l.constraint_type.clone())),
                    ("constraint_date", l.constraint_date.clone().map(Value::String).unwrap_or(Value::Null)),
                    ("start_date", json!(l.start_date.clone().unwrap_or_else(|| day_iso(l.es())))),
                ]);
                (l.id.clone(), after, before)
            })
            .collect();
        if changes.is_empty() {
            return BarCommit::Nothing;
        }
        return BarCommit::Phase { changes, held: Preview { start: (drag.orig_start + delta) as f64, end: (drag.orig_end + delta) as f64 } };
    }
    let (mut r_start, mut new_dur) = (drag.orig_start, drag.orig_dur);
    match drag.mode {
        HitMode::Move => r_start = cal.next_working(js_round(prev.start).max(0.0) as i32),
        HitMode::ResizeLeft => {
            r_start = cal.next_working(js_round(prev.start).max(0.0) as i32);
            if r_start >= drag.orig_end {
                r_start = cal.next_working((drag.orig_end - 1).max(0));
            }
            new_dur = cal.working_days_between(r_start, drag.orig_end).max(1);
        }
        _ => new_dur = cal.working_days_between(drag.orig_start, js_round(prev.end) as i32).max(1),
    }
    if r_start == drag.orig_start && new_dur == drag.orig_dur {
        return BarCommit::Nothing;
    }
    let predicted_end = if new_dur > 0 { cal.advance(r_start, new_dur) } else { r_start };
    let settled = Preview { start: r_start as f64, end: predicted_end as f64 };
    let resize = drag.mode == HitMode::Resize;
    let before = if resize {
        obj(vec![("duration_days", json!(drag.orig_dur))])
    } else {
        let mut b = obj(vec![
            ("constraint_type", json!(drag.orig_ctype)),
            ("constraint_date", drag.orig_cdate.clone().map(Value::String).unwrap_or(Value::Null)),
            ("start_date", json!(day_iso(drag.orig_start))),
        ]);
        if drag.mode == HitMode::ResizeLeft {
            b.insert("duration_days".into(), json!(drag.orig_dur));
        }
        b
    };
    let after = if resize {
        obj(vec![("duration_days", json!(new_dur))])
    } else {
        let mut a = obj(vec![("constraint_type", json!("SNET")), ("constraint_date", json!(day_iso(r_start))), ("start_date", json!(day_iso(r_start)))]);
        if drag.mode == HitMode::ResizeLeft {
            a.insert("duration_days".into(), json!(new_dur));
        }
        a
    };
    BarCommit::Task { before, after, settled }
}

/// A boundary marker drag (`markerDragRef`).
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerDrag {
    pub which: MarkerKind,
    pub grab_day_float: f64,
    pub orig_day: i32,
    pub start_cx: f64,
    pub active: bool,
}

impl MarkerDrag {
    /// Activates after 4 px of horizontal travel.
    pub fn activate(&mut self, client_x: f64) -> bool {
        if !self.active && (client_x - self.start_cx).abs() >= DRAG_THRESHOLD {
            self.active = true;
        }
        self.active
    }

    /// `recomputeMarker`: the start stays a day before the target end; the end stays after day 1.
    pub fn preview(&self, cur_float: f64, target_end_day: Option<i32>) -> f64 {
        let day = self.orig_day as f64 + (cur_float - self.grab_day_float);
        match self.which {
            MarkerKind::Start => day.min(target_end_day.map(|t| (t - 1) as f64).unwrap_or(f64::INFINITY)),
            MarkerKind::End => day.max(1.0),
        }
    }

    /// The day committed on release, or `None` when nothing moved.
    pub fn commit(&self, preview_day: f64) -> Option<i32> {
        if !self.active {
            return None;
        }
        let day = js_round(preview_day) as i32;
        (day != self.orig_day).then_some(day)
    }
}

/// The `link` gesture's target: the task under the release, if a new link may go there (not itself, not an
/// existing link). `rows` are the visible rows.
pub fn link_target(data: &ProjectData, rows: &[usize], from_id: &str, y: f64) -> Option<String> {
    if y < HEADER_H {
        return None;
    }
    let row = usize::try_from(((y - HEADER_H) / ROW_H).floor() as i64).ok()?;
    let target = &data.tasks[*rows.get(row)?];
    if target.id == from_id || data.dependencies.iter().any(|d| d.from_task_id == from_id && d.to_task_id == target.id) {
        return None;
    }
    Some(target.id.clone())
}

/// The visible dependency connectors: `(from row, to row, from x end, to x start)` in world x
/// (`GanttRenderer` draws only links whose both ends are visible).
pub fn connectors(data: &ProjectData, rows: &[usize], frame: &Frame) -> Vec<(usize, usize, f64, f64)> {
    let pos: HashMap<&str, usize> = rows.iter().enumerate().map(|(r, &i)| (data.tasks[i].id.as_str(), r)).collect();
    data.dependencies
        .iter()
        .filter_map(|d| {
            let (&fr, &tr) = (pos.get(d.from_task_id.as_str())?, pos.get(d.to_task_id.as_str())?);
            let from = &data.tasks[rows[fr]];
            let to = &data.tasks[rows[tr]];
            Some((fr, tr, frame.x_of_day(from.cal_end() as f64), frame.x_of_day(to.es() as f64)))
        })
        .collect()
}

/// The elbow of a finish-to-start connector (`GanttRenderer.elbow`): the polyline's points, the arrowhead
/// ending at the last one.
pub fn elbow(from_x: f64, from_y: f64, to_x: f64, to_y: f64) -> Vec<(f64, f64)> {
    let (stub, approach) = (10.0, 8.0);
    if to_x - approach >= from_x + stub {
        vec![(from_x, from_y), (from_x + stub, from_y), (from_x + stub, to_y), (to_x, to_y)]
    } else {
        let mid_y = from_y + if to_y > from_y { ROW_H / 2.0 } else { -ROW_H / 2.0 };
        vec![(from_x, from_y), (from_x + stub, from_y), (from_x + stub, mid_y), (to_x - approach, mid_y), (to_x - approach, to_y), (to_x, to_y)]
    }
}

/// Whether some leaf finishes after the end marker (`overrun`: the marker turns red).
pub fn overruns(data: &ProjectData, end_day: Option<f64>) -> bool {
    let Some(end) = end_day else { return false };
    data.tasks.iter().any(|t| !t.is_summary_type() && t.cal_end() as f64 > end)
}

/// The ids of tasks with children (`summaryIds`, for « Tout replier »).
pub fn summary_ids(data: &ProjectData) -> HashSet<String> {
    crate::rollup::parent_ids(&data.tasks).into_iter().filter(|id| data.task(id).is_some()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::Date;

    fn task(id: &str, parent: Option<&str>, es: i32, ef: i32, progress: i32) -> Task {
        let mut t = Task::new(id, "p");
        t.parent_id = parent.map(str::to_string);
        t.early_start = Some(es);
        t.early_finish = Some(ef);
        t.duration_days = ef - es;
        t.progress = progress;
        t
    }

    fn data() -> ProjectData {
        let mut d = ProjectData::default();
        d.project.start_date = Some("2026-09-07".into());
        let mut ph = task("ph", None, 0, 9, 0);
        ph.task_type = "summary".into();
        d.tasks = vec![ph, task("a", Some("ph"), 0, 3, 50), task("b", Some("ph"), 7, 9, 0)];
        d
    }

    fn fr() -> Frame {
        Frame { day_w: 26.0, lead_days: 7, draw_right: 30 }
    }

    #[test]
    fn hits_grips_handles_and_locks() {
        let d = data();
        let rows = [0, 1, 2];
        let f = fr();
        let y = |r: f64| HEADER_H + r * ROW_H + 5.0;
        // b: x from (7+7)·26 = 364, width 52.
        assert_eq!(hit(&d, &rows, &f, 364.0 + 26.0, y(2.0)).map(|h| h.mode), Some(HitMode::Move));
        assert_eq!(hit(&d, &rows, &f, 365.0, y(2.0)).map(|h| h.mode), Some(HitMode::ResizeLeft));
        assert_eq!(hit(&d, &rows, &f, 364.0 + 51.0, y(2.0)).map(|h| h.mode), Some(HitMode::Resize));
        assert_eq!(hit(&d, &rows, &f, 364.0 + 52.0 + 6.0, y(2.0)).map(|h| h.mode), Some(HitMode::Link));
        // a has started: moving it is locked, resizing its finish is not.
        assert_eq!(hit(&d, &rows, &f, 182.0 + 39.0, y(1.0)).map(|h| h.mode), Some(HitMode::Locked));
        assert_eq!(hit(&d, &rows, &f, 182.0 + 77.0, y(1.0)).map(|h| h.mode), Some(HitMode::Resize));
        // The phase still has a movable leaf (b).
        assert_eq!(hit(&d, &rows, &f, 200.0, y(0.0)).map(|h| h.mode), Some(HitMode::MoveSummary));
        assert!(hit(&d, &rows, &f, 100.0, y(2.0)).is_none());
    }

    #[test]
    fn a_move_snaps_and_pins_with_snet() {
        let d = data();
        let cal = WorkingCalendar::default_week(false, Date::new(2026, 9, 7).expect("d"));
        let h = hit(&d, &[0, 1, 2], &fr(), 390.0, HEADER_H + 2.0 * ROW_H + 5.0).expect("hit");
        let mut drag = start_bar_drag(&d, &h, 390.0, 0.0).expect("drag");
        assert!(drag.activate(400.0, 0.0));
        // The preview glides in fractional days: two days to the right.
        let p = drag.preview(h.day_float + 2.0);
        assert_eq!(p, Preview { start: 9.0, end: 11.0 });
        let start = Date::new(2026, 9, 7).expect("d");
        let iso = |o: i32| start.add_days(o as i64).iso();
        match commit_bar_drag(&d, &drag, Preview { start: 12.4, end: 14.4 }, &cal, &iso) {
            BarCommit::Task { after, before, settled } => {
                // 12 is a Saturday → Monday 14.
                assert_eq!(after.get("constraint_date"), Some(&serde_json::json!("2026-09-21")));
                assert_eq!(before.get("constraint_type"), Some(&serde_json::json!("ASAP")));
                assert_eq!(settled, Preview { start: 14.0, end: 16.0 });
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_phase_moves_only_its_free_leaves() {
        let d = data();
        let cal = WorkingCalendar::all_days(Date::new(2026, 9, 7).expect("d"));
        let h = hit(&d, &[0, 1, 2], &fr(), 200.0, HEADER_H + 5.0).expect("hit");
        let mut drag = start_bar_drag(&d, &h, 0.0, 0.0).expect("drag");
        // b (free) starts at 7: the phase may go back 7 days, not 0 (a is pinned).
        assert_eq!(drag.min_delta, -7.0);
        drag.active = true;
        let iso = |o: i32| format!("d{o}");
        match commit_bar_drag(&d, &drag, Preview { start: 3.0, end: 12.0 }, &cal, &iso) {
            BarCommit::Phase { changes, .. } => {
                assert_eq!(changes.len(), 1);
                assert_eq!(changes[0].0, "b");
                assert_eq!(changes[0].1.get("start_date"), Some(&serde_json::json!("d10")));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bars_follow_a_dragged_parent() {
        let d = data();
        let b = bars(&d, &[0, 1, 2], &fr(), Some(("ph", Preview { start: 2.0, end: 11.0 })));
        // a has started: it stays; b slides by 2; the bracket spans 0..11.
        assert_eq!(b[2].start_day, 9.0);
        assert_eq!(b[0].shape, BarShape::Summary { x: 7.0 * 26.0, w: 11.0 * 26.0 });
        assert_eq!(b[0].progress, 30);
    }

    #[test]
    fn markers_and_links() {
        let mut m = MarkerDrag { which: MarkerKind::Start, grab_day_float: 0.0, orig_day: 0, start_cx: 0.0, active: false };
        assert!(!m.activate(3.0));
        assert!(m.activate(5.0));
        assert_eq!(m.preview(12.0, Some(10)), 9.0);
        assert_eq!(m.commit(-2.5), Some(-2));
        let d = data();
        assert_eq!(link_target(&d, &[0, 1, 2], "a", HEADER_H + 2.0 * ROW_H + 1.0), Some("b".into()));
        assert_eq!(link_target(&d, &[0, 1, 2], "a", HEADER_H + ROW_H + 1.0), None);
        assert_eq!(elbow(0.0, 10.0, 50.0, 38.0).len(), 4);
        assert_eq!(elbow(40.0, 10.0, 20.0, 38.0).len(), 6);
    }
}
