//! The table half of the Gantt: the header (`ganttHeaderCells`) and one row per visible task (`TaskRow.tsx`),
//! with the « Ajouter une tâche » row after the last one, and where a pointer lands in it.

use std::collections::HashMap;

use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Graphics, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::Rect;
use kubuno_office_projects_common::model::columns::{Col, Layout};
use kubuno_office_projects_common::model::line_edit::LineEdit;
use kubuno_office_projects_core::date::{self, Date, DateFormat, Lang};
use kubuno_office_projects_core::gantt::{HEADER_H, ROW_H};
use kubuno_office_projects_core::model::ProjectData;
use kubuno_office_projects_core::view::{self, Row};
use kubuno_office_projects_core::views::{assignees, initial, priority_color};

use crate::controls::model::{hex, Palette};

const FONT: &str = "Segoe UI";
/// The « Ajouter une tâche » row (`py-2` + `text-xs`).
pub const ADD_ROW_H: f64 = 32.0;
/// The drag handle's width at the row's left (`w-4`).
pub const GRIP_W: f64 = 16.0;

fn font(size: f32, bold: bool) -> Font {
    Font::with_dip(FONT, size, if bold { FontStyle::BOLD } else { FontStyle::REGULAR })
}

fn fmt(align: StringAlignment) -> StringFormat {
    StringFormat { alignment: align, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP }
}

/// The header labels, in the UI language (`ganttHeaderCells`).
pub struct Labels {
    pub task: String,
    pub duration: String,
    pub priority: String,
    pub start: String,
    pub end: String,
    pub variance: String,
    pub predecessors: String,
    pub resources: String,
    pub add_task: String,
    /// « {n}j »
    pub days_short: String,
    pub priorities: [String; 4],
}

/// What a point of the table is.
#[derive(Debug, Clone, PartialEq)]
pub enum TableHit {
    /// The header; `Some(col)` when on a column's right edge (resize grip).
    Header { edge: Option<Col> },
    /// A row's chevron (fold / unfold).
    Chevron(usize),
    /// A row's completion ring.
    Ring(usize),
    /// The drag handle at a row's left.
    Grip(usize),
    /// A cell of a row.
    Cell(usize, Col),
    /// « Ajouter une tâche ».
    AddRow,
    Nothing,
}

/// The cell being edited in place.
#[derive(Debug, Clone, PartialEq)]
pub struct CellEdit {
    pub task: String,
    pub col: Col,
    pub le: LineEdit,
}

/// A frame of the table.
pub struct TableScene<'a> {
    pub data: &'a ProjectData,
    pub rows: &'a [Row],
    pub layout: &'a Layout,
    pub has_baseline: bool,
    pub baseline: Option<&'a HashMap<String, (i32, i32)>>,
    pub labels: &'a Labels,
    pub lang: Lang,
    pub project_start: Date,
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub selected: Option<&'a str>,
    pub hover_row: Option<usize>,
    pub edit: Option<&'a CellEdit>,
    pub caret_on: bool,
    pub progress: &'a HashMap<String, i32>,
    /// Row being dragged, and where it would land (`before`/`after`/`inside`).
    pub drag: Option<(usize, Option<(usize, u8)>)>,
    pub collapsed: &'a std::collections::HashSet<String>,
}

impl TableScene<'_> {
    /// The visible columns with their x (control-local) and width.
    pub fn columns(&self) -> Vec<(Col, f32, f32)> {
        let mut x = self.left - self.scroll_x as f32;
        self.layout
            .visible(self.has_baseline)
            .into_iter()
            .map(|c| {
                let w = self.layout.width(c) as f32;
                let r = (c, x, w);
                x += w;
                r
            })
            .collect()
    }

    fn row_top(&self, i: usize) -> f32 {
        self.top + HEADER_H as f32 + i as f32 * ROW_H as f32 - self.scroll_y as f32
    }

    pub fn hit(&self, x: f32, y: f32) -> TableHit {
        if x < self.left || x >= self.left + self.width {
            return TableHit::Nothing;
        }
        if y < self.top + HEADER_H as f32 {
            let edge = self.columns().into_iter().find(|(c, cx, w)| c.resizable() && (x - (cx + w)).abs() <= 4.0).map(|(c, _, _)| c);
            return TableHit::Header { edge };
        }
        let rel = y - self.top - HEADER_H as f32 + self.scroll_y as f32;
        if rel < 0.0 {
            return TableHit::Nothing;
        }
        let i = (rel / ROW_H as f32) as usize;
        if i >= self.rows.len() {
            return if rel < (self.rows.len() as f64 * ROW_H + ADD_ROW_H) as f32 { TableHit::AddRow } else { TableHit::Nothing };
        }
        if (x - self.left) < GRIP_W as f32 && self.hover_row == Some(i) {
            return TableHit::Grip(i);
        }
        for (c, cx, w) in self.columns() {
            if x >= cx && x < cx + w {
                if c == Col::Name {
                    let row = &self.rows[i];
                    let indent = cx + row.depth as f32 * 12.0 + 4.0;
                    if row.has_children && x >= indent && x < indent + 14.0 {
                        return TableHit::Chevron(i);
                    }
                    let t = &self.data.tasks[row.index];
                    let leaf_task = !(t.is_summary_type() || row.has_children || t.is_milestone());
                    if leaf_task && x >= indent + 12.0 && x < indent + 28.0 {
                        return TableHit::Ring(i);
                    }
                }
                return TableHit::Cell(i, c);
            }
        }
        TableHit::Nothing
    }

    /// The rectangle of a cell (for the in-place editor).
    pub fn cell_rect(&self, i: usize, col: Col) -> Option<Rect> {
        let (_, x, w) = self.columns().into_iter().find(|(c, _, _)| *c == col)?;
        let y = self.row_top(i);
        Some(Rect::new(x, y, x + w, y + ROW_H as f32))
    }

    pub fn paint(&self, g: &Graphics<'_>, th: &Palette) {
        let (l, t, w, h) = (self.left, self.top, self.width, self.height);
        g.fill_rectangle(Brush::solid(th.surface0), Rect::new(l, t, l + w, t + h));
        let saved = g.save();
        g.intersect_clip(Rect::new(l, t, l + w, t + h));
        let cols = self.columns();
        let table_w = cols.iter().map(|(_, _, w)| *w).sum::<f32>();
        // Rows (under the header).
        g.with_saved(|g| {
            g.intersect_clip(Rect::new(l, t + HEADER_H as f32, l + w, t + h));
            for (i, row) in self.rows.iter().enumerate() {
                let y = self.row_top(i);
                if y + (ROW_H as f32) < t + HEADER_H as f32 || y > t + h {
                    continue;
                }
                self.row(g, th, &cols, i, row, y, table_w.max(w));
            }
            // « + Ajouter une tâche ».
            let y = self.row_top(self.rows.len());
            let r = Rect::new(l - self.scroll_x as f32, y, l - self.scroll_x as f32 + table_w.max(w), y + ADD_ROW_H as f32);
            if self.hover_row == Some(self.rows.len()) {
                g.fill_rectangle(Brush::solid(th.surface1), r);
            }
            let ink = if self.hover_row == Some(self.rows.len()) { th.primary } else { th.text_tertiary };
            g.draw_icon("Plus", Rect::new(r.left + 16.0, y, r.left + 28.0, y + ADD_ROW_H as f32), 12.0, ink);
            g.draw_string(&self.labels.add_task, &font(12.0, false), Brush::solid(ink), Rect::new(r.left + 34.0, y, r.right, y + ADD_ROW_H as f32), &fmt(StringAlignment::Near));
            g.draw_line(&Pen::new(th.row_line, 1.0), PointF::new(r.left, r.bottom - 0.5), PointF::new(r.right, r.bottom - 0.5));
        });
        // Header.
        let hb = t + HEADER_H as f32;
        g.fill_rectangle(Brush::solid(th.surface1), Rect::new(l, t, l + w, hb));
        g.draw_line(&Pen::new(th.border, 1.0), PointF::new(l, hb - 0.5), PointF::new(l + w, hb - 0.5));
        let last = cols.len().saturating_sub(1);
        for (k, (c, x, cw)) in cols.iter().enumerate() {
            let r = Rect::new(*x, t, x + cw, hb - 1.0);
            let (label, align, pad) = match c {
                Col::Idx => ("#".to_string(), StringAlignment::Center, 0.0),
                Col::Mode => {
                    g.draw_icon("GanttChartSquare", r, 13.0, th.text_secondary);
                    (String::new(), StringAlignment::Center, 0.0)
                }
                Col::Name => (self.labels.task.clone(), StringAlignment::Near, 6.0),
                Col::Dur => (self.labels.duration.clone(), StringAlignment::Far, 6.0),
                Col::Progress => ("%".to_string(), StringAlignment::Far, 6.0),
                Col::Priority => (self.labels.priority.clone(), StringAlignment::Near, 6.0),
                Col::Start => (self.labels.start.clone(), StringAlignment::Far, 6.0),
                Col::End => (self.labels.end.clone(), StringAlignment::Far, 6.0),
                Col::Variance => (self.labels.variance.clone(), StringAlignment::Near, 6.0),
                Col::Pred => (self.labels.predecessors.clone(), StringAlignment::Near, 6.0),
                Col::Res => (self.labels.resources.clone(), StringAlignment::Near, 6.0),
            };
            if !label.is_empty() {
                g.draw_string(&label, &font(11.0, true), Brush::solid(th.text_secondary), Rect::new(r.left + pad, r.top, r.right - pad, r.bottom), &fmt(align));
            }
            if k < last {
                g.draw_line(&Pen::new(th.border, 1.0), PointF::new(x + cw - 0.5, t), PointF::new(x + cw - 0.5, hb - 1.0));
            }
        }
        g.restore(saved);
    }

    #[allow(clippy::too_many_arguments)]
    fn row(&self, g: &Graphics<'_>, th: &Palette, cols: &[(Col, f32, f32)], i: usize, row: &Row, y: f32, row_w: f32) {
        let task = &self.data.tasks[row.index];
        let x0 = self.left - self.scroll_x as f32;
        let r = Rect::new(x0, y, x0 + row_w, y + ROW_H as f32);
        let selected = self.selected == Some(task.id.as_str());
        let dragging = self.drag.is_some_and(|(d, _)| d == i);
        let bg = if dragging || selected {
            Some(th.primary.with_alpha(0.05))
        } else if task.is_summary_type() {
            Some(if self.hover_row == Some(i) { th.surface2 } else { th.surface1 })
        } else if self.hover_row == Some(i) {
            Some(th.surface1)
        } else {
            None
        };
        if let Some(c) = bg {
            g.fill_rectangle(Brush::solid(c), r);
        }
        let parent = task.is_summary_type() || row.has_children;
        for (c, cx, cw) in cols {
            let cell = Rect::new(*cx, y, cx + cw, y + ROW_H as f32);
            let inner = Rect::new(cx + 6.0, y, cx + cw - 6.0, y + ROW_H as f32);
            let editing = self.edit.filter(|e| e.task == task.id && e.col == *c);
            if let Some(e) = editing {
                self.editor(g, th, inner, e, *c);
            } else {
                match c {
                    Col::Idx => {
                        g.draw_string(&(row.index + 1).to_string(), &font(12.0, false), Brush::solid(th.text_tertiary), cell, &fmt(StringAlignment::Center));
                    }
                    Col::Mode => g.draw_icon("GanttChartSquare", cell, 12.0, th.text_tertiary),
                    Col::Name => self.name_cell(g, th, *cx, *cw, y, row, parent),
                    Col::Dur => {
                        let text = if parent { self.labels.days_short.replace("{n}", &task.duration_days.to_string()) } else { task.duration_days.to_string() };
                        let ink = if parent { th.text_tertiary } else { th.text_primary };
                        g.draw_string(&text, &font(12.0, false), Brush::solid(ink), inner, &fmt(StringAlignment::Far));
                    }
                    Col::Progress => {
                        let pct = if parent { self.progress.get(&task.id).copied().unwrap_or(task.progress) } else { task.progress };
                        let (pbg, ink) = if pct >= 100 {
                            (th.success.with_alpha(0.15), th.success)
                        } else if pct > 0 {
                            (th.primary.with_alpha(0.10), th.primary)
                        } else {
                            (th.surface2, th.text_tertiary)
                        };
                        let f = font(11.0, true);
                        let text = format!("{pct}%");
                        let tw = g.measure_string(&text, &f, None, &fmt(StringAlignment::Near)).width + 12.0;
                        let tw = if parent { tw } else { tw.max(38.0) };
                        let pill = Rect::new(inner.right - tw, y + 5.0, inner.right, y + ROW_H as f32 - 5.0);
                        g.fill_rounded_rectangle(Brush::solid(pbg), pill, 4.0);
                        g.draw_string(&text, &f, Brush::solid(ink), Rect::new(pill.left, pill.top, pill.right - 6.0, pill.bottom), &fmt(StringAlignment::Far));
                    }
                    Col::Priority => {
                        let dot = PointF::new(inner.left + 4.0, y + ROW_H as f32 / 2.0);
                        g.fill_ellipse(Brush::solid(Color::from_hex(priority_color(&task.priority)).unwrap_or(hex(0x9aa0a6))), Rect::new(dot.x - 4.0, dot.y - 4.0, dot.x + 4.0, dot.y + 4.0));
                        let label = match task.priority.as_str() {
                            "low" => &self.labels.priorities[0],
                            "high" => &self.labels.priorities[2],
                            "critical" => &self.labels.priorities[3],
                            _ => &self.labels.priorities[1],
                        };
                        g.draw_string(label, &font(12.0, false), Brush::solid(th.text_primary), Rect::new(inner.left + 14.0, y, inner.right - 14.0, y + ROW_H as f32), &fmt(StringAlignment::Near));
                        g.draw_icon("ChevronDown", Rect::new(inner.right - 14.0, y, inner.right, y + ROW_H as f32), 12.0, th.text_tertiary);
                    }
                    Col::Start | Col::End => {
                        let d = if *c == Col::Start { view::sched_start(task, self.project_start) } else { view::sched_end(task, self.project_start) };
                        g.draw_string(&date::format(d, DateFormat::DayMonthYear2, self.lang), &font(12.0, false), Brush::solid(th.text_secondary), inner, &fmt(StringAlignment::Far));
                    }
                    Col::Variance => {
                        let (text, ink, bold) = match self.baseline.and_then(|b| b.get(&task.id)) {
                            Some((es, _)) => {
                                let slip = task.es() - es;
                                match slip.cmp(&0) {
                                    std::cmp::Ordering::Equal => ("0 j".to_string(), th.text_tertiary, false),
                                    std::cmp::Ordering::Greater => (format!("+{slip} j"), th.danger, true),
                                    std::cmp::Ordering::Less => (format!("−{} j", -slip), th.success, true),
                                }
                            }
                            None => ("—".to_string(), th.text_tertiary, false),
                        };
                        g.draw_string(&text, &font(12.0, bold), Brush::solid(ink), inner, &fmt(StringAlignment::Near));
                    }
                    Col::Pred => {
                        let text = view::predecessor_text(self.data, &task.id);
                        let (text, ink) = if text.is_empty() { ("—".to_string(), th.text_tertiary) } else { (text, th.text_secondary) };
                        g.draw_string(&text, &font(12.0, false), Brush::solid(ink), inner, &fmt(StringAlignment::Near));
                    }
                    Col::Res => {
                        let people = assignees(self.data, task);
                        if people.is_empty() {
                            let cy = y + ROW_H as f32 / 2.0;
                            let circle = Rect::new(inner.left, cy - 10.0, inner.left + 20.0, cy + 10.0);
                            g.draw_ellipse(&Pen::new(th.border, 1.0).with_dash_pattern(&[3.0, 2.0]), circle);
                            g.draw_icon("UserRound", circle, 11.0, th.text_tertiary.with_alpha(0.5));
                        } else {
                            avatar_stack(g, th, &people, inner.left, y + ROW_H as f32 / 2.0, 20.0);
                        }
                    }
                }
            }
            g.draw_line(&Pen::new(th.row_line, 1.0), PointF::new(cx + cw - 0.5, y), PointF::new(cx + cw - 0.5, y + ROW_H as f32));
        }
        g.draw_line(&Pen::new(th.row_line, 1.0), PointF::new(r.left, r.bottom - 0.5), PointF::new(r.right, r.bottom - 0.5));
        if dragging {
            g.draw_rectangle(&Pen::new(th.primary, 1.0), Rect::new(r.left + 0.5, r.top + 0.5, r.right - 0.5, r.bottom - 0.5));
        }
        // The drop hint: a 2-px line before/after, a 2-px ring inside.
        if let Some((_, Some((target, place)))) = self.drag {
            if target == i {
                match place {
                    0 => g.fill_rectangle(Brush::solid(th.primary), Rect::new(r.left, r.top - 1.0, r.right, r.top + 1.0)),
                    1 => g.fill_rectangle(Brush::solid(th.primary), Rect::new(r.left, r.bottom - 1.0, r.right, r.bottom + 1.0)),
                    _ => {
                        g.fill_rectangle(Brush::solid(th.primary.with_alpha(0.05)), r);
                        g.draw_rectangle(&Pen::new(th.primary, 2.0), Rect::new(r.left + 1.0, r.top + 1.0, r.right - 1.0, r.bottom - 1.0));
                    }
                }
            }
        }
        // The drag handle, on hover.
        if self.hover_row == Some(i) && self.drag.is_none() {
            g.draw_icon("GripVertical", Rect::new(x0, y, x0 + GRIP_W as f32, y + ROW_H as f32), 13.0, th.text_tertiary);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn name_cell(&self, g: &Graphics<'_>, th: &Palette, cx: f32, cw: f32, y: f32, row: &Row, parent: bool) {
        let task = &self.data.tasks[row.index];
        let mid = y + ROW_H as f32 / 2.0;
        let mut x = cx + row.depth as f32 * 12.0 + 4.0;
        if row.has_children {
            let icon = if self.collapsed.contains(&task.id) { "ChevronRight" } else { "ChevronDown" };
            g.draw_icon(icon, Rect::new(x, mid - 6.0, x + 12.0, mid + 6.0), 12.0, th.text_tertiary);
            x += 14.0;
        } else {
            x += 12.0;
        }
        if parent {
            g.draw_icon("FolderKanban", Rect::new(x, mid - 6.0, x + 11.0, mid + 5.0), 11.0, th.text_tertiary);
            x += 15.0;
        } else if task.is_milestone() {
            g.draw_icon("Milestone", Rect::new(x, mid - 6.0, x + 11.0, mid + 5.0), 11.0, hex(0xf97316));
            x += 15.0;
        } else {
            let ring = Rect::new(x, mid - 7.5, x + 15.0, mid + 7.5);
            if task.progress >= 100 {
                g.draw_icon("CheckCircle2", ring, 14.0, th.success);
            } else {
                g.draw_icon("Circle", ring, 14.0, if task.progress > 0 { th.primary } else { th.text_tertiary });
            }
            x += 19.0;
        }
        // Folded-in details while their columns are hidden: the priority dot, the assignees.
        let mut right = cx + cw - 6.0;
        if !self.layout.shown(Col::Res, self.has_baseline) {
            let people = assignees(self.data, task);
            if !people.is_empty() {
                let n = people.len().min(4) + usize::from(people.len() > 4);
                let wstack = 16.0 + (n as f32 - 1.0) * 12.0;
                avatar_stack(g, th, &people, right - wstack, mid, 16.0);
                right -= wstack + 6.0;
            }
        }
        if !self.layout.shown(Col::Priority, self.has_baseline) && task.priority != "medium" {
            let c = Color::from_hex(priority_color(&task.priority)).unwrap_or(hex(0x9aa0a6));
            g.fill_ellipse(Brush::solid(c), Rect::new(right - 8.0, mid - 4.0, right, mid + 4.0));
            right -= 14.0;
        }
        let bold = task.is_summary_type();
        g.draw_string(&task.name, &font(12.0, bold), Brush::solid(th.text_primary), Rect::new(x, y, right.max(x), y + ROW_H as f32), &fmt(StringAlignment::Near));
    }

    fn editor(&self, g: &Graphics<'_>, th: &Palette, inner: Rect, e: &CellEdit, col: Col) {
        let f = font(12.0, false);
        let field = Rect::new(inner.left - 2.0, inner.top + 3.0, inner.right + 2.0, inner.bottom - 3.0);
        g.fill_rounded_rectangle(Brush::solid(th.surface0), field, 3.0);
        let measure = |s: &str| g.measure_string(s, &f, None, &StringFormat { flags: StringFormatFlags::NO_WRAP | StringFormatFlags::MEASURE_TRAILING_SPACES, ..fmt(StringAlignment::Near) }).width;
        if col == Col::Name {
            g.draw_line(&Pen::new(th.primary, 1.0), PointF::new(field.left, field.bottom - 0.5), PointF::new(field.right, field.bottom - 0.5));
        } else {
            g.draw_rounded_rectangle(&Pen::new(th.primary, 1.0), field, 3.0);
        }
        let right_aligned = matches!(col, Col::Dur | Col::Progress);
        let text = e.le.text();
        let tw = measure(&text);
        let x0 = if right_aligned { inner.right - tw } else { inner.left };
        let (s0, s1) = e.le.selection();
        if s1 > s0 {
            let (a, b) = (measure(&e.le.prefix(s0)), measure(&e.le.prefix(s1)));
            g.fill_rectangle(Brush::solid(th.primary.with_alpha(0.3)), Rect::new(x0 + a, field.top + 2.0, x0 + b, field.bottom - 2.0));
        }
        g.draw_string(&text, &f, Brush::solid(th.text_primary), Rect::new(x0, inner.top, x0 + tw + 4.0, inner.bottom), &StringFormat { trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP | StringFormatFlags::NO_CLIP, ..fmt(StringAlignment::Near) });
        if self.caret_on {
            let cxp = x0 + measure(&e.le.prefix(e.le.caret));
            g.draw_line(&Pen::new(th.text_primary, 1.0), PointF::new(cxp, field.top + 3.0), PointF::new(cxp, field.bottom - 3.0));
        }
    }
}

/// Up to four overlapping avatars (+N beyond), `size` px each, starting at `x`.
fn avatar_stack(g: &Graphics<'_>, th: &Palette, people: &[&kubuno_office_projects_core::model::Resource], x: f32, mid: f32, size: f32) {
    let step = if size <= 16.0 { size - 4.0 } else { size - 6.0 };
    let f = font(if size <= 16.0 { 8.0 } else { 9.0 }, true);
    let center = fmt(StringAlignment::Center);
    let mut cx = x;
    for r in people.iter().take(4) {
        let rect = Rect::new(cx, mid - size / 2.0, cx + size, mid + size / 2.0);
        g.fill_ellipse(Brush::solid(th.surface0), Rect::new(rect.left - 1.0, rect.top - 1.0, rect.right + 1.0, rect.bottom + 1.0));
        g.fill_ellipse(Brush::solid(Color::from_hex(&r.color).unwrap_or(th.primary)), rect);
        g.draw_string(&initial(&r.name), &f, Brush::solid(Color::WHITE), rect, &center);
        cx += step;
    }
    if people.len() > 4 {
        let rect = Rect::new(cx, mid - size / 2.0, cx + size, mid + size / 2.0);
        g.fill_ellipse(Brush::solid(th.surface2), rect);
        g.draw_string(&format!("+{}", people.len() - 4), &f, Brush::solid(th.text_secondary), rect, &center);
    }
}
