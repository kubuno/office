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
use kubuno_office_data_core::editor::Editor;
use kubuno_office_data_core::model::ReportDoc;
use serde_json::Value;

use crate::controls::model::{shared, Shared};
use crate::controls::report_canvas::ReportCanvas;
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
