//! `GanttView` - the plan's main view: the task table on the left (`TaskRow.tsx`), the splitter, the Gantt chart
//! on the right (`GanttRenderer.ts`), one vertical scroll for both, and the input of `ProjectEditorPage.tsx`:
//!
//! * the table: a click selects a row (a second click on the selected row deselects it, `onSelect`), the
//!   chevron folds a summary, the ring marks a task done or not, a double click renames, a click into the
//!   duration, completion or predecessors cell edits it in place, the priority cell opens its menu, the handle
//!   on a row's left drags it before/after/into another row (`reorderTaskCmd`), « Ajouter une tâche » creates
//!   one; the header resizes its columns (drag an edge, double click: the default width) and opens the columns
//!   menu on a right click; the splitter resizes the table (double click: hug the columns) and its chevron
//!   toggles the name-only table;
//! * the chart (`ganttBarHit` and the drag handlers): a bar moves, its ends resize, a phase slides its free
//!   leaves, the dot after a bar draws a link to another row, the project's start and end markers move the
//!   project's dates; a press only becomes a drag after 4 px, the plan scrolls at the edges, Escape aborts.
//!
//! The control edits the shared editor directly (`model::Shared`) and raises `Edited` after each command, so
//! the window can hand the outbox to the server session; `StateChanged` tells the window to re-bind its ribbon.

pub mod chart;
pub mod table;

use std::collections::HashMap;

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, Pen, PointF};
use kubuno_desktop::ui::range::{ScrollBar, ScrollPart};
use kubuno_desktop::ui::{Canvas, Rect, Size, Widget, WidgetState};
use kubuno_desktop::views::component::{Component, Control, ControlCore, EventCx, Keys, PaintEventCx};
use kubuno_desktop::views::component::AccessiblePart;
use kubuno_desktop::views::events::Event;
use kubuno_office_projects_common::model::columns::Col;
use kubuno_office_projects_common::model::line_edit::LineEdit;
use kubuno_office_projects_core::editor::Place;
use kubuno_office_projects_core::gantt::{self, BarDrag, BarCommit, CursorKind, Frame, HitMode, MarkerDrag, MarkerKind, Preview, DRAG_MARGIN, HEADER_H, ROW_H, VISIBLE_LEAD};
use kubuno_office_projects_core::view;
use serde_json::json;

use self::chart::{ChartScene, Flags};
use self::table::{CellEdit, Labels, TableHit, TableScene, ADD_ROW_H};
use crate::controls::model::{sample_model, Palette, Shared};
use crate::Resources;

/// One wheel notch (Chromium's 100 px).
const WHEEL_STEP: f64 = 100.0;

/// The window should re-read the selection and the editor's state.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct GanttStateEventArgs {
    /// The selected task's id (empty: none).
    pub selected: String,
    pub revision: u64,
}

/// A command changed the project (the window feeds the session).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct GanttEditedEventArgs {
    pub label: String,
}

/// A right click: on a row (`row`), on the table's header (`header`), on a row's priority cell (`priority`).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct GanttContextEventArgs {
    pub kind: String,
    pub task: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq)]
enum Drag {
    None,
    Column { col: Col, x0: f64, w0: f64 },
    Splitter { x0: f64, w0: f64 },
    Row { from: usize, sx: f64, sy: f64, active: bool, hint: Option<(usize, u8)> },
    Bar(Box<BarDrag>),
    Marker(MarkerDrag),
    Link { from: String, x1: f64, y1: f64, x2: f64, y2: f64 },
    VScroll(f32),
    HScroll(f32),
}

/// The Gantt view (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Projects")]
#[toolbox(icon = "chart")]
#[default_event("StateChanged")]
pub struct GanttView {
    base: ControlCore,
    /// Occurs when the selection or the project changes.
    #[event]
    #[category("Action")]
    pub state_changed: Event<GanttStateEventArgs>,
    /// Occurs after an edit made in the view.
    #[event]
    #[category("Action")]
    pub edited: Event<GanttEditedEventArgs>,
    /// Occurs on a right click (the window shows the matching menu).
    #[event]
    #[category("Action")]
    pub context_menu_requested: Event<GanttContextEventArgs>,
    model: Option<Shared>,
    bounds: Rect,
    scale: f32,
    scroll_y: f64,
    /// The chart's horizontal scroll (world x at its left edge).
    scroll_x: f64,
    /// The table's horizontal scroll (when the pane is narrower than its columns).
    table_x: f64,
    drag: Drag,
    drag_at: Option<(f64, f64)>,
    preview: Option<(String, Preview)>,
    marker_preview: Option<(MarkerKind, f64)>,
    hover_marker: Option<MarkerKind>,
    hover_row: Option<usize>,
    edit: Option<CellEdit>,
    caret_ms: u64,
    has_focus: bool,
    flags: Flags,
    reported: Option<GanttStateEventArgs>,
    vhot: bool,
    hhot: bool,
    reveal: bool,
}

impl Default for GanttView {
    fn default() -> Self {
        Self {
            base: ControlCore::default(),
            state_changed: Event::default(),
            edited: Event::default(),
            context_menu_requested: Event::default(),
            model: None,
            bounds: Rect::default(),
            scale: 1.0,
            scroll_y: 0.0,
            scroll_x: 0.0,
            table_x: 0.0,
            drag: Drag::None,
            drag_at: None,
            preview: None,
            marker_preview: None,
            hover_marker: None,
            hover_row: None,
            edit: None,
            caret_ms: 0,
            has_focus: false,
            flags: Vec::new(),
            reported: None,
            vhot: false,
            hhot: false,
            reveal: false,
        }
    }
}

fn now() -> u64 {
    kubuno_desktop::controls::host::now_ms()
}

fn labels() -> Labels {
    Labels {
        task: Resources::col_task().into(),
        duration: Resources::col_duration().into(),
        priority: Resources::col_priority().into(),
        start: Resources::col_start().into(),
        end: Resources::col_end().into(),
        variance: Resources::col_variance().into(),
        predecessors: Resources::col_pred().into(),
        resources: Resources::col_res().into(),
        add_task: Resources::add_task().into(),
        days_short: Resources::days_short().into(),
        priorities: [Resources::priority_low().into(), Resources::priority_medium().into(), Resources::priority_high().into(), Resources::priority_critical().into()],
    }
}

impl GanttView {
    /// Shows the shared model (the window's editor and view state).
    pub fn set_model(&mut self, model: Shared) {
        self.model = Some(model);
        self.scroll_x = 0.0;
        self.scroll_y = 0.0;
        self.reported = None;
        self.invalidate();
    }

    pub fn model(&self) -> Option<Shared> {
        self.model.clone()
    }

    /// The selected row is kept in view at the next paint.
    pub fn reveal_selection(&mut self) {
        self.reveal = true;
        self.invalidate();
    }

    fn palette(&self) -> Palette {
        Palette::of(kubuno_desktop::Application::theme().mode == kubuno_desktop::ui::ThemeMode::Dark)
    }

    // ── Geometry ────────────────────────────────────────────────────────────

    fn width(&self) -> f64 {
        (self.bounds.right - self.bounds.left) as f64
    }

    fn height(&self) -> f64 {
        (self.bounds.bottom - self.bounds.top) as f64
    }

    fn pane_w(&self) -> f64 {
        let Some(m) = self.model.as_ref() else { return 0.0 };
        let m = m.borrow();
        m.view.layout.pane_width(m.view.baseline_id.is_some(), self.width())
    }

    fn frame(&self) -> Option<Frame> {
        let m = self.model.as_ref()?.borrow();
        let ed = m.ed.as_ref()?;
        let start = ed.project_start();
        let target = view::target_end_day(ed.data(), start);
        let dragging = matches!(&self.drag, Drag::Marker(md) if md.active);
        Some(gantt::frame(ed.data(), target, m.view.zoom, dragging))
    }

    fn content_h(&self) -> f64 {
        let n = self.model.as_ref().map(|m| m.borrow().rows().len()).unwrap_or(0);
        HEADER_H + n as f64 * ROW_H + ADD_ROW_H
    }

    fn max_scroll_y(&self) -> f64 {
        (self.content_h() - self.height()).max(0.0)
    }

    fn max_scroll_x(&self) -> f64 {
        let Some(f) = self.frame() else { return 0.0 };
        (f.width() - (self.width() - self.pane_w())).max(0.0)
    }

    fn clamp_scroll(&mut self) {
        self.scroll_y = self.scroll_y.clamp(0.0, self.max_scroll_y());
        self.scroll_x = self.scroll_x.clamp(0.0, self.max_scroll_x());
        let tw = self.model.as_ref().map(|m| {
            let m = m.borrow();
            m.view.layout.table_width(m.view.baseline_id.is_some())
        });
        self.table_x = self.table_x.clamp(0.0, (tw.unwrap_or(0.0) - self.pane_w()).max(0.0));
    }

    fn vbar(&self) -> Option<(ScrollBar, Rect)> {
        let b = self.bounds;
        let inner = Rect::new(b.left, b.top + HEADER_H as f32, b.right, b.bottom);
        let bar = ScrollBar::from_content(false, self.content_h() as f32 - HEADER_H as f32, (self.height() - HEADER_H) as f32, self.scroll_y as f32)?.with_expanded(self.vhot);
        let rail = bar.rail(&inner);
        Some((bar, rail))
    }

    fn hbar(&self) -> Option<(ScrollBar, Rect)> {
        let f = self.frame()?;
        let b = self.bounds;
        let inner = Rect::new(b.left + self.pane_w() as f32, b.top, b.right, b.bottom);
        let bar = ScrollBar::from_content(true, f.width() as f32, (self.width() - self.pane_w()) as f32, self.scroll_x as f32)?.with_expanded(self.hhot);
        let rail = bar.rail(&inner);
        Some((bar, rail))
    }

    /// World x of a control-local x in the chart.
    fn world_x(&self, x: f64) -> f64 {
        x - self.pane_w() + self.scroll_x
    }

    fn table_scene<'a>(&'a self, m: &'a crate::controls::model::Model, rows: &'a [view::Row], labels: &'a Labels, progress: &'a HashMap<String, i32>, baseline: Option<&'a HashMap<String, (i32, i32)>>) -> Option<TableScene<'a>> {
        let ed = m.ed.as_ref()?;
        let drag = match &self.drag {
            Drag::Row { from, active: true, hint, .. } => Some((*from, *hint)),
            _ => None,
        };
        Some(TableScene {
            data: ed.data(),
            rows,
            layout: &m.view.layout,
            has_baseline: m.view.baseline_id.is_some(),
            baseline,
            labels,
            lang: m.view.lang,
            project_start: ed.project_start(),
            left: self.bounds.left,
            top: self.bounds.top,
            width: self.pane_w() as f32,
            height: self.height() as f32,
            scroll_x: self.table_x,
            scroll_y: self.scroll_y,
            selected: None,
            hover_row: self.hover_row,
            edit: self.edit.as_ref(),
            caret_on: self.has_focus && (now().saturating_sub(self.caret_ms) < 500 || (now() - self.caret_ms) / 500 % 2 == 1),
            progress,
            drag,
            collapsed: &m.view.collapsed,
        })
    }

    /// The table's hit for a control-local point.
    fn table_hit(&self, x: f64, y: f64) -> (TableHit, Vec<view::Row>) {
        let Some(model) = self.model.clone() else { return (TableHit::Nothing, Vec::new()) };
        let m = model.borrow();
        let rows = m.rows();
        let labels = labels();
        let progress = HashMap::new();
        let hit = match self.table_scene(&m, &rows, &labels, &progress, None) {
            Some(s) => s.hit(self.bounds.left + x as f32, self.bounds.top + y as f32),
            None => TableHit::Nothing,
        };
        (hit, rows)
    }

    fn cell_rect(&self, row: usize, col: Col) -> Option<Rect> {
        let model = self.model.clone()?;
        let m = model.borrow();
        let rows = m.rows();
        let labels = labels();
        let progress = HashMap::new();
        self.table_scene(&m, &rows, &labels, &progress, None)?.cell_rect(row, col)
    }

    // ── Editing ─────────────────────────────────────────────────────────────

    /// Runs an editor command and reports it (`Edited`); a refusal stays in the editor's messages.
    pub fn command(&mut self, label: &str, f: impl FnOnce(&mut kubuno_office_projects_core::editor::Editor) -> Result<(), kubuno_office_projects_core::change::Refused>) {
        let Some(model) = self.model.clone() else { return };
        {
            let mut m = model.borrow_mut();
            let Some(ed) = m.ed.as_mut() else { return };
            ed.set_now(kubuno_office_projects_common::platform::current().clock.now_ms());
            let _ = f(ed);
        }
        self.invalidate();
        self.raise_edited(GanttEditedEventArgs { label: label.into() });
    }

    fn selected_id(&self) -> Option<String> {
        self.model.as_ref()?.borrow().selected_task()
    }

    fn select(&mut self, id: Option<String>) {
        if let Some(m) = &self.model {
            m.borrow_mut().view.selected = id;
        }
        self.invalidate();
    }

    pub fn is_editing(&self) -> bool {
        self.edit.is_some()
    }

    /// Starts editing a cell of row `i` (`Name`, `Dur`, `Progress`, `Pred`).
    pub fn begin_edit(&mut self, i: usize, col: Col) {
        let Some(model) = self.model.clone() else { return };
        let m = model.borrow();
        let rows = m.rows();
        let (Some(ed), Some(row)) = (m.ed.as_ref(), rows.get(i)) else { return };
        let t = &ed.data().tasks[row.index];
        let parent = t.is_summary_type() || row.has_children;
        let text = match col {
            Col::Name => t.name.clone(),
            Col::Dur if !parent => t.duration_days.to_string(),
            Col::Progress if !parent => t.progress.to_string(),
            Col::Pred => view::predecessor_text(ed.data(), &t.id),
            _ => return,
        };
        let task = t.id.clone();
        drop(m);
        self.edit = Some(CellEdit { task, col, le: LineEdit::new(&text) });
        self.caret_ms = now();
        self.invalidate();
    }

    /// Edits the selected task's name (F2, « Renommer »).
    pub fn rename_selected(&mut self) {
        let Some(id) = self.selected_id() else { return };
        let i = self.model.as_ref().and_then(|m| {
            let m = m.borrow();
            let d = m.data()?;
            m.rows().iter().position(|r| d.tasks[r.index].id == id)
        });
        if let Some(i) = i {
            self.begin_edit(i, Col::Name);
        }
    }

    /// Commits the cell being edited, as the web's input handlers do on blur.
    pub fn commit_edit(&mut self) {
        let Some(e) = self.edit.take() else { return };
        let text = e.le.text();
        let id = e.task.clone();
        let (min_dur, parent_pct) = self
            .model
            .as_ref()
            .and_then(|m| m.borrow().data().and_then(|d| d.task(&id)).map(|t| (if t.is_milestone() { 0 } else { 1 }, t.progress)))
            .unwrap_or((1, 0));
        let parse = |s: &str| -> i32 {
            let d: String = s.trim().chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
            d.parse().unwrap_or(0)
        };
        match e.col {
            Col::Name => {
                let name = text.trim().to_string();
                if !name.is_empty() {
                    self.command("task-edit", |ed| ed.set_task_field(&id, "name", json!(name)));
                }
            }
            Col::Dur => {
                let v = parse(&text).max(min_dur);
                self.command("task-edit", |ed| ed.set_task_field(&id, "duration_days", json!(v)));
            }
            Col::Progress => {
                let v = parse(&text).clamp(0, 100);
                if v != parent_pct {
                    self.command("task-edit", |ed| ed.set_task_field(&id, "progress", json!(v)));
                }
            }
            Col::Pred => {
                let current = self.model.as_ref().and_then(|m| m.borrow().data().map(|d| view::predecessor_text(d, &id))).unwrap_or_default();
                if text != current {
                    self.command("predecessors", |ed| ed.set_predecessors(&id, &text));
                }
            }
            _ => {}
        }
        self.invalidate();
    }

    pub fn cancel_edit(&mut self) {
        self.edit = None;
        self.invalidate();
    }

    /// The text being edited, for the ribbon's clipboard group: cut/copy the selection, paste over it.
    pub fn edit_clipboard(&mut self, op: &str, text: Option<&str>) -> Option<String> {
        let e = self.edit.as_mut()?;
        let out = match op {
            "cut" => Some(e.le.cut()),
            "copy" => Some(e.le.selected_text()),
            "paste" => {
                e.le.insert(text.unwrap_or(""));
                None
            }
            _ => None,
        };
        self.invalidate();
        out
    }

    // ── Scrolling ───────────────────────────────────────────────────────────

    fn scroll(&mut self, dx: f64, dy: f64) {
        self.scroll_x += dx;
        self.scroll_y += dy;
        self.clamp_scroll();
        self.invalidate();
    }

    /// Edge auto-scroll while a bar or a marker is dragged (`armAutoScroll`): 16 px a tick near the chart's edges.
    fn auto_scroll(&mut self) {
        let Some((x, _)) = self.drag_at else { return };
        let active = matches!(&self.drag, Drag::Bar(b) if b.active) || matches!(&self.drag, Drag::Marker(m) if m.active);
        if !active {
            return;
        }
        let pane = self.pane_w();
        let dir = if x > self.width() - gantt::AUTO_SCROLL_EDGE {
            1.0
        } else if x < pane + gantt::AUTO_SCROLL_EDGE {
            -1.0
        } else {
            0.0
        };
        if dir != 0.0 {
            let before = self.scroll_x;
            self.scroll_x = (self.scroll_x + dir * gantt::AUTO_SCROLL_STEP).max(0.0).min(self.max_scroll_x());
            if self.scroll_x != before {
                self.drag_to(x, self.drag_at.map(|p| p.1).unwrap_or(0.0));
            }
            kubuno_desktop::controls::host::request_repaint_after(16);
        }
    }

    // ── Pointer ─────────────────────────────────────────────────────────────

    /// The chart's hit at a control-local point (below the header).
    fn chart_hit(&self, x: f64, y: f64) -> Option<gantt::Hit> {
        let model = self.model.clone()?;
        let m = model.borrow();
        let ed = m.ed.as_ref()?;
        let rows: Vec<usize> = m.rows().iter().map(|r| r.index).collect();
        let f = self.frame()?;
        gantt::hit(ed.data(), &rows, &f, self.world_x(x), y + self.scroll_y)
    }

    fn marker_at(&self, x: f64) -> Option<MarkerKind> {
        let f = self.frame()?;
        gantt::marker_hit(self.world_x(x), &f, &self.flags).map(|(k, _)| k)
    }

    fn drag_to(&mut self, x: f64, y: f64) {
        let Some(f) = self.frame() else { return };
        let cur = f.day_at(self.world_x(x));
        match &mut self.drag {
            Drag::Column { col, x0, w0 } => {
                let (c, w) = (*col, *w0 + (x - *x0));
                if let Some(m) = &self.model {
                    m.borrow_mut().view.layout.set_width(c, w);
                }
            }
            Drag::Splitter { x0, w0 } => {
                let w = (*w0 + (x - *x0)).max(kubuno_office_projects_common::model::columns::PANE_MIN);
                if let Some(m) = &self.model {
                    m.borrow_mut().view.layout.pane_width = Some(w.round());
                }
            }
            Drag::Row { from, sx, sy, active, hint } => {
                if !*active && (x - *sx).hypot(y - *sy) >= gantt::DRAG_THRESHOLD {
                    *active = true;
                }
                if *active {
                    let rel = y + self.scroll_y - HEADER_H;
                    let i = (rel / ROW_H).floor();
                    let (from, rows) = (*from, self.model.as_ref().map(|m| m.borrow().rows()).unwrap_or_default());
                    *hint = None;
                    if i >= 0.0 && (i as usize) < rows.len() && i as usize != from {
                        let frac = rel / ROW_H - i;
                        let can_nest = self.model.as_ref().is_some_and(|m| {
                            let m = m.borrow();
                            m.view.filter.sort_by.is_empty() && m.view.filter.group_by.is_empty() && m.data().is_some_and(|d| !d.tasks[rows[i as usize].index].is_milestone())
                        });
                        let place = if can_nest && frac > 0.33 && frac < 0.67 { 2 } else if frac < 0.5 { 0 } else { 1 };
                        *hint = Some((i as usize, place));
                    }
                }
            }
            Drag::Bar(b) => {
                if b.activate(x, y) {
                    self.preview = Some((b.task_id.clone(), b.preview(cur)));
                }
            }
            Drag::Marker(md) => {
                let was = md.active;
                if md.activate(x) {
                    if !was {
                        // The drag margins open: keep the view where it is.
                        self.scroll_x += (DRAG_MARGIN - VISIBLE_LEAD) as f64 * f.day_w;
                    }
                    let target = self.model.as_ref().and_then(|m| m.borrow().ed.as_ref().and_then(|e| view::target_end_day(e.data(), e.project_start())));
                    let f2 = self.frame().unwrap_or(f);
                    let cur = f2.day_at(self.world_x(x));
                    if let Drag::Marker(md) = &self.drag {
                        self.marker_preview = Some((md.which, md.preview(cur, target)));
                    }
                }
            }
            Drag::Link { x2, y2, .. } => {
                *x2 = x + self.bounds.left as f64;
                *y2 = y + self.bounds.top as f64;
            }
            _ => {}
        }
        self.invalidate();
    }

    fn end_drag(&mut self, x: f64, y: f64) {
        let drag = std::mem::replace(&mut self.drag, Drag::None);
        self.drag_at = None;
        match drag {
            Drag::Row { from, active: true, hint: Some((target, place)), .. } => {
                let ids = self.model.as_ref().and_then(|m| {
                    let m = m.borrow();
                    let d = m.data()?;
                    let rows = m.rows();
                    Some((d.tasks[rows.get(from)?.index].id.clone(), d.tasks[rows.get(target)?.index].id.clone()))
                });
                if let Some((a, b)) = ids {
                    let p = match place {
                        0 => Place::Before,
                        1 => Place::After,
                        _ => Place::Inside,
                    };
                    self.command("task-reorder", |ed| ed.reorder(&a, &b, p));
                }
            }
            Drag::Bar(b) => {
                let prev = self.preview.take().map(|(_, p)| p);
                if let (true, Some(p)) = (b.active, prev) {
                    let commit = self.model.as_ref().and_then(|m| {
                        let m = m.borrow();
                        let ed = m.ed.as_ref()?;
                        let start = ed.project_start();
                        let cal = ed.prediction_calendar();
                        Some(gantt::commit_bar_drag(ed.data(), &b, p, &cal, &|o| start.add_days(o as i64).iso()))
                    });
                    if let Some(c) = commit {
                        if c != BarCommit::Nothing {
                            let id = b.task_id.clone();
                            self.command("gantt", |ed| ed.commit_bar(&id, &c));
                        }
                    }
                }
            }
            Drag::Marker(md) => {
                let f = self.frame();
                let preview = self.marker_preview.take();
                if md.active {
                    let compensate = f.map(|f| (DRAG_MARGIN - VISIBLE_LEAD) as f64 * f.day_w).unwrap_or(0.0);
                    match preview.and_then(|(_, d)| md.commit(d)) {
                        Some(day) => {
                            let start = md.which == MarkerKind::Start;
                            self.command(if start { "project-start" } else { "project-end" }, |ed| ed.commit_marker(start, day));
                            // A new start shifts day 0: back to the resting view.
                            self.scroll_x = if start { 0.0 } else { (self.scroll_x - compensate).max(0.0) };
                        }
                        None => self.scroll_x = (self.scroll_x - compensate).max(0.0),
                    }
                }
            }
            Drag::Link { from, .. } => {
                let target = self.model.as_ref().and_then(|m| {
                    let m = m.borrow();
                    let rows: Vec<usize> = m.rows().iter().map(|r| r.index).collect();
                    gantt::link_target(m.data()?, &rows, &from, y + self.scroll_y)
                });
                let _ = x;
                if let Some(t) = target {
                    self.command("dep-add", |ed| ed.add_dependency(&from, &t, true));
                }
            }
            _ => {}
        }
        self.clamp_scroll();
        self.invalidate();
    }

    /// Escape: the gesture in flight is dropped, nothing is sent.
    fn abort_drag(&mut self) -> bool {
        let had = !matches!(self.drag, Drag::None);
        if let Drag::Marker(md) = &self.drag {
            if md.active {
                if let Some(f) = self.frame() {
                    self.scroll_x = (self.scroll_x - (DRAG_MARGIN - VISIBLE_LEAD) as f64 * f.day_w).max(0.0);
                }
            }
        }
        self.drag = Drag::None;
        self.preview = None;
        self.marker_preview = None;
        self.drag_at = None;
        self.invalidate();
        had
    }

    fn report(&mut self) {
        if self.design_mode() {
            return;
        }
        let Some(model) = self.model.clone() else { return };
        let args = {
            let m = model.borrow();
            GanttStateEventArgs { selected: m.selected_task().unwrap_or_default(), revision: m.ed.as_ref().map(|e| e.revision()).unwrap_or(0) }
        };
        if self.reported.as_ref() != Some(&args) {
            self.reported = Some(args.clone());
            self.raise_state_changed(args);
            kubuno_desktop::controls::host::request_repaint_after(0);
        }
    }

    fn reveal_selected(&mut self) {
        let Some(model) = self.model.clone() else { return };
        let m = model.borrow();
        let Some(id) = m.selected_task() else { return };
        let Some(d) = m.data() else { return };
        let Some(i) = m.rows().iter().position(|r| d.tasks[r.index].id == id) else { return };
        drop(m);
        let top = i as f64 * ROW_H;
        let view_h = self.height() - HEADER_H;
        if top < self.scroll_y {
            self.scroll_y = top;
        } else if top + ROW_H > self.scroll_y + view_h {
            self.scroll_y = top + ROW_H - view_h;
        }
        self.clamp_scroll();
    }

    /// The selection moves one row up or down (a desktop convention: the web has no keyboard selection).
    fn move_selection(&mut self, delta: i64) {
        let Some(model) = self.model.clone() else { return };
        let next = {
            let m = model.borrow();
            let Some(d) = m.data() else { return };
            let rows = m.rows();
            if rows.is_empty() {
                return;
            }
            let cur = m.selected_task().and_then(|id| rows.iter().position(|r| d.tasks[r.index].id == id));
            let i = match cur {
                Some(c) => (c as i64 + delta).clamp(0, rows.len() as i64 - 1) as usize,
                None => 0,
            };
            d.tasks[rows[i].index].id.clone()
        };
        self.select(Some(next));
        self.reveal = true;
    }
}

impl Control for GanttView {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 900.0, height: 500.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        self.bounds = e.clip_rectangle;
        self.scale = e.graphics.dpi_scale().max(0.5);
        if self.model.is_none() && self.design_mode() {
            self.model = Some(std::rc::Rc::new(std::cell::RefCell::new(sample_model())));
        }
        self.clamp_scroll();
        if self.reveal {
            self.reveal = false;
            self.reveal_selected();
        }
        self.auto_scroll();
        let th = self.palette();
        let g = e.graphics;
        let Some(model) = self.model.clone() else {
            g.fill_rectangle(Brush::solid(th.surface0), self.bounds);
            return;
        };
        let pane = self.pane_w() as f32;
        let frame = self.frame();
        {
            let m = model.borrow();
            let Some(ed) = m.ed.as_ref() else {
                g.fill_rectangle(Brush::solid(th.surface0), self.bounds);
                return;
            };
            let rows = m.rows();
            let indexes: Vec<usize> = rows.iter().map(|r| r.index).collect();
            let labels = labels();
            let progress = kubuno_office_projects_core::rollup::effective_progress(&ed.data().tasks);
            let baseline = m.baseline_map();
            let selected = m.selected_task();
            if let Some(mut scene) = self.table_scene(&m, &rows, &labels, &progress, baseline.as_ref()) {
                scene.selected = selected.as_deref();
                scene.paint(g, &th);
            }
            if let Some(f) = frame {
                let hover_text = match self.hover_marker {
                    Some(MarkerKind::Start) => Resources::marker_start_tip().to_string(),
                    Some(MarkerKind::End) => Resources::marker_end_tip().to_string(),
                    None => String::new(),
                };
                let link_preview = match &self.drag {
                    Drag::Link { x1, y1, x2, y2, .. } => Some((*x1, *y1, *x2, *y2)),
                    _ => None,
                };
                let scene = ChartScene {
                    data: ed.data(),
                    rows: &indexes,
                    frame: f,
                    project_start: ed.project_start(),
                    today: m.view.today,
                    lang: m.view.lang,
                    left: self.bounds.left + pane,
                    top: self.bounds.top,
                    width: (self.width() as f32 - pane).max(0.0),
                    height: self.height() as f32,
                    scroll_x: self.scroll_x,
                    scroll_y: self.scroll_y,
                    dpr: self.scale,
                    preview: self.preview.clone(),
                    link_preview,
                    baseline,
                    target_end_day: view::target_end_day(ed.data(), ed.project_start()),
                    marker_preview: self.marker_preview,
                    hover_marker: self.hover_marker,
                    hover_text,
                    selected,
                };
                self.flags = scene.paint(g, &th);
            }
        }
        // The splitter: the pane's border, and its chevron (compact table) on the header band.
        let sx = self.bounds.left + pane;
        g.draw_line(&Pen::new(th.border, 1.0), PointF::new(sx - 0.5, self.bounds.top), PointF::new(sx - 0.5, self.bounds.bottom));
        let compact = model.borrow().view.layout.compact;
        let chip = Rect::new(sx - 9.0, self.bounds.top + HEADER_H as f32 / 2.0 - 11.0, sx + 9.0, self.bounds.top + HEADER_H as f32 / 2.0 + 11.0);
        g.fill_rounded_rectangle(Brush::solid(th.surface0), chip, 4.0);
        g.draw_rounded_rectangle(&Pen::new(th.border, 1.0), chip, 4.0);
        g.draw_icon(if compact { "ChevronsRight" } else { "ChevronsLeft" }, chip, 12.0, th.text_secondary);
        {
            let c: &dyn Canvas = e.graphics;
            if let Some((bar, rail)) = self.vbar() {
                bar.paint(c, rail, if self.vhot { WidgetState::REST.hot(true) } else { WidgetState::REST });
            }
            if let Some((bar, rail)) = self.hbar() {
                bar.paint(c, rail, if self.hhot { WidgetState::REST.hot(true) } else { WidgetState::REST });
            }
        }
        if self.edit.is_some() && self.has_focus {
            kubuno_desktop::controls::host::request_repaint_after(500);
        }
        self.report();
        e.raise(self, "OnPaint");
    }

    fn is_input_key(&self, key: Keys) -> bool {
        use kubuno_desktop::controls::host::vk;
        [vk::DOWN, vk::UP, vk::LEFT, vk::RIGHT, vk::HOME, vk::END, vk::TAB, vk::ENTER, vk::BACK, vk::DELETE, vk::ESCAPE, vk::F2].contains(&key.key.0)
    }

    fn on_got_focus(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        self.has_focus = true;
        self.invalidate();
        e.raise(&*self, "OnGotFocus");
    }

    fn on_lost_focus(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        self.has_focus = false;
        // The web commits an input when it loses the focus.
        self.commit_edit();
        self.invalidate();
        e.raise(&*self, "OnLostFocus");
    }

    fn on_mouse_wheel(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let notches = e.args().delta as f64;
        if e.args().mods.shift {
            self.scroll(notches * WHEEL_STEP, 0.0);
        } else {
            self.scroll(0.0, notches * WHEEL_STEP);
        }
        e.raise(&*self, "OnMouseWheel");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x as f64, e.args().y as f64);
        let (ax, ay) = (e.args().x + self.bounds.left, e.args().y + self.bounds.top);
        let button = e.args().button;
        self.focus();
        self.has_focus = true;
        // Scroll bars.
        if let Some((bar, rail)) = self.vbar().filter(|(_, r)| r.contains(ax, ay)) {
            let page = self.height() - HEADER_H;
            match bar.part_at(rail, ax, ay) {
                Some(ScrollPart::Thumb) => self.drag = Drag::VScroll(ay - bar.thumb_rect(rail).top),
                Some(ScrollPart::PageLow) => self.scroll(0.0, -page),
                Some(ScrollPart::PageHigh) => self.scroll(0.0, page),
                Some(ScrollPart::ArrowLow) => self.scroll(0.0, -40.0),
                Some(ScrollPart::ArrowHigh) => self.scroll(0.0, 40.0),
                None => {}
            }
            e.raise(&*self, "OnMouseDown");
            return;
        }
        if let Some((bar, rail)) = self.hbar().filter(|(_, r)| r.contains(ax, ay)) {
            let page = self.width() - self.pane_w();
            match bar.part_at(rail, ax, ay) {
                Some(ScrollPart::Thumb) => self.drag = Drag::HScroll(ax - bar.thumb_rect(rail).left),
                Some(ScrollPart::PageLow) => self.scroll(-page, 0.0),
                Some(ScrollPart::PageHigh) => self.scroll(page, 0.0),
                Some(ScrollPart::ArrowLow) => self.scroll(-40.0, 0.0),
                Some(ScrollPart::ArrowHigh) => self.scroll(40.0, 0.0),
                None => {}
            }
            e.raise(&*self, "OnMouseDown");
            return;
        }
        // An edit in progress is committed by any click elsewhere (the input's blur).
        if let Some(ed) = &self.edit {
            let row = self.model.as_ref().and_then(|m| {
                let m = m.borrow();
                let d = m.data()?;
                m.rows().iter().position(|r| d.tasks[r.index].id == ed.task)
            });
            let inside = row.and_then(|i| self.cell_rect(i, ed.col)).is_some_and(|r| r.contains(ax, ay));
            if inside {
                e.raise(&*self, "OnMouseDown");
                return;
            }
            self.commit_edit();
        }
        let pane = self.pane_w();
        // The splitter's chevron, then the splitter.
        if (x - pane).abs() <= 9.0 && (y - HEADER_H / 2.0).abs() <= 11.0 && button == MouseButton::Left {
            if let Some(m) = &self.model {
                let mut m = m.borrow_mut();
                m.view.layout.compact = !m.view.layout.compact;
            }
            self.clamp_scroll();
            self.invalidate();
            e.raise(&*self, "OnMouseDown");
            return;
        }
        if (x - pane).abs() <= 3.0 && button == MouseButton::Left {
            self.drag = Drag::Splitter { x0: x, w0: pane };
            e.raise(&*self, "OnMouseDown");
            return;
        }
        if x < pane {
            let (hit, rows) = self.table_hit(x, y);
            let id_of = |i: usize| -> Option<String> { self.model.as_ref().and_then(|m| m.borrow().data().map(|d| d.tasks[rows[i].index].id.clone())) };
            if button == MouseButton::Right {
                match hit {
                    TableHit::Header { .. } => self.raise_context_menu_requested(GanttContextEventArgs { kind: "header".into(), task: String::new(), x: ax, y: ay }),
                    TableHit::Cell(i, _) | TableHit::Chevron(i) | TableHit::Ring(i) | TableHit::Grip(i) => {
                        if let Some(id) = id_of(i) {
                            self.select(Some(id.clone()));
                            self.raise_context_menu_requested(GanttContextEventArgs { kind: "row".into(), task: id, x: ax, y: ay });
                        }
                    }
                    _ => {}
                }
                e.raise(&*self, "OnMouseDown");
                return;
            }
            if button != MouseButton::Left {
                e.raise(&*self, "OnMouseDown");
                return;
            }
            match hit {
                TableHit::Header { edge: Some(c) } => {
                    let w0 = self.model.as_ref().map(|m| m.borrow().view.layout.width(c)).unwrap_or(c.default_width());
                    self.drag = Drag::Column { col: c, x0: x, w0 };
                }
                TableHit::Chevron(i) => {
                    if let (Some(id), Some(m)) = (id_of(i), &self.model) {
                        let mut m = m.borrow_mut();
                        if !m.view.collapsed.remove(&id) {
                            m.view.collapsed.insert(id);
                        }
                    }
                }
                TableHit::Ring(i) => {
                    if let Some(id) = id_of(i) {
                        self.command("task-edit", |ed| ed.toggle_done(&id));
                    }
                }
                TableHit::Grip(i) => {
                    let sortable = self.model.as_ref().is_some_and(|m| {
                        let m = m.borrow();
                        m.view.filter.sort_by.is_empty() && m.view.filter.group_by.is_empty()
                    });
                    if sortable {
                        self.drag = Drag::Row { from: i, sx: x, sy: y, active: false, hint: None };
                    }
                }
                TableHit::Cell(i, c) => {
                    let parent = self.model.as_ref().is_some_and(|m| {
                        let m = m.borrow();
                        let rows = m.rows();
                        m.data().is_some_and(|d| rows.get(i).is_some_and(|r| r.has_children || d.tasks[r.index].is_summary_type()))
                    });
                    match c {
                        Col::Dur | Col::Progress if !parent => self.begin_edit(i, c),
                        Col::Pred => self.begin_edit(i, c),
                        Col::Priority => {
                            if let Some(id) = id_of(i) {
                                self.select(Some(id.clone()));
                                self.raise_context_menu_requested(GanttContextEventArgs { kind: "priority".into(), task: id, x: ax, y: ay });
                            }
                        }
                        _ => {
                            // `onSelect`: a click on the selected row deselects it.
                            let id = id_of(i);
                            let cur = self.selected_id();
                            self.select(if id == cur { None } else { id });
                        }
                    }
                }
                TableHit::AddRow => self.command("task-create", |ed| ed.create_task(None, None, None).map(|_| ())),
                _ => {}
            }
            self.drag_at = Some((x, y));
            self.invalidate();
            e.raise(&*self, "OnMouseDown");
            return;
        }
        // The chart.
        if button != MouseButton::Left {
            if button == MouseButton::Right {
                if let Some(h) = self.chart_hit(x, y) {
                    if let Some(i) = h.task {
                        let id = self.model.as_ref().and_then(|m| m.borrow().data().map(|d| d.tasks[i].id.clone()));
                        if let Some(id) = id {
                            self.select(Some(id.clone()));
                            self.raise_context_menu_requested(GanttContextEventArgs { kind: "row".into(), task: id, x: ax, y: ay });
                        }
                    }
                }
            }
            e.raise(&*self, "OnMouseDown");
            return;
        }
        if y < HEADER_H {
            if let Some(which) = self.marker_at(x) {
                let f = self.frame();
                let target = self.model.as_ref().and_then(|m| m.borrow().ed.as_ref().and_then(|e| view::target_end_day(e.data(), e.project_start())));
                let orig = match which {
                    MarkerKind::Start => 0,
                    MarkerKind::End => target.unwrap_or(0),
                };
                self.drag = Drag::Marker(MarkerDrag { which, grab_day_float: f.map(|f| f.day_at(self.world_x(x))).unwrap_or(0.0), orig_day: orig, start_cx: x, active: false });
            }
            e.raise(&*self, "OnMouseDown");
            return;
        }
        if let Some(h) = self.chart_hit(x, y) {
            let Some(i) = h.task else { return };
            let id = self.model.as_ref().and_then(|m| m.borrow().data().map(|d| d.tasks[i].id.clone()));
            self.select(id.clone());
            match h.mode {
                HitMode::Locked => {}
                HitMode::Link => {
                    if let Some(id) = id {
                        let x1 = self.bounds.left as f64 + self.pane_w() + h.bx + h.bw - self.scroll_x + 6.0;
                        let y1 = self.bounds.top as f64 + HEADER_H + h.row as f64 * ROW_H + ROW_H / 2.0 - self.scroll_y;
                        self.drag = Drag::Link { from: id, x1, y1, x2: x1, y2: y1 };
                    }
                }
                _ => {
                    let d = self.model.as_ref().and_then(|m| m.borrow().data().and_then(|d| gantt::start_bar_drag(d, &h, x, y)));
                    if let Some(d) = d {
                        self.drag = Drag::Bar(Box::new(d));
                    }
                }
            }
        }
        self.drag_at = Some((x, y));
        self.invalidate();
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x as f64, e.args().y as f64);
        let (ax, ay) = (e.args().x + self.bounds.left, e.args().y + self.bounds.top);
        if e.args().button != MouseButton::Left && !matches!(self.drag, Drag::None) {
            self.end_drag(x, y);
        }
        match self.drag.clone() {
            Drag::VScroll(grab) => {
                if let Some((bar, rail)) = self.vbar() {
                    let thumb = bar.thumb_rect(rail);
                    let travel = (rail.bottom - rail.top) - (thumb.bottom - thumb.top);
                    if travel > 0.0 {
                        self.scroll_y = (((ay - grab - rail.top).clamp(0.0, travel)) / travel) as f64 * self.max_scroll_y();
                        self.invalidate();
                    }
                }
            }
            Drag::HScroll(grab) => {
                if let Some((bar, rail)) = self.hbar() {
                    let thumb = bar.thumb_rect(rail);
                    let travel = (rail.right - rail.left) - (thumb.right - thumb.left);
                    if travel > 0.0 {
                        self.scroll_x = (((ax - grab - rail.left).clamp(0.0, travel)) / travel) as f64 * self.max_scroll_x();
                        self.invalidate();
                    }
                }
            }
            Drag::None => {
                let vhot = self.vbar().is_some_and(|(_, r)| r.inflate(4.0, 0.0).contains(ax, ay));
                let hhot = self.hbar().is_some_and(|(_, r)| r.inflate(0.0, 4.0).contains(ax, ay));
                let hover_row = if x < self.pane_w() && y >= HEADER_H {
                    let i = ((y + self.scroll_y - HEADER_H) / ROW_H).floor();
                    (i >= 0.0).then_some(i as usize)
                } else {
                    None
                };
                let hover_marker = if x >= self.pane_w() && y < HEADER_H { self.marker_at(x) } else { None };
                if vhot != self.vhot || hhot != self.hhot || hover_row != self.hover_row || hover_marker != self.hover_marker {
                    self.vhot = vhot;
                    self.hhot = hhot;
                    self.hover_row = hover_row;
                    self.hover_marker = hover_marker;
                    self.invalidate();
                }
            }
            _ => {
                self.drag_at = Some((x, y));
                self.drag_to(x, y);
                kubuno_desktop::controls::host::request_repaint_after(16);
            }
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x as f64, e.args().y as f64);
        self.end_drag(x, y);
        e.raise(&*self, "OnMouseUp");
    }

    fn on_mouse_double_click(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x as f64, e.args().y as f64);
        let pane = self.pane_w();
        if (x - pane).abs() <= 3.0 {
            // Hug the columns again.
            if let Some(m) = &self.model {
                m.borrow_mut().view.layout.pane_width = None;
            }
        } else if x < pane {
            match self.table_hit(x, y).0 {
                TableHit::Header { edge: Some(c) } => {
                    if let Some(m) = &self.model {
                        m.borrow_mut().view.layout.reset_width(c);
                    }
                }
                TableHit::Cell(i, Col::Name) => self.begin_edit(i, Col::Name),
                _ => {}
            }
        }
        self.drag = Drag::None;
        self.invalidate();
        e.raise(&*self, "OnMouseDoubleClick");
    }

    fn on_mouse_leave(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        if matches!(self.drag, Drag::None) {
            self.vhot = false;
            self.hhot = false;
            self.hover_row = None;
            self.hover_marker = None;
            self.invalidate();
        }
        e.raise(&*self, "OnMouseLeave");
    }

    fn cursor_at(&self, x: f32, y: f32) -> Option<kubuno_desktop::controls::host::Cursor> {
        use kubuno_desktop::controls::host::Cursor;
        let (x, y) = (x as f64, y as f64);
        let pane = self.pane_w();
        Some(match &self.drag {
            Drag::Column { .. } | Drag::Splitter { .. } | Drag::Marker(_) => Cursor::ResizeEW,
            Drag::Link { .. } => Cursor::Crosshair,
            Drag::Bar(b) if b.active => Cursor::Move,
            Drag::Row { active: true, .. } => Cursor::Move,
            _ if (x - pane).abs() <= 3.0 => Cursor::ResizeEW,
            _ if x < pane => match self.table_hit(x, y).0 {
                TableHit::Header { edge: Some(_) } => Cursor::ResizeEW,
                TableHit::Grip(_) => Cursor::Move,
                TableHit::Cell(_, Col::Name) => Cursor::IBeam,
                TableHit::Chevron(_) | TableHit::Ring(_) | TableHit::AddRow => Cursor::Hand,
                _ => Cursor::Arrow,
            },
            _ if y < HEADER_H => match self.marker_at(x) {
                Some(_) => Cursor::ResizeEW,
                None => Cursor::Arrow,
            },
            _ => match gantt::cursor_for(self.chart_hit(x, y).as_ref().map(|h| &h.mode)) {
                CursorKind::ResizeEw => Cursor::ResizeEW,
                CursorKind::NotAllowed => Cursor::NotAllowed,
                CursorKind::Crosshair => Cursor::Crosshair,
                CursorKind::Grab => Cursor::Move,
                CursorKind::Default => Cursor::Arrow,
            },
        })
    }

    fn tool_tip_at(&self, x: f32, y: f32) -> Option<String> {
        let (x, y) = (x as f64, y as f64);
        let pane = self.pane_w();
        if (x - pane).abs() <= 9.0 && (y - HEADER_H / 2.0).abs() <= 11.0 {
            let compact = self.model.as_ref().is_some_and(|m| m.borrow().view.layout.compact);
            return Some(if compact { Resources::cols_expand() } else { Resources::cols_compact() }.to_string());
        }
        if (x - pane).abs() <= 3.0 {
            return Some(Resources::pane_resize_hint().to_string());
        }
        if x < pane {
            return match self.table_hit(x, y).0 {
                TableHit::Header { edge: Some(_) } => Some(Resources::col_resize_hint().to_string()),
                TableHit::Grip(_) => Some(Resources::reorder_drag().to_string()),
                _ => None,
            };
        }
        None
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let Some(model) = self.model.clone() else { return Vec::new() };
        let m = model.borrow();
        let Some(d) = m.data() else { return Vec::new() };
        let pane = self.pane_w() as f32;
        let w = self.width() as f32;
        let mut out = vec![AccessiblePart { name: Resources::col_task().to_string(), role: AccessRole::ColumnHeader, bounds: Rect::new(0.0, 0.0, pane, HEADER_H as f32) }];
        for (i, r) in m.rows().iter().enumerate() {
            let y = (HEADER_H + i as f64 * ROW_H - self.scroll_y) as f32;
            if y < HEADER_H as f32 - ROW_H as f32 || y > self.height() as f32 {
                continue;
            }
            let t = &d.tasks[r.index];
            out.push(AccessiblePart { name: format!("{} {}", t.wbs, t.name), role: AccessRole::Row, bounds: Rect::new(0.0, y, w, y + ROW_H as f32) });
        }
        out
    }

    fn on_key_down(&mut self, e: &mut EventCx<'_, KeyEventArgs>) {
        use kubuno_desktop::controls::host::vk;
        let (key, mods) = (e.args().key.0, e.args().mods);
        let mut handled = true;
        if let Some(ed) = self.edit.as_mut() {
            match key {
                k if k == vk::ENTER || k == vk::TAB => self.commit_edit(),
                k if k == vk::ESCAPE => self.cancel_edit(),
                k if k == vk::LEFT => ed.le.left(mods.shift),
                k if k == vk::RIGHT => ed.le.right(mods.shift),
                k if k == vk::HOME => ed.le.home(mods.shift),
                k if k == vk::END => ed.le.end(mods.shift),
                k if k == vk::BACK => ed.le.backspace(),
                k if k == vk::DELETE => ed.le.delete(),
                0x41 if mods.ctrl => ed.le.select_all(),
                _ => handled = false,
            }
            self.caret_ms = now();
            self.invalidate();
        } else {
            match key {
                k if k == vk::ESCAPE => handled = self.abort_drag(),
                k if k == vk::UP => self.move_selection(-1),
                k if k == vk::DOWN => self.move_selection(1),
                k if k == vk::F2 => self.rename_selected(),
                _ => handled = false,
            }
        }
        if handled {
            e.args_mut().handled = true;
            e.args_mut().suppress_key_press = true;
        }
        e.raise(&*self, "OnKeyDown");
    }

    fn on_key_press(&mut self, e: &mut EventCx<'_, KeyPressEventArgs>) {
        let c = e.args().key_char;
        if let Some(ed) = self.edit.as_mut() {
            if !c.is_control() {
                ed.le.insert(&c.to_string());
                self.caret_ms = now();
                self.invalidate();
                e.args_mut().handled = true;
            }
        }
        e.raise(&*self, "OnKeyPress");
    }
}
