//! The window's floating-object commands (`web/src/SpreadsheetApp.tsx`): Insertion › Images, Formes,
//! Graphique (gallery) and Assistant de diagramme; the object menus (cut, copy, paste, duplicate, delete,
//! paint order, rotation, alignment, chart type and data, shape text, alternative text, size and format);
//! the double click that opens a chart's wizard or a shape's text. The insertion entries the web has not
//! built yet (icons, SmartArt, sparklines, link, text box, WordArt) answer « bientôt disponible » as the web
//! does.

use std::collections::HashMap;

use kubuno_desktop::prelude::*;
use kubuno_desktop::views::component::Control as _;
use kubuno_office_sheets_core::chart::data::CellVal;
use kubuno_office_sheets_core::chart::wizard::{apply_subtype, catalog};
use kubuno_office_sheets_core::objects::order::OrderOp;
use kubuno_office_sheets_core::objects::{AlignMode, ObjKind};
use serde_json::{json, Map, Value as Json};

use super::SpreadsheetWindow;
use crate::controls::grid::ObjectEventArgs;
use crate::Resources;

impl SpreadsheetWindow {
    fn show_note(&mut self, text: &str) {
        self.note = text.to_string();
        self.has_note = true;
    }

    /// The sheet's values by A1 ref (what the chart wizard reads).
    fn cell_values(&mut self) -> HashMap<String, CellVal> {
        self.grid.with(|g| {
            let mut out = HashMap::new();
            if let Some(ed) = g.editor() {
                let keys: Vec<String> = ed.sheet().cells.keys().cloned().collect();
                for k in keys {
                    if let Some(p) = kubuno_office_sheets_core::addr::Cell::parse(&k) {
                        let s = ed.shown(p.col, p.row);
                        let v = match s.num {
                            Some(n) => CellVal::Num(n),
                            None => CellVal::Str(s.raw_text),
                        };
                        out.insert(k, v);
                    }
                }
            }
            out
        })
        .unwrap_or_default()
    }

    fn lang(&mut self) -> String {
        self.grid.with(|g| g.editor().map(|e| e.lang.clone())).flatten().unwrap_or_else(|| "fr".into())
    }

    // ── Insertion ───────────────────────────────────────────────────────────

    /// Insertion › Images (`insertImageFromFile`): a local file, embedded as a `data:` URL.
    pub(super) fn images_execute(&mut self) {
        let Some(path) = crate::platform::file_dialog::open_image(Resources::sheet_insert_image()) else { return };
        let src = match crate::platform::images::file_data_url(&path) {
            Ok(s) => s,
            Err(why) => {
                self.show_note(&why);
                return;
            }
        };
        let natural = crate::platform::images::data_url_bytes(&src).and_then(|b| crate::platform::images::natural_size(&b)).unwrap_or((240, 180));
        self.grid.with(|g| g.insert_image(&src, natural));
        self.grid.with(|g| g.focus());
    }

    /// Insertion › Formes: the gallery arms a kind; the next drag on the grid draws it (`armShape`).
    pub(super) fn shapes_execute(&mut self) {
        let Some(dispatcher) = self.dispatcher() else { return };
        let armed = self.grid.with(|g| g.objects.armed.clone()).flatten().unwrap_or_default();
        crate::controls::shape_gallery::ShapeGalleryPanel::open(&armed, move |kind| {
            drop(dispatcher.begin_invoke(move |w: &mut SpreadsheetWindow| {
                w.grid.with(|g| g.arm_shape(&kind));
                w.grid.with(|g| g.focus());
            }));
        });
    }

    /// Insertion › Graphique (`chartsGalleryRender`): a subtype inserts a chart on the auto range; the
    /// footer opens the wizard.
    pub(super) fn chart_gallery_execute(&mut self) {
        let Some(dispatcher) = self.dispatcher() else { return };
        crate::controls::open_chart_gallery(move |type_id, subtype| {
            drop(dispatcher.begin_invoke(move |w: &mut SpreadsheetWindow| {
                if type_id == "wizard" {
                    w.chart_wizard_execute();
                    return;
                }
                let range = w.grid.with(|g| g.chart_auto_range()).unwrap_or_else(|| "A1:B5".into());
                let cat = catalog();
                if let Some(st) = cat.iter().find(|t| t.id == type_id).and_then(|t| t.subtypes.iter().find(|s| s.id == subtype)) {
                    let proto = apply_subtype(&json!({ "type": "bar", "range": range }), st);
                    w.grid.with(|g| g.insert_chart(&proto));
                }
                w.grid.with(|g| g.focus());
            }));
        });
    }

    /// Insertion › Assistant de diagramme (`openChartWizard`): clustered columns on the auto range.
    pub(super) fn chart_wizard_execute(&mut self) {
        let range = self.grid.with(|g| g.chart_auto_range()).unwrap_or_else(|| "A1:B5".into());
        let cells = self.cell_values();
        let lang = self.lang();
        if let Some(chart) = crate::views::chart_wizard::ChartWizard::run(self, &json!({ "type": "bar", "range": range }), 0, cells, &lang) {
            self.grid.with(|g| g.insert_chart(&chart));
        }
        self.grid.with(|g| g.focus());
    }

    /// The wizard on an existing chart (`openChartWizardEdit`), at `step`.
    fn edit_chart(&mut self, id: &str, step: usize) {
        let chart = self.grid.with(|g| g.editor().and_then(|e| e.object(&kubuno_office_sheets_core::objects::ObjRef::new(ObjKind::Chart, id)).cloned())).flatten();
        let Some(chart) = chart else { return };
        let cells = self.cell_values();
        let lang = self.lang();
        if let Some(edited) = crate::views::chart_wizard::ChartWizard::run(self, &chart, step, cells, &lang) {
            let id = id.to_string();
            self.grid.with(|g| g.replace_chart(&id, &edited));
        }
        self.grid.with(|g| g.focus());
    }

    /// The entries the web shows as not built yet (`soon`).
    pub(super) fn soon_execute(&mut self) {
        self.show_note(Resources::sheet_coming_soon());
    }

    // ── The grid's object events ────────────────────────────────────────────

    pub(super) fn grid_object_menu(&mut self, e: &ObjectEventArgs) {
        let count = self.grid.with(|g| g.selected_objects().len()).unwrap_or(0);
        let menu = if count > 1 {
            "group_menu"
        } else {
            match e.kind.as_str() {
                "chart" => "chart_menu",
                "image" => "image_menu",
                _ => "shape_menu",
            }
        };
        kubuno_desktop::views::window::show_context_menu(menu, kubuno_desktop::views::window::MenuAnchor::Point(e.x, e.y));
    }

    pub(super) fn grid_object_open(&mut self, e: &ObjectEventArgs) {
        match e.kind.as_str() {
            "chart" => {
                let id = e.id.clone();
                self.edit_chart(&id, 0);
            }
            "shape" => self.shape_text_execute(),
            _ => {}
        }
    }

    /// The object clipboard first (`objKeyRef`): returns whether objects took the command.
    pub(super) fn objects_copy(&mut self, cut: bool) -> bool {
        self.grid.with(|g| g.copy_selected_objects(cut)).unwrap_or(false)
    }

    pub(super) fn objects_paste(&mut self) -> bool {
        let has_sel = self.grid.with(|g| !g.selected_objects().is_empty()).unwrap_or(false);
        let can = self.grid.with(|g| g.can_paste_objects()).unwrap_or(false);
        // The object clipboard pastes while objects are selected, or when the cell clipboard is empty.
        if can && (has_sel || self.grid.with(|g| g.editor().is_none_or(|e| e.clip_text().is_none())).unwrap_or(true)) {
            return self.grid.with(|g| g.paste_objects()).unwrap_or(false);
        }
        false
    }

    pub(super) fn objects_delete(&mut self) -> bool {
        let any = self.grid.with(|g| !g.selected_objects().is_empty()).unwrap_or(false);
        if any {
            self.grid.with(|g| g.delete_selected_objects());
        }
        any
    }

    // ── Object menu commands ────────────────────────────────────────────────

    pub(super) fn obj_duplicate_execute(&mut self) {
        self.grid.with(|g| g.duplicate_selected_objects());
    }

    pub(super) fn obj_delete_execute(&mut self) {
        self.grid.with(|g| g.delete_selected_objects());
    }

    fn order(&mut self, op: OrderOp) {
        self.grid.with(|g| g.order_selected_objects(op));
    }

    pub(super) fn obj_front_execute(&mut self) {
        self.order(OrderOp::Front);
    }

    pub(super) fn obj_forward_execute(&mut self) {
        self.order(OrderOp::Forward);
    }

    pub(super) fn obj_backward_execute(&mut self) {
        self.order(OrderOp::Backward);
    }

    pub(super) fn obj_back_execute(&mut self) {
        self.order(OrderOp::Back);
    }

    pub(super) fn obj_rot_right_execute(&mut self) {
        self.grid.with(|g| g.rotate_selected_objects(90.0, false));
    }

    pub(super) fn obj_rot_left_execute(&mut self) {
        self.grid.with(|g| g.rotate_selected_objects(-90.0, false));
    }

    pub(super) fn obj_rot_reset_execute(&mut self) {
        self.grid.with(|g| g.rotate_selected_objects(0.0, true));
    }

    fn align(&mut self, m: AlignMode) {
        self.grid.with(|g| g.align_selected_objects(m));
    }

    pub(super) fn obj_align_left_execute(&mut self) {
        self.align(AlignMode::Left);
    }

    pub(super) fn obj_align_center_execute(&mut self) {
        self.align(AlignMode::CenterH);
    }

    pub(super) fn obj_align_right_execute(&mut self) {
        self.align(AlignMode::Right);
    }

    pub(super) fn obj_align_top_execute(&mut self) {
        self.align(AlignMode::Top);
    }

    pub(super) fn obj_align_middle_execute(&mut self) {
        self.align(AlignMode::MiddleV);
    }

    pub(super) fn obj_align_bottom_execute(&mut self) {
        self.align(AlignMode::Bottom);
    }

    pub(super) fn obj_dist_h_execute(&mut self) {
        self.align(AlignMode::DistH);
    }

    pub(super) fn obj_dist_v_execute(&mut self) {
        self.align(AlignMode::DistV);
    }

    fn active_chart_id(&mut self) -> Option<String> {
        self.grid.with(|g| g.active_object()).flatten().filter(|(r, _)| r.kind == ObjKind::Chart).map(|(r, _)| r.id)
    }

    pub(super) fn chart_type_execute(&mut self) {
        if let Some(id) = self.active_chart_id() {
            self.edit_chart(&id, 0);
        }
    }

    pub(super) fn chart_data_execute(&mut self) {
        if let Some(id) = self.active_chart_id() {
            self.edit_chart(&id, 1);
        }
    }

    /// « Modifier le texte » of a shape: the caption in a text dialog (the web edits it in place).
    pub(super) fn shape_text_execute(&mut self) {
        let Some((r, o)) = self.grid.with(|g| g.active_object()).flatten() else { return };
        if r.kind != ObjKind::Shape {
            return;
        }
        let current = o.get("text").and_then(Json::as_str).unwrap_or("").to_string();
        if let Some(text) = crate::views::prompt_dialog::PromptDialog::ask(self, Resources::sheet_obj_shape_edit_text(), Resources::sheet_obj_shape_edit_text(), &current) {
            let mut m = Map::new();
            m.insert("text".into(), if text.is_empty() { Json::Null } else { json!(text) });
            self.grid.with(|g| g.patch_active_object(&m));
        }
    }

    /// « Afficher le texte de remplacement… » (`AltTextDialog`).
    pub(super) fn obj_alt_text_execute(&mut self) {
        let Some((_, o)) = self.grid.with(|g| g.active_object()).flatten() else { return };
        let current = o.get("altText").and_then(Json::as_str).unwrap_or("").to_string();
        if let Some(text) = crate::views::prompt_dialog::PromptDialog::ask(self, Resources::sheet_obj_alt_title(), Resources::sheet_obj_alt_help(), &current) {
            let mut m = Map::new();
            m.insert("altText".into(), if text.trim().is_empty() { Json::Null } else { json!(text.trim()) });
            self.grid.with(|g| g.patch_active_object(&m));
        }
    }

    /// « Taille et propriétés… » and the format panes (`SizePropsDialog`, `FormatObjectDialog`).
    pub(super) fn obj_format_execute(&mut self) {
        let Some((r, o)) = self.grid.with(|g| g.active_object()).flatten() else { return };
        let bx = self.grid.with(|g| g.editor().and_then(|e| e.object_box(&r, None))).flatten();
        let Some((b, rot)) = bx else { return };
        if let Some(patch) = crate::views::object_format_dialog::ObjectFormatDialog::run(self, r.kind, &o, b, rot) {
            self.grid.with(|g| g.patch_active_object(&patch));
        }
    }

    pub(super) fn shape_flip_h_execute(&mut self) {
        self.flip("flipH");
    }

    pub(super) fn shape_flip_v_execute(&mut self) {
        self.flip("flipV");
    }

    fn flip(&mut self, key: &str) {
        let Some((r, o)) = self.grid.with(|g| g.active_object()).flatten() else { return };
        if r.kind != ObjKind::Shape {
            return;
        }
        let on = !o.get(key).and_then(Json::as_bool).unwrap_or(false);
        let mut m = Map::new();
        m.insert(key.into(), if on { json!(true) } else { Json::Null });
        self.grid.with(|g| g.patch_active_object(&m));
    }

    /// « Rétablir la taille de l'image » (`resetImageNaturalSize`).
    pub(super) fn img_reset_size_execute(&mut self) {
        let Some((r, o)) = self.grid.with(|g| g.active_object()).flatten() else { return };
        if r.kind != ObjKind::Image {
            return;
        }
        let Some((w, h)) = o.get("src").and_then(Json::as_str).and_then(crate::platform::images::data_url_bytes).and_then(|b| crate::platform::images::natural_size(&b)) else { return };
        let mut m = Map::new();
        m.insert("bw".into(), json!(w));
        m.insert("bh".into(), json!(h));
        for k in ["cropL", "cropT", "cropR", "cropB"] {
            m.insert(k.into(), Json::Null);
        }
        self.grid.with(|g| g.patch_active_object(&m));
    }
}
