//! Code-behind of Kubuno Maths' window (`maths_window.kbview`) — Windows Forms' `Form1.cs`.
//!
//! The canvas owns the open formula document; the window opens the document into it, binds what it reports
//! (`StateChanged`: the status bar, the Backstage, the title) and turns the ribbon's commands into view
//! changes. Increment A is read-only (open a local file or a server formula and display it at parity); the
//! editing and save commands land in increments B and C.

use kubuno_desktop::prelude::*;
use kubuno_office_maths_common::model::MathsDoc;

use crate::controls::maths_canvas::{MathsCanvas, MathsStateEventArgs};
use crate::Resources;

/// How the window starts: desktop/common's launch options.
pub use kubuno_office_maths_common::app::Options;

/// Where the open formula comes from.
#[derive(Debug, Clone, Default)]
enum Source {
    #[default]
    Sample,
    File,
    Server(String),
}

/// Kubuno Maths' main window.
#[kubuno_desktop::view("maths_window.kbview")]
pub struct MathsWindow {
    #[control]
    canvas: Custom<MathsCanvas>,
    #[bind]
    window_title: String,
    #[bind]
    doc_title: String,
    #[bind]
    can_save: bool,
    #[bind]
    can_undo: bool,
    #[bind]
    can_redo: bool,
    #[bind]
    starred: bool,
    #[bind]
    is_server: bool,
    #[bind]
    visual_mode: bool,
    #[bind]
    code_mode: bool,
    #[bind]
    pages_text: String,
    #[bind]
    blocks_text: String,
    #[bind]
    zoom_text: String,
    /// The document to open at load (a local file / the sample); a server formula is fetched in load.
    pending: Option<MathsDoc>,
    source: Source,
    dark: bool,
    open_zoom: Option<f32>,
    header_sample: bool,
}

impl MathsWindow {
    pub fn new(options: Options, document: Option<MathsDoc>) -> Self {
        let source = match &options.doc {
            Some(id) => Source::Server(id.clone()),
            None if options.file.is_some() => Source::File,
            None => Source::Sample,
        };
        let title = document.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| "Formule".to_string());
        let mut window = Self {
            window_title: format!("{} — {}", title, Resources::app_title()),
            doc_title: title,
            is_server: matches!(source, Source::Server(_)),
            visual_mode: true,
            zoom_text: "100 %".to_string(),
            pages_text: Resources::status_pages().replace("{n}", "1"),
            blocks_text: Resources::status_blocks().replace("{n}", "0"),
            source,
            pending: document,
            dark: options.dark,
            open_zoom: options.zoom,
            header_sample: options.sample,
            ..Self::default()
        };
        window.initialize_component();
        window
    }

    fn maths_window_load(&mut self) {
        let dark = self.dark;
        let zoom = self.open_zoom.map(|z| z / 100.0);
        if let Some(doc) = self.pending.take() {
            self.canvas.with(|c| {
                c.set_dark(dark);
                if let Some(z) = zoom {
                    c.set_zoom(z);
                }
                c.open(doc);
            });
        }
        if let Source::Server(id) = self.source.clone() {
            self.open_server(id);
        }
        let mut header = kubuno_desktop_header_data::FeedConfig::new("kubuno-maths");
        if !self.header_sample {
            header.proxy = kubuno_desktop_sync::get_proxy();
        }
        kubuno_desktop_header_data::start(
            kubuno_desktop_header_data::HeaderOptions::for_app(&["office-maths"]),
            header,
            self.header_sample,
            self.dispatcher(),
        );
    }

    /// Fetches a server formula (`GET /office/maths/formulas/:id`) off the UI thread and opens it.
    fn open_server(&mut self, id: String) {
        let Some(dispatcher) = self.dispatcher() else {
            kubuno_desktop::tracing::warn!("[maths] no dispatcher: the server formula cannot open");
            return;
        };
        std::thread::spawn(move || {
            let fetched = (|| {
                let transport = kubuno_office_maths_common::api::connect_account()?;
                let body = kubuno_office_maths_common::api::send(&*transport, &kubuno_office_maths_common::api::routes::get_formula(&id))?;
                let f = body.get("formula").ok_or_else(|| "no formula in response".to_string())?;
                let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("Formule").to_string();
                let latex = f.get("latex").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let starred = f.get("is_starred").and_then(|v| v.as_bool()).unwrap_or(false);
                let mut doc = MathsDoc::from_latex(name, latex);
                doc.id = Some(id.clone());
                doc.starred = starred;
                Ok::<MathsDoc, String>(doc)
            })();
            drop(dispatcher.begin_invoke(move |w: &mut MathsWindow| match fetched {
                Ok(doc) => w.on_fetched(doc),
                Err(e) => kubuno_desktop::tracing::error!("[maths] open --doc failed: {e}"),
            }));
        });
    }

    fn on_fetched(&mut self, doc: MathsDoc) {
        self.starred = doc.starred;
        self.doc_title = doc.name.clone();
        self.window_title = format!("{} — {}", doc.name, Resources::app_title());
        self.canvas.with(|c| c.open(doc));
    }

    fn canvas_state_changed(&mut self, e: &MathsStateEventArgs) {
        let s = e.clone();
        if !s.title.is_empty() {
            self.doc_title = s.title.clone();
        }
        self.pages_text = Resources::status_pages().replace("{n}", &s.pages.to_string());
        self.blocks_text = Resources::status_blocks().replace("{n}", &s.blocks.to_string());
        self.zoom_text = format!("{} %", s.zoom.round() as i32);
        self.can_save = s.dirty;
    }

    // ── Zoom ───────────────────────────────────────────────────────────────
    fn zin_execute(&mut self) {
        self.scale_zoom(1.1);
    }
    fn zout_execute(&mut self) {
        self.scale_zoom(1.0 / 1.1);
    }
    fn zreset_execute(&mut self) {
        self.canvas.with(|c| c.set_zoom(1.0));
    }
    fn scale_zoom(&mut self, factor: f32) {
        self.canvas.with(|c| {
            let z = c.zoom() * factor;
            c.set_zoom(z);
        });
    }

    // ── Mode (increment B wires real editing; A just reflects the toggle) ────
    fn visual_execute(&mut self) {
        self.visual_mode = true;
        self.code_mode = false;
    }
    fn code_execute(&mut self) {
        self.code_mode = true;
        self.visual_mode = false;
    }

    // ── Backstage / document ─────────────────────────────────────────────────
    fn bs_close_click(&mut self) {
        self.close();
    }

    fn star_execute(&mut self) {
        let Source::Server(id) = self.source.clone() else { return };
        let to = !self.starred;
        self.starred = to;
        std::thread::spawn(move || {
            if let Ok(transport) = kubuno_office_maths_common::api::connect_account() {
                let _ = kubuno_office_maths_common::api::send(&*transport, &kubuno_office_maths_common::api::routes::update_formula(&id, None, None, Some(to)));
            }
        });
    }

    // ── Commands that land in increment B / C (announced, not silently missing) ──
    fn save_execute(&mut self) {
        kubuno_desktop::tracing::info!("[maths] save: increment C (PATCH /office/maths/formulas/:id)");
    }
    fn undo_execute(&mut self) {}
    fn redo_execute(&mut self) {}
    fn new_execute(&mut self) {
        self.canvas.with(|c| c.open(MathsDoc::blank()));
    }
    fn dup_execute(&mut self) {}
    fn ins_formula_execute(&mut self) {
        kubuno_desktop::tracing::info!("[maths] insert formula: increment B");
    }
    fn ins_graph_execute(&mut self) {
        kubuno_desktop::tracing::info!("[maths] insert graph: increment B");
    }
    fn ins_text_execute(&mut self) {
        kubuno_desktop::tracing::info!("[maths] insert text: increment B");
    }
    fn add_page_execute(&mut self) {
        kubuno_desktop::tracing::info!("[maths] add page: increment B");
    }
}
