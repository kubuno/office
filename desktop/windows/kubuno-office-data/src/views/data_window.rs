//! Code-behind of Kubuno Data's window (`data_window.kbview`).
//!
//! The window owns the shared model (the open report's editor and the datasets' cached rows,
//! `controls::model`) that the canvas paints, and turns the ribbon's commands into the editor's operations
//! (`kubuno_office_data_core::editor`). Increment A opens a report — the built-in sample, a local `.kbdrp`,
//! or a server report (`--doc <id>`, fetched through the shell's token broker) — and renders it read-only.

use std::collections::HashMap;

use kubuno_desktop::prelude::*;
use kubuno_desktop::views::component::Control as _;
use kubuno_desktop::{Row, Rows, Value as BValue};
use kubuno_office_data_common::api::{self, routes};
use kubuno_office_desktop_common::api::client::{Method, Request, Transport};
use kubuno_office_data_core::editor::{EditOp, Editor};
use kubuno_office_data_core::model::{ReportDoc, Widget};
use kubuno_office_data_core::visuals;
use serde_json::{json, Map, Value};

/// A mutation waiting to reach the server (the offline queue). Server-coupled persistence: each edit is one
/// REST call; a failed call is kept here and replayed when the server is reachable again.
#[derive(Debug, Clone)]
struct Pending {
    method: Method,
    path: String,
    body: Option<Value>,
}

impl Pending {
    fn request(&self) -> Request {
        let mut r = Request::new(self.method, self.path.clone());
        r.body = self.body.clone();
        r
    }
}

use crate::controls::model::{shared, Shared};
use crate::controls::panels::{FieldPickedEventArgs, FieldsPanel, FiltersPanel, FormatPanel, VisualGallery, VisualPickedEventArgs};
use crate::controls::report_canvas::{EditedEventArgs, ReportCanvas, SelectionEventArgs};
use crate::Resources;

pub use kubuno_office_data_common::app::Options;

/// Kubuno Data's main window.
#[kubuno_desktop::view("data_window.kbview")]
pub struct DataWindow {
    #[control]
    canvas: Custom<ReportCanvas>,
    #[control]
    fields_panel: Custom<FieldsPanel>,
    #[control]
    gallery: Custom<VisualGallery>,
    #[control]
    format_panel: Custom<FormatPanel>,
    #[control]
    filters_panel: Custom<FiltersPanel>,
    #[bind]
    window_title: String,
    #[bind]
    can_undo: bool,
    #[bind]
    can_redo: bool,
    #[bind]
    starred: bool,
    #[bind]
    has_selection: bool,
    #[bind]
    status_report: String,
    #[bind]
    status_pages: String,
    #[bind]
    status_visuals: String,
    #[bind]
    note: String,
    #[bind]
    has_note: bool,
    #[bind]
    theme_value: String,
    #[bind]
    theme_items: Rows,
    #[bind]
    page_label: String,
    model: Shared,
    options: Options,
    pending: Option<ReportDoc>,
    /// Server session (a report opened with `--doc`): the transport, the report id and the offline queue.
    transport: Option<Box<dyn Transport>>,
    report_id: Option<String>,
    outbox: Vec<Pending>,
}

impl DataWindow {
    pub fn new(options: Options, report: Option<ReportDoc>) -> Self {
        let model = shared();
        model.borrow_mut().dark = options.dark;
        let mut window = Self {
            window_title: Resources::app_title().to_string(),
            model,
            pending: report,
            options,
            ..Self::default()
        };
        window.initialize_component();
        window
    }

    fn data_window_load(&mut self) {
        let m = self.model.clone();
        self.canvas.with(|c| c.set_model(m.clone()));
        self.fields_panel.with(|p| p.set_model(m.clone()));
        self.format_panel.with(|p| p.set_model(m.clone()));
        self.filters_panel.with(|p| p.set_model(m.clone()));
        // The report themes offered by the Theme combo (data/palettes.ts).
        self.theme_items = Rows::from(
            kubuno_office_data_core::palettes::REPORT_THEMES
                .iter()
                .map(|t| Row::new().with("Value", BValue::Str(t.id.into())).with("Label", BValue::Str(t.name.into())))
                .collect::<Vec<_>>(),
        );
        if let Some(doc) = self.pending.take() {
            let rows = if self.options.file.is_none() { kubuno_office_data_common::sample::dataset_rows() } else { HashMap::new() };
            self.set_doc(doc, rows);
        } else if let Some(id) = self.options.doc.clone() {
            self.open_server(id);
        } else {
            self.set_doc(kubuno_office_data_common::sample::report(), kubuno_office_data_common::sample::dataset_rows());
        }
        // The shell's header (waffle, account, « also editing »).
        let mut header = kubuno_desktop_header_data::FeedConfig::new("kubuno-data");
        if !self.options.sample {
            header.proxy = kubuno_desktop_sync::get_proxy();
        }
        kubuno_desktop_header_data::start(kubuno_desktop_header_data::HeaderOptions::for_app(&["office-data"]), header, self.options.sample, self.dispatcher());
        self.refresh();
    }

    fn set_doc(&mut self, doc: ReportDoc, rows: HashMap<String, Vec<Value>>) {
        let mut m = self.model.borrow_mut();
        m.editor = Some(Editor::new(doc));
        m.rows = rows;
    }

    // ── A report of the server (`--doc <id>`) ─────────────────────────────────

    fn open_server(&mut self, id: String) {
        // Connect (dev core directly when KUBUNO_DEV_BASE is set, else the shell's token broker) and keep the
        // transport for the server-coupled saves (one REST mutation per edit).
        let transport = match api::connect() {
            Ok(t) => t,
            Err(e) => {
                kubuno_desktop::tracing::error!("[data] connect: {e}");
                self.note = e;
                self.has_note = true;
                return;
            }
        };
        match Self::fetch_server(&*transport, &id) {
            Ok((doc, rows)) => {
                let server_id = doc.report.id.clone();
                self.set_doc(doc, rows);
                self.transport = Some(transport);
                self.report_id = Some(server_id);
                self.load_journal();
                self.flush_outbox();
            }
            Err(e) => {
                kubuno_desktop::tracing::error!("[data] open {id}: {e}");
                self.note = e;
                self.has_note = true;
            }
        }
    }

    /// Fetch a report and its datasets' cached rows (blocking; Data is server-coupled — the DB is the source
    /// of truth, each render is a REST read). `--doc <id>` resolves a report id or a `.kbdrp` file id.
    fn fetch_server(transport: &dyn Transport, id: &str) -> Result<(ReportDoc, HashMap<String, Vec<Value>>), String> {
        let body = api::send(transport, &routes::get_report(id))?;
        let doc: ReportDoc = serde_json::from_value(body).map_err(|e| format!("{e}"))?;
        let mut rows = HashMap::new();
        for ds in &doc.report.dataset_ids {
            if let Ok(preview) = api::send(transport, &routes::preview_dataset(ds)) {
                if let Some(arr) = preview.get("rows").and_then(Value::as_array) {
                    rows.insert(ds.clone(), arr.clone());
                }
            }
        }
        Ok((doc, rows))
    }

    // ── Server-coupled persistence (increment C): one REST mutation per edit ────

    /// Path of the offline-queue journal for the open report.
    fn journal_path(&self) -> Option<std::path::PathBuf> {
        let id = self.report_id.as_ref()?;
        let dir = kubuno_desktop_account::paths::user_data_dir().unwrap_or_default().join("data").join("outbox");
        Some(dir.join(format!("{id}.json")))
    }

    fn save_journal(&self) {
        let Some(path) = self.journal_path() else { return };
        let items: Vec<Value> = self
            .outbox
            .iter()
            .map(|p| json!({ "method": p.method.as_str(), "path": p.path, "body": p.body }))
            .collect();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec(&items) {
            let _ = std::fs::write(&path, bytes);
        }
    }

    fn load_journal(&mut self) {
        let Some(path) = self.journal_path() else { return };
        let Ok(bytes) = std::fs::read(&path) else { return };
        let Ok(items) = serde_json::from_slice::<Vec<Value>>(&bytes) else { return };
        for it in items {
            let method = match it.get("method").and_then(Value::as_str) {
                Some("POST") => Method::Post,
                Some("PATCH") => Method::Patch,
                Some("DELETE") => Method::Delete,
                _ => Method::Get,
            };
            if let Some(path) = it.get("path").and_then(Value::as_str) {
                self.outbox.push(Pending { method, path: path.to_string(), body: it.get("body").cloned().filter(|v| !v.is_null()) });
            }
        }
    }

    /// Send a mutation now; on failure, queue it for later (the offline queue).
    fn mutate(&mut self, pending: Pending) {
        let online = self.transport.is_some();
        let sent = if let Some(t) = self.transport.as_ref() {
            api::send(&**t, &pending.request()).is_ok()
        } else {
            false
        };
        if !sent {
            self.outbox.push(pending);
            self.save_journal();
            if online {
                self.note = "modifications en attente (serveur injoignable)".to_string();
                self.has_note = true;
            }
        }
    }

    /// Replay the queued mutations while the server accepts them.
    fn flush_outbox(&mut self) {
        let Some(t) = self.transport.as_ref() else { return };
        let mut kept = Vec::new();
        let mut flushed = false;
        for p in std::mem::take(&mut self.outbox) {
            if api::send(&**t, &p.request()).is_ok() {
                flushed = true;
            } else {
                kept.push(p);
            }
        }
        self.outbox = kept;
        self.save_journal();
        if flushed && self.outbox.is_empty() {
            self.has_note = false;
        }
    }

    /// Persist an edit the canvas or the ribbon made, as its REST mutation (server-coupled; no file digest).
    fn persist(&mut self, kind: &str, widget_id: &str) {
        let Some(page_id) = self.model.borrow().editor.as_ref().and_then(|e| e.current_page.clone()) else { return };
        let pending = match kind {
            "delete" => Some(Pending { method: Method::Delete, path: routes::delete_widget(widget_id).path, body: None }),
            "add" => {
                let w = self.model.borrow().editor.as_ref().and_then(|e| e.widget(widget_id).ok().cloned());
                w.map(|w| {
                    let body = json!({ "widget_type": w.widget_type, "x": w.x, "y": w.y, "width": w.width, "height": w.height, "config": w.config });
                    Pending { method: Method::Post, path: routes::create_widget(&page_id, json!({})).path, body: Some(body) }
                })
            }
            _ => {
                // move / resize / type / config: PATCH the widget's current geometry and config.
                let w = self.model.borrow().editor.as_ref().and_then(|e| e.widget(widget_id).ok().cloned());
                w.map(|w| {
                    let body = json!({ "x": w.x, "y": w.y, "width": w.width, "height": w.height, "config": w.config, "z_index": w.z_index, "widget_type": w.widget_type });
                    Pending { method: Method::Patch, path: routes::update_widget(widget_id, json!({})).path, body: Some(body) }
                })
            }
        };
        if let Some(p) = pending {
            self.mutate(p);
        }
    }

    // ── Commands ──────────────────────────────────────────────────────────────

    fn refresh_execute(&mut self) {
        self.refresh();
    }

    fn star_execute(&mut self) {
        if let Some(ed) = self.model.borrow_mut().editor.as_mut() {
            ed.doc.report.is_starred = !ed.doc.report.is_starred;
        }
        self.refresh();
    }

    fn undo_execute(&mut self) {
        {
            let mut m = self.model.borrow_mut();
            if let Some(ed) = m.editor.as_mut() {
                let _ = ed.undo();
            }
        }
        self.refresh();
    }

    fn redo_execute(&mut self) {
        {
            let mut m = self.model.borrow_mut();
            if let Some(ed) = m.editor.as_mut() {
                let _ = ed.redo();
            }
        }
        self.refresh();
    }

    // ── Editing (increment B) ─────────────────────────────────────────────────

    fn add_bar_execute(&mut self) {
        self.add_or_switch("bar_chart");
    }
    fn add_line_execute(&mut self) {
        self.add_or_switch("line_chart");
    }
    fn add_pie_execute(&mut self) {
        self.add_or_switch("pie_chart");
    }
    fn add_kpi_execute(&mut self) {
        self.add_or_switch("kpi_card");
    }
    fn add_table_execute(&mut self) {
        self.add_or_switch("data_table");
    }
    fn add_text_execute(&mut self) {
        self.add_or_switch("text");
    }

    /// Clicking a visual button changes the selected widget's type, or adds a new widget — exactly as the
    /// web Visualizations pane does.
    fn add_or_switch(&mut self, vtype: &str) {
        let selected = self.canvas.with(|c| c.selected()).flatten();
        if let Some(id) = selected {
            if let Some(ed) = self.model.borrow_mut().editor.as_mut() {
                let _ = ed.do_op(EditOp::SetWidgetType { id: id.clone(), widget_type: vtype.to_string() });
            }
            self.persist("type", &id);
        } else {
            self.add_widget(vtype);
        }
        self.canvas.with(|c| c.invalidate());
        self.refresh();
    }

    fn add_widget(&mut self, vtype: &str) {
        let widget = {
            let m = self.model.borrow();
            let Some(ed) = m.editor.as_ref() else { return };
            new_widget(ed, vtype)
        };
        let id = widget.id.clone();
        if let Some(ed) = self.model.borrow_mut().editor.as_mut() {
            let _ = ed.do_op(EditOp::AddWidget(widget));
        }
        self.persist("add", &id);
        self.canvas.with(|c| c.set_selected(Some(id)));
        self.has_selection = true;
    }

    fn delete_execute(&mut self) {
        let Some(id) = self.canvas.with(|c| c.selected()).flatten() else { return };
        if let Some(ed) = self.model.borrow_mut().editor.as_mut() {
            let _ = ed.do_op(EditOp::DeleteWidget(id.clone()));
        }
        self.persist("delete", &id);
        self.canvas.with(|c| c.set_selected(None));
        self.has_selection = false;
        self.canvas.with(|c| c.invalidate());
        self.refresh();
    }

    /// The canvas edited the report (move/resize): persist it server-coupled (one REST mutation, queued when
    /// offline), then refresh.
    fn canvas_edited(&mut self, e: &EditedEventArgs) {
        if self.transport.is_some() && !e.widget.is_empty() {
            self.persist(&e.kind, &e.widget);
        }
        self.refresh();
    }

    fn canvas_selection(&mut self, e: &SelectionEventArgs) {
        self.has_selection = !e.widget.is_empty();
        self.refresh();
    }

    // ── Panels (Champs / Visualisations / Format / Filtres) ─────────────────────

    /// A visual was picked in the gallery: add it, or switch the selected widget's type.
    fn visual_picked(&mut self, e: &VisualPickedEventArgs) {
        self.add_or_switch(&e.visual_type);
    }

    /// A field was clicked in the Fields pane: add it to the selected widget's config (a well).
    fn field_picked(&mut self, e: &FieldPickedEventArgs) {
        let Some(id) = self.model.borrow().selected.clone() else {
            self.note = "Sélectionnez un visuel pour y ajouter un champ.".to_string();
            self.has_note = true;
            self.refresh();
            return;
        };
        let new_config = {
            let m = self.model.borrow();
            let Some(w) = m.editor.as_ref().and_then(|ed| ed.widget(&id).ok()) else { return };
            let mut cfg = w.config.clone();
            if !cfg.is_object() {
                cfg = json!({});
            }
            if let Some(obj) = cfg.as_object_mut() {
                if e.kind == "metric" {
                    let arr = obj.entry("metrics").or_insert_with(|| json!([]));
                    if let Some(a) = arr.as_array_mut() {
                        a.push(json!({ "column": e.column, "function": "SUM" }));
                    }
                } else {
                    let arr = obj.entry("dimensions").or_insert_with(|| json!([]));
                    if let Some(a) = arr.as_array_mut() {
                        if !a.iter().any(|v| v.as_str() == Some(e.column.as_str())) {
                            a.push(json!(e.column));
                        }
                    }
                }
            }
            cfg
        };
        if let Some(ed) = self.model.borrow_mut().editor.as_mut() {
            let _ = ed.do_op(EditOp::SetConfig { id: id.clone(), config: new_config });
        }
        self.persist("config", &id);
        self.canvas.with(|c| c.invalidate());
        self.refresh();
    }

    /// Apply a report theme (the Theme combo): set `report.theme` and persist it.
    fn theme_changed(&mut self) {
        let id = self.theme_value.clone();
        if id.is_empty() {
            return;
        }
        let theme = kubuno_office_data_core::palettes::theme_by_id(Some(&id));
        let new_theme = json!({ "themeId": theme.id, "paletteId": theme.palette_id });
        if let Some(ed) = self.model.borrow_mut().editor.as_mut() {
            let _ = ed.do_op(EditOp::ApplyTheme { theme: new_theme.clone(), widget_configs: vec![] });
        }
        if let Some(rid) = self.report_id.clone() {
            self.mutate(Pending { method: Method::Patch, path: routes::update_report(&rid, json!({})).path, body: Some(json!({ "theme": new_theme })) });
        }
        self.canvas.with(|c| c.invalidate());
        self.refresh();
    }

    fn page_add_click(&mut self) {
        if let Some(rid) = self.report_id.clone() {
            self.mutate(Pending { method: Method::Post, path: routes::create_page(&rid, json!({})).path, body: Some(json!({ "title": "Nouvelle page" })) });
            self.note = "Page ajoutée (rouvrez pour l'éditer).".to_string();
            self.has_note = true;
        }
        self.refresh();
    }

    fn pane_fields_execute(&mut self) {
        self.fields_panel.with(|p| p.invalidate());
    }
    fn pane_viz_execute(&mut self) {
        self.gallery.with(|p| p.invalidate());
    }
    fn pane_format_execute(&mut self) {
        self.format_panel.with(|p| p.invalidate());
    }
    fn pane_filters_execute(&mut self) {
        self.filters_panel.with(|p| p.invalidate());
    }

    fn refresh_panels(&mut self) {
        self.fields_panel.with(|p| p.invalidate());
        self.format_panel.with(|p| p.invalidate());
        self.filters_panel.with(|p| p.invalidate());
        self.gallery.with(|p| p.invalidate());
    }

    fn refresh(&mut self) {
        {
            let m = self.model.borrow();
            if let Some(ed) = m.editor.as_ref() {
                let report = &ed.doc.report;
                let title = if report.title.is_empty() { Resources::app_title().to_string() } else { report.title.clone() };
                self.window_title = title.clone();
                self.status_report = title;
                self.starred = report.is_starred;
                self.can_undo = ed.can_undo();
                self.can_redo = ed.can_redo();
                let pages = ed.doc.pages.len();
                let visuals = ed.doc.widgets.len();
                self.status_pages = format!("{pages} page(s)");
                self.status_visuals = format!("{visuals} visuel(s)");
                self.theme_value = report.theme.get("themeId").and_then(|v| v.as_str()).unwrap_or("default").to_string();
                let page = ed.current_page.as_deref().and_then(|id| ed.doc.pages.iter().find(|p| p.id == id)).or_else(|| ed.doc.pages.first());
                self.page_label = page.map(|p| p.title.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| "Page 1".to_string());
            }
        }
        self.canvas.with(|c| c.invalidate());
        self.refresh_panels();
    }
}

/// Build a new widget for `vtype` on the current page, bound to the report's first dataset so it renders
/// against the sample's cached rows (the server assigns the real id and geometry on create in increment C).
fn new_widget(ed: &Editor, vtype: &str) -> Widget {
    let id = format!("w-{}", kubuno_office_data_common::platform::now_ms());
    let dataset = ed.doc.report.dataset_ids.first().cloned();
    let title = visuals::visual_def(vtype).map(|d| d.label.to_string()).unwrap_or_default();
    let (width, height) = match vtype {
        "kpi_card" | "card" => (280, 140),
        "text" => (480, 60),
        "data_table" => (360, 240),
        _ => (420, 280),
    };
    let config = match vtype {
        "text" => json!({ "content": "Nouveau texte", "fontSize": 16 }),
        "kpi_card" | "card" => json!({ "title": title, "dataset_id": dataset, "metrics": [{ "column": "amount", "function": "SUM" }], "format": "currency_eur" }),
        "data_table" => json!({ "title": title, "dataset_id": dataset, "dimensions": ["region"], "metrics": [{ "column": "amount", "function": "SUM" }, { "column": "orders", "function": "SUM" }] }),
        _ => json!({ "title": title, "dataset_id": dataset, "dimensions": ["region"], "metrics": [{ "column": "amount", "function": "SUM" }] }),
    };
    Widget {
        id,
        page_id: ed.current_page.clone(),
        report_id: Some(ed.doc.report.id.clone()),
        widget_type: vtype.to_string(),
        x: 40,
        y: 40,
        width,
        height,
        config,
        z_index: ed.z_range().1 + 1,
        extra: Map::new(),
    }
}
