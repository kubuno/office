//! Code-behind of Kubuno Documents' window (`document_window.kbview`) — Windows Forms' `Form1.cs`.
//!
//! The page canvas owns the open document (the core's editor) and its view state; the window opens
//! the document into it, binds what it reports (`ViewChanged`: the rulers, the status bar, the
//! zoom; `EditorStateChanged`: the ribbon's checked states, undo/redo, the title's « modifié »),
//! and turns the ribbon's commands, the context menu and the rulers into the canvas's edits.

use std::path::PathBuf;

use kubuno_desktop::prelude::*;
use kubuno_desktop::views::component::Control as _;
use kubuno_office_docs_core::model::Node;
use serde_json::{json, Value as Json};

use crate::controls::page_canvas::{DragGuide, EditorStateEventArgs, PageCanvas, PageViewEventArgs};
use crate::controls::ruler::{RulerGuideEventArgs, RulerIndentsEventArgs, RulerMarginsEventArgs, RulerTabStopsEventArgs};
use crate::model::state::App;
use crate::Resources;

/// How the window starts: desktop/common's launch options (the command line, read by the common app).
pub use kubuno_office_desktop_common::app::Options;

/// Kubuno Documents' main window.
#[kubuno_desktop::view("document_window.kbview")]
pub struct DocumentWindow {
    #[control]
    page: Custom<PageCanvas>,
    #[bind]
    window_title: String,
    #[bind]
    show_ruler: bool,
    #[bind]
    ruler_inset: f32,
    // ── What the page canvas reports, bound to the rulers ──
    #[bind]
    zoom: f32,
    #[bind]
    page_origin: f32,
    #[bind]
    page_top: f32,
    #[bind]
    page_width: f32,
    #[bind]
    page_height: f32,
    #[bind]
    margin_left: f32,
    #[bind]
    margin_right: f32,
    #[bind]
    margin_top: f32,
    #[bind]
    margin_bottom: f32,
    #[bind]
    indent_left: f32,
    #[bind]
    indent_first_line: f32,
    #[bind]
    indent_right: f32,
    #[bind]
    tab_stops: String,
    #[bind]
    tab_type: String,
    // ── The status bar and the Backstage ──
    #[bind]
    page_text: String,
    #[bind]
    hidden_status: String,
    #[bind]
    has_hidden: bool,
    #[bind]
    zoom_percent: f32,
    #[bind]
    doc_title: String,
    #[bind]
    pages_text: String,
    #[bind]
    zoom_text: String,
    #[bind]
    hidden_text: String,
    // ── The ribbon's state, from the selection ──
    #[bind]
    font_families: Rows,
    #[bind]
    font_family: String,
    #[bind]
    font_size: String,
    #[bind]
    style_value: String,
    #[bind]
    bold: bool,
    #[bind]
    italic: bool,
    #[bind]
    underline: bool,
    #[bind]
    strike: bool,
    #[bind]
    subscript: bool,
    #[bind]
    superscript: bool,
    #[bind]
    format_painter: bool,
    #[bind]
    list_bullet: bool,
    #[bind]
    list_ordered: bool,
    #[bind]
    list_task: bool,
    #[bind]
    align_left: bool,
    #[bind]
    align_center: bool,
    #[bind]
    align_right: bool,
    #[bind]
    align_justify: bool,
    #[bind]
    ls1: bool,
    #[bind]
    ls1_15: bool,
    #[bind]
    ls1_5: bool,
    #[bind]
    ls2: bool,
    #[bind]
    ls2_5: bool,
    #[bind]
    ls3: bool,
    #[bind]
    space_before: bool,
    #[bind]
    space_after: bool,
    #[bind]
    first_line: bool,
    #[bind]
    hanging: bool,
    #[bind]
    keep_next: bool,
    #[bind]
    keep_lines: bool,
    #[bind]
    page_break_before: bool,
    #[bind]
    on_link: bool,
    #[bind]
    code_block: bool,
    #[bind]
    show_marks: bool,
    #[bind]
    show_bounds: bool,
    #[bind]
    heading_numbers: bool,
    #[bind]
    tracking: bool,
    #[bind]
    final_view: bool,
    // ── The find bar ──
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
    #[bind]
    page_number_position: String,
    #[bind]
    orientation: String,
    #[bind]
    paper_size: String,
    #[bind]
    columns: String,
    #[bind]
    citation_style: String,
    #[bind]
    can_save: bool,
    #[bind]
    can_undo: bool,
    #[bind]
    can_redo: bool,
    /// The document to open at load (the canvas exists once the window is open).
    pending: Option<App>,
    /// The server document's session (`--doc`), kept in step by a background thread.
    live: Option<crate::api::live::Live>,
    /// The server document to open at load.
    open_doc: Option<String>,
    /// The session is in conflict and the user has not chosen yet (the question was closed).
    in_conflict: bool,
    /// The last edit revision seen; `publish_pending` until its bytes reach the session.
    last_revision: u64,
    #[bind]
    publish_pending: bool,
    /// The editor revision of the bytes last handed to the session.
    published_revision: u64,
    #[bind]
    sync_status: String,
    #[bind]
    has_sync: bool,
    /// Where the document was opened from (a local file), for Ctrl+S.
    file: Option<PathBuf>,
    /// The zoom to open at (`--zoom`).
    open_zoom: Option<f32>,
    /// The title bar's header shows the offline sample (`Options::sample`).
    header_sample: bool,
    /// The last colours applied (the split buttons' main part reapplies them).
    last_color: String,
    last_highlight: String,
}

impl DocumentWindow {
    pub fn new(options: Options, document: App) -> Self {
        let mut window = Self {
            window_title: format!("{} — {}", document.title, Resources::app_title()),
            pending: Some(document),
            file: options.file.clone(),
            open_doc: options.doc.clone(),
            open_zoom: options.zoom,
            header_sample: options.sample,
            show_ruler: true,
            ruler_inset: crate::controls::ruler::RULER_SZ,
            zoom: 1.0,
            page_origin: f32::NAN,
            page_width: kubuno_office_docs_core::editor::PAGE_W,
            page_height: kubuno_office_docs_core::editor::PAGE_H,
            margin_left: kubuno_office_docs_core::editor::MARGIN,
            margin_right: kubuno_office_docs_core::editor::MARGIN,
            margin_top: kubuno_office_docs_core::editor::MARGIN,
            margin_bottom: kubuno_office_docs_core::editor::MARGIN,
            tab_type: "left".to_string(),
            zoom_percent: 100.0,
            font_family: "Arial".to_string(),
            font_size: "11".to_string(),
            style_value: "normal".to_string(),
            align_left: true,
            ls1_15: true,
            page_number_position: "none".to_string(),
            orientation: "portrait".to_string(),
            paper_size: "a4".to_string(),
            columns: "1".to_string(),
            citation_style: "APA".to_string(),
            last_color: "#ff0000".to_string(),
            last_highlight: "#fff475".to_string(),
            ..Self::default()
        };
        window.initialize_component();
        window
    }

    fn document_window_load(&mut self) {
        self.font_families = font_rows();
        if let Some(document) = self.pending.take() {
            self.doc_title = document.title.clone();
            self.hidden_text = Resources::info_nothing_hidden().to_string();
            let zoom = self.open_zoom.map(|z| z / 100.0);
            self.page.with(|p| {
                p.open(document);
                if let Some(z) = zoom {
                    p.set_zoom(z);
                }
                p.focus();
            });
        }
        if let Some(id) = self.open_doc.take() {
            self.start_live(id);
        }
        // The title bar's waffle and avatar: the account's apps, favourites and other accounts, through the
        // shell's broker (the sample: the controls' design data).
        let mut header = kubuno_desktop_header_data::FeedConfig::new("kubuno-documents");
        if !self.header_sample {
            header.proxy = kubuno_desktop_sync::get_proxy();
        }
        kubuno_desktop_header_data::start(kubuno_desktop_header_data::HeaderOptions::for_app(&["office-documents"]), header, self.header_sample, self.dispatcher());
        self.fetch_track_user();
    }

    // ── A document of the server ────────────────────────────────────────────

    /// Opens document `id` of the server: a background session joins it, keeps the crash journal and
    /// saves (vskubuno docs/DOCUMENTS-EDITING.md §D); its events come back on the UI thread.
    fn start_live(&mut self, id: String) {
        let Some(dispatcher) = self.dispatcher() else {
            kubuno_desktop::tracing::warn!("[documents] the window has no dispatcher: the server document cannot open");
            return;
        };
        self.sync_status = Resources::sync_opening().to_string();
        self.has_sync = true;
        let post = dispatcher.clone();
        let sink: crate::api::live::Sink = std::sync::Arc::new(move |event| {
            drop(post.begin_invoke(move |w: &mut DocumentWindow| w.on_live(event)));
        });
        let doc = id.clone();
        self.live = Some(crate::api::live::Live::open(id, sink, move || crate::api::remote::connect(&doc)));
    }

    /// The document's bytes for the session, remembering which edit they are (`published_revision`).
    fn document_bytes(&mut self) -> Option<Vec<u8>> {
        let (bytes, revision) = self.page.with(|p| {
            let editor = &mut p.state_mut().editor;
            (editor.to_bytes().ok(), editor.revision)
        })?;
        self.published_revision = revision;
        bytes
    }

    /// Loads `bytes` into the canvas, titled `title`.
    fn load_bytes(&mut self, bytes: &[u8], title: &str) -> bool {
        match kubuno_office_docs_core::editor::Editor::open(bytes) {
            Ok(editor) => {
                self.doc_title = title.to_string();
                // The document the server sends replaces the placeholder: the zoom the window had
                // (`--zoom`, or the user's) is kept.
                self.page.with(|p| {
                    let zoom = p.state().zoom;
                    p.open(App::new(editor, title.to_string()));
                    p.set_zoom(zoom);
                    p.focus();
                });
                true
            }
            Err(e) => {
                self.sync_status = Resources::open_failed().replace("{why}", &e.to_string());
                false
            }
        }
    }

    fn on_live(&mut self, event: crate::api::live::Event) {
        use crate::api::live::Event;
        use crate::api::session::{Protection, State};
        match event {
            Event::Opened { title, content, recovered, protection, offline } => {
                self.load_bytes(&content, &title);
                if let Some(copy) = recovered {
                    // Closing the question restores too: the local copy is the work nothing else holds, and
                    // the server's copy stays the server's until a save (still digest-guarded).
                    let answer = super::confirm_dialog::ConfirmDialog::choose(self, Resources::recover_title(), Resources::recover_text(), Resources::recover_yes(), Resources::recover_no());
                    let restore = answer != super::confirm_dialog::Answer::Second;
                    if restore && self.load_bytes(&copy, &title) {
                        // Not what the server holds: the title says « modifié » and Ctrl+S is enabled.
                        let revision = self.page.with(|p| {
                            p.state_mut().editor.mark_unsaved();
                            p.invalidate();
                            p.state().editor.revision
                        });
                        if let Some(r) = revision {
                            self.published_revision = r;
                        }
                        // The restored copy is unsaved work: published to the session, saved by it.
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
            }
            Event::Failed(why) => {
                self.sync_status = Resources::open_failed().replace("{why}", &why);
            }
            Event::Status(s) => {
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
                // « Saved » is about the bytes the session was handed: an edit made since (still
                // waiting for `publish_timer`, or typed during the save) keeps the document unsaved.
                if s.state == State::Clean && !s.unsaved && !self.publish_pending {
                    let published = self.published_revision;
                    self.page.with(|p| {
                        if p.state().editor.revision == published {
                            p.state_mut().editor.mark_saved();
                            p.invalidate();
                        }
                    });
                }
            }
            Event::Conflict => {
                self.in_conflict = true;
                self.ask_conflict();
            }
            Event::Reload(content) => {
                let title = self.doc_title.clone();
                self.load_bytes(&content, &title);
            }
            Event::Closed => {}
        }
    }

    /// The server copy moved under ours: only the user decides. Closing the question decides nothing (the
    /// session stays in conflict, saves nothing, keeps the journal; Ctrl+S asks again) — « take theirs »
    /// discards the local work, so it is never what a dismissal picks.
    fn ask_conflict(&mut self) {
        use super::confirm_dialog::{Answer, ConfirmDialog};
        use crate::api::session::Resolution;
        let choice = match ConfirmDialog::choose(self, Resources::conflict_title(), Resources::conflict_text(), Resources::conflict_mine(), Resources::conflict_theirs()) {
            Answer::First => Resolution::KeepMine,
            Answer::Second => Resolution::TakeTheirs,
            Answer::Dismissed => return,
        };
        self.in_conflict = false;
        if let Some(live) = &self.live {
            live.resolve(choice);
        }
    }

    /// `publish_timer`: hands the edited document to the session (its journal and its next save).
    fn publish_tick(&mut self) {
        if !std::mem::take(&mut self.publish_pending) {
            return;
        }
        if let Some(bytes) = self.document_bytes() {
            if let Some(live) = &self.live {
                live.edited(bytes, false);
            }
        }
    }

    /// The window closes: the session saves what it can, keeps the rest in the journal and leaves.
    fn document_window_closing(&mut self) {
        if let Some(live) = self.live.take() {
            let bytes = self.document_bytes();
            if !live.close(bytes) {
                kubuno_desktop::tracing::warn!("[documents] the server session did not finish closing in time; the local copy is kept");
            }
        }
    }

    // ── The page canvas ─────────────────────────────────────────────────────

    fn page_view_changed(&mut self, e: &PageViewEventArgs) {
        self.zoom = e.zoom;
        self.page_origin = e.origin_x;
        self.page_top = e.page_top;
        self.page_width = e.page_width;
        self.page_height = e.page_height;
        self.margin_left = e.margin_left;
        self.margin_right = e.margin_right;
        self.margin_top = e.margin_top;
        self.margin_bottom = e.margin_bottom;
        self.indent_left = e.indent_left;
        self.indent_first_line = e.indent_first_line;
        self.indent_right = e.indent_right;
        self.tab_stops = e.tab_stops.clone();
        let percent = (e.zoom * 100.0).round();
        self.zoom_percent = percent;
        self.zoom_text = format!("{percent} %");
        self.pages_text = e.page_count.max(1).to_string();
        self.page_text = Resources::status_page().replace("{current}", &e.current_page.to_string()).replace("{pages}", &e.page_count.max(1).to_string());
    }

    fn page_editor_state_changed(&mut self, e: &EditorStateEventArgs) {
        self.bold = e.bold;
        self.italic = e.italic;
        self.underline = e.underline;
        self.strike = e.strike;
        self.subscript = e.subscript;
        self.superscript = e.superscript;
        self.font_family = e.font_family.clone();
        self.font_size = e.font_size.clone();
        self.style_value = e.style.clone();
        self.list_bullet = e.list == "bulletList";
        self.list_ordered = e.list == "orderedList";
        self.list_task = e.list == "taskList";
        self.align_left = e.align == "left";
        self.align_center = e.align == "center";
        self.align_right = e.align == "right";
        self.align_justify = e.align == "justify";
        let ls = |v: f32| (e.line_height - v).abs() < 0.001;
        (self.ls1, self.ls1_15, self.ls1_5, self.ls2, self.ls2_5, self.ls3) = (ls(1.0), ls(1.15), ls(1.5), ls(2.0), ls(2.5), ls(3.0));
        self.space_before = e.space_before;
        self.space_after = e.space_after;
        self.first_line = e.first_line;
        self.hanging = e.hanging;
        self.keep_next = e.keep_next;
        self.keep_lines = e.keep_lines;
        self.page_break_before = e.page_break_before;
        self.code_block = e.code_block;
        self.on_link = !e.link.is_empty();
        self.can_undo = e.can_undo;
        self.can_redo = e.can_redo;
        self.can_save = e.dirty;
        self.heading_numbers = e.heading_numbers;
        self.tracking = e.tracking;
        self.final_view = e.final_view;
        self.orientation = e.orientation.clone();
        self.paper_size = e.paper_size.clone();
        self.columns = e.columns.clone();
        self.page_number_position = e.page_numbers.clone();
        // The session gets the document's bytes at most once a second (`publish_timer`), not at every
        // keystroke: serialising a large document per character made typing lag behind the keyboard.
        if e.dirty && e.revision != self.last_revision && self.live.is_some() {
            self.last_revision = e.revision;
            self.publish_pending = true;
        }
        let title = if e.dirty { Resources::title_modified().replace("{title}", &self.doc_title) } else { self.doc_title.clone() };
        self.window_title = format!("{title} — {}", Resources::app_title());
        self.has_hidden = !e.message.is_empty();
        self.hidden_status = e.message.clone();
    }

    // ── File ────────────────────────────────────────────────────────────────

    fn save_execute(&mut self) {
        // A local file is written back in place (envelope and sibling keys kept). A document opened
        // from the server saves through its session (vskubuno docs/DOCUMENTS-EDITING.md §D).
        if self.live.is_some() {
            if self.in_conflict {
                self.ask_conflict();
                return;
            }
            if let Some(bytes) = self.document_bytes() {
                if let Some(live) = &self.live {
                    live.save_now(bytes);
                }
            }
            return;
        }
        let Some(path) = self.file.clone() else { return };
        let result: Result<(), String> = match self.page.with(|p| p.state_mut().editor.to_bytes()) {
            Some(Ok(bytes)) => std::fs::write(&path, bytes).map_err(|e| e.to_string()),
            Some(Err(e)) => Err(e.to_string()),
            None => Err("no document".to_string()),
        };
        match result {
            Ok(()) => {
                self.page.with(|p| {
                    p.state_mut().editor.mark_saved();
                    p.invalidate();
                });
            }
            Err(why) => {
                kubuno_desktop::tracing::error!("[documents] save {}: {why}", path.display());
                self.hidden_status = Resources::msg_save_failed().replace("{why}", &why);
                self.has_hidden = true;
            }
        }
    }

    fn bs_close_click(&mut self) {
        self.close();
    }

    // ── Clipboard and history ───────────────────────────────────────────────

    fn undo_execute(&mut self) {
        self.page.with(|p| p.undo());
    }

    fn redo_execute(&mut self) {
        self.page.with(|p| p.redo());
    }

    fn cut_execute(&mut self) {
        self.page.with(|p| p.cut());
    }

    fn copy_execute(&mut self) {
        self.page.with(|p| {
            p.copy();
        });
    }

    fn paste_execute(&mut self) {
        self.page.with(|p| p.paste(false));
    }

    fn paste_plain_execute(&mut self) {
        self.page.with(|p| p.paste(true));
    }

    fn select_all_execute(&mut self) {
        self.page.with(|p| p.select_all());
    }

    fn fmtpainter_execute(&mut self) {
        self.page.with(|p| p.toggle_format_painter());
    }

    fn clear_execute(&mut self) {
        self.page.with(|p| p.clear_formatting());
    }

    // ── Font ────────────────────────────────────────────────────────────────

    fn bold_execute(&mut self) {
        self.page.with(|p| p.toggle_mark("bold"));
    }

    fn italic_execute(&mut self) {
        self.page.with(|p| p.toggle_mark("italic"));
    }

    fn underline_execute(&mut self) {
        self.page.with(|p| p.toggle_mark("underline"));
    }

    fn strike_execute(&mut self) {
        self.page.with(|p| p.toggle_mark("strike"));
    }

    fn subscript_execute(&mut self) {
        self.page.with(|p| p.toggle_mark("subscript"));
    }

    fn superscript_execute(&mut self) {
        self.page.with(|p| p.toggle_mark("superscript"));
    }

    fn font_family_changed(&mut self, e: &TextChangedEventArgs) {
        let f = e.new.trim().to_string();
        if !f.is_empty() {
            self.page.with(|p| p.set_text_style("fontFamily", Some(f)));
        }
    }

    fn font_size_changed(&mut self, e: &TextChangedEventArgs) {
        let s = e.new.clone();
        self.page.with(|p| p.set_font_size(&s));
    }

    fn grow_execute(&mut self) {
        self.page.with(|p| p.grow_font(1.0));
    }

    fn shrink_execute(&mut self) {
        self.page.with(|p| p.grow_font(-1.0));
    }

    fn case_upper_execute(&mut self) {
        self.page.with(|p| p.change_case("upper"));
    }

    fn case_lower_execute(&mut self) {
        self.page.with(|p| p.change_case("lower"));
    }

    fn case_title_execute(&mut self) {
        self.page.with(|p| p.change_case("title"));
    }

    fn case_sentence_execute(&mut self) {
        self.page.with(|p| p.change_case("sentence"));
    }

    fn case_toggle_execute(&mut self) {
        self.page.with(|p| p.change_case("toggle"));
    }

    fn case_smallcaps_execute(&mut self) {
        self.page.with(|p| p.toggle_small_caps());
    }

    fn apply_color(&mut self, color: Option<&str>) {
        if let Some(c) = color {
            self.last_color = c.to_string();
        }
        let c = color.map(str::to_string);
        self.page.with(|p| p.set_text_style("color", c));
    }

    fn color_execute(&mut self) {
        let c = self.last_color.clone();
        self.apply_color(Some(&c));
    }

    fn color_auto_execute(&mut self) {
        self.apply_color(None);
    }

    fn color_red_execute(&mut self) {
        self.apply_color(Some("#ff0000"));
    }

    fn color_blue_execute(&mut self) {
        self.apply_color(Some("#0070c0"));
    }

    fn apply_highlight(&mut self, color: Option<&str>) {
        if let Some(c) = color {
            self.last_highlight = c.to_string();
        }
        let c = color.map(str::to_string);
        self.page.with(|p| p.set_highlight(c.as_deref()));
    }

    fn highlight_execute(&mut self) {
        let c = self.last_highlight.clone();
        self.apply_highlight(Some(&c));
    }

    fn hl_yellow_execute(&mut self) {
        self.apply_highlight(Some("#fff475"));
    }

    fn hl_green_execute(&mut self) {
        self.apply_highlight(Some("#ccff90"));
    }

    fn hl_cyan_execute(&mut self) {
        self.apply_highlight(Some("#a7ffeb"));
    }

    fn hl_none_execute(&mut self) {
        self.apply_highlight(None);
    }

    // ── Paragraph ───────────────────────────────────────────────────────────

    fn ul_execute(&mut self) {
        self.page.with(|p| p.toggle_list("bulletList"));
    }

    fn ol_execute(&mut self) {
        self.page.with(|p| p.toggle_list("orderedList"));
    }

    fn task_execute(&mut self) {
        self.page.with(|p| p.toggle_list("taskList"));
    }

    fn al_left_execute(&mut self) {
        self.page.with(|p| p.set_align("left"));
    }

    fn al_center_execute(&mut self) {
        self.page.with(|p| p.set_align("center"));
    }

    fn al_right_execute(&mut self) {
        self.page.with(|p| p.set_align("right"));
    }

    fn al_justify_execute(&mut self) {
        self.page.with(|p| p.set_align("justify"));
    }

    fn ind_dec_execute(&mut self) {
        self.page.with(|p| p.indent(-1));
    }

    fn ind_inc_execute(&mut self) {
        self.page.with(|p| p.indent(1));
    }

    fn line_height(&mut self, v: f64) {
        self.page.with(|p| p.set_paragraph_attr("lineHeight", json!(v)));
    }

    fn ls1_execute(&mut self) {
        self.line_height(1.0);
    }

    fn ls1_15_execute(&mut self) {
        self.line_height(1.15);
    }

    fn ls1_5_execute(&mut self) {
        self.line_height(1.5);
    }

    fn ls2_execute(&mut self) {
        self.line_height(2.0);
    }

    fn ls2_5_execute(&mut self) {
        self.line_height(2.5);
    }

    fn ls3_execute(&mut self) {
        self.line_height(3.0);
    }

    fn sp_before_execute(&mut self) {
        let v = if self.space_before { 0 } else { 12 };
        self.page.with(|p| p.set_paragraph_attr("spaceBefore", json!(v)));
    }

    fn sp_after_execute(&mut self) {
        let v = if self.space_after { 0 } else { 12 };
        self.page.with(|p| p.set_paragraph_attr("spaceAfter", json!(v)));
    }

    fn firstline_execute(&mut self) {
        let v = if self.first_line { 0 } else { 36 };
        self.page.with(|p| p.set_paragraph_attr("indentFirstLine", json!(v)));
    }

    fn hanging_execute(&mut self) {
        let on = self.hanging;
        let left = self.indent_left;
        self.page.with(|p| p.set_hanging(!on, left));
    }

    fn toggle_flag(&mut self, key: &str, on: bool) {
        let v = if on { Json::Null } else { Json::Bool(true) };
        let k = key.to_string();
        self.page.with(|p| p.set_paragraph_attr(&k, v));
    }

    fn flow_keepnext_execute(&mut self) {
        let on = self.keep_next;
        self.toggle_flag("keepNext", on);
    }

    fn flow_keeplines_execute(&mut self) {
        let on = self.keep_lines;
        self.toggle_flag("keepLines", on);
    }

    fn flow_pagebreak_execute(&mut self) {
        let on = self.page_break_before;
        self.toggle_flag("pageBreakBefore", on);
    }

    fn style_gallery_item_click(&mut self, e: &TextChangedEventArgs) {
        let id = e.new.clone();
        self.page.with(|p| p.apply_style(&id));
    }

    fn code_execute(&mut self) {
        let on = self.code_block;
        self.page.with(|p| p.toggle_code_block(on));
    }

    // ── Insert ──────────────────────────────────────────────────────────────

    fn pb_execute(&mut self) {
        self.page.with(|p| p.insert_page_break());
    }

    fn hr_execute(&mut self) {
        self.page.with(|p| p.insert_blocks(vec![Node::of_type("horizontalRule")]));
    }

    fn table_insert_execute(&mut self) {
        self.page.with(|p| p.insert_blocks(vec![table_node(3, 3)]));
    }

    fn img_execute(&mut self) {
        let Some(path) = crate::platform::file_dialog::open_image() else { return };
        match std::fs::read(&path) {
            Ok(bytes) => {
                self.page.with(|p| p.insert_image_bytes(&bytes));
            }
            Err(e) => kubuno_desktop::tracing::warn!("[documents] {}: {e}", path.display()),
        }
    }

    fn insert_symbol(&mut self, s: &str) {
        let t = s.to_string();
        self.page.with(|p| p.insert_text(&t));
    }

    fn sym_emdash_execute(&mut self) {
        self.insert_symbol("—");
    }
    fn sym_endash_execute(&mut self) {
        self.insert_symbol("–");
    }
    fn sym_ellipsis_execute(&mut self) {
        self.insert_symbol("…");
    }
    fn sym_nbsp_execute(&mut self) {
        self.insert_symbol("\u{00A0}");
    }
    fn sym_guillemets_execute(&mut self) {
        self.insert_symbol("«\u{00A0}\u{00A0}»");
    }
    fn sym_euro_execute(&mut self) {
        self.insert_symbol("€");
    }
    fn sym_tm_execute(&mut self) {
        self.insert_symbol("™");
    }
    fn sym_copyright_execute(&mut self) {
        self.insert_symbol("©");
    }
    fn sym_registered_execute(&mut self) {
        self.insert_symbol("®");
    }
    fn sym_section_execute(&mut self) {
        self.insert_symbol("§");
    }
    fn sym_bullet_execute(&mut self) {
        self.insert_symbol("•");
    }
    fn sym_degree_execute(&mut self) {
        self.insert_symbol("°");
    }
    fn sym_times_execute(&mut self) {
        self.insert_symbol("×");
    }
    fn sym_divide_execute(&mut self) {
        self.insert_symbol("÷");
    }
    fn sym_plusminus_execute(&mut self) {
        self.insert_symbol("±");
    }
    fn sym_arrow_execute(&mut self) {
        self.insert_symbol("→");
    }
    fn sym_notequal_execute(&mut self) {
        self.insert_symbol("≠");
    }
    fn sym_half_execute(&mut self) {
        self.insert_symbol("½");
    }
    fn sym_approx_execute(&mut self) {
        self.insert_symbol("≈");
    }
    fn sym_infinity_execute(&mut self) {
        self.insert_symbol("∞");
    }
    fn sym_sqrt_execute(&mut self) {
        self.insert_symbol("√");
    }
    fn sym_sum_execute(&mut self) {
        self.insert_symbol("∑");
    }
    fn sym_pi_execute(&mut self) {
        self.insert_symbol("π");
    }
    fn sym_delta_execute(&mut self) {
        self.insert_symbol("Δ");
    }

    fn link_execute(&mut self) {
        self.later(Self::edit_link);
    }

    fn edit_link(&mut self) {
        let current = self.page.with(|p| p.current_link()).unwrap_or_default();
        if let Some(url) = crate::views::link_dialog::LinkDialog::ask(self, &current) {
            self.page.with(|p| p.set_link((!url.is_empty()).then_some(url)));
        }
    }

    fn link_rm_execute(&mut self) {
        self.page.with(|p| p.set_link(None));
    }

    fn open_find(&mut self, replace: bool) {
        let selected = self.page.with(|p| p.selected_line()).unwrap_or_default();
        if !selected.is_empty() {
            self.find_text = selected;
        }
        self.show_find = true;
        self.show_replace = replace;
        self.find_status.clear();
    }

    fn find_execute(&mut self) {
        self.open_find(false);
    }

    fn replace_execute(&mut self) {
        self.open_find(true);
    }

    fn show_found(&mut self, (index, total): (usize, usize)) {
        self.find_status = if total == 0 { Resources::find_none().to_string() } else { Resources::find_count().replace("{index}", &index.to_string()).replace("{total}", &total.to_string()) };
    }

    fn find_next_click(&mut self) {
        let (q, c) = (self.find_text.clone(), self.find_match_case);
        if q.is_empty() {
            return;
        }
        let r = self.page.with(|p| p.find(&q, c, false)).unwrap_or((0, 0));
        self.show_found(r);
    }

    fn find_prev_click(&mut self) {
        let (q, c) = (self.find_text.clone(), self.find_match_case);
        if q.is_empty() {
            return;
        }
        let r = self.page.with(|p| p.find(&q, c, true)).unwrap_or((0, 0));
        self.show_found(r);
    }

    fn find_text_changed(&mut self) {
        self.find_status.clear();
    }

    fn find_key_down(&mut self, e: &mut KeyEventArgs) {
        use kubuno_desktop::controls::host::vk;
        if e.key.0 == vk::ENTER {
            if e.mods.shift { self.find_prev_click() } else { self.find_next_click() }
            e.handled = true;
        } else if e.key.0 == vk::ESCAPE {
            self.find_close_click();
            e.handled = true;
        }
    }

    fn find_case_changed(&mut self, e: &CheckedChangedEventArgs) {
        self.find_match_case = e.new;
        self.find_status.clear();
    }

    fn find_replace_click(&mut self) {
        let (q, w, c) = (self.find_text.clone(), self.replace_text.clone(), self.find_match_case);
        if q.is_empty() {
            return;
        }
        let r = self.page.with(|p| p.replace(&q, &w, c)).unwrap_or((0, 0));
        self.show_found(r);
    }

    fn find_replace_all_click(&mut self) {
        let (q, w, c) = (self.find_text.clone(), self.replace_text.clone(), self.find_match_case);
        if q.is_empty() {
            return;
        }
        let n = self.page.with(|p| p.replace_all(&q, &w, c)).unwrap_or(0);
        self.find_status = Resources::find_replaced().replace("{count}", &n.to_string());
    }

    fn find_close_click(&mut self) {
        self.show_find = false;
        self.page.with(|p| {
            p.focus();
        });
    }

    fn wordcount_execute(&mut self) {
        self.page.with(|p| p.show_word_count());
    }

    // ── Layout ──────────────────────────────────────────────────────────────

    fn margins(&mut self, top: f32, right: f32, bottom: f32, left: f32) {
        self.page.with(|p| p.set_page_margins(left, right, top, bottom, true));
    }

    /// A page-setup change (`scheduleSave`'s short debounce on the web): handed to the session at
    /// once, as a structural edit.
    fn page_setup(&mut self, f: impl FnOnce(&mut kubuno_office_docs_core::editor::Editor) -> Result<(), kubuno_office_docs_core::edit::step::EditError>) {
        let changed = self.page.with(|p| p.page_setup(f)).unwrap_or(false);
        if changed && self.live.is_some() {
            if let Some(bytes) = self.document_bytes() {
                if let Some(live) = &self.live {
                    live.edited(bytes, true);
                }
            }
        }
    }

    fn orientation_changed(&mut self, e: &TextChangedEventArgs) {
        let o = e.new.clone();
        self.page_setup(move |ed| ed.set_orientation(&o, crate::controls::page_canvas::now()));
    }

    fn paper_size_changed(&mut self, e: &TextChangedEventArgs) {
        let p = e.new.clone();
        self.page_setup(move |ed| ed.set_paper_size(&p));
    }

    fn columns_changed(&mut self, e: &TextChangedEventArgs) {
        let Ok(n) = e.new.trim().parse::<usize>() else { return };
        self.page_setup(move |ed| ed.set_columns(n));
    }

    // ── Paragraph shading and border, outline level, heading numbers, bounds ─

    /// `onShading` / `onBorder` of the « Trame et bordure » popover, on the selected paragraphs
    /// (`null` removes). The web offers a palette; these are its default shading, its light grey
    /// swatch and its default border.
    fn para_attr(&mut self, key: &str, value: Json) {
        let k = key.to_string();
        self.page.with(|p| p.set_paragraph_attr(&k, value));
    }

    fn parashade_execute(&mut self) {
        self.para_attr("shading", json!("#fff2cc"));
    }

    fn shade_none_execute(&mut self) {
        self.para_attr("shading", Json::Null);
    }

    fn shade_grey_execute(&mut self) {
        self.para_attr("shading", json!("#efefef"));
    }

    fn border_box_execute(&mut self) {
        self.para_attr("paraBorder", json!({ "color": "#9aa0a6", "width": 1, "style": "solid" }));
    }

    fn border_none_execute(&mut self) {
        self.para_attr("paraBorder", Json::Null);
    }

    /// Références › Ajouter le texte (`setOutlineLevel`): the explicit outline level of the
    /// selected paragraphs, 0 = body text (kept explicit, so a heading leaves the table).
    fn outline_level(&mut self, level: u64) {
        self.para_attr("outlineLevel", json!(level));
    }

    fn ol_0_execute(&mut self) {
        self.outline_level(0);
    }

    fn ol_1_execute(&mut self) {
        self.outline_level(1);
    }

    fn ol_2_execute(&mut self) {
        self.outline_level(2);
    }

    fn ol_3_execute(&mut self) {
        self.outline_level(3);
    }

    /// « Numérotation des titres » (`handleToggleHeadingNumbers`): written only when on, as
    /// `serializeDoc` does (`headingNumbers || undefined`).
    fn hnum_execute(&mut self) {
        let on = self.page.with(|p| p.state().editor.envelope_value("headingNumbers").and_then(|v| v.as_bool()).unwrap_or(false)).unwrap_or(false);
        self.envelope("headingNumbers", if on { None } else { Some(json!(true)) });
    }

    fn bounds_execute(&mut self) {
        let on = self.show_bounds;
        self.page.with(|p| p.set_show_bounds(on));
    }

    // ── Révision › Suivi (`documents/track-changes.ts`, `documents/ReviewPane.tsx`) ──

    /// The signed-in account as the author of tracked changes (`setTrackChangesUser`: its display
    /// name, else its e-mail, else « Anonyme »), asked off the UI thread (the broker may be slow).
    fn fetch_track_user(&mut self) {
        if self.header_sample {
            return;
        }
        let Some(d) = self.dispatcher() else { return };
        std::thread::spawn(move || {
            let Ok(Some(a)) = kubuno_desktop_sync::current_account() else { return };
            let pick = |v: Option<&str>| v.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
            let mut name = pick(a.display_name.as_deref()).or_else(|| pick(a.email.as_deref()));
            if name.is_none() {
                // An account stored without its profile: the web's `authUser` (`/auth/me`).
                let mut api = kubuno_desktop_sync::account_api(&a);
                if let Ok(r) = api.request("GET", "/api/v1/auth/me", &[], None) {
                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&r.body) {
                        let u = v.get("user").unwrap_or(&v);
                        let field = |k: &str| pick(u.get(k).and_then(serde_json::Value::as_str));
                        name = field("display_name").or_else(|| field("username")).or_else(|| field("email"));
                    }
                }
            }
            let id = a.user_id;
            drop(d.begin_invoke(move |w: &mut DocumentWindow| {
                let name = name.unwrap_or_else(|| Resources::review_anonymous().to_string());
                w.page.with(|p| p.set_track_user(kubuno_office_docs_core::edit::track::TrackUser { id, name }));
            }));
        });
    }

    /// « Suivi des modifications »: a document setting, saved in the envelope.
    fn trackchanges_execute(&mut self) {
        let on = !self.page.with(|p| p.state().editor.tracking()).unwrap_or(false);
        self.page_setup(move |ed| ed.set_tracking(on));
    }

    /// « Afficher le document final »: a view setting.
    fn revfinal_execute(&mut self) {
        self.page.with(|p| {
            let on = !p.state().editor.final_view();
            p.set_final_view(on);
        });
    }

    /// « Volet Vérifications »: walks the revisions. The dialog shows one; each action closes it, is
    /// applied here, and the dialog opens again on the revision now current.
    fn reviewpane_execute(&mut self) {
        self.later(Self::review_changes);
    }

    /// Runs `f` once the current frame is done. A ribbon command runs inside the window's frame, and a
    /// modal dialog opened there lays out against a half-built surface (its first control and its action
    /// bar missing): every dialog a command opens goes through here.
    fn later(&mut self, f: fn(&mut DocumentWindow)) {
        match self.dispatcher() {
            Some(d) => drop(d.begin_invoke(move |w: &mut DocumentWindow| f(w))),
            None => f(self),
        }
    }

    fn review_changes(&mut self) {
        use super::confirm_dialog::{Answer, ConfirmDialog};
        use super::review_dialog::{ReviewAction, ReviewDialog};
        // Start on the first revision at or after the caret.
        let Some(mut index) = self.page.with(|p| {
            let e = &p.state().editor;
            let here = e.selection().from();
            e.changes().iter().position(|c| c.to >= here).unwrap_or(0)
        }) else {
            return;
        };
        loop {
            let changes = self.page.with(|p| p.state().editor.changes()).unwrap_or_default();
            if index >= changes.len() {
                index = changes.len().saturating_sub(1);
            }
            let current = changes.get(index).cloned();
            if let Some(c) = &current {
                let (from, to) = (c.from, c.to);
                self.page.with(|p| p.select_range(from, to));
            }
            let view = review_view(&changes, index, epoch_ms());
            let n = changes.len().max(1);
            match ReviewDialog::ask(self, &view) {
                ReviewAction::Close => break,
                ReviewAction::Prev => index = (index + n - 1) % n,
                ReviewAction::Next => index = (index + 1) % n,
                action @ (ReviewAction::Accept | ReviewAction::Reject) => {
                    if let Some(c) = current {
                        let accept = action == ReviewAction::Accept;
                        self.page.with(|p| p.decide_changes(Some(vec![c.id]), accept));
                    }
                }
                ReviewAction::AcceptAll => {
                    self.page.with(|p| p.decide_changes(None, true));
                }
                ReviewAction::RejectAll => {
                    if ConfirmDialog::choose(self, Resources::review_reject_all_title(), Resources::review_reject_all_msg(), Resources::review_reject_all(), Resources::cancel()) == Answer::First {
                        self.page.with(|p| p.decide_changes(None, false));
                    }
                }
            }
        }
    }

    // ── Text tools, sorting, conversions and small insertions ───────────────

    fn tt_empties_execute(&mut self) {
        self.page.with(|p| p.run(kubuno_office_docs_core::edit::texttools::remove_empty_paragraphs));
    }

    fn tt_spaces_execute(&mut self) {
        use kubuno_office_docs_core::edit::texttools as tt;
        self.page.with(|p| p.run(|s| tt::transform_text(s, &tt::collapse_spaces)));
    }

    fn tt_tabs_execute(&mut self) {
        use kubuno_office_docs_core::edit::texttools as tt;
        self.page.with(|p| p.run(|s| tt::transform_text(s, &tt::tabs_to_spaces)));
    }

    fn tt_quotes_execute(&mut self) {
        use kubuno_office_docs_core::edit::texttools as tt;
        self.page.with(|p| p.run(|s| tt::transform_text(s, &tt::smart_quotes)));
    }

    fn tt_number_execute(&mut self) {
        self.page.with(|p| p.run(kubuno_office_docs_core::edit::texttools::number_paragraphs));
    }

    fn tt_reverse_execute(&mut self) {
        self.page.with(|p| p.run(kubuno_office_docs_core::edit::texttools::reverse_paragraphs));
    }

    fn tt_dedupe_execute(&mut self) {
        self.page.with(|p| p.run(kubuno_office_docs_core::edit::texttools::dedupe_paragraphs));
    }

    /// `handleInsertTitle`: the document's title as text at the caret.
    fn tt_title_execute(&mut self) {
        let title = self.doc_title.clone();
        if !title.is_empty() {
            self.page.with(|p| p.run(|s| kubuno_office_docs_core::edit::commands::insert_text(s, &title)));
        }
    }

    /// `handleClearAllFormatting`: select all, then remove every mark and block type.
    fn tt_clear_execute(&mut self) {
        self.page.with(|p| {
            p.select_all();
            p.run(kubuno_office_docs_core::edit::commands::clear_formatting)
        });
    }

    fn sortasc_execute(&mut self) {
        self.page.with(|p| p.run(|s| kubuno_office_docs_core::edit::texttools::sort_paragraphs(s, true)));
    }

    fn sortdesc_execute(&mut self) {
        self.page.with(|p| p.run(|s| kubuno_office_docs_core::edit::texttools::sort_paragraphs(s, false)));
    }

    fn txt2tbl_execute(&mut self) {
        self.page.with(|p| p.run(kubuno_office_docs_core::edit::texttools::text_to_table));
    }

    fn tbl2txt_execute(&mut self) {
        self.page.with(|p| p.run(kubuno_office_docs_core::edit::texttools::table_to_text));
    }

    fn cover(&mut self, variant: u8) {
        use kubuno_office_docs_core::edit::texttools as tt;
        let title = if self.doc_title.is_empty() { Resources::cover_title().to_string() } else { self.doc_title.clone() };
        let blocks = tt::cover_blocks(variant, &title, Resources::cover_subtitle(), Resources::cover_author(), &crate::platform::today_long(), &crate::platform::today());
        self.page.with(|p| p.run(|s| tt::insert_at_start(s, blocks)));
    }

    fn cover1_execute(&mut self) {
        self.cover(1);
    }

    fn cover2_execute(&mut self) {
        self.cover(2);
    }

    fn ins_caption_execute(&mut self) {
        use kubuno_office_docs_core::edit::{commands as cmd, texttools as tt};
        self.page.with(|p| {
            let blocks = tt::caption_blocks(p.state().editor.doc());
            p.run(|s| cmd::insert_blocks(s, blocks))
        });
    }

    fn sign_execute(&mut self) {
        use kubuno_office_docs_core::edit::{commands as cmd, texttools as tt};
        let blocks = tt::signature_blocks(Resources::sig_label());
        self.page.with(|p| p.run(|s| cmd::insert_blocks(s, blocks)));
    }

    /// `handleInsertPageXofY`: « Page 3 sur 12 » as plain text (the page of the caret, the count now).
    fn pagexy_execute(&mut self) {
        let text = self.page_text.clone();
        let text = if text.is_empty() { Resources::page_x_of_y().replace("{x}", "1").replace("{y}", "1") } else { text };
        self.page.with(|p| p.run(|s| kubuno_office_docs_core::edit::commands::insert_text(s, &text)));
    }

    fn insert_now(&mut self, text: String) {
        self.page.with(|p| p.run(|s| kubuno_office_docs_core::edit::commands::insert_text(s, &text)));
    }

    /// `nowFieldText`: static text, not a field (as on the web).
    fn fld_date_execute(&mut self) {
        self.insert_now(crate::platform::today_long());
    }

    fn fld_time_execute(&mut self) {
        self.insert_now(crate::platform::time_now());
    }

    fn fld_datetime_execute(&mut self) {
        self.insert_now(format!("{} {}", crate::platform::today(), crate::platform::time_now()));
    }

    /// `handleEmailLink`: a `mailto:` link on the selection.
    fn link_mail_execute(&mut self) {
        self.later(Self::ask_mail_link);
    }

    fn ask_mail_link(&mut self) {
        if let Some(addr) = super::prompt_dialog::PromptDialog::ask(self, Resources::email_title(), Resources::email_hint(), Resources::prompt_apply()) {
            let href = format!("mailto:{addr}");
            self.page.with(|p| p.run(|s| kubuno_office_docs_core::edit::commands::set_link(s, Some(&href))));
        }
    }

    /// `handleInsertBookmark`.
    fn bookmark_execute(&mut self) {
        self.later(Self::ask_bookmark);
    }

    fn ask_bookmark(&mut self) {
        if let Some(name) = super::prompt_dialog::PromptDialog::ask(self, Resources::bookmark_title(), Resources::bookmark_hint(), Resources::prompt_insert()) {
            self.page.with(|p| p.run(|s| kubuno_office_docs_core::edit::commands::set_bookmark(s, &name)));
        }
    }

    // ── Table of contents ───────────────────────────────────────────────────

    /// The gallery's automatic tables and Insertion › Table des matières (the web opens its options
    /// dialog there; the desktop inserts the default table: three levels, numbers after dots).
    fn toc_auto_execute(&mut self) {
        let now = crate::controls::page_canvas::now();
        self.page.with(|p| p.measured(|e, m| e.generate_toc(&Default::default(), Resources::toc_title(), Resources::toc_empty(), now, m)));
    }

    /// « Mettre à jour la table »: the table the document has, rebuilt with its own settings.
    fn toc_update_execute(&mut self) {
        let now = crate::controls::page_canvas::now();
        self.page.with(|p| {
            p.measured(|e, m| {
                let s = e.detect_toc_settings();
                e.generate_toc(&s, Resources::toc_title(), Resources::toc_empty(), now, m)
            })
        });
    }

    fn toc_manual_execute(&mut self) {
        let now = crate::controls::page_canvas::now();
        let entries = [Resources::toc_manual_1(), Resources::toc_manual_2(), Resources::toc_manual_3()];
        self.page.with(|p| p.page_setup(|e| e.manual_toc(Resources::toc_title(), entries, now)));
    }

    fn toc_remove_execute(&mut self) {
        self.page.with(|p| p.run(kubuno_office_docs_core::edit::toc::remove));
    }

    // ── Notes ───────────────────────────────────────────────────────────────

    /// Inserts an empty note at the caret and opens its editor (`handleInsertFootnote`,
    /// `handleInsertEndnote`).
    fn insert_note(&mut self, kind: &'static str) {
        use kubuno_office_docs_core::edit::commands as cmd;
        let pos = self.page.with(|p| p.run(|s| cmd::insert_note(s, kind)).then(|| p.state().editor.selection().head.saturating_sub(1))).flatten();
        if let Some(pos) = pos {
            match self.dispatcher() {
                Some(d) => drop(d.begin_invoke(move |w: &mut DocumentWindow| w.edit_note(pos))),
                None => self.edit_note(pos),
            }
        }
    }

    /// The note editor on the note at `pos`: OK writes its text, « Supprimer la note » deletes it.
    fn edit_note(&mut self, pos: usize) {
        use super::note_dialog::{NoteAnswer, NoteDialog};
        use kubuno_office_docs_core::edit::commands as cmd;
        let Some(((kind, text), rank)) = self
            .page
            .with(|p| {
                let doc = p.state().editor.doc();
                cmd::note_at(doc, pos).map(|n| {
                    let rank = cmd::note_positions(doc, &n.0).iter().position(|&x| x == pos).map(|i| i + 1).unwrap_or(1);
                    (n, rank)
                })
            })
            .flatten()
        else {
            return;
        };
        // The endnote's title carries its roman number, so it matches its call on the page.
        let title = if kind == "endnote" {
            format!("{} — {}", Resources::note_endnote(), kubuno_office_docs_core::layout::parse::roman_lower(rank))
        } else {
            Resources::note_footnote().to_string()
        };
        match NoteDialog::ask(self, &title, &text) {
            NoteAnswer::Text(t) => {
                self.page.with(|p| p.run(|s| cmd::set_note_text(s, pos, &t)));
            }
            NoteAnswer::Delete => {
                self.page.with(|p| p.run(|s| cmd::delete_note(s, pos)));
            }
            NoteAnswer::Cancel => {}
        }
    }

    fn footnote_execute(&mut self) {
        self.insert_note("footnote");
    }

    fn endnote_execute(&mut self) {
        self.insert_note("endnote");
    }

    fn page_note_clicked(&mut self, e: &crate::controls::page_canvas::NoteEventArgs) {
        // Raised while the canvas paints: the modal editor opens once that frame is done.
        let pos = e.pos;
        match self.dispatcher() {
            Some(d) => drop(d.begin_invoke(move |w: &mut DocumentWindow| w.edit_note(pos))),
            None => self.edit_note(pos),
        }
    }

    /// `gotoNote`: the caret on the next (previous) note of `kind`, wrapping around.
    fn goto_note(&mut self, kind: &str, forward: bool) {
        use kubuno_office_docs_core::edit::commands as cmd;
        self.page.with(|p| {
            let e = &p.state().editor;
            if let Some(pos) = cmd::next_note(e.doc(), kind, e.selection().from(), forward) {
                p.select_range(pos, pos);
            }
        });
    }

    fn note_footnote_next_execute(&mut self) {
        self.goto_note("footnote", true);
    }

    fn note_footnote_prev_execute(&mut self) {
        self.goto_note("footnote", false);
    }

    fn note_endnote_next_execute(&mut self) {
        self.goto_note("endnote", true);
    }

    fn note_endnote_prev_execute(&mut self) {
        self.goto_note("endnote", false);
    }

    /// « Afficher les notes »: the editor of the footnote the caret is on (else the first), else of
    /// an endnote.
    fn shownotes_execute(&mut self) {
        use kubuno_office_docs_core::edit::commands as cmd;
        let pos = self
            .page
            .with(|p| {
                let e = &p.state().editor;
                let here = e.selection().from();
                cmd::note_to_show(e.doc(), "footnote", here).or_else(|| cmd::note_to_show(e.doc(), "endnote", here))
            })
            .flatten();
        if let Some(pos) = pos {
            self.edit_note(pos);
        }
    }

    /// A root key of the envelope set (`Some`) or removed (`None`) — the web's document settings.
    fn envelope(&mut self, key: &'static str, value: Option<serde_json::Value>) {
        self.page_setup(move |ed| ed.set_envelope_value(key, value));
    }

    fn page_numbers_changed(&mut self, e: &TextChangedEventArgs) {
        let v = e.new.trim().to_string();
        if !matches!(v.as_str(), "none" | "footer-right" | "footer-center" | "header-right" | "header-center") {
            return;
        }
        self.envelope("pageNumbers", Some(json!(v)));
    }

    fn page_number_format(&mut self, fmt: &'static str) {
        self.envelope("pageNumFormat", Some(json!(fmt)));
    }

    fn pgfmt_arabic_execute(&mut self) {
        self.page_number_format("arabic");
    }

    fn pgfmt_roman_lower_execute(&mut self) {
        self.page_number_format("roman-lower");
    }

    fn pgfmt_roman_upper_execute(&mut self) {
        self.page_number_format("roman-upper");
    }

    fn pgfmt_alpha_lower_execute(&mut self) {
        self.page_number_format("alpha-lower");
    }

    fn pgfmt_alpha_upper_execute(&mut self) {
        self.page_number_format("alpha-upper");
    }

    /// The page colour of the caret's section (the web offers a colour picker; these are its two
    /// usual tints and « none »).
    fn page_color(&mut self, color: Option<&'static str>) {
        self.page_setup(move |ed| ed.set_page_color(color, crate::controls::page_canvas::now()));
    }

    fn pc_none_execute(&mut self) {
        self.page_color(None);
    }

    fn pc_cream_execute(&mut self) {
        self.page_color(Some("#fff8e1"));
    }

    fn pc_grey_execute(&mut self) {
        self.page_color(Some("#f1f3f4"));
    }

    /// A watermark with the web's defaults (`DEFAULT_WATERMARK`: grey, 45 %, diagonal, size fitted)
    /// and this text, the preset buttons of its popover.
    fn watermark(&mut self, text: &str) {
        let current = self.page.with(|p| p.state().editor.envelope_value("watermark")).flatten().filter(|v| v.is_object());
        let mut wm = current.unwrap_or_else(|| json!({ "text": "CONFIDENTIEL", "color": "#bdbdbd", "opacity": 0.45, "diagonal": true, "size": 0 }));
        wm["text"] = json!(text);
        self.envelope("watermark", Some(wm));
    }

    fn watermark_execute(&mut self) {
        // The button toggles: the default watermark, or none (`onChange(null)` writes `watermark: null`).
        let on = self.page.with(|p| p.state().editor.envelope_value("watermark")).flatten().is_some_and(|v| v.is_object());
        if on {
            self.envelope("watermark", Some(serde_json::Value::Null));
        } else {
            self.watermark("CONFIDENTIEL");
        }
    }

    fn wm_confidential_execute(&mut self) {
        self.watermark("CONFIDENTIEL");
    }

    fn wm_draft_execute(&mut self) {
        self.watermark("BROUILLON");
    }

    fn wm_urgent_execute(&mut self) {
        self.watermark("URGENT");
    }

    fn wm_nocopy_execute(&mut self) {
        self.watermark("NE PAS COPIER");
    }

    fn wm_sample_execute(&mut self) {
        self.watermark("ÉCHANTILLON");
    }

    fn wm_original_execute(&mut self) {
        self.watermark("ORIGINAL");
    }

    fn wm_remove_execute(&mut self) {
        self.envelope("watermark", Some(serde_json::Value::Null));
    }

    /// The page border with the web's defaults (`DEFAULT_PAGE_BORDER`: accent blue, 2 px, 24 px from
    /// the edge) in this style.
    fn page_border(&mut self, style: &str) {
        let current = self.page.with(|p| p.state().editor.envelope_value("pageBorder")).flatten().filter(|v| v.is_object());
        let mut pb = current.unwrap_or_else(|| json!({ "color": "#1a73e8", "width": 2, "style": "solid", "margin": 24 }));
        pb["style"] = json!(style);
        self.envelope("pageBorder", Some(pb));
    }

    fn pageborder_execute(&mut self) {
        let on = self.page.with(|p| p.state().editor.envelope_value("pageBorder")).flatten().is_some_and(|v| v.is_object());
        if on {
            self.envelope("pageBorder", Some(serde_json::Value::Null));
        } else {
            self.page_border("solid");
        }
    }

    fn pb_solid_execute(&mut self) {
        self.page_border("solid");
    }

    fn pb_dashed_execute(&mut self) {
        self.page_border("dashed");
    }

    fn pb_dotted_execute(&mut self) {
        self.page_border("dotted");
    }

    fn pb_double_execute(&mut self) {
        self.page_border("double");
    }

    fn pb_remove_execute(&mut self) {
        self.envelope("pageBorder", Some(serde_json::Value::Null));
    }

    /// Line numbers (`lineNumbers: {mode, interval}`, `null` for none), every line by default.
    fn line_numbers(&mut self, mode: Option<&str>) {
        let value = match mode {
            Some(m) => {
                let interval = self.page.with(|p| p.state().editor.envelope_value("lineNumbers")).flatten().and_then(|v| v.get("interval").cloned()).unwrap_or(json!(1));
                json!({ "mode": m, "interval": interval })
            }
            None => serde_json::Value::Null,
        };
        self.envelope("lineNumbers", Some(value));
    }

    fn ln_none_execute(&mut self) {
        self.line_numbers(None);
    }

    fn ln_continuous_execute(&mut self) {
        self.line_numbers(Some("continuous"));
    }

    fn ln_page_execute(&mut self) {
        self.line_numbers(Some("page"));
    }

    fn sb_execute(&mut self) {
        self.page_setup(|ed| ed.insert_section_break(crate::controls::page_canvas::now()));
    }

    fn mg_normal_execute(&mut self) {
        self.margins(96.0, 96.0, 96.0, 96.0);
    }

    fn mg_narrow_execute(&mut self) {
        self.margins(48.0, 48.0, 48.0, 48.0);
    }

    fn mg_moderate_execute(&mut self) {
        self.margins(96.0, 72.0, 96.0, 72.0);
    }

    fn mg_wide_execute(&mut self) {
        self.margins(96.0, 192.0, 96.0, 192.0);
    }

    // ── View ────────────────────────────────────────────────────────────────

    fn ruler_execute(&mut self) {
        self.ruler_inset = if self.show_ruler { crate::controls::ruler::RULER_SZ } else { 0.0 };
    }

    fn marks_execute(&mut self) {
        let on = self.show_marks;
        self.page.with(|p| p.set_show_marks(on));
    }

    fn zoom_100_execute(&mut self) {
        self.page.with(|p| p.set_zoom(1.0));
    }

    fn zoom_one_page_execute(&mut self) {
        self.page.with(|p| p.zoom_one_page());
    }

    fn zoom_page_width_execute(&mut self) {
        self.page.with(|p| p.zoom_page_width());
    }

    // ── The status bar ──────────────────────────────────────────────────────

    fn zoom_slider_value_changed(&mut self, e: &NumericValueChangedEventArgs) {
        let zoom = e.new / 100.0;
        self.page.with(|p| p.set_zoom(zoom));
    }

    // ── The rulers ──────────────────────────────────────────────────────────

    fn horizontal_ruler_margins_changed(&mut self, e: &RulerMarginsEventArgs) {
        let (top, bottom) = (self.margin_top, self.margin_bottom);
        let (s, end, c) = (e.start, e.end, e.commit);
        self.page.with(|p| p.set_page_margins(s, end, top, bottom, c));
    }

    fn vertical_ruler_margins_changed(&mut self, e: &RulerMarginsEventArgs) {
        let (left, right) = (self.margin_left, self.margin_right);
        let (s, end, c) = (e.start, e.end, e.commit);
        self.page.with(|p| p.set_page_margins(left, right, s, end, c));
    }

    fn horizontal_ruler_indents_changed(&mut self, e: &RulerIndentsEventArgs) {
        let (l, f, r, c) = (e.left, e.first_line, e.right, e.commit);
        self.page.with(|p| p.set_indents(l, f, r, c));
    }

    fn horizontal_ruler_tab_stops_changed(&mut self, e: &RulerTabStopsEventArgs) {
        self.tab_stops = e.tab_stops.clone();
        let t = e.tab_stops.clone();
        self.page.with(|p| p.set_tab_stops(&t));
    }

    fn horizontal_ruler_drag_guide_changed(&mut self, e: &RulerGuideEventArgs) {
        self.show_guide(e);
    }

    fn vertical_ruler_drag_guide_changed(&mut self, e: &RulerGuideEventArgs) {
        self.show_guide(e);
    }

    fn show_guide(&mut self, e: &RulerGuideEventArgs) {
        let guide = e.visible.then(|| DragGuide { vertical: e.vertical, position: e.position, label: e.label.clone(), pointer: e.pointer });
        self.page.with(|p| p.set_drag_guide(guide));
    }

    fn ruler_corner_tab_type_changed(&mut self, e: &TextChangedEventArgs) {
        self.tab_type = e.new.clone();
    }
}

/// `makeTableNode(rows, cols)` (`DocumentEditorPage.tsx:1608`): empty cells holding one paragraph.
pub fn table_node(rows: usize, cols: usize) -> Node {
    let cell = || Node::element("tableCell", None, vec![Node::of_type("paragraph")]);
    let row = || Node::element("tableRow", None, (0..cols).map(|_| cell()).collect());
    Node::element("table", None, (0..rows).map(|_| row()).collect())
}

/// The installed font families, for the ribbon's font list (`Text` rows).
fn font_rows() -> Rows {
    Rows::from(kubuno_desktop::ui::editors::system_font_families().iter().map(|f| Row::new().with("Text", Value::Str(f.clone()))).collect::<Vec<_>>())
}

/// Unix time in milliseconds (a revision's age).
fn epoch_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// What the review dialog shows for revision `index` of `changes` (a card of `ReviewPane.tsx`: the
/// author, the age, « Inséré » / « Supprimé », the text or « (marque de paragraphe) »).
fn review_view(changes: &[kubuno_office_docs_core::edit::track::TrackedChange], index: usize, now_ms: u64) -> super::review_dialog::ReviewView {
    use kubuno_office_docs_core::edit::track::{age, Age, ChangeKind};
    let Some(c) = changes.get(index) else {
        return super::review_dialog::ReviewView { head: Resources::review_empty().to_string(), ..Default::default() };
    };
    let n = changes.len().to_string();
    let count = Resources::review_count().replace("{n}", &n).replace("{i}", &(index + 1).to_string());
    let author = if c.author.is_empty() { Resources::review_unknown_author().to_string() } else { c.author.clone() };
    let kind = if c.kind == ChangeKind::Insertion { Resources::review_inserted() } else { Resources::review_deleted() };
    let when = match age(&c.date, now_ms) {
        Some(Age::Now) => Resources::review_time_now().to_string(),
        Some(Age::Minutes(m)) => Resources::review_time_min().replace("{n}", &m.to_string()),
        Some(Age::Hours(h)) => Resources::review_time_hour().replace("{n}", &h.to_string()),
        Some(Age::Date(ts)) => crate::platform::short_date(ts),
        None => String::new(),
    };
    let head = [author, kind.to_string(), when].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
    // `line-clamp-3`: the dialog shows the start of a long change.
    let mut body: String = c.text.chars().take(400).collect();
    if body.is_empty() {
        body = Resources::review_no_text().to_string();
    } else if c.text.chars().count() > 400 {
        body.push('…');
    }
    super::review_dialog::ReviewView { count, head, body, has_items: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_has_the_shape_the_web_inserts() {
        let t = table_node(2, 3);
        assert_eq!(t.children().len(), 2);
        assert_eq!(t.children()[0].children().len(), 3);
        assert_eq!(t.children()[0].children()[0].children()[0].node_type(), Some("paragraph"));
    }
}
