//! Code-behind of Kubuno Data's window (`data_window.kbview`).
//!
//! The window owns the shared model (the open report's editor and the datasets' cached rows,
//! `controls::model`) that the canvas paints, and turns the ribbon's commands into the editor's operations
//! (`kubuno_office_data_core::editor`). Increment A opens a report — the built-in sample, a local `.kbdrp`,
//! or a server report (`--doc <id>`, fetched through the shell's token broker) — and renders it read-only.

use std::collections::HashMap;

use kubuno_desktop::prelude::*;
use kubuno_desktop::views::component::Control as _;
use kubuno_office_data_common::api::{self, routes};
use kubuno_office_data_core::editor::{EditOp, Editor};
use kubuno_office_data_core::model::{ReportDoc, Widget};
use kubuno_office_data_core::visuals;
use serde_json::{json, Map, Value};

use crate::controls::model::{shared, Shared};
use crate::controls::report_canvas::{EditedEventArgs, ReportCanvas, SelectionEventArgs};
use crate::Resources;

pub use kubuno_office_data_common::app::Options;

/// Kubuno Data's main window.
#[kubuno_desktop::view("data_window.kbview")]
pub struct DataWindow {
    #[control]
    canvas: Custom<ReportCanvas>,
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
    model: Shared,
    options: Options,
    pending: Option<ReportDoc>,
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
        match self.fetch_server(&id) {
            Ok((doc, rows)) => self.set_doc(doc, rows),
            Err(e) => {
                kubuno_desktop::tracing::error!("[data] open {id}: {e}");
                self.note = e;
                self.has_note = true;
            }
        }
    }

    /// Fetch a report and its datasets' cached rows through the shell's token broker (blocking; Data is
    /// server-coupled — the DB is the source of truth, each render is a REST read).
    fn fetch_server(&self, id: &str) -> Result<(ReportDoc, HashMap<String, Vec<Value>>), String> {
        let transport = api::connect_account()?;
        let body = api::send(&*transport, &routes::get_report(id))?;
        let doc: ReportDoc = serde_json::from_value(body).map_err(|e| format!("{e}"))?;
        let mut rows = HashMap::new();
        for ds in &doc.report.dataset_ids {
            if let Ok(preview) = api::send(&*transport, &routes::preview_dataset(ds)) {
                if let Some(arr) = preview.get("rows").and_then(Value::as_array) {
                    rows.insert(ds.clone(), arr.clone());
                }
            }
        }
        Ok((doc, rows))
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
                let _ = ed.do_op(EditOp::SetWidgetType { id, widget_type: vtype.to_string() });
            }
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
        self.canvas.with(|c| c.set_selected(Some(id)));
        self.has_selection = true;
    }

    fn delete_execute(&mut self) {
        let Some(id) = self.canvas.with(|c| c.selected()).flatten() else { return };
        if let Some(ed) = self.model.borrow_mut().editor.as_mut() {
            let _ = ed.do_op(EditOp::DeleteWidget(id));
        }
        self.canvas.with(|c| c.set_selected(None));
        self.has_selection = false;
        self.canvas.with(|c| c.invalidate());
        self.refresh();
    }

    /// The canvas edited the report (move/resize). Increment C maps `_e` to a REST mutation + offline queue.
    fn canvas_edited(&mut self, _e: &EditedEventArgs) {
        self.refresh();
    }

    fn canvas_selection(&mut self, e: &SelectionEventArgs) {
        self.has_selection = !e.widget.is_empty();
        self.refresh();
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
            }
        }
        self.canvas.with(|c| c.invalidate());
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
