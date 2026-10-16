//! The chart half of the Gantt: the port of `web/src/project/GanttRenderer.ts` (`render`, `boundary`,
//! `markerTooltip`, `elbow`, `padlock`, `avatarDot`, `check`, `rightLabel`) on the framework's `Graphics`. The
//! geometry (where each bar is, the drag preview) comes from the shared core (`gantt::bars`).

use std::collections::HashMap;

use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Graphics, LineCap, LineJoin, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::Rect;
use kubuno_office_projects_core::date::{self, Date, DateFormat, Lang};
use kubuno_office_projects_core::gantt::{self, Bar, BarShape, Frame, MarkerKind, Preview, BAR_H, HEADER_H, ROW_H};
use kubuno_office_projects_core::model::ProjectData;

use crate::controls::model::{hex, Palette};

const FONT: &str = "Segoe UI";
const SUMMARY: Color = hex(0x666666);
const MARKER: Color = hex(0x7c3aed);

fn font(size: f32, bold: bool) -> Font {
    Font::with_dip(FONT, size, if bold { FontStyle::BOLD } else { FontStyle::REGULAR })
}

fn one_line(align: StringAlignment) -> StringFormat {
    StringFormat { alignment: align, line_alignment: StringAlignment::Center, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP | StringFormatFlags::NO_CLIP }
}

/// Snaps a coordinate to the middle of a device pixel (`crisp`).
fn crisp(v: f32, dpr: f32) -> f32 {
    ((v * dpr).round() + 0.5) / dpr
}

/// What a frame of the chart needs besides the project.
pub struct ChartScene<'a> {
    pub data: &'a ProjectData,
    pub rows: &'a [usize],
    pub frame: Frame,
    pub project_start: Date,
    pub today: Date,
    pub lang: Lang,
    /// The chart's left edge in the control and its width, the scroll (world x at the left edge), the top of
    /// the rows (the header's bottom, minus the vertical scroll).
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub dpr: f32,
    pub preview: Option<(String, Preview)>,
    pub link_preview: Option<(f64, f64, f64, f64)>,
    pub baseline: Option<HashMap<String, (i32, i32)>>,
    pub target_end_day: Option<i32>,
    pub marker_preview: Option<(MarkerKind, f64)>,
    pub hover_marker: Option<MarkerKind>,
    pub hover_text: String,
    pub selected: Option<String>,
}

/// The two boundary flags as hit-testing needs them: `(kind, day, flag x in world, flag width)`.
pub type Flags = Vec<(MarkerKind, i32, f64, f64)>;

impl ChartScene<'_> {
    fn px(&self, day: f64) -> f32 {
        self.left + (self.frame.x_of_day(day) - self.scroll_x) as f32
    }

    /// Paints the chart; returns the boundary flags' rectangles (world x) for hit-testing.
    pub fn paint(&self, g: &Graphics<'_>, th: &Palette) -> Flags {
        let (l, t, w, h) = (self.left, self.top, self.width, self.height);
        let rows_h = self.rows.len() as f32 * ROW_H as f32;
        let body_top = t + HEADER_H as f32 - self.scroll_y as f32;
        let body_bottom = (body_top + rows_h).min(t + h);
        let day_w = self.frame.day_w;
        g.fill_rectangle(Brush::solid(th.surface0), Rect::new(l, t, l + w, t + h));
        let saved = g.save();
        g.intersect_clip(Rect::new(l, t, l + w, t + h));

        let start_day = (self.scroll_x / day_w).floor() as i32 - self.frame.lead_days;
        let end_day = self.frame.draw_right.min(start_day + (w as f64 / day_w).ceil() as i32 + 1);
        let show_days = day_w >= 12.0;
        let step_w = day_w as f32;
        let rows_clip = Rect::new(l, t + HEADER_H as f32, l + w, t + h);

        // Weekends.
        g.with_saved(|g| {
            g.intersect_clip(rows_clip);
            for d in start_day..=end_day {
                let date = self.project_start.add_days(d as i64);
                if date.is_weekend() {
                    g.fill_rectangle(Brush::solid(th.surface2.with_alpha(0.5)), Rect::new(self.px(d as f64), body_top, self.px(d as f64) + step_w, body_top + rows_h));
                }
            }
        });

        // Today band, and its header cell.
        let today = self.today.days_since(self.project_start) as i32;
        if today >= start_day && today <= end_day {
            let tx = self.px(today as f64);
            g.with_saved(|g| {
                g.intersect_clip(rows_clip);
                g.fill_rectangle(Brush::solid(th.primary.with_alpha(0.12)), Rect::new(tx, body_top, tx + step_w, body_top + rows_h));
            });
        }

        // Vertical grid: weeks always, days when zoomed in.
        g.with_saved(|g| {
            g.intersect_clip(rows_clip);
            for d in start_day..=end_day {
                let date = self.project_start.add_days(d as i64);
                let week = date.weekday_mon0() == 0;
                if !show_days && !week {
                    continue;
                }
                let x = crisp(self.px(d as f64), self.dpr);
                let c = if week { th.border } else { th.border.with_alpha(0.5) };
                g.draw_line(&Pen::new(c, 1.0), PointF::new(x, body_top), PointF::new(x, body_top + rows_h));
            }
        });

        // Header: months, then day numbers (or ISO weeks).
        // QUIRK kept: the web tints today's header cell, then paints the header over it - it never shows.
        g.fill_rectangle(Brush::solid(th.surface1), Rect::new(l, t, l + w, t + HEADER_H as f32));
        let pen = Pen::new(th.border, 1.0);
        let hy = crisp(t + HEADER_H as f32, self.dpr);
        g.draw_line(&pen, PointF::new(l, hy), PointF::new(l + w, hy));
        let hm = crisp(t + HEADER_H as f32 / 2.0, self.dpr);
        g.draw_line(&pen, PointF::new(l, hm), PointF::new(l + w, hm));
        let mut cur_month = 0u32;
        let left_fmt = one_line(StringAlignment::Near);
        for d in start_day..=end_day {
            let date = self.project_start.add_days(d as i64);
            if date.month != cur_month {
                cur_month = date.month;
                let x = self.px(d as f64) + 5.0;
                g.draw_string(&date::format(date, DateFormat::MonthYear, self.lang), &font(11.0, true), Brush::solid(th.text_primary), Rect::new(x, t + 6.0, x + 400.0, t + 24.0), &left_fmt);
            }
        }
        let center = one_line(StringAlignment::Center);
        if show_days {
            for d in start_day..=end_day {
                let date = self.project_start.add_days(d as i64);
                let x = self.px(d as f64);
                let c = if date.is_weekend() { th.text_tertiary } else { th.text_secondary };
                g.draw_string(&date.day.to_string(), &font(9.0, false), Brush::solid(c), Rect::new(x, t + 34.0, x + step_w, t + 46.0), &center);
            }
        } else {
            for d in start_day..=end_day {
                let date = self.project_start.add_days(d as i64);
                if date.weekday_mon0() != 0 {
                    continue;
                }
                let x = self.px(d as f64) + step_w * 3.5;
                g.draw_string(&format!("S{}", date.iso_week()), &font(9.0, false), Brush::solid(th.text_secondary), Rect::new(x - 30.0, t + 34.0, x + 30.0, t + 46.0), &center);
            }
        }

        // The rows: dependency elbows under the bars, then the bars.
        let bars = gantt::bars(self.data, self.rows, &self.frame, self.preview.as_ref().map(|(id, p)| (id.as_str(), *p)));
        g.with_saved(|g| {
            g.intersect_clip(rows_clip);
            for (fr, tr, fx, tx) in gantt::connectors(self.data, self.rows, &self.frame) {
                let fy = body_top as f64 + fr as f64 * ROW_H + ROW_H / 2.0;
                let ty = body_top as f64 + tr as f64 * ROW_H + ROW_H / 2.0;
                let off = self.left as f64 - self.scroll_x;
                self.elbow(g, gantt::elbow(fx + off, fy, tx + off, ty), th.text_tertiary);
            }
            for (row, bar) in bars.iter().enumerate() {
                self.bar(g, th, bar, body_top + row as f32 * ROW_H as f32);
            }
            // The elastic link line.
            if let Some((x1, y1, x2, y2)) = self.link_preview {
                let p = Pen::new(th.primary, 2.0).with_dash_pattern(&[2.5, 1.5]);
                g.draw_line(&p, PointF::new(x1 as f32, y1 as f32), PointF::new(x2 as f32, y2 as f32));
                g.fill_ellipse(Brush::solid(th.primary), Rect::new(x2 as f32 - 3.0, y2 as f32 - 3.0, x2 as f32 + 3.0, y2 as f32 + 3.0));
            }
        });

        // The project window: veils outside it, then the two boundary markers.
        let start_marker = match self.marker_preview {
            Some((MarkerKind::Start, d)) => d,
            _ => 0.0,
        };
        let end_marker = match self.marker_preview {
            Some((MarkerKind::End, d)) => Some(d),
            _ => self.target_end_day.map(|d| d as f64),
        };
        let overrun = gantt::overruns(self.data, end_marker);
        // The web's canvas is as tall as its rows: the veils and the rules stop there.
        let rows_bottom = body_bottom.max(t + HEADER_H as f32);
        let sx = self.px(start_marker);
        if sx > l {
            g.fill_rectangle(Brush::solid(MARKER.with_alpha(0.07)), Rect::new(l, t + HEADER_H as f32, sx.min(l + w), rows_bottom));
        }
        if let Some(e) = end_marker {
            let ex = self.px(e);
            if ex < l + w {
                g.fill_rectangle(Brush::solid(MARKER.with_alpha(0.07)), Rect::new(ex.max(l), t + HEADER_H as f32, l + w, rows_bottom));
            }
        }
        let mut flags = Vec::new();
        let label = |d: f64| date::format(self.project_start.add_days(d.round() as i64), DateFormat::DayMonth, self.lang);
        if let Some(f) = self.boundary(g, sx, rows_bottom, MARKER, false, &label(start_marker)) {
            flags.push((MarkerKind::Start, 0, f.0, f.1));
        }
        if let (Some(e), Some(target)) = (end_marker, self.target_end_day) {
            let c = if overrun { th.danger } else { MARKER };
            if let Some(f) = self.boundary(g, self.px(e), rows_bottom, c, true, &label(e)) {
                flags.push((MarkerKind::End, target, f.0, f.1));
            }
        }
        if let Some(which) = self.hover_marker {
            let hx = match which {
                MarkerKind::Start => self.px(start_marker),
                MarkerKind::End => end_marker.map(|e| self.px(e)).unwrap_or(sx),
            };
            self.marker_tooltip(g, hx, &self.hover_text);
        }
        g.restore(saved);
        flags
    }

    fn bar(&self, g: &Graphics<'_>, th: &Palette, bar: &Bar, y: f32) {
        let task = &self.data.tasks[bar.index];
        let bar_y = y + (ROW_H - BAR_H) as f32 / 2.0;
        let bh = BAR_H as f32;
        let to_local = |x: f64| self.left + (x - self.scroll_x) as f32;
        let (w_left, w_right) = (self.left - 200.0, self.left + self.width);
        // The baseline's ghost: a thin strip under the bar (red late, green early, grey on time).
        if let Some((es, dur)) = self.baseline.as_ref().and_then(|m| m.get(&task.id)) {
            let bx = self.px(*es as f64);
            let bw = (*dur as f32 * self.frame.day_w as f32).max(4.0);
            let slip = bar.start_day - *es as f64;
            let c = if slip > 0.0 { th.danger } else if slip < 0.0 { th.success } else { th.text_tertiary };
            g.fill_rounded_rectangle(Brush::solid(c.with_alpha(0.85)), Rect::new(bx, bar_y + bh + 1.0, bx + bw, bar_y + bh + 4.0), 1.5);
        }
        match &bar.shape {
            BarShape::Milestone { cx } => {
                let (cx, cy, s) = (to_local(*cx), y + ROW_H as f32 / 2.0, 7.0);
                if cx + 200.0 < w_left || cx > w_right {
                    return;
                }
                let c = if task.is_critical { th.danger } else { SUMMARY };
                g.fill_polygon(Brush::solid(c), &[PointF::new(cx, cy - s), PointF::new(cx + s, cy), PointF::new(cx, cy + s), PointF::new(cx - s, cy)]);
                self.right_label(g, th, &task.name, cx + s + 8.0, cy);
            }
            BarShape::Summary { x, w } => {
                let (sx, sbw) = (to_local(*x), *w as f32);
                if sx + sbw < w_left || sx > w_right {
                    return;
                }
                let (top, sh, tab, tab_w) = (y + 5.0, 9.0, 6.0, 9.0);
                let b = Brush::solid(SUMMARY);
                g.fill_rounded_rectangle(b.clone(), Rect::new(sx, top, sx + sbw, top + sh), 1.5);
                g.fill_polygon(b.clone(), &[PointF::new(sx, top), PointF::new(sx, top + sh + tab), PointF::new(sx + tab_w, top + sh), PointF::new(sx + tab_w, top)]);
                let e = sx + sbw;
                g.fill_polygon(b, &[PointF::new(e, top), PointF::new(e, top + sh + tab), PointF::new(e - tab_w, top + sh), PointF::new(e - tab_w, top)]);
                self.right_label(g, th, &format!("{}  ·  {}%", task.name, bar.progress), e + 8.0, y + ROW_H as f32 / 2.0);
            }
            BarShape::Task { x, w } => {
                let (x, bw) = (to_local(*x), *w as f32);
                if x + bw < w_left || x > w_right {
                    return;
                }
                let color = if task.is_critical { th.danger } else { th.primary };
                let r = Rect::new(x, bar_y, x + bw, bar_y + bh);
                g.fill_rounded_rectangle(Brush::solid(color), r, 4.0);
                if task.progress > 0 && task.progress < 100 {
                    let pw = bw * task.progress as f32 / 100.0;
                    g.with_saved(|g| {
                        let mut clip = kubuno_desktop::ui::graphics::GraphicsPath::new();
                        clip.add_rounded_rectangle(r, 4.0);
                        g.intersect_clip_path(&clip);
                        g.fill_rectangle(Brush::solid(Color::BLACK.with_alpha(0.22)), Rect::new(x, bar_y, x + pw, bar_y + bh));
                    });
                }
                let cy = bar_y + bh / 2.0;
                let done = task.progress >= 100;
                if task.progress > 0 && bw >= 14.0 && x + 14.0 > self.left {
                    padlock(g, x + 9.0, cy);
                }
                if done && x + bw > self.left {
                    if bw >= 30.0 {
                        padlock(g, x + bw - 9.0, cy);
                    }
                    if bw >= 52.0 {
                        check(g, x + bw / 2.0, cy);
                    } else if (22.0..30.0).contains(&bw) {
                        check(g, x + bw - 9.0, cy);
                    }
                }
                // Assignee avatars, then the name, right of the bar.
                let mut label_x = x + bw + 16.0;
                let people = kubuno_office_projects_core::views::assignees(self.data, task);
                if !people.is_empty() {
                    let mut ax = x + bw + 20.0;
                    for r in people.iter().take(3) {
                        avatar_dot(g, ax, y + ROW_H as f32 / 2.0, &kubuno_office_projects_core::views::initial(&r.name), Color::from_hex(&r.color).unwrap_or(th.primary));
                        ax += 12.0;
                    }
                    label_x = ax + 6.0;
                }
                self.right_label(g, th, &task.name, label_x, y + ROW_H as f32 / 2.0);
                // The link handle.
                let hc = PointF::new(x + bw + 6.0, bar_y + bh / 2.0);
                let hr = Rect::new(hc.x - 3.0, hc.y - 3.0, hc.x + 3.0, hc.y + 3.0);
                g.fill_ellipse(Brush::solid(th.surface0), hr);
                g.draw_ellipse(&Pen::new(color, 1.25), hr);
            }
        }
    }

    fn right_label(&self, g: &Graphics<'_>, th: &Palette, name: &str, x: f32, mid_y: f32) {
        if name.is_empty() || x > self.left + self.width {
            return;
        }
        g.draw_string(name, &font(12.0, false), Brush::solid(th.text_primary), Rect::new(x, mid_y - 9.0, x + 2000.0, mid_y + 9.0), &one_line(StringAlignment::Near));
    }

    fn elbow(&self, g: &Graphics<'_>, pts: Vec<(f64, f64)>, color: Color) {
        let points: Vec<PointF> = pts.iter().map(|(x, y)| PointF::new(*x as f32, *y as f32)).collect();
        g.draw_lines(&Pen::new(color, 1.5), &points);
        if let Some(end) = points.last() {
            g.fill_polygon(Brush::solid(color), &[*end, PointF::new(end.x - 6.0, end.y - 3.5), PointF::new(end.x - 6.0, end.y + 3.5)]);
        }
    }

    /// A boundary: a dashed rule and a dated flag pointing into the window. Returns the flag (world x, width).
    fn boundary(&self, g: &Graphics<'_>, gx: f32, bottom: f32, col: Color, left_side: bool, label: &str) -> Option<(f64, f64)> {
        if gx < self.left - 80.0 || gx > self.left + self.width + 80.0 {
            return None;
        }
        let t = self.top + HEADER_H as f32;
        let x = crisp(gx, self.dpr);
        g.draw_line(&Pen::new(col, 1.5).with_dash_pattern(&[2.0 / 1.5, 3.0 / 1.5]), PointF::new(x, t), PointF::new(x, bottom));
        let f = font(9.0, true);
        let tw = g.measure_string(label, &f, None, &one_line(StringAlignment::Near)).width + 12.0;
        let bx = if left_side { gx - tw } else { gx };
        let r = Rect::new(bx, t - 15.0, bx + tw, t - 1.0);
        g.fill_rounded_rectangle(Brush::solid(col), r, 3.0);
        g.draw_string(label, &f, Brush::solid(Color::WHITE), r, &one_line(StringAlignment::Center));
        Some(((bx - self.left) as f64 + self.scroll_x, tw as f64))
    }

    fn marker_tooltip(&self, g: &Graphics<'_>, gx: f32, text: &str) {
        let f = font(11.0, false);
        let tw = g.measure_string(text, &f, None, &one_line(StringAlignment::Near)).width + 16.0;
        let x = (gx - tw / 2.0).clamp(self.left + 4.0, (self.left + self.width - tw - 4.0).max(self.left + 4.0));
        let y = self.top + HEADER_H as f32 + 4.0;
        let bg = Color::rgba_f(32.0 / 255.0, 33.0 / 255.0, 36.0 / 255.0, 0.94);
        g.fill_rounded_rectangle(Brush::solid(bg), Rect::new(x, y, x + tw, y + 22.0), 5.0);
        let px = gx.clamp(x + 6.0, x + tw - 6.0);
        g.fill_polygon(Brush::solid(bg), &[PointF::new(px, y - 5.0), PointF::new(px - 5.0, y + 1.0), PointF::new(px + 5.0, y + 1.0)]);
        g.draw_string(text, &f, Brush::solid(Color::WHITE), Rect::new(x, y, x + tw, y + 22.0), &one_line(StringAlignment::Center));
    }
}

/// A white padlock: the task has started (its start is fixed), or is done (its finish too).
fn padlock(g: &Graphics<'_>, cx: f32, cy: f32) {
    let (bw, bh) = (8.0, 6.0);
    let (bx, by) = (cx - bw / 2.0, cy - bh / 2.0 + 1.0);
    g.draw_arc(&Pen::new(Color::WHITE, 1.4), Rect::new(cx - 2.4, by - 2.4, cx + 2.4, by + 2.4), 180.0, 180.0);
    g.fill_rounded_rectangle(Brush::solid(Color::WHITE), Rect::new(bx, by, bx + bw, by + bh), 1.5);
    g.fill_ellipse(Brush::solid(Color::BLACK.with_alpha(0.4)), Rect::new(cx - 0.95, by + bh / 2.0 - 0.95, cx + 0.95, by + bh / 2.0 + 0.95));
}

/// A white check mark: completed.
fn check(g: &Graphics<'_>, cx: f32, cy: f32) {
    let pen = Pen::new(Color::WHITE, 1.8).with_caps(LineCap::Round).with_join(LineJoin::Round);
    g.draw_lines(&pen, &[PointF::new(cx - 3.5, cy + 0.3), PointF::new(cx - 1.0, cy + 2.8), PointF::new(cx + 3.6, cy - 2.8)]);
}

/// An assignee: an initial on a coloured dot ringed in white.
pub fn avatar_dot(g: &Graphics<'_>, cx: f32, cy: f32, label: &str, color: Color) {
    g.fill_ellipse(Brush::solid(Color::WHITE), Rect::new(cx - 8.0, cy - 8.0, cx + 8.0, cy + 8.0));
    g.fill_ellipse(Brush::solid(color), Rect::new(cx - 6.75, cy - 6.75, cx + 6.75, cy + 6.75));
    g.draw_string(label, &font(8.0, true), Brush::solid(Color::WHITE), Rect::new(cx - 8.0, cy - 7.5, cx + 8.0, cy + 8.5), &one_line(StringAlignment::Center));
}
