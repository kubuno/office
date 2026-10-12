//! The dockable side panels of the report editor, ported from `DataReportEditor.tsx`'s panes: the fields
//! (`FieldsPane`), the visualizations gallery (`VisualizationsPane`), the format pane (`FormatPane`) and the
//! filters pane (`FiltersPane`). Painted with the framework's `Surface`, they read the shared model and raise
//! events the window turns into editor operations.

use kubuno_desktop::ui::graphics::{Brush, Font, FontStyle, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::{Event, MouseButton, MouseEventArgs};

use kubuno_office_data_core::format;
use kubuno_office_data_core::{visuals, wells};
use serde_json::Value;

use crate::controls::model::{tint, Palette, Shared};

const PAD: f32 = 12.0;

fn near() -> StringFormat {
    StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP }
}
fn center() -> StringFormat {
    StringFormat { alignment: StringAlignment::Center, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP }
}

/// Fill a panel background and draw the uppercase section header; returns the Y below the header.
fn panel_bg(g: &kubuno_desktop::ui::graphics::Graphics<'_>, th: &Palette, b: Rect, title: &str) -> f32 {
    g.fill_rectangle(Brush::solid(th.surface0), b);
    let hr = Rect::new(b.left + PAD, b.top + 10.0, b.right - PAD, b.top + 30.0);
    g.draw_string(&title.to_uppercase(), &Font::with_dip("Segoe UI", 10.0, FontStyle::BOLD), Brush::solid(th.text_tertiary), hr, &near());
    g.draw_line(&Pen::new(th.border, 1.0), PointF::new(b.left, b.top + 36.0), PointF::new(b.right, b.top + 36.0));
    b.top + 44.0
}

// ── Visualizations gallery ──────────────────────────────────────────────────

/// A visual type was clicked (adds a widget, or switches the selected widget's type).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct VisualPickedEventArgs {
    pub visual_type: String,
}

#[derive(kubuno_desktop::views::component::Component, Default)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Data")]
#[toolbox(icon = "layout-grid")]
#[default_event("VisualPicked")]
pub struct VisualGallery {
    base: ControlCore,
    #[event]
    #[category("Action")]
    pub visual_picked: Event<VisualPickedEventArgs>,
    bounds: Rect,
    tiles: Vec<(Rect, &'static str)>,
}

impl VisualGallery {
    fn relayout(&mut self, top: f32) {
        self.tiles.clear();
        let b = self.bounds;
        let cols = ((b.right - b.left - PAD) / 120.0).floor().max(1.0) as usize;
        let gap = 8.0;
        let tw = (b.right - b.left - 2.0 * PAD - gap * (cols as f32 - 1.0)) / cols as f32;
        let th_ = 46.0;
        for (i, v) in visuals::VISUALS.iter().enumerate() {
            let col = i % cols;
            let row = i / cols;
            let x = b.left + PAD + col as f32 * (tw + gap);
            let y = top + row as f32 * (th_ + gap);
            self.tiles.push((Rect::new(x, y, x + tw, y + th_), v.visual_type));
        }
    }
}

impl Control for VisualGallery {
    fn get_preferred_size(&self, _c: &dyn Canvas, p: Size) -> Size {
        p
    }
    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        self.bounds = e.clip_rectangle;
        let th = Palette::of(kubuno_desktop::Application::theme().mode == kubuno_desktop::ui::ThemeMode::Dark);
        let g = e.graphics;
        let top = panel_bg(g, &th, self.bounds, "Visualisations");
        self.relayout(top);
        let f = Font::with_dip("Segoe UI", 10.0, FontStyle::REGULAR);
        for (r, vtype) in self.tiles.clone() {
            g.fill_rounded_rectangle(Brush::solid(th.surface1), r, 6.0);
            g.draw_rounded_rectangle(&Pen::new(th.border, 1.0), r, 6.0);
            let label = visuals::visual_def(vtype).map(|d| d.label).unwrap_or(vtype);
            g.draw_string(label, &f, Brush::solid(th.text_secondary), Rect::new(r.left + 4.0, r.top, r.right - 4.0, r.bottom), &center());
        }
        e.raise(self, "OnPaint");
    }
    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button == MouseButton::Left {
            let (x, y) = (e.args().x + self.bounds.left, e.args().y + self.bounds.top);
            if let Some((_, vtype)) = self.tiles.iter().find(|(r, _)| x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) {
                self.raise_visual_picked(VisualPickedEventArgs { visual_type: vtype.to_string() });
            }
        }
        e.raise(&*self, "OnMouseDown");
    }
}

// ── Fields panel ────────────────────────────────────────────────────────────

/// A field was clicked (assigned to the selected widget).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct FieldPickedEventArgs {
    pub column: String,
    pub kind: String,
}

#[derive(kubuno_desktop::views::component::Component, Default)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Data")]
#[toolbox(icon = "list")]
#[default_event("FieldPicked")]
pub struct FieldsPanel {
    base: ControlCore,
    #[event]
    #[category("Action")]
    pub field_picked: Event<FieldPickedEventArgs>,
    model: Option<Shared>,
    bounds: Rect,
    rows: Vec<(Rect, String, String)>,
}

impl FieldsPanel {
    pub fn set_model(&mut self, model: Shared) {
        self.model = Some(model);
        self.invalidate();
    }
    /// dataset id → (column, is_numeric) list, from the cached rows.
    fn datasets(&self) -> Vec<(String, Vec<(String, bool)>)> {
        let Some(m) = self.model.as_ref() else { return Vec::new() };
        let m = m.borrow();
        m.rows
            .iter()
            .map(|(ds, rows)| {
                let cols = rows
                    .first()
                    .and_then(Value::as_object)
                    .map(|o| o.iter().map(|(k, v)| (k.clone(), format::is_numeric_value(v))).collect())
                    .unwrap_or_default();
                (ds.clone(), cols)
            })
            .collect()
    }
}

impl Control for FieldsPanel {
    fn get_preferred_size(&self, _c: &dyn Canvas, p: Size) -> Size {
        p
    }
    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        self.bounds = e.clip_rectangle;
        let th = Palette::of(kubuno_desktop::Application::theme().mode == kubuno_desktop::ui::ThemeMode::Dark);
        let g = e.graphics;
        let mut y = panel_bg(g, &th, self.bounds, "Champs");
        self.rows.clear();
        let datasets = self.datasets();
        let f = Font::with_dip("Segoe UI", 11.0, FontStyle::REGULAR);
        let fb = Font::with_dip("Segoe UI", 11.0, FontStyle::BOLD);
        if datasets.is_empty() {
            g.draw_string("Aucun jeu de données.", &f, Brush::solid(th.text_tertiary), Rect::new(self.bounds.left + PAD, y, self.bounds.right - PAD, y + 20.0), &near());
            e.raise(self, "OnPaint");
            return;
        }
        for (ds, cols) in datasets {
            let name = ds.split('-').next().unwrap_or(&ds).to_string();
            g.draw_icon("Database", Rect::new(self.bounds.left + PAD, y + 2.0, self.bounds.left + PAD + 13.0, y + 15.0), 13.0, th.text_secondary);
            g.draw_string(&format!("Jeu de données {name}"), &fb, Brush::solid(th.text_primary), Rect::new(self.bounds.left + PAD + 18.0, y, self.bounds.right - PAD, y + 18.0), &near());
            y += 24.0;
            for (col, numeric) in cols {
                let r = Rect::new(self.bounds.left + PAD + 10.0, y, self.bounds.right - PAD, y + 22.0);
                let icon = if numeric { "Hash" } else { "Type" };
                g.draw_icon(icon, Rect::new(r.left, r.top + 4.0, r.left + 12.0, r.top + 16.0), 12.0, th.text_tertiary);
                g.draw_string(&col, &f, Brush::solid(th.text_secondary), Rect::new(r.left + 18.0, r.top, r.right, r.bottom), &near());
                self.rows.push((r, col, if numeric { "metric".into() } else { "dimension".into() }));
                y += 24.0;
            }
            y += 6.0;
        }
        e.raise(self, "OnPaint");
    }
    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button == MouseButton::Left {
            let (x, y) = (e.args().x + self.bounds.left, e.args().y + self.bounds.top);
            if let Some((_, col, kind)) = self.rows.iter().find(|(r, _, _)| x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) {
                self.raise_field_picked(FieldPickedEventArgs { column: col.clone(), kind: kind.clone() });
            }
        }
        e.raise(&*self, "OnMouseDown");
    }
}

// ── Format panel ────────────────────────────────────────────────────────────

#[derive(kubuno_desktop::views::component::Component, Default)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Data")]
#[toolbox(icon = "sliders")]
pub struct FormatPanel {
    base: ControlCore,
    model: Option<Shared>,
    bounds: Rect,
}

impl FormatPanel {
    pub fn set_model(&mut self, model: Shared) {
        self.model = Some(model);
        self.invalidate();
    }
}

fn field_row(g: &kubuno_desktop::ui::graphics::Graphics<'_>, th: &Palette, left: f32, right: f32, y: f32, label: &str, value: &str) {
    let f = Font::with_dip("Segoe UI", 11.0, FontStyle::REGULAR);
    g.draw_string(label, &f, Brush::solid(th.text_tertiary), Rect::new(left, y, left + 120.0, y + 18.0), &near());
    let vr = Rect::new(left + 110.0, y, right, y + 24.0);
    g.fill_rounded_rectangle(Brush::solid(th.surface1), vr, 4.0);
    g.draw_rounded_rectangle(&Pen::new(th.border, 1.0), vr, 4.0);
    g.draw_string(value, &f, Brush::solid(th.text_primary), Rect::new(vr.left + 6.0, vr.top, vr.right - 6.0, vr.bottom), &near());
}

impl Control for FormatPanel {
    fn get_preferred_size(&self, _c: &dyn Canvas, p: Size) -> Size {
        p
    }
    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        self.bounds = e.clip_rectangle;
        let th = Palette::of(kubuno_desktop::Application::theme().mode == kubuno_desktop::ui::ThemeMode::Dark);
        let g = e.graphics;
        let b = self.bounds;
        let y0 = panel_bg(g, &th, b, "Format");
        let (left, right) = (b.left + PAD, b.right - PAD);
        let Some(m) = self.model.as_ref() else { return };
        let m = m.borrow();
        let Some(w) = m.selected_widget() else {
            g.draw_string("Sélectionnez un visuel.", &Font::with_dip("Segoe UI", 11.0, FontStyle::REGULAR), Brush::solid(th.text_tertiary), Rect::new(left, y0, right, y0 + 20.0), &near());
            return;
        };
        let title = w.title().unwrap_or_default();
        let vlabel = visuals::visual_def(&w.widget_type).map(|d| d.label.to_string()).unwrap_or_else(|| w.widget_type.clone());
        let fmt = w.config_get("format").as_str().map(|s| s.to_string()).unwrap_or_else(|| "auto".into());
        let dataset = w.dataset_id().unwrap_or_else(|| "—".into());
        let dataset = dataset.split('-').next().unwrap_or(&dataset).to_string();
        let mut y = y0;
        field_row(g, &th, left, right, y, "Type", &vlabel);
        y += 34.0;
        field_row(g, &th, left, right, y, "Titre", &title);
        y += 34.0;
        field_row(g, &th, left, right, y, "Format num.", &fmt);
        y += 34.0;
        field_row(g, &th, left, right, y, "Jeu de données", &dataset);
        y += 40.0;
        // The wells this visual exposes (axis/legend/values…), with the fields currently assigned.
        g.draw_string("CHAMPS DU VISUEL", &Font::with_dip("Segoe UI", 10.0, FontStyle::BOLD), Brush::solid(th.text_tertiary), Rect::new(left, y, right, y + 16.0), &near());
        y += 22.0;
        let f = Font::with_dip("Segoe UI", 11.0, FontStyle::REGULAR);
        for well in wells::wells_for(&w.widget_type) {
            let key = match well.id {
                wells::WellId::Values | wells::WellId::Size | wells::WellId::Tooltips => "metrics",
                _ => "dimensions",
            };
            let assigned = w
                .config_get(key)
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string).or_else(|| v.get("column").and_then(Value::as_str).map(str::to_string)))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let dr = Rect::new(left, y, right, y + 26.0);
            g.fill_rounded_rectangle(Brush::solid(tint(th.border, 0.25)), dr, 4.0);
            g.draw_string(well.label, &f, Brush::solid(th.text_tertiary), Rect::new(dr.left + 6.0, dr.top, dr.left + 90.0, dr.bottom), &near());
            g.draw_string(&assigned, &f, Brush::solid(th.text_primary), Rect::new(dr.left + 96.0, dr.top, dr.right - 6.0, dr.bottom), &near());
            y += 30.0;
        }
    }
}

// ── Filters panel ───────────────────────────────────────────────────────────

#[derive(kubuno_desktop::views::component::Component, Default)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Data")]
#[toolbox(icon = "filter")]
pub struct FiltersPanel {
    base: ControlCore,
    model: Option<Shared>,
    bounds: Rect,
}

impl FiltersPanel {
    pub fn set_model(&mut self, model: Shared) {
        self.model = Some(model);
        self.invalidate();
    }
}

impl Control for FiltersPanel {
    fn get_preferred_size(&self, _c: &dyn Canvas, p: Size) -> Size {
        p
    }
    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        self.bounds = e.clip_rectangle;
        let th = Palette::of(kubuno_desktop::Application::theme().mode == kubuno_desktop::ui::ThemeMode::Dark);
        let g = e.graphics;
        let b = self.bounds;
        let y0 = panel_bg(g, &th, b, "Filtres");
        let (left, right) = (b.left + PAD, b.right - PAD);
        g.draw_string("FILTRES SUR CETTE PAGE", &Font::with_dip("Segoe UI", 10.0, FontStyle::BOLD), Brush::solid(th.text_tertiary), Rect::new(left, y0, right, y0 + 16.0), &near());
        let msg = "Aucun filtre. Les filtres s'appliquent à tous les visuels de la page.";
        let f = Font::with_dip("Segoe UI", 11.0, FontStyle::REGULAR);
        let wrap = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Near, trimming: StringTrimming::Word, ..StringFormat::default() };
        g.draw_string(msg, &f, Brush::solid(th.text_tertiary), Rect::new(left, y0 + 24.0, right, y0 + 80.0), &wrap);
    }
}
