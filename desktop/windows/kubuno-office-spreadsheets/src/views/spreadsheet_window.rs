//! Code-behind of Kubuno Spreadsheets' window (`spreadsheet_window.kbview`) — Windows Forms' `Form1.cs`.
//!
//! The grid owns the open workbook (the core's editor); the window opens the workbook into it, binds what it
//! reports (`StateChanged`: the formula bar, the Name Box, the status bar, the ribbon's checked states, the
//! tabs) and turns the ribbon's commands, the context menu, the formula bar, the tabs and the status bar's
//! zoom into the grid's calls. A workbook of the server (`--doc <id>`) is saved by a background session (the
//! digest-guarded session shared with Kubuno Documents: `kubuno_office_spreadsheets_common::api`).

use std::path::PathBuf;
use std::rc::Rc;

use kubuno_desktop::prelude::*;
use kubuno_desktop::views::component::Control as _;
use kubuno_office_sheets_core::addr::{Cell as Pos, MAX_COLS, MAX_ROWS};
use kubuno_office_sheets_core::editor::commands::{Axis, Borders, PasteMode};
use kubuno_office_sheets_core::editor::Editor;
use kubuno_office_sheets_core::model::workbook::Workbook;
use kubuno_office_desktop_common::api::client;
use kubuno_office_spreadsheets_common::api::{live, session};
use kubuno_office_spreadsheets_common::model::state::AggLabels;
use kubuno_office_spreadsheets_common::platform::SystemEnv;

use crate::controls::grid::{GridCanvas, GridMessageEventArgs, GridStateEventArgs};
use crate::controls::sheet_tabs::SheetTabEventArgs;
use crate::Resources;

/// How the window starts: desktop/common's launch options.
pub use kubuno_office_spreadsheets_common::app::Options;

#[path = "spreadsheet_window_collab.rs"]
mod collab;

/// The fonts the web offers first (`SHEET_FONTS`).
const SHEET_FONTS: [&str; 6] = ["Arial", "Helvetica", "Times New Roman", "Courier New", "Georgia", "Verdana"];
/// The border colour the web's border menu applies (`applyBorders` default).
const BORDER_COLOR: &str = "#5f6368";

/// Kubuno Spreadsheets' main window.
#[kubuno_desktop::view("spreadsheet_window.kbview")]
pub struct SpreadsheetWindow {
    #[control]
    grid: Custom<GridCanvas>,
    #[bind]
    window_title: String,
    #[bind]
    can_save: bool,
    #[bind]
    can_undo: bool,
    #[bind]
    can_redo: bool,
    #[bind]
    painter: bool,
    #[bind]
    bold: bool,
    #[bind]
    italic: bool,
    #[bind]
    underline: bool,
    #[bind]
    strike: bool,
    #[bind]
    text_color: String,
    #[bind]
    fill_color: String,
    #[bind]
    align_left: bool,
    #[bind]
    align_center: bool,
    #[bind]
    align_right: bool,
    #[bind]
    wrap: bool,
    #[bind]
    merged: bool,
    #[bind]
    fmt_currency: bool,
    #[bind]
    fmt_percent: bool,
    #[bind]
    fmt_number: bool,
    #[bind]
    filter_mode: bool,
    #[bind]
    gridlines: bool,
    #[bind]
    frozen: bool,
    #[bind]
    full_screen: bool,
    #[bind]
    font_families: Rows,
    #[bind]
    font_family: String,
    #[bind]
    font_size: String,
    // Backstage › Informations.
    #[bind]
    wb_title: String,
    #[bind]
    sheets_text: String,
    #[bind]
    source_text: String,
    // Find / replace.
    #[bind]
    show_find: bool,
    #[bind]
    show_replace: bool,
    #[bind]
    find_text: String,
    #[bind]
    replace_text: String,
    #[bind]
    find_status: String,
    find_match_case: bool,
    // The formula bar.
    #[bind]
    name_box_text: String,
    #[bind]
    editing: bool,
    #[bind]
    formula_text: String,
    // The status bar.
    #[bind]
    zoom_percent: f32,
    #[bind]
    status_ref: String,
    #[bind]
    status_size: String,
    #[bind]
    has_size: bool,
    #[bind]
    status_error: String,
    #[bind]
    has_error: bool,
    #[bind]
    status_agg: String,
    #[bind]
    has_agg: bool,
    #[bind]
    sync_status: String,
    #[bind]
    has_sync: bool,
    #[bind]
    note: String,
    #[bind]
    has_note: bool,
    #[bind]
    status_sheet: String,
    // The tabs.
    #[bind]
    tab_names: String,
    #[bind]
    active_tab: i32,
    /// The workbook to open at load.
    pending: Option<Workbook>,
    /// The server workbook's session (`--doc`).
    live: Option<live::Live>,
    open_doc: Option<String>,
    /// The revision last published to the session.
    last_revision: u64,
    /// A local file, written back by Ctrl+S.
    file: Option<PathBuf>,
    open_zoom: Option<f32>,
    header_sample: bool,
    lang: String,
    /// The format painter's capture while it is armed.
    painter_style: Option<Option<serde_json::Value>>,
    /// The formula bar is being typed in (its text drives the draft).
    bar_typing: bool,
    last_text_color: String,
    last_fill_color: String,
    // Real-time collaboration (`spreadsheet_window_collab.rs`).
    collab: Option<collab::CollabLink>,
    #[bind]
    collab_active: bool,
    #[bind]
    collab_status: String,
    #[bind]
    has_collab: bool,
    #[bind]
    presence_users: String,
    #[bind]
    has_presence: bool,
}

impl SpreadsheetWindow {
    pub fn new(options: Options, workbook: Workbook) -> Self {
        let lang = options.culture.clone().unwrap_or_else(|| kubuno_desktop::resources::culture().split('-').next().unwrap_or("fr").to_string());
        let mut window = Self {
            window_title: format!("{} — {}", workbook.title, Resources::app_title()),
            pending: Some(workbook),
            file: options.file.clone(),
            open_doc: options.doc.clone(),
            open_zoom: options.zoom,
            header_sample: options.sample,
            lang,
            zoom_percent: 100.0,
            font_family: "Arial".into(),
            font_size: "11".into(),
            align_left: true,
            gridlines: true,
            last_text_color: "#d93025".into(),
            last_fill_color: "#fff2cc".into(),
            ..Self::default()
        };
        window.initialize_component();
        window
    }

    fn labels() -> AggLabels {
        AggLabels { avg: Resources::status_avg().into(), count: Resources::status_count().into(), sum: Resources::status_sum().into(), min: Resources::status_min().into(), max: Resources::status_max().into() }
    }

    fn spreadsheet_window_load(&mut self) {
        self.font_families = font_rows();
        if let Some(wb) = self.pending.take() {
            self.open_workbook(wb);
        }
        if let Some(id) = self.open_doc.take() {
            self.start_live(id);
        }
        let mut header = kubuno_desktop_header_data::FeedConfig::new("kubuno-spreadsheets");
        if !self.header_sample {
            header.proxy = kubuno_desktop_sync::get_proxy();
        }
        kubuno_desktop_header_data::start(kubuno_desktop_header_data::HeaderOptions::for_app(&["office-spreadsheets"]), header, self.header_sample, self.dispatcher());
    }

    fn open_workbook(&mut self, wb: Workbook) {
        self.wb_title = wb.title.clone();
        self.source_text = if self.live.is_some() || self.open_doc.is_some() { Resources::info_source_server().to_string() } else { Resources::info_source_local().to_string() };
        let mut ed = Editor::new(wb, Rc::new(SystemEnv::default()));
        ed.lang = self.lang.clone();
        if let Some(z) = self.open_zoom {
            ed.set_zoom(z as f64 / 100.0);
        }
        self.grid.with(|g| {
            g.set_labels(Self::labels());
            g.open(ed);
            g.focus();
        });
    }

    /// The workbook's bytes for the session, remembering which edit they are (`last_revision`).
    fn workbook_bytes(&mut self) -> Option<Vec<u8>> {
        let (bytes, revision) = self.grid.with(|g| g.editor().map(|e| (e.wb.to_bundle_bytes(), e.revision()))).flatten()?;
        self.last_revision = revision;
        Some(bytes)
    }

    // ── A workbook of the server ────────────────────────────────────────────

    fn start_live(&mut self, id: String) {
        let Some(dispatcher) = self.dispatcher() else {
            kubuno_desktop::tracing::warn!("[spreadsheets] the window has no dispatcher: the server workbook cannot open");
            return;
        };
        self.sync_status = Resources::sync_opening().to_string();
        self.has_sync = true;
        let post = dispatcher.clone();
        let sink: live::Sink = std::sync::Arc::new(move |event| {
            drop(post.begin_invoke(move |w: &mut SpreadsheetWindow| w.on_live(event)));
        });
        let doc = id.clone();
        self.live = Some(live::Live::open(id, sink, move || kubuno_office_spreadsheets_common::api::remote::connect(&doc)));
    }

    fn load_bytes(&mut self, bytes: &[u8], title: &str) -> bool {
        match Workbook::from_bytes(bytes, title) {
            Ok(mut wb) => {
                if wb.title.is_empty() {
                    wb.title = title.to_string();
                }
                let keep_live = self.live.is_some();
                self.open_workbook(wb);
                if keep_live {
                    self.source_text = Resources::info_source_server().to_string();
                }
                true
            }
            Err(e) => {
                self.sync_status = Resources::open_failed().replace("{why}", &e.to_string());
                false
            }
        }
    }

    fn on_live(&mut self, event: live::Event) {
        use session::{Protection, Resolution, State};
        match event {
            live::Event::Opened { title, content, recovered, protection, offline } => {
                self.load_bytes(&content, &title);
                if let Some(copy) = recovered {
                    // Closing the question restores too (the local copy is the work nothing else holds).
                    let answer = super::confirm_dialog::ConfirmDialog::choose(self, Resources::recover_title(), Resources::recover_text(), Resources::recover_yes(), Resources::recover_no());
                    let restore = answer != super::confirm_dialog::Answer::Second;
                    if restore && self.load_bytes(&copy, &title) {
                        // Not what the server holds: the title says « modifié » and Ctrl+S is enabled.
                        self.grid.with(|g| g.with_editor(|ed| ed.touch()));
                        if let Some(live) = &self.live {
                            live.recovered(true);
                            live.edited(copy, false);
                        }
                    } else if let Some(live) = &self.live {
                        live.recovered(false);
                    }
                }
                self.sync_status = if offline {
                    Resources::sync_offline().to_string()
                } else if protection == Protection::DigestOnly {
                    Resources::sync_digest_only().to_string()
                } else {
                    Resources::sync_saved().to_string()
                };
                self.has_sync = true;
                if let Some(id) = self.live.as_ref().map(|l| l.id.clone()) {
                    self.start_collab(&id);
                }
            }
            live::Event::Failed(why) => {
                self.sync_status = Resources::open_failed().replace("{why}", &why);
                self.has_sync = true;
            }
            live::Event::Status(s) => {
                let mut text = match s.state {
                    State::Clean if !s.unsaved => Resources::sync_saved().to_string(),
                    State::Saving => Resources::sync_saving().to_string(),
                    State::Failed => Resources::sync_failed().to_string(),
                    State::TooLarge => Resources::sync_too_large().to_string(),
                    State::Conflict => Resources::sync_conflict().to_string(),
                    _ => Resources::sync_unsaved().to_string(),
                };
                if s.protection == Protection::DigestOnly && matches!(s.state, State::Clean | State::Dirty) {
                    text = format!("{text} · {}", Resources::sync_digest_only());
                }
                if !s.editors.is_empty() {
                    text = format!("{text} · {}", Resources::sync_editors().replace("{names}", &s.editors.join(", ")));
                }
                self.sync_status = text;
                // « Saved » is about the bytes the session was handed: an edit made since keeps the workbook unsaved.
                if s.state == State::Clean && !s.unsaved {
                    let published = self.last_revision;
                    self.grid.with(|g| {
                        if let Some(e) = g.editor_mut().filter(|e| e.revision() == published) {
                            e.mark_saved();
                        }
                        g.invalidate();
                    });
                }
            }
            live::Event::Conflict => {
                if self.collab_resolves_conflicts() {
                    if let Some(live) = &self.live {
                        live.resolve(Resolution::KeepMine);
                    }
                    return;
                }
                // Closing the question decides nothing: « take theirs » discards local work.
                let resolution = match super::confirm_dialog::ConfirmDialog::choose(self, Resources::conflict_title(), Resources::conflict_text(), Resources::conflict_mine(), Resources::conflict_theirs()) {
                    super::confirm_dialog::Answer::First => Resolution::KeepMine,
                    super::confirm_dialog::Answer::Second => Resolution::TakeTheirs,
                    super::confirm_dialog::Answer::Dismissed => return,
                };
                if let Some(live) = &self.live {
                    live.resolve(resolution);
                }
            }
            live::Event::Reload(content) => {
                let title = self.wb_title.clone();
                self.load_bytes(&content, &title);
                self.collab_reloaded();
            }
            live::Event::Closed => {}
        }
    }

    fn spreadsheet_window_closing(&mut self) {
        self.stop_collab();
        self.grid.with(|g| g.commit_edit(None));
        if let Some(live) = self.live.take() {
            let bytes = self.workbook_bytes();
            if !live.close(bytes) {
                kubuno_desktop::tracing::warn!("[spreadsheets] the server session did not finish closing in time; the local copy is kept");
            }
        }
    }

    // ── What the grid reports ───────────────────────────────────────────────

    fn grid_state_changed(&mut self, e: &GridStateEventArgs) {
        let s = &e.state;
        self.name_box_text = s.name_box.clone();
        self.editing = e.editing;
        if !self.bar_typing {
            self.formula_text = s.formula.clone();
        }
        self.status_ref = s.status_ref.clone();
        self.status_size = s.status_size.clone();
        self.has_size = !s.status_size.is_empty();
        match &s.error {
            Some((code, help)) => {
                self.status_error = format!("⚠ {code} — {help}");
                self.has_error = true;
            }
            None => {
                self.status_error.clear();
                self.has_error = false;
            }
        }
        self.status_agg = s.status_agg.iter().map(|(k, v)| format!("{k} : {v}")).collect::<Vec<_>>().join("   ");
        self.has_agg = !s.status_agg.is_empty();
        self.status_sheet = Resources::status_sheet().replace("{n}", &(s.active_sheet + 1).to_string()).replace("{total}", &s.sheets.len().to_string());
        self.tab_names = s.sheets.join("\n");
        self.active_tab = s.active_sheet as i32;
        self.sheets_text = s.sheets.len().to_string();
        self.bold = s.bold;
        self.italic = s.italic;
        self.underline = s.underline;
        self.strike = s.strike;
        self.wrap = s.wrap;
        self.align_left = s.align == "left";
        self.align_center = s.align == "center";
        self.align_right = s.align == "right";
        self.merged = s.merged;
        self.fmt_currency = s.num_fmt == "currency";
        self.fmt_percent = s.num_fmt == "percent";
        self.fmt_number = s.num_fmt == "number";
        self.font_family = s.font_family.clone();
        self.font_size = s.font_size.clone();
        self.gridlines = s.gridlines;
        self.frozen = s.frozen_rows > 0 || s.frozen_cols > 0;
        self.filter_mode = s.filter_mode;
        self.can_undo = s.can_undo;
        self.can_redo = s.can_redo;
        self.can_save = s.dirty;
        self.zoom_percent = s.zoom_percent as f32;
        if s.dirty && s.revision != self.last_revision && self.live.is_some() {
            if let Some(bytes) = self.workbook_bytes() {
                if let Some(live) = &self.live {
                    live.edited(bytes, false);
                }
            }
        }
        let title = if s.dirty { Resources::title_modified().replace("{title}", &self.wb_title) } else { self.wb_title.clone() };
        self.window_title = format!("{title} — {}", Resources::app_title());
        self.collab_state_changed();
    }

    fn grid_message(&mut self, e: &GridMessageEventArgs) {
        self.note = match e.kind.as_str() {
            "protected" => Resources::msg_protected().to_string(),
            _ if !e.message.is_empty() => e.message.clone(),
            _ => Resources::msg_rejected().to_string(),
        };
        self.has_note = true;
    }

    fn clear_note(&mut self) {
        self.has_note = false;
        self.note.clear();
    }

    /// Runs an editor command on the grid.
    /// Runs an editor command on the selection; nothing happens while no cell is selected (the web's commands
    /// start with `if (!selectedCell) return`).
    fn cmd<R>(&mut self, f: impl FnOnce(&mut Editor) -> Result<R, kubuno_office_sheets_core::editor::commands::Refused>) -> Option<R> {
        if !self.grid.with(|g| g.editor().is_some_and(|e| e.selected)).unwrap_or(false) {
            return None;
        }
        self.cmd_any(f)
    }

    /// Runs an editor command that does not need a selection (undo, redo, sheet options).
    fn cmd_any<R>(&mut self, f: impl FnOnce(&mut Editor) -> Result<R, kubuno_office_sheets_core::editor::commands::Refused>) -> Option<R> {
        self.clear_note();
        let r = self.grid.with(|g| g.run(f)).flatten();
        self.grid.with(|g| g.focus());
        r
    }

    // ── File ────────────────────────────────────────────────────────────────

    fn save_execute(&mut self) {
        self.grid.with(|g| g.commit_edit(None));
        if self.live.is_some() {
            if let Some(bytes) = self.workbook_bytes() {
                if let Some(live) = &self.live {
                    live.save_now(bytes);
                }
            }
            return;
        }
        let Some(path) = self.file.clone() else { return };
        // A local file is written in its own shape: a bundle stays a bundle, a content file stays one
        // (gzipped when it was).
        let original = std::fs::read(&path).unwrap_or_default();
        let gz = original.len() >= 2 && original[0] == 0x1f && original[1] == 0x8b;
        let result = self
            .grid
            .with(|g| {
                g.editor().map(|ed| {
                    let is_bundle = kubuno_office_sheets_core::model::workbook::gunzip(&original).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()).is_some_and(|v| v.get("content").is_some());
                    let body = if is_bundle { ed.wb.to_bundle_bytes() } else { serde_json::to_vec(&ed.wb.to_content()).unwrap_or_default() };
                    if gz {
                        kubuno_office_sheets_core::model::workbook::gzip(&body)
                    } else {
                        body
                    }
                })
            })
            .flatten();
        let write = match result {
            Some(bytes) => std::fs::write(&path, bytes).map_err(|e| e.to_string()),
            None => Err("no workbook".into()),
        };
        match write {
            Ok(()) => {
                self.grid.with(|g| {
                    if let Some(e) = g.editor_mut() {
                        e.mark_saved();
                    }
                    g.invalidate();
                });
            }
            Err(why) => {
                self.note = Resources::msg_save_failed().replace("{why}", &why);
                self.has_note = true;
            }
        }
    }

    fn bs_close_click(&mut self) {
        self.close();
    }

    fn export(&mut self, format: &str) {
        let Some(live) = &self.live else {
            self.note = Resources::export_local().to_string();
            self.has_note = true;
            return;
        };
        let id = live.id.clone();
        let title = self.wb_title.clone();
        let ext = format.to_string();
        let result = (|| -> Result<PathBuf, String> {
            let account = kubuno_desktop_sync::current_account().map_err(|e| e.to_string())?.ok_or("aucun compte")?;
            use client::Transport;
            let t = client::SyncTransport::for_account(account);
            let r = t.send(&kubuno_office_spreadsheets_common::api::routes::export(&id, &ext)).map_err(|e| e.to_string())?;
            if !r.ok() {
                return Err(format!("HTTP {}", r.status));
            }
            let dir = std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Downloads")).filter(|p| p.is_dir()).unwrap_or_else(std::env::temp_dir);
            let safe: String = title.chars().map(|c| if "<>:\"/\\|?*".contains(c) { '_' } else { c }).collect();
            let path = dir.join(format!("{}.{ext}", if safe.trim().is_empty() { "classeur" } else { safe.trim() }));
            std::fs::write(&path, &r.body).map_err(|e| e.to_string())?;
            Ok(path)
        })();
        match result {
            Ok(p) => self.note = Resources::export_done().replace("{path}", &p.display().to_string()),
            Err(why) => self.note = Resources::msg_save_failed().replace("{why}", &why),
        }
        self.has_note = true;
    }

    fn bs_export_xlsx_click(&mut self) {
        self.export("xlsx");
    }

    fn bs_export_ods_click(&mut self) {
        self.export("ods");
    }

    // ── Clipboard and history ───────────────────────────────────────────────

    fn undo_execute(&mut self) {
        self.cmd_any(|ed| Ok(ed.undo()));
    }

    fn redo_execute(&mut self) {
        self.cmd_any(|ed| Ok(ed.redo()));
    }

    fn copy_execute(&mut self) {
        let text = self.grid.with(|g| {
            g.with_editor(|ed| {
                ed.copy_selection(false);
                ed.clip_text()
            })
        });
        if let Some(Some(Some(t))) = text {
            crate::platform::clipboard::set_text(&t);
        }
    }

    fn cut_execute(&mut self) {
        let text = self.grid.with(|g| {
            g.with_editor(|ed| {
                ed.copy_selection(true);
                ed.clip_text()
            })
        });
        if let Some(Some(Some(t))) = text {
            crate::platform::clipboard::set_text(&t);
        }
    }

    /// Ctrl+V: the internal clipboard when the system clipboard still holds what it copied (formulas and
    /// formats travel), else the system clipboard's text (a desktop addition: the web's clipboard is
    /// internal only).
    fn paste_execute(&mut self) {
        let sys = crate::platform::clipboard::get_text();
        let internal = self.grid.with(|g| g.editor().and_then(|e| e.clip_text())).flatten();
        match (sys, internal) {
            (Some(s), Some(i)) if s.trim_end() == i.trim_end() => {
                self.cmd(|ed| ed.paste());
            }
            (Some(s), _) => {
                self.cmd(|ed| ed.paste_text(&s));
            }
            (None, Some(_)) => {
                self.cmd(|ed| ed.paste());
            }
            (None, None) => {}
        }
    }

    fn paste_values_execute(&mut self) {
        self.cmd(|ed| ed.paste_special(PasteMode::Values));
    }

    fn paste_format_execute(&mut self) {
        self.cmd(|ed| ed.paste_special(PasteMode::Format));
    }

    fn paste_formula_execute(&mut self) {
        self.cmd(|ed| ed.paste_special(PasteMode::Formula));
    }

    fn paste_transpose_execute(&mut self) {
        self.cmd(|ed| ed.paste_special(PasteMode::Transpose));
    }

    fn paste_noborders_execute(&mut self) {
        self.cmd(|ed| ed.paste_special(PasteMode::NoBorders));
    }

    /// Reproduire la mise en forme: captures the active cell's style; the next selection receives it
    /// (`applyPainter` on mouse-up, here on the next command or selection change through the toggle).
    fn painter_execute(&mut self) {
        if let Some(style) = self.painter_style.take() {
            self.cmd(|ed| ed.apply_painter(style));
            self.painter = false;
            return;
        }
        let s = self.grid.with(|g| g.editor().map(|e| e.capture_format())).flatten();
        self.painter_style = Some(s.flatten());
        self.painter = true;
    }

    fn select_all_execute(&mut self) {
        self.grid.with(|g| g.with_editor(|ed| ed.select_all_used()));
    }

    // ── Font ────────────────────────────────────────────────────────────────

    fn bold_execute(&mut self) {
        self.cmd(|ed| ed.toggle_style("bold"));
    }

    fn italic_execute(&mut self) {
        self.cmd(|ed| ed.toggle_style("italic"));
    }

    fn underline_execute(&mut self) {
        self.cmd(|ed| ed.toggle_style("underline"));
    }

    fn strike_execute(&mut self) {
        self.cmd(|ed| ed.toggle_style("strike"));
    }

    fn font_family_changed(&mut self, e: &TextChangedEventArgs) {
        let f = e.new.trim().to_string();
        if !f.is_empty() && f != self.font_family {
            self.cmd(|ed| ed.set_font(Some(&f), None));
        }
    }

    fn font_size_changed(&mut self, e: &TextChangedEventArgs) {
        if let Ok(n) = e.new.trim().replace(',', ".").parse::<f64>() {
            if e.new.trim() != self.font_size && n > 0.0 && n <= 400.0 {
                self.cmd(|ed| ed.set_font(None, Some(n)));
            }
        }
    }

    fn text_color_click(&mut self) {
        let c = self.last_text_color.clone();
        self.cmd(|ed| ed.set_color(Some(&c)));
    }

    fn text_color_changed(&mut self, e: &TextChangedEventArgs) {
        let c = e.new.clone();
        if c.is_empty() {
            self.cmd(|ed| ed.set_color(None));
        } else {
            self.last_text_color = c.clone();
            self.text_color = c.clone();
            self.cmd(|ed| ed.set_color(Some(&c)));
        }
    }

    fn fill_color_click(&mut self) {
        let c = self.last_fill_color.clone();
        self.cmd(|ed| ed.set_fill(Some(&c)));
    }

    fn fill_color_changed(&mut self, e: &TextChangedEventArgs) {
        let c = e.new.clone();
        if c.is_empty() {
            self.cmd(|ed| ed.set_fill(None));
        } else {
            self.last_fill_color = c.clone();
            self.fill_color = c.clone();
            self.cmd(|ed| ed.set_fill(Some(&c)));
        }
    }

    // ── Alignment and number ────────────────────────────────────────────────

    fn al_left_execute(&mut self) {
        self.cmd(|ed| ed.set_align("left"));
    }

    fn al_center_execute(&mut self) {
        self.cmd(|ed| ed.set_align("center"));
    }

    fn al_right_execute(&mut self) {
        self.cmd(|ed| ed.set_align("right"));
    }

    fn wrap_execute(&mut self) {
        self.cmd(|ed| ed.toggle_style("wrap"));
    }

    fn merge_execute(&mut self) {
        self.cmd(|ed| ed.toggle_merge());
    }

    fn currency_execute(&mut self) {
        self.cmd(|ed| ed.set_num_fmt("currency"));
    }

    fn percent_execute(&mut self) {
        self.cmd(|ed| ed.set_num_fmt("percent"));
    }

    fn number_execute(&mut self) {
        self.cmd(|ed| ed.set_num_fmt("number"));
    }

    fn dec_less_execute(&mut self) {
        self.cmd(|ed| ed.adjust_decimals(-1));
    }

    fn dec_more_execute(&mut self) {
        self.cmd(|ed| ed.adjust_decimals(1));
    }

    // ── Cells ───────────────────────────────────────────────────────────────

    fn insert_rows_execute(&mut self) {
        self.cmd(|ed| ed.insert_or_delete(Axis::Row, true));
    }

    fn insert_cols_execute(&mut self) {
        self.cmd(|ed| ed.insert_or_delete(Axis::Col, true));
    }

    fn delete_rows_execute(&mut self) {
        self.cmd(|ed| ed.insert_or_delete(Axis::Row, false));
    }

    fn delete_cols_execute(&mut self) {
        self.cmd(|ed| ed.insert_or_delete(Axis::Col, false));
    }

    fn insert_cells_right_execute(&mut self) {
        self.cmd(|ed| ed.insert_cells_shift(false));
    }

    fn insert_cells_down_execute(&mut self) {
        self.cmd(|ed| ed.insert_cells_shift(true));
    }

    fn delete_cells_left_execute(&mut self) {
        self.cmd(|ed| ed.delete_cells_shift(false));
    }

    fn delete_cells_up_execute(&mut self) {
        self.cmd(|ed| ed.delete_cells_shift(true));
    }

    fn borders(&mut self, kind: Borders) {
        self.cmd(|ed| ed.apply_borders(kind, BORDER_COLOR));
    }

    fn b_all_execute(&mut self) {
        self.borders(Borders::All);
    }

    fn b_inner_execute(&mut self) {
        self.borders(Borders::Inner);
    }

    fn b_outer_execute(&mut self) {
        self.borders(Borders::Outer);
    }

    fn b_none_execute(&mut self) {
        self.borders(Borders::None);
    }

    fn b_top_execute(&mut self) {
        self.borders(Borders::Top);
    }

    fn b_bottom_execute(&mut self) {
        self.borders(Borders::Bottom);
    }

    fn b_left_execute(&mut self) {
        self.borders(Borders::Left);
    }

    fn b_right_execute(&mut self) {
        self.borders(Borders::Right);
    }

    fn hide_rows_execute(&mut self) {
        self.cmd(|ed| ed.hide_selection(Axis::Row));
    }

    fn unhide_rows_execute(&mut self) {
        self.cmd(|ed| ed.unhide_selection(Axis::Row));
    }

    fn hide_cols_execute(&mut self) {
        self.cmd(|ed| ed.hide_selection(Axis::Col));
    }

    fn unhide_cols_execute(&mut self) {
        self.cmd(|ed| ed.unhide_selection(Axis::Col));
    }

    // ── Editing ─────────────────────────────────────────────────────────────

    fn autosum_execute(&mut self) {
        let done = self.cmd(|ed| ed.auto_sum()).flatten();
        if done.is_none() {
            self.grid.with(|g| g.begin_edit(Some("=SUM(")));
        }
    }

    fn function_execute(&mut self) {
        self.grid.with(|g| {
            g.begin_edit(Some("="));
            g.focus();
        });
    }

    fn fb_fx_click(&mut self) {
        self.function_execute();
    }

    fn clear_formats_execute(&mut self) {
        self.cmd(|ed| ed.clear_formats());
    }

    fn clear_contents_execute(&mut self) {
        self.cmd(|ed| ed.clear_selection());
    }

    fn filter_execute(&mut self) {
        self.grid.with(|g| {
            g.with_editor(|ed| {
                if ed.filter_mode || !ed.col_filters.is_empty() {
                    ed.filter_mode = false;
                    ed.col_filters.clear();
                    ed.touch();
                } else {
                    ed.filter_mode = true;
                }
            })
        });
    }

    fn symbol_click(&mut self, e: &TextChangedEventArgs) {
        let s = e.new.clone();
        self.cmd(|ed| ed.insert_symbol(&s));
    }

    fn comment_execute(&mut self) {
        let (at, current) = match self.grid.with(|g| g.editor().map(|e| (e.sel.active, e.cells().get(&e.sel.active.key()).and_then(|c| c.note()).unwrap_or("").to_string()))).flatten() {
            Some(x) => x,
            None => return,
        };
        if let Some(text) = super::prompt_dialog::PromptDialog::ask(self, Resources::cmd_comment(), Resources::cmd_comment(), &current) {
            self.cmd(|ed| ed.set_comment(at, Some(&text)));
        }
    }

    fn comment_del_execute(&mut self) {
        let at = self.grid.with(|g| g.editor().map(|e| e.sel.active)).flatten();
        if let Some(at) = at {
            self.cmd(|ed| ed.set_comment(at, None));
        }
    }

    fn sort_asc_execute(&mut self) {
        self.cmd(|ed| ed.sort_range(true));
    }

    fn sort_desc_execute(&mut self) {
        self.cmd(|ed| ed.sort_range(false));
    }

    fn group_rows_execute(&mut self) {
        self.cmd(|ed| ed.group(Axis::Row));
    }

    fn group_cols_execute(&mut self) {
        self.cmd(|ed| ed.group(Axis::Col));
    }

    fn collapse_execute(&mut self) {
        self.cmd(|ed| {
            ed.collapse_groups(Axis::Row, Some(true))?;
            ed.collapse_groups(Axis::Col, Some(true))
        });
    }

    fn expand_execute(&mut self) {
        self.cmd(|ed| {
            ed.collapse_groups(Axis::Row, Some(false))?;
            ed.collapse_groups(Axis::Col, Some(false))
        });
    }

    fn ungroup_execute(&mut self) {
        self.cmd(|ed| {
            ed.ungroup(Axis::Row)?;
            ed.ungroup(Axis::Col)
        });
    }

    fn dedupe_execute(&mut self) {
        if let Some(n) = self.cmd(|ed| ed.remove_duplicates()) {
            self.note = if n == 0 { Resources::msg_dup_none().to_string() } else { Resources::msg_dup_removed().replace("{n}", &n.to_string()) };
            self.has_note = true;
        }
    }

    // ── Sheet options ───────────────────────────────────────────────────────

    fn gridlines_execute(&mut self) {
        self.cmd_any(|ed| ed.toggle_gridlines());
    }

    fn freeze(&mut self, rows: Option<u32>, cols: Option<u32>) {
        self.cmd_any(|ed| {
            let s = ed.sheet();
            let (r, c) = (rows.unwrap_or(s.frozen_rows()), cols.unwrap_or(s.frozen_cols()));
            ed.set_freeze(r, c)
        });
    }

    fn freeze_none_execute(&mut self) {
        self.freeze(Some(0), Some(0));
    }

    fn freeze_r1_execute(&mut self) {
        self.freeze(Some(1), None);
    }

    fn freeze_r2_execute(&mut self) {
        self.freeze(Some(2), None);
    }

    fn freeze_rsel_execute(&mut self) {
        let r = self.grid.with(|g| g.editor().map(|e| e.sel.active.row)).flatten().unwrap_or(1);
        self.freeze(Some(r), None);
    }

    fn freeze_c1_execute(&mut self) {
        self.freeze(None, Some(1));
    }

    fn freeze_c2_execute(&mut self) {
        self.freeze(None, Some(2));
    }

    fn freeze_csel_execute(&mut self) {
        let c = self.grid.with(|g| g.editor().map(|e| e.sel.active.col + 1)).flatten().unwrap_or(1);
        self.freeze(None, Some(c));
    }

    fn fullscreen_execute(&mut self) {
        self.full_screen = !self.full_screen;
        let max = self.full_screen;
        crate::platform::set_maximized(max);
    }

    // ── The formula bar and the Name Box ────────────────────────────────────

    fn formula_box_text_changed(&mut self, e: &TextChangedEventArgs) {
        if e.source != kubuno_desktop::views::events::ChangeSource::User {
            return;
        }
        self.bar_typing = true;
        let t = e.new.clone();
        self.grid.with(|g| g.bar_text_changed(&t));
    }

    fn formula_box_key_down(&mut self, e: &mut KeyEventArgs) {
        use kubuno_desktop::controls::host::vk;
        match e.key.0 {
            vk::ENTER => {
                self.bar_typing = false;
                let shift = e.mods.shift;
                self.grid.with(|g| {
                    g.commit_edit(Some(if shift { kubuno_office_sheets_core::editor::Dir::Up } else { kubuno_office_sheets_core::editor::Dir::Down }));
                    g.focus();
                });
                e.handled = true;
            }
            vk::ESCAPE => {
                self.fb_cancel_click();
                e.handled = true;
            }
            _ => {}
        }
    }

    fn fb_cancel_click(&mut self) {
        self.bar_typing = false;
        self.grid.with(|g| {
            g.cancel_edit();
            g.focus();
        });
    }

    fn fb_confirm_click(&mut self) {
        self.bar_typing = false;
        self.grid.with(|g| {
            g.commit_edit(None);
            g.focus();
        });
    }

    /// The Name Box: a cell, a range or a defined name (`jumpToRef`).
    fn name_box_key_down(&mut self, e: &mut KeyEventArgs) {
        use kubuno_desktop::controls::host::vk;
        if e.key.0 != vk::ENTER {
            return;
        }
        e.handled = true;
        let raw = self.name_box_text.trim().to_string();
        let ok = self
            .grid
            .with(|g| {
                g.with_editor(|ed| {
                    let mut target = raw.clone();
                    if let Some(def) = ed.wb.names.get(&raw.to_uppercase()).and_then(|v| v.as_str()) {
                        let t = def.trim_start_matches('=');
                        let t = match t.find('!') {
                            Some(i) => &t[i + 1..],
                            None => t,
                        };
                        target = t.replace('$', "");
                    }
                    match kubuno_office_sheets_core::addr::Rect::parse(&target) {
                        Some(r) if r.c2 < MAX_COLS && r.r2 <= MAX_ROWS && r.r1 >= 1 => {
                            if r.c1 == r.c2 && r.r1 == r.r2 {
                                ed.select(Pos::new(r.c1, r.r1));
                            } else {
                                ed.select_rect(Pos::new(r.c1, r.r1), Pos::new(r.c2, r.r2));
                            }
                            true
                        }
                        _ => false,
                    }
                })
            })
            .flatten()
            .unwrap_or(false);
        let ok = ok || self.define_name_from_box(&raw);
        if ok {
            self.grid.with(|g| g.focus());
        }
    }

    // ── Find / replace ──────────────────────────────────────────────────────

    fn find_execute(&mut self) {
        self.show_find = true;
        self.show_replace = false;
        self.find_status.clear();
    }

    fn replace_execute(&mut self) {
        self.show_find = true;
        self.show_replace = true;
        self.find_status.clear();
    }

    fn find_close_click(&mut self) {
        self.show_find = false;
        self.grid.with(|g| g.focus());
    }

    fn find_case_changed(&mut self, e: &CheckedChangedEventArgs) {
        self.find_match_case = e.new;
    }

    fn find_key_down(&mut self, e: &mut KeyEventArgs) {
        use kubuno_desktop::controls::host::vk;
        if e.key.0 == vk::ENTER {
            if e.mods.shift {
                self.find_prev_click()
            } else {
                self.find_next_click()
            }
            e.handled = true;
        } else if e.key.0 == vk::ESCAPE {
            self.find_close_click();
            e.handled = true;
        }
    }

    fn find_step(&mut self, back: bool) {
        let (q, c) = (self.find_text.clone(), self.find_match_case);
        let found = self.grid.with(|g| g.with_editor(|ed| ed.find_next(&q, c, back))).flatten().unwrap_or(false);
        self.find_status = if found { String::new() } else { Resources::find_none().to_string() };
    }

    fn find_next_click(&mut self) {
        self.find_step(false);
    }

    fn find_prev_click(&mut self) {
        self.find_step(true);
    }

    fn find_replace_click(&mut self) {
        let (q, w, c) = (self.find_text.clone(), self.replace_text.clone(), self.find_match_case);
        self.cmd(|ed| ed.replace_one(&q, &w, c));
    }

    fn find_replace_all_click(&mut self) {
        let (q, w, c) = (self.find_text.clone(), self.replace_text.clone(), self.find_match_case);
        if let Some(n) = self.cmd(|ed| ed.replace_all(&q, &w, c)) {
            self.find_status = Resources::find_replaced().replace("{n}", &n.to_string());
        }
    }

    // ── Tabs and zoom ───────────────────────────────────────────────────────

    fn tabs_tab_click(&mut self, e: &SheetTabEventArgs) {
        let i = e.index.max(0) as usize;
        self.grid.with(|g| {
            g.commit_edit(None);
            g.with_editor(|ed| ed.set_active(i));
            g.focus();
        });
    }

    fn tabs_add_click(&mut self, _e: &SheetTabEventArgs) {
        let id = client::fresh_uuid_v4();
        self.grid.with(|g| {
            g.commit_edit(None);
            g.with_editor(|ed| ed.add_sheet(id, None));
            g.focus();
        });
    }

    fn tabs_tab_double_click(&mut self, e: &SheetTabEventArgs) {
        let i = e.index.max(0) as usize;
        let current = self.grid.with(|g| g.editor().and_then(|ed| ed.wb.sheets.get(i).map(|s| s.meta.name.clone()))).flatten().unwrap_or_default();
        if let Some(name) = super::prompt_dialog::PromptDialog::ask(self, Resources::sheet_rename_title(), Resources::sheet_rename_label(), &current) {
            self.grid.with(|g| g.with_editor(|ed| ed.rename_sheet(i, &name)));
        }
    }

    fn tabs_delete_click(&mut self, e: &SheetTabEventArgs) {
        let i = e.index.max(0) as usize;
        // As on the web: no question (`deleteSheetMut` on the trash button).
        self.grid.with(|g| g.with_editor(|ed| ed.delete_sheet(i)));
    }

    fn status_zoom_value_changed(&mut self, e: &NumericValueChangedEventArgs) {
        let z = e.new as f64 / 100.0;
        self.grid.with(|g| g.with_editor(|ed| ed.set_zoom(z)));
    }
}

fn font_rows() -> Rows {
    let mut names: Vec<String> = SHEET_FONTS.iter().map(|s| s.to_string()).collect();
    for f in kubuno_desktop::ui::editors::system_font_families().iter() {
        if !names.iter().any(|n| n.eq_ignore_ascii_case(f)) {
            names.push(f.clone());
        }
    }
    Rows::from(names.into_iter().map(|f| Row::new().with("Text", Value::Str(f))).collect::<Vec<_>>())
}
