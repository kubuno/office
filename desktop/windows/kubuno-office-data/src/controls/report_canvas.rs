//! `ReportCanvas` — the report page and its widgets, painted at parity with the web Data editor
//! (`DataReportEditor.tsx` canvas + `data/visuals.tsx`). Increment A is read-only: the page scaled to fit,
//! each widget as a card (title + body) drawn from the shared engine's shaping, formatting and palettes.
//! A visual with no dedicated painter yet shows its title and type (so all 38 render).

use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Graphics, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Control, ControlCore, PaintEventCx};

use kubuno_office_data_core::format::{self, FormatOpts, NumberFormat};
use kubuno_office_data_core::model::Widget;
use kubuno_office_data_core::query::QueryResult;
use kubuno_office_data_core::{shape, visuals};
use serde_json::{json, Value};

use crate::controls::model::{chart_colors, parse_hex, Palette, Shared};

#[derive(kubuno_desktop::views::component::Component, Default)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Data")]
#[toolbox(icon = "layout-dashboard")]
pub struct ReportCanvas {
    base: ControlCore,
    model: Option<Shared>,
    bounds: Rect,
}

impl ReportCanvas {
    pub fn set_model(&mut self, model: Shared) {
        self.model = Some(model);
        self.invalidate();
    }

    fn near() -> StringFormat {
        StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP }
    }
    fn center() -> StringFormat {
        StringFormat { alignment: StringAlignment::Center, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP }
    }
}

/// The report theme colours, resolved from the open report's `theme`.
struct Theme {
    page_bg: Color,
    card_bg: Color,
    fg: Color,
    colors: Vec<Color>,
}

fn theme_of(report_theme: &Value, _th: &Palette) -> Theme {
    // The report theme is the document's (not the OS'): resolve it from `themeId` via the shared engine, so a
    // "default" report stays light even when the app is dark — exactly as the web does. Inline colours, when
    // present, win.
    let theme_id = report_theme.get("themeId").and_then(Value::as_str);
    let base = kubuno_office_data_core::palettes::theme_by_id(theme_id);
    let palette_id = report_theme.get("paletteId").and_then(Value::as_str).unwrap_or(base.palette_id);
    let color = |key: &str, fallback: &str| {
        report_theme
            .get(key)
            .and_then(Value::as_str)
            .map(|s| parse_hex(s, parse_hex(fallback, Color::WHITE)))
            .unwrap_or_else(|| parse_hex(fallback, Color::WHITE))
    };
    Theme {
        page_bg: color("pageBackground", base.page_background),
        card_bg: color("cardBackground", base.card_background),
        fg: color("foreground", base.foreground),
        colors: chart_colors(palette_id),
    }
}

fn metric_total(result: &QueryResult, column: &str, alias: &str) -> f64 {
    result.rows.iter().map(|r| r.get(alias).or_else(|| r.get(column)).map(format::to_num).unwrap_or(0.0)).sum()
}

fn first_metric(widget: &Widget) -> (String, String, NumberFormat) {
    let metrics = widget.config.get("metrics").and_then(Value::as_array);
    let (col, alias) = metrics
        .and_then(|a| a.first())
        .map(|m| {
            let col = m.get("column").and_then(Value::as_str).unwrap_or("").to_string();
            let alias = m.get("alias").and_then(Value::as_str).unwrap_or(&col).to_string();
            (col, alias)
        })
        .unwrap_or_default();
    let fmt = widget.config.get("format").and_then(Value::as_str).map(NumberFormat::from_token).unwrap_or(NumberFormat::Auto);
    (col, alias, fmt)
}

impl ReportCanvas {
    fn paint_widget(&self, g: &Graphics<'_>, th: &Palette, theme: &Theme, r: Rect, widget: &Widget, data: Option<&QueryResult>) {
        // Card frame.
        g.fill_rounded_rectangle(Brush::solid(theme.card_bg), r, 8.0);
        g.draw_rounded_rectangle(&Pen::new(th.border, 1.0), r, 8.0);
        let pad = 10.0;
        let mut body = Rect::new(r.left + pad, r.top + pad, r.right - pad, r.bottom - pad);
        // Title (config.title), when shown.
        if let Some(title) = widget.title() {
            if widget.widget_type != "text" {
                let tr = Rect::new(body.left, body.top, body.right, body.top + 20.0);
                g.draw_string(&title, &Font::with_dip("Segoe UI", 12.0, FontStyle::BOLD), Brush::solid(theme.fg), tr, &Self::near());
                body = Rect::new(body.left, body.top + 26.0, body.right, body.bottom);
            }
        }
        match widget.widget_type.as_str() {
            "text" => self.paint_text(g, theme, body, widget),
            "card" | "kpi_card" | "scorecard" | "gauge" | "bullet" | "progress_ring" => self.paint_card(g, th, theme, body, widget, data),
            "bar_chart" | "column_h" | "stacked_bar" | "stacked_bar_100" => self.paint_bars(g, th, theme, body, widget, data),
            "line_chart" | "smooth_line" | "step_line" | "area_chart" | "stacked_area" | "sparkline" => self.paint_lines(g, th, theme, body, widget, data),
            "pie_chart" | "donut_chart" => self.paint_pie(g, th, theme, body, widget, data),
            "data_table" | "matrix" => self.paint_table(g, th, theme, body, widget, data),
            other => self.paint_placeholder(g, th, body, other),
        }
    }

    fn paint_text(&self, g: &Graphics<'_>, theme: &Theme, body: Rect, widget: &Widget) {
        let content = widget.config.get("content").and_then(Value::as_str).unwrap_or("");
        let size = widget.config.get("fontSize").and_then(Value::as_f64).unwrap_or(14.0) as f32;
        let bold = widget.config.get("bold").and_then(Value::as_bool).unwrap_or(false);
        let color = widget.config.get("color").and_then(Value::as_str).map(|s| parse_hex(s, theme.fg)).unwrap_or(theme.fg);
        let style = if bold { FontStyle::BOLD } else { FontStyle::REGULAR };
        g.draw_string(content, &Font::with_dip("Segoe UI", size, style), Brush::solid(color), body, &Self::near());
    }

    fn paint_card(&self, g: &Graphics<'_>, th: &Palette, theme: &Theme, body: Rect, widget: &Widget, data: Option<&QueryResult>) {
        let (col, alias, fmt) = first_metric(widget);
        let value = data.map(|d| metric_total(d, &col, &alias)).unwrap_or(0.0);
        let text = format::format_value(&json!(value), fmt, &FormatOpts::default());
        let accent = theme.colors.first().copied().unwrap_or(th.primary);
        g.draw_string(&text, &Font::with_dip("Segoe UI", 28.0, FontStyle::BOLD), Brush::solid(accent), body, &Self::center());
    }

    fn paint_placeholder(&self, g: &Graphics<'_>, th: &Palette, body: Rect, visual_type: &str) {
        let label = visuals::visual_def(visual_type).map(|d| d.label).unwrap_or(visual_type);
        g.draw_string(label, &Font::with_dip("Segoe UI", 12.0, FontStyle::REGULAR), Brush::solid(th.text_tertiary), body, &Self::center());
    }

    /// Shape the executed rows into categories + one series (first metric), pivoted by legend if set.
    fn series(&self, widget: &Widget, data: Option<&QueryResult>) -> (Vec<String>, Vec<Vec<f64>>) {
        let Some(d) = data else { return (Vec::new(), Vec::new()) };
        let dims: Vec<String> = widget.config.get("dimensions").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let (_, alias, _) = first_metric(widget);
        let axis = dims.first().cloned().unwrap_or_default();
        let legend = dims.get(1).cloned();
        let shaped = shape::shape_series(&d.rows, &axis, &[alias], legend.as_deref());
        (shaped.categories, shaped.series.into_iter().map(|s| s.values).collect())
    }

    fn paint_bars(&self, g: &Graphics<'_>, th: &Palette, theme: &Theme, body: Rect, widget: &Widget, data: Option<&QueryResult>) {
        let (cats, series) = self.series(widget, data);
        let Some(values) = series.first() else { return self.paint_placeholder(g, th, body, "bar_chart") };
        if values.is_empty() {
            return;
        }
        let max = values.iter().cloned().fold(0.0_f64, f64::max).max(1.0);
        let n = values.len() as f32;
        let gap = 8.0;
        let bw = ((body.right - body.left) / n - gap).max(2.0);
        let f9 = Font::with_dip("Segoe UI", 9.0, FontStyle::REGULAR);
        for (i, &v) in values.iter().enumerate() {
            let x = body.left + i as f32 * (bw + gap);
            let h = ((v / max) as f32 * (body.bottom - body.top - 16.0)).max(0.0);
            let r = Rect::new(x, body.bottom - 16.0 - h, x + bw, body.bottom - 16.0);
            g.fill_rounded_rectangle(Brush::solid(theme.colors[i % theme.colors.len()]), r, 3.0);
            if let Some(c) = cats.get(i) {
                g.draw_string(c, &f9, Brush::solid(th.text_tertiary), Rect::new(x, body.bottom - 15.0, x + bw, body.bottom), &Self::center());
            }
        }
    }

    fn paint_lines(&self, g: &Graphics<'_>, th: &Palette, theme: &Theme, body: Rect, widget: &Widget, data: Option<&QueryResult>) {
        let (_cats, series) = self.series(widget, data);
        let Some(values) = series.first() else { return self.paint_placeholder(g, th, body, "line_chart") };
        if values.len() < 2 {
            return;
        }
        let max = values.iter().cloned().fold(0.0_f64, f64::max).max(1.0);
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min).min(0.0);
        let span = (max - min).max(1.0);
        let w = body.right - body.left;
        let h = body.bottom - body.top;
        let color = theme.colors.first().copied().unwrap_or(th.primary);
        let pt = |i: usize, v: f64| PointF::new(body.left + (i as f32 / (values.len() - 1) as f32) * w, body.bottom - (((v - min) / span) as f32 * h));
        for i in 0..values.len() - 1 {
            g.draw_line(&Pen::new(color, 2.0), pt(i, values[i]), pt(i + 1, values[i + 1]));
        }
    }

    fn paint_pie(&self, g: &Graphics<'_>, th: &Palette, theme: &Theme, body: Rect, widget: &Widget, data: Option<&QueryResult>) {
        // A simple proportional column of swatches (true arc fill lands in increment B).
        let (cats, series) = self.series(widget, data);
        let Some(values) = series.first() else { return self.paint_placeholder(g, th, body, "pie_chart") };
        let total: f64 = values.iter().sum::<f64>().max(1.0);
        let f10 = Font::with_dip("Segoe UI", 10.0, FontStyle::REGULAR);
        let mut y = body.top;
        let row_h = ((body.bottom - body.top) / values.len().max(1) as f32).min(24.0);
        for (i, &v) in values.iter().enumerate() {
            let sw = Rect::new(body.left, y + 2.0, body.left + 12.0, y + row_h - 2.0);
            g.fill_rounded_rectangle(Brush::solid(theme.colors[i % theme.colors.len()]), sw, 2.0);
            let pct = format::format_value(&json!(v / total), NumberFormat::Percent, &FormatOpts::default());
            let label = format!("{}  {}", cats.get(i).cloned().unwrap_or_default(), pct);
            g.draw_string(&label, &f10, Brush::solid(theme.fg), Rect::new(body.left + 18.0, y, body.right, y + row_h), &Self::near());
            y += row_h;
        }
    }

    fn paint_table(&self, g: &Graphics<'_>, th: &Palette, theme: &Theme, body: Rect, widget: &Widget, data: Option<&QueryResult>) {
        let Some(d) = data else { return };
        let f10 = Font::with_dip("Segoe UI", 10.0, FontStyle::REGULAR);
        let cols = &d.columns;
        if cols.is_empty() {
            return self.paint_placeholder(g, th, body, &widget.widget_type);
        }
        let cw = (body.right - body.left) / cols.len() as f32;
        // Header.
        for (c, name) in cols.iter().enumerate() {
            let x = body.left + c as f32 * cw;
            g.draw_string(name, &Font::with_dip("Segoe UI", 10.0, FontStyle::BOLD), Brush::solid(th.text_secondary), Rect::new(x, body.top, x + cw, body.top + 18.0), &Self::near());
        }
        let mut y = body.top + 20.0;
        for row in d.rows.iter().take(((body.bottom - body.top - 20.0) / 18.0) as usize) {
            for (c, name) in cols.iter().enumerate() {
                let x = body.left + c as f32 * cw;
                let cell = row.get(name).map(|v| match v { Value::String(s) => s.clone(), other => format::format_value(other, NumberFormat::Auto, &FormatOpts::default()) }).unwrap_or_default();
                g.draw_string(&cell, &f10, Brush::solid(theme.fg), Rect::new(x, y, x + cw, y + 18.0), &Self::near());
            }
            y += 18.0;
            g.draw_line(&Pen::new(th.row_line, 1.0), PointF::new(body.left, y - 1.0), PointF::new(body.right, y - 1.0));
        }
    }
}

impl Control for ReportCanvas {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, proposed: Size) -> Size {
        proposed
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        self.bounds = e.clip_rectangle;
        let th = Palette::of(kubuno_desktop::Application::theme().mode == kubuno_desktop::ui::ThemeMode::Dark);
        let g = e.graphics;
        let b = self.bounds;
        g.fill_rectangle(Brush::solid(th.surface1), b);

        let Some(model) = self.model.clone() else { return };
        let m = model.borrow();
        let Some(ed) = m.editor.as_ref() else {
            g.draw_string("Aucun rapport ouvert", &Font::with_dip("Segoe UI", 13.0, FontStyle::REGULAR), Brush::solid(th.text_tertiary), b, &Self::center());
            return;
        };
        let theme = theme_of(&ed.doc.report.theme, &th);
        let page = match ed.current_page.as_deref() {
            Some(id) => ed.doc.pages.iter().find(|p| p.id == id),
            None => ed.doc.pages.first(),
        };
        let Some(page) = page else { return };

        // Scale the page to fit the viewport with a 24 px margin.
        let margin = 24.0;
        let avail_w = (b.right - b.left) - 2.0 * margin;
        let avail_h = (b.bottom - b.top) - 2.0 * margin;
        let pw = page.width.max(1) as f32;
        let ph = page.height.max(1) as f32;
        let scale = (avail_w / pw).min(avail_h / ph).clamp(0.1, 1.0);
        let ox = b.left + margin + (avail_w - pw * scale).max(0.0) / 2.0;
        let oy = b.top + margin;
        let page_rect = Rect::new(ox, oy, ox + pw * scale, oy + ph * scale);
        let page_bg = page.background.as_deref().map(|s| parse_hex(s, theme.page_bg)).unwrap_or(theme.page_bg);
        g.fill_rectangle(Brush::solid(page_bg), page_rect);
        g.draw_rectangle(&Pen::new(th.border, 1.0), page_rect);

        let map = |wx: i64, wy: i64, ww: i64, wh: i64| Rect::new(ox + wx as f32 * scale, oy + wy as f32 * scale, ox + (wx + ww) as f32 * scale, oy + (wy + wh) as f32 * scale);
        let widgets = ed.current_widgets();
        for widget in widgets {
            let r = map(widget.x, widget.y, widget.width, widget.height);
            let data = m.widget_data(widget);
            self.paint_widget(g, &th, &theme, r, widget, data.as_ref());
        }
        e.raise(self, "OnPaint");
    }
}
