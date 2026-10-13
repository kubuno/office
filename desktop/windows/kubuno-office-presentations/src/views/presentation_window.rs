//! Code-behind of Kubuno Presentations' window (`presentation_window.kbview`) — Windows Forms' `Form1.cs`.
//!
//! The window owns the open presentation ([`Doc`]: desktop/common's `Workspace` and the image bytes) and hands
//! it to the slide list and the slide canvas; it binds what they report to the ribbon, the toolbar row, the
//! side panel, the notes and the status bar, and turns every command into the workspace's operations (the
//! web's `PresentationEditorPage` handlers). A server presentation (`--doc <id>`) is kept in step by the
//! background session of desktop/common (`api/live.rs`): per-slide saves guarded by a digest, the crash
//! journal, the conflict question.

use std::path::PathBuf;

use kubuno_desktop::prelude::*;
use kubuno_desktop::views::component::Control as _;
use kubuno_office_presentations_common::api::live::{self, Event as LiveEvent, Live, State};
use kubuno_office_presentations_common::api::session::Resolution;
use kubuno_office_presentations_common::app::{self, Options};
use kubuno_office_presentations_common::model::actions;
use kubuno_office_presentations_common::model::gradient_css;
use kubuno_office_presentations_common::model::workspace::Workspace;
use kubuno_office_slides_core::edit::{Outcome, Tool};
use kubuno_office_slides_core::format;
use kubuno_office_slides_core::insert::{self, TextBoxOpts};
use kubuno_office_slides_core::model::{Background, Deck, Element};
use kubuno_office_slides_core::smartart::SmartArtKind;
use serde_json::{json, Value as Json};

use super::confirm_dialog::{Answer, ConfirmDialog};
use super::prompt_dialog::PromptDialog;
use crate::controls::slide_canvas::{CanvasMenuEventArgs, CanvasStateEventArgs, ImageActionEventArgs, SlideCanvas};
use crate::controls::slide_panel::{PanelEventArgs, SlidePanel};
use crate::controls::start_page::{StartCommandEventArgs, StartItem, StartPage};
use crate::model::{now_ms, Doc, Shared};
use crate::platform::images::ImageStore;
use crate::Resources;

/// `FONT_FAMILIES` of the web toolbar (the system's fonts are added after them).
const FONT_FAMILIES: [&str; 12] = ["Arial", "Calibri", "Cambria", "Courier New", "Georgia", "Helvetica", "Inter", "Roboto", "Segoe UI", "Tahoma", "Times New Roman", "Verdana"];
/// The text toolbar's sizes (`TextFormatControls`).
const FONT_SIZES: [u32; 21] = [8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 32, 36, 40, 44, 54, 60, 66, 72, 80, 96];
/// `CHART_TYPES`.
const CHART_TYPES: [(&str, &str); 6] = [("column", "Colonnes"), ("bar", "Barres"), ("line", "Courbes"), ("area", "Aires"), ("pie", "Secteurs"), ("donut", "Anneau")];
/// `EMOJIS` and `SYMBOLS` of the « + » menu.
const EMOJIS: [&str; 24] = ["😀", "😎", "🚀", "💡", "✅", "❌", "⭐", "🔥", "📈", "📉", "💰", "🎯", "⚡", "🏆", "❤️", "👍", "👎", "🔔", "📌", "🌍", "☀️", "🌙", "🍌", "🎉"];
const SYMBOLS: [&str; 30] = ["→", "←", "↑", "↓", "↔", "⇒", "✓", "✗", "★", "☆", "•", "·", "°", "€", "$", "£", "©", "®", "™", "±", "×", "÷", "≈", "≠", "≤", "≥", "∞", "π", "Σ", "Δ"];

/// Kubuno Presentations' main window.
#[kubuno_desktop::view("presentation_window.kbview")]
pub struct PresentationWindow {
    #[control]
    canvas: Custom<SlideCanvas>,
    #[control]
    slides: Custom<SlidePanel>,
    #[control]
    start_page: Custom<StartPage>,
    #[bind]
    window_title: String,
    /// Whether the ribbon's content tabs are shown: hidden until a deck is open, like the web's
    /// `ModuleHome` (`kubuno_office_desktop_shared::ribbon`).
    #[bind]
    show_content_tabs: bool,
    /// The ribbon's active tab: the backstage with no deck, the editing tab once one is open.
    #[bind]
    active_ribbon_tab: String,
    // ── History, clipboards ──
    #[bind]
    can_undo: bool,
    #[bind]
    can_redo: bool,
    #[bind]
    can_paste_slides: bool,
    #[bind]
    can_paste_elements: bool,
    #[bind]
    has_style_clip: bool,
    // ── The selection ──
    #[bind]
    has_active: bool,
    #[bind]
    nothing_selected: bool,
    #[bind]
    text_or_image: bool,
    #[bind]
    shape_selected: bool,
    #[bind]
    round_rect_selected: bool,
    #[bind]
    image_selected: bool,
    #[bind]
    text_selected: bool,
    #[bind]
    line_selected: bool,
    #[bind]
    chart_selected: bool,
    #[bind]
    table_selected: bool,
    #[bind]
    has_geometry: bool,
    #[bind]
    can_group: bool,
    #[bind]
    can_ungroup: bool,
    #[bind]
    shadow: bool,
    #[bind]
    lock_label: String,
    #[bind]
    hide_label: String,
    #[bind]
    arrow_end_label: String,
    #[bind]
    arrow_start_label: String,
    #[bind]
    change_shape_rows: Rows,
    // ── Tools ──
    #[bind]
    tool_select: bool,
    #[bind]
    tool_text: bool,
    #[bind]
    tool_line: bool,
    // ── Text format ──
    #[bind]
    font_families: Rows,
    #[bind]
    font_family: String,
    #[bind]
    font_sizes: Rows,
    #[bind]
    font_size: String,
    #[bind]
    bold: bool,
    #[bind]
    italic: bool,
    #[bind]
    underline: bool,
    #[bind]
    strike: bool,
    #[bind]
    superscript: bool,
    #[bind]
    subscript: bool,
    #[bind]
    text_left: bool,
    #[bind]
    text_center: bool,
    #[bind]
    text_right: bool,
    #[bind]
    text_justify: bool,
    #[bind]
    list_bullet: bool,
    #[bind]
    list_number: bool,
    #[bind]
    text_color: String,
    #[bind]
    highlight_color: String,
    #[bind]
    fit_none: bool,
    #[bind]
    fit_shape: bool,
    #[bind]
    fit_shrink: bool,
    // ── Shape, chart, table ──
    #[bind]
    shape_fill: String,
    #[bind]
    shape_stroke: String,
    #[bind]
    chart_legend: bool,
    #[bind]
    chart_types: Rows,
    #[bind]
    chart_type: String,
    #[bind]
    table_header: bool,
    #[bind]
    table_banded: bool,
    #[bind]
    table_first_col: bool,
    #[bind]
    table_border: String,
    #[bind]
    table_font: String,
    #[bind]
    table_font_sizes: Rows,
    // ── Animations, transitions ──
    #[bind]
    anim_entry: String,
    #[bind]
    anim_exit: String,
    #[bind]
    anim_duration: String,
    #[bind]
    anim_delay: String,
    #[bind]
    trans_type: String,
    #[bind]
    trans_duration: String,
    // ── The side panel, the notes ──
    #[bind]
    side_position: String,
    #[bind]
    side_elements: String,
    #[bind]
    side_hide_label: String,
    #[bind]
    bg_color: String,
    #[bind]
    bg_gradient: String,
    #[bind]
    notes: String,
    // ── The « + » menu ──
    #[bind]
    emoji_rows: Rows,
    #[bind]
    symbol_rows: Rows,
    // ── The Backstage, the status bar ──
    #[bind]
    pres_title: String,
    #[bind]
    pres_format: String,
    #[bind]
    slides_text: String,
    #[bind]
    status_slide: String,
    #[bind]
    status_objects: String,
    #[bind]
    status_mode: String,
    #[bind]
    sync_status: String,
    #[bind]
    has_sync: bool,
    #[bind]
    publish_pending: bool,
    // ── The title band, the View tab ──
    #[bind]
    nav_text: String,
    #[bind]
    can_prev_slide: bool,
    #[bind]
    can_next_slide: bool,
    #[bind]
    starred: bool,
    #[bind]
    star_label: String,
    #[bind]
    is_server: bool,
    #[bind]
    full_screen: bool,
    #[bind]
    presence_users: String,
    #[bind]
    has_presence: bool,
    #[bind]
    unread_count: f64,
    #[bind]
    light_theme: bool,
    #[bind]
    dark_theme: bool,
    #[bind]
    notification_rows: Rows,
    /// This window's notifications (`notificationStore`), newest first.
    notices: Vec<Notice>,
    /// The start page row a menu was opened on.
    start_target: String,
    /// The window is in full screen (View > Full screen).
    fullscreen_saved: Option<crate::platform::fullscreen::Saved>,
    /// A title typed and not yet sent to the server (`PATCH :id { title }`, at the next publish tick).
    pending_title: Option<String>,
    /// The open presentation, shared with the controls.
    doc: Option<Shared>,
    /// The presentation to open at load (the controls exist once the window is open).
    pending: Option<Deck>,
    /// Where it came from: a local file (Ctrl+S writes it back), the server, or the sample.
    file: Option<PathBuf>,
    open_doc: Option<String>,
    live: Option<Live>,
    in_conflict: Vec<String>,
    /// The revision last handed to the session.
    published: u64,
    /// The revision last saved to the local file.
    saved_revision: u64,
    open_zoom: Option<f32>,
    open_slide: Option<usize>,
    open_show: bool,
    header_sample: bool,
    /// The notes text last pushed to the box (so its own echo is not written back).
    shown_notes: String,
}

impl PresentationWindow {
    pub fn new(options: Options, deck: Deck) -> Self {
        let file = options.file.clone();
        // No file and no `--doc` = the empty default placeholder: the ribbon shows only « Fichier ».
        let has_document = options.file.is_some() || options.doc.is_some();
        let tabs = kubuno_office_desktop_shared::ribbon::DocumentTabs::for_document(has_document, "tab_home", "backstage");
        let mut window = Self {
            window_title: format!("{} — {}", deck.title, Resources::app_title()),
            show_content_tabs: tabs.show_content,
            active_ribbon_tab: tabs.active_tab,
            pending: Some(deck),
            pres_format: if options.doc.is_some() {
                Resources::format_server().to_string()
            } else if file.is_some() {
                Resources::format_file().to_string()
            } else {
                Resources::format_sample().to_string()
            },
            file,
            open_doc: options.doc.clone(),
            open_zoom: options.zoom,
            open_slide: options.slide,
            open_show: options.show,
            header_sample: options.sample,
            nothing_selected: true,
            tool_select: true,
            anim_entry: "none".into(),
            anim_exit: "none".into(),
            anim_duration: "450".into(),
            anim_delay: "0".into(),
            trans_type: "none".into(),
            trans_duration: "500".into(),
            font_size: "24".into(),
            text_color: "#202124".into(),
            highlight_color: "#fff176".into(),
            text_left: true,
            fit_none: true,
            status_mode: Resources::status_mode_edit().to_string(),
            ..Self::default()
        };
        window.initialize_component();
        window
    }

    fn presentation_window_load(&mut self) {
        self.font_families = font_rows();
        self.font_sizes = Rows::from(FONT_SIZES.iter().map(|s| Row::new().with("Text", Value::Str(s.to_string()))).collect::<Vec<_>>());
        self.emoji_rows = Rows::from(EMOJIS.iter().map(|e| Row::new().with("Text", Value::Str(e.to_string())).with("Key", Value::Str(format!("e:{e}")))).collect::<Vec<_>>());
        self.symbol_rows = Rows::from(SYMBOLS.iter().map(|s| Row::new().with("Text", Value::Str(s.to_string())).with("Key", Value::Str(format!("s:{s}")))).collect::<Vec<_>>());
        self.change_shape_rows = shape_rows();
        self.chart_types = Rows::from(CHART_TYPES.iter().map(|(v, t)| Row::new().with("Value", Value::Str(v.to_string())).with("Text", Value::Str(t.to_string()))).collect::<Vec<_>>());
        self.table_font_sizes = Rows::from([10, 12, 14, 16, 20, 24].iter().map(|s| Row::new().with("Text", Value::Str(s.to_string()))).collect::<Vec<_>>());
        if let Some(deck) = self.pending.take() {
            self.open_deck(deck);
        }
        if let Some(z) = self.open_zoom {
            self.canvas.with(|c| c.zoom_to_percent(z as f64));
        }
        if let Some(id) = self.open_doc.take() {
            self.start_live(id);
        }
        let mut header = kubuno_desktop_header_data::FeedConfig::new(app::PROGRAM);
        if !self.header_sample {
            header.proxy = kubuno_desktop_sync::get_proxy();
        }
        kubuno_desktop_header_data::start(kubuno_desktop_header_data::HeaderOptions::for_app(&["office-presentations"]), header, self.header_sample, self.dispatcher());
        self.refresh_start();
        if self.open_show {
            self.slideshow_execute();
        }
    }

    /// Shows `deck` in the controls (a new [`Doc`]; the image store fetches the server's assets for `--doc`).
    fn open_deck(&mut self, deck: Deck) {
        let fetch = self.live.as_ref().map(|l| live::asset_fetch(&l.id)).or_else(|| self.open_doc.as_deref().map(live::asset_fetch));
        let wake: Option<crate::platform::images::Wake> = self.dispatcher().map(|d| {
            let d = std::sync::Mutex::new(d);
            std::sync::Arc::new(move || {
                if let Ok(d) = d.lock() {
                    drop(d.begin_invoke(|w: &mut PresentationWindow| w.repaint_all()));
                }
            }) as crate::platform::images::Wake
        });
        let seed = now_ms() ^ std::process::id() as u64;
        let mut ws = Workspace::new(deck, seed);
        ws.placeholders = placeholders();
        ws.item_label = Resources::smartart_item().to_string();
        if let Some(n) = self.open_slide.take() {
            if let Some(s) = ws.deck.slides.get(n.saturating_sub(1)) {
                let id = s.id.clone();
                ws.select_slide(&id, false, false);
            }
        }
        let doc = Doc::new(ws, ImageStore::new(fetch, wake));
        self.canvas.with(|c| {
            c.set_doc(doc.clone());
            c.focus();
        });
        self.slides.with(|p| p.set_doc(doc.clone()));
        self.saved_revision = doc.ws.borrow().revision;
        self.published = self.saved_revision;
        self.doc = Some(doc);
        self.refresh();
    }

    fn repaint_all(&mut self) {
        self.canvas.with(|c| c.invalidate());
        self.slides.with(|p| p.invalidate());
    }

    // ── A presentation of the server ────────────────────────────────────────

    fn start_live(&mut self, id: String) {
        let Some(dispatcher) = self.dispatcher() else {
            kubuno_desktop::tracing::warn!("[presentations] the window has no dispatcher: the server presentation cannot open");
            return;
        };
        self.sync_status = Resources::sync_opening().to_string();
        self.has_sync = true;
        let post = std::sync::Mutex::new(dispatcher);
        let sink: live::Sink = std::sync::Arc::new(move |event| {
            if let Ok(post) = post.lock() {
                drop(post.begin_invoke(move |w: &mut PresentationWindow| w.on_live(event)));
            }
        });
        let doc = id.clone();
        self.live = Some(Live::open(id, sink, move || live::connect(&doc)));
    }

    fn on_live(&mut self, event: LiveEvent) {
        match event {
            LiveEvent::Opened { deck, recovered, offline } => {
                self.open_deck(*deck);
                if let Some(copy) = recovered {
                    let restore = ConfirmDialog::choose(self, Resources::recover_title(), Resources::recover_text(), Resources::recover_yes(), Resources::recover_no()) != Answer::Second;
                    if restore {
                        self.open_deck(*copy);
                        if let Some(doc) = &self.doc {
                            doc.ws.borrow_mut().revision += 1;
                        }
                    }
                    if let Some(l) = &self.live {
                        l.recovered(restore);
                    }
                    if restore {
                        self.publish_now();
                    }
                }
                self.sync_status = if offline { Resources::sync_offline().to_string() } else { Resources::sync_saved().to_string() };
                if !offline {
                    self.migrate_legacy_images();
                }
            }
            LiveEvent::Failed(why) => self.sync_status = Resources::open_failed().replace("{why}", &why),
            LiveEvent::Status(s) => {
                let mut text = match s.state {
                    State::Saved if self.publish_pending => Resources::sync_unsaved().to_string(),
                    State::Saved => Resources::sync_saved().to_string(),
                    State::Dirty => Resources::sync_unsaved().to_string(),
                    State::Saving => Resources::sync_saving().to_string(),
                    State::Failed => Resources::sync_failed().to_string(),
                    State::Offline => Resources::sync_offline().to_string(),
                };
                if !s.too_large.is_empty() {
                    text = format!("{text} · {}", Resources::sync_too_large().replace("{n}", &s.too_large.len().to_string()));
                }
                if !s.conflicts.is_empty() {
                    text = format!("{text} · {}", Resources::sync_conflict());
                }
                if !s.editors.is_empty() {
                    text = format!("{text} · {}", Resources::sync_editors().replace("{names}", &s.editors.join(", ")));
                }
                self.sync_status = text;
                self.set_presence(&s.editors);
                if !s.too_large.is_empty() {
                    let body = Resources::sync_too_large().replace("{n}", &s.too_large.len().to_string());
                    self.notify(Resources::app_title(), &body);
                }
            }
            LiveEvent::Conflict(slides) => {
                self.notify(Resources::conflict_title(), &Resources::conflict_text().replace("{n}", &slides.len().to_string()));
                self.in_conflict = slides;
                self.ask_conflict();
            }
            LiveEvent::Reload(slides) => {
                if let Some(doc) = &self.doc {
                    let mut ws = doc.ws.borrow_mut();
                    for (sid, json) in slides {
                        if let Some(s) = ws.deck.slide_mut(&sid) {
                            s.data = kubuno_office_slides_core::model::SlideData::parse(&json);
                        }
                    }
                    ws.history = Default::default();
                    ws.revision += 1;
                    self.published = ws.revision;
                }
                self.refresh();
            }
            LiveEvent::Closed => {}
        }
    }

    /// Slides changed on the server under ours: only the user decides; closing the question decides nothing.
    fn ask_conflict(&mut self) {
        if self.in_conflict.is_empty() {
            return;
        }
        let text = Resources::conflict_text().replace("{n}", &self.in_conflict.len().to_string());
        let choice = match ConfirmDialog::choose(self, Resources::conflict_title(), &text, Resources::conflict_mine(), Resources::conflict_theirs()) {
            Answer::First => Resolution::KeepMine,
            Answer::Second => Resolution::TakeTheirs,
            Answer::Dismissed => return,
        };
        let slides = std::mem::take(&mut self.in_conflict);
        if let Some(l) = &self.live {
            l.resolve(slides, choice);
        }
    }

    /// `publish_timer`: hands the edited deck to the session (its journal and its next save).
    fn publish_tick(&mut self) {
        if std::mem::take(&mut self.publish_pending) {
            self.publish_now();
        }
        if let (Some(title), Some(live)) = (self.pending_title.take(), &self.live) {
            let id = live.id.clone();
            self.with_server(move |c| c.rename(&id, &title).map_err(|e| format!("{e:?}")));
        }
    }

    fn publish_now(&mut self) {
        let (Some(live), Some(doc)) = (&self.live, &self.doc) else { return };
        let ws = doc.ws.borrow();
        self.published = ws.revision;
        live.edited(ws.deck.clone());
    }

    fn presentation_window_closing(&mut self) {
        if let Some(live) = self.live.take() {
            let deck = self.doc.as_ref().map(|d| {
                let mut ws = d.ws.borrow_mut();
                ws.end_text();
                ws.deck.clone()
            });
            if !live.close(deck) {
                kubuno_desktop::tracing::warn!("[presentations] the server session did not finish closing in time; the local copy is kept");
            }
        }
    }

    // ── Keeping the window in step ──────────────────────────────────────────

    fn with_ws<R>(&self, f: impl FnOnce(&mut Workspace) -> R) -> Option<R> {
        let doc = self.doc.as_ref()?;
        let mut ws = doc.ws.borrow_mut();
        Some(f(&mut ws))
    }

    /// Runs `f` on the workspace, then repaints and refreshes everything bound.
    fn act(&mut self, f: impl FnOnce(&mut Workspace)) {
        if self.with_ws(f).is_some() {
            self.repaint_all();
            self.refresh();
            self.canvas.with(|c| c.report());
        }
    }

    /// An editor operation on the active slide's elements.
    fn edit(&mut self, f: impl FnOnce(&mut Vec<Element>, &mut kubuno_office_slides_core::edit::Editor, &mut kubuno_office_slides_core::insert::Ids) -> Outcome) {
        self.act(|ws| {
            ws.edit(now_ms(), f);
        });
    }

    /// Reads the workspace into the bound properties.
    fn refresh(&mut self) {
        let Some(doc) = self.doc.clone() else { return };
        let ws = doc.ws.borrow();
        let n = ws.deck.slides.len();
        let idx = ws.active_index();
        let els = ws.active_elements();
        let sel = ws.editor.single(els).cloned();
        let kind = sel.as_ref().map(|e| e.kind().to_string()).unwrap_or_default();
        self.has_active = idx.is_some();
        self.can_undo = ws.history.can_undo();
        self.can_redo = ws.history.can_redo();
        self.can_paste_slides = !ws.slide_clipboard.is_empty();
        self.can_paste_elements = !ws.editor.clipboard.is_empty();
        self.has_style_clip = ws.editor.style_clip.is_some();
        self.can_group = ws.editor.can_group();
        self.can_ungroup = ws.editor.can_ungroup(els);
        self.nothing_selected = ws.editor.selection.is_empty();
        self.shape_selected = kind == "shape";
        self.round_rect_selected = sel.as_ref().is_some_and(|e| e.kind() == "shape" && e.s("shape") == Some("roundRect"));
        self.image_selected = kind == "image";
        self.text_selected = kind == "text";
        self.line_selected = kind == "line";
        self.chart_selected = kind == "chart";
        self.table_selected = kind == "table";
        self.text_or_image = self.text_selected || self.image_selected;
        self.has_geometry = sel.is_some() && kind != "line";
        self.shadow = sel.as_ref().is_some_and(|e| e.truthy("shadow"));
        self.lock_label = if sel.as_ref().is_some_and(|e| e.locked()) { Resources::cmd_unlock() } else { Resources::cmd_lock() }.to_string();
        let (end, start) = actions::arrows_of(&ws);
        self.arrow_end_label = if end { Resources::arrow_end_off() } else { Resources::arrow_end_on() }.to_string();
        self.arrow_start_label = if start { Resources::arrow_start_off() } else { Resources::arrow_start_on() }.to_string();
        self.tool_select = ws.editor.tool == Tool::Select;
        self.tool_text = ws.editor.tool == Tool::Text;
        self.tool_line = matches!(ws.editor.tool, Tool::Line(_));
        // Text.
        if let Some(e) = sel.as_ref().filter(|e| e.kind() == "text" || e.kind() == "shape") {
            self.font_family = e.s("fontFamily").unwrap_or("Arial").to_string();
            self.font_size = e.f("fontSize").map(|f| format!("{}", f.round())).unwrap_or_else(|| "24".into());
            self.text_color = e.s("color").unwrap_or("#202124").to_string();
            let marks = ws.text.as_ref().map(|t| t.marks_state());
            self.bold = e.truthy("bold") || marks.as_ref().is_some_and(|m| m.b);
            self.italic = e.truthy("italic") || marks.as_ref().is_some_and(|m| m.i);
            self.underline = e.truthy("underline") || marks.as_ref().is_some_and(|m| m.u);
            self.strike = marks.as_ref().is_some_and(|m| m.s);
            self.superscript = marks.as_ref().is_some_and(|m| m.sup);
            self.subscript = marks.as_ref().is_some_and(|m| m.sub);
            let align = e.s("align").unwrap_or("left");
            (self.text_left, self.text_center, self.text_right) = (align == "left", align == "center", align == "right");
            let paras = kubuno_office_slides_core::richtext::doc_to_paras(e.get("content"));
            let first = paras.first();
            self.text_justify = first.and_then(|p| p.attrs.align()) == Some("justify");
            self.list_bullet = first.and_then(|p| p.attrs.list()) == Some("bullet");
            self.list_number = first.and_then(|p| p.attrs.list()) == Some("number");
            let fit = e.s("autofit").unwrap_or("none");
            (self.fit_none, self.fit_shape, self.fit_shrink) = (fit == "none", fit == "shape", fit == "shrink");
        }
        // Shape, chart, table.
        if let Some(e) = sel.as_ref() {
            match e.kind() {
                "shape" => {
                    self.shape_fill = e.get("fill").filter(|f| f.get("type").and_then(Json::as_str) == Some("color")).and_then(|f| f.get("color")).and_then(Json::as_str).unwrap_or("").to_string();
                    let st = e.get("stroke");
                    let w = st.and_then(|s| s.get("width")).and_then(Json::as_f64).unwrap_or(0.0);
                    self.shape_stroke = if w > 0.0 { st.and_then(|s| s.get("color")).and_then(Json::as_str).unwrap_or("").to_string() } else { String::new() };
                }
                "chart" => {
                    self.chart_legend = e.truthy("showLegend");
                    self.chart_type = e.s("chartType").unwrap_or("column").to_string();
                }
                "table" => {
                    self.table_header = e.truthy("headerRow");
                    self.table_banded = e.truthy("banded");
                    self.table_first_col = e.truthy("firstCol");
                    self.table_border = e.s("borderColor").unwrap_or("#9aa0a6").to_string();
                    self.table_font = format!("{}", e.f("fontSize").unwrap_or(14.0));
                }
                _ => {}
            }
        }
        // Animations.
        let (anim, dur, delay, exit) = ws.editor.anim_meta(els);
        self.anim_entry = if anim.is_empty() { "none".into() } else { anim };
        self.anim_exit = if exit.is_empty() { "none".into() } else { exit };
        self.anim_duration = format!("{dur}");
        self.anim_delay = format!("{delay}");
        // The slide.
        if let Some(s) = ws.active_slide() {
            let (t, d) = s.data.transition();
            self.trans_type = t;
            self.trans_duration = d.map(|d| format!("{d}")).unwrap_or_else(|| "500".into());
            match s.data.background() {
                Background::Color(c) => {
                    self.bg_color = c.unwrap_or_else(|| "#ffffff".into());
                    self.bg_gradient = String::new();
                }
                Background::Gradient(g) => self.bg_gradient = gradient_css::to_css(&g),
                _ => {}
            }
            let notes = s.data.notes().to_string();
            if notes != self.shown_notes {
                self.shown_notes = notes.clone();
                self.notes = notes;
            }
            self.side_hide_label = if s.hidden { Resources::side_show() } else { Resources::side_hide() }.to_string();
        }
        self.hide_label = if ws.active_slide().is_some_and(|s| s.hidden) { Resources::cmd_show_slide() } else { Resources::cmd_hide_slide() }.to_string();
        let pos = idx.map(|i| i + 1).unwrap_or(0);
        self.side_position = Resources::position_value().replace("{index}", &pos.to_string()).replace("{total}", &n.to_string());
        self.side_elements = Resources::element_count().replace("{count}", &els.len().to_string());
        self.status_slide = Resources::status_slide().replace("{current}", &pos.to_string()).replace("{total}", &n.to_string());
        self.status_objects = Resources::status_objects().replace("{count}", &els.len().to_string());
        self.slides_text = n.to_string();
        if self.pres_title != ws.deck.title {
            self.pres_title = ws.deck.title.clone();
        }
        self.nav_text = format!("{pos} / {n}");
        self.can_prev_slide = idx.is_some_and(|i| i > 0);
        self.can_next_slide = idx.is_some_and(|i| i + 1 < n);
        self.is_server = self.live.is_some();
        self.starred = ws.deck.meta.get("is_starred").and_then(Json::as_bool).unwrap_or(false);
        self.star_label = if self.starred { Resources::star_remove() } else { Resources::star_add() }.to_string();
        // The title and the save state.
        let dirty = if self.live.is_some() { ws.revision != self.published } else { ws.revision != self.saved_revision };
        let title = if dirty && self.live.is_none() { Resources::title_modified().replace("{title}", &ws.deck.title) } else { ws.deck.title.clone() };
        self.window_title = format!("{title} — {}", Resources::app_title());
        if self.live.is_some() && ws.revision != self.published {
            self.publish_pending = true;
        }
    }

    // ── The controls ────────────────────────────────────────────────────────

    fn canvas_state_changed(&mut self, _e: &CanvasStateEventArgs) {
        self.slides.with(|p| p.invalidate());
        self.refresh();
    }

    fn canvas_menu_requested(&mut self, e: &CanvasMenuEventArgs) {
        let menu = if e.on_element { "element_menu" } else { "canvas_menu" };
        self.refresh();
        kubuno_desktop::views::window::show_context_menu(menu, kubuno_desktop::views::window::MenuAnchor::Point(e.x, e.y));
    }

    fn canvas_image_action(&mut self, e: &ImageActionEventArgs) {
        match e.action.as_str() {
            "replace" => self.replace_img_execute(),
            "alt" => self.alt_execute(),
            _ => {}
        }
    }

    fn slides_panel_changed(&mut self, e: &PanelEventArgs) {
        match e.action.as_str() {
            "new" => self.new_slide_execute(),
            "menu" => {
                self.refresh();
                kubuno_desktop::views::window::show_context_menu("slide_menu", kubuno_desktop::views::window::MenuAnchor::Point(e.x, e.y));
            }
            _ => {
                self.canvas.with(|c| c.invalidate());
                self.refresh();
            }
        }
    }

    fn notes_changed(&mut self, e: &TextChangedEventArgs) {
        if e.new == self.shown_notes {
            return;
        }
        self.shown_notes = e.new.clone();
        let notes = e.new.clone();
        self.act(|ws| ws.set_notes(&notes));
    }

    // ── File ────────────────────────────────────────────────────────────────

    fn save_execute(&mut self) {
        if self.live.is_some() {
            if !self.in_conflict.is_empty() {
                self.ask_conflict();
                return;
            }
            self.publish_pending = false;
            self.publish_now();
            if let Some(live) = &self.live {
                live.save_now();
            }
            return;
        }
        match self.file.clone() {
            Some(path) => self.write_to(&path),
            None => self.save_copy_execute(),
        }
    }

    fn write_to(&mut self, path: &std::path::Path) {
        let Some(doc) = self.doc.clone() else { return };
        let res = app::write_file(path, &doc.ws.borrow().deck);
        match res {
            Ok(()) => {
                self.saved_revision = doc.ws.borrow().revision;
                self.sync_status = Resources::saved_to().replace("{path}", &path.display().to_string());
                self.has_sync = true;
            }
            Err(e) => {
                self.sync_status = Resources::save_failed().replace("{why}", &e);
                self.has_sync = true;
            }
        }
        self.refresh();
    }

    fn save_copy_execute(&mut self) {
        let name = self.doc.as_ref().map(|d| d.ws.borrow().deck.title.clone()).unwrap_or_default();
        let Some(path) = crate::platform::file_dialog::save_presentation(&name) else { return };
        self.write_to(&path);
        if self.live.is_none() {
            self.file = Some(path);
            self.pres_format = Resources::format_file().to_string();
        }
    }

    fn open_file_execute(&mut self) {
        let Some(path) = crate::platform::file_dialog::open_presentation() else { return };
        match app::read_file(&path) {
            Ok(deck) => {
                if let Some(live) = self.live.take() {
                    let deck = self.doc.as_ref().map(|d| d.ws.borrow().deck.clone());
                    live.close(deck);
                    self.has_sync = false;
                }
                self.file = Some(path);
                self.pres_format = Resources::format_file().to_string();
                self.open_deck(deck);
            }
            Err(e) => {
                self.sync_status = Resources::open_failed().replace("{why}", &e);
                self.has_sync = true;
            }
        }
    }

    fn bs_close_click(&mut self) {
        self.close();
    }

    fn export_pdf_execute(&mut self) {
        self.export(crate::platform::export::Kind::Pdf);
    }

    fn export_png_execute(&mut self) {
        self.export(crate::platform::export::Kind::Png);
    }

    fn export_jpg_execute(&mut self) {
        self.export(crate::platform::export::Kind::Jpeg);
    }

    fn export_all_execute(&mut self) {
        self.export(crate::platform::export::Kind::AllPng);
    }

    fn export(&mut self, kind: crate::platform::export::Kind) {
        let Some(doc) = self.doc.clone() else { return };
        let fonts = self.canvas.with(|c| c.fonts()).flatten();
        let result = crate::platform::export::run(kind, &doc, fonts);
        match result {
            Ok(Some(path)) => self.sync_status = Resources::exported_to().replace("{path}", &path.display().to_string()),
            Ok(None) => return,
            Err(e) => self.sync_status = Resources::export_failed().replace("{why}", &e),
        }
        self.has_sync = true;
    }

    // ── History ─────────────────────────────────────────────────────────────

    fn undo_execute(&mut self) {
        self.act(|ws| {
            ws.end_text();
            ws.undo();
        });
    }

    fn redo_execute(&mut self) {
        self.act(|ws| {
            ws.end_text();
            ws.redo();
        });
    }

    // ── Slides ──────────────────────────────────────────────────────────────

    fn new_slide_execute(&mut self) {
        self.act(|ws| {
            let after = ws.active.clone();
            ws.new_slide_after(after.as_deref());
        });
        self.slides.with(|p| p.reveal_active());
    }

    fn dup_slide_execute(&mut self) {
        self.act(|ws| ws.duplicate_selected());
    }

    fn delete_slides_execute(&mut self) {
        self.act(|ws| ws.delete_selected());
    }

    fn hide_slides_execute(&mut self) {
        self.act(|ws| ws.toggle_hidden_selected());
    }

    fn copy_slides_execute(&mut self) {
        self.act(|ws| ws.copy_selected());
    }

    fn cut_slides_execute(&mut self) {
        self.act(|ws| ws.cut_selected());
    }

    fn paste_slides_execute(&mut self) {
        self.act(|ws| {
            let after = ws.ordered_selection().last().cloned().or_else(|| ws.active.clone());
            ws.paste_slides_after(after.as_deref());
        });
    }

    fn move_up_execute(&mut self) {
        self.act(|ws| {
            if let Some(i) = ws.active_index().filter(|i| *i > 0) {
                ws.reorder(i, i - 1);
            }
        });
    }

    fn move_down_execute(&mut self) {
        self.act(|ws| {
            if let Some(i) = ws.active_index().filter(|i| *i + 1 < ws.deck.slides.len()) {
                ws.reorder(i, i + 1);
            }
        });
    }

    fn side_delete_click(&mut self) {
        self.delete_slides_execute();
    }

    fn side_duplicate_click(&mut self) {
        self.dup_slide_execute();
    }

    fn side_hide_click(&mut self) {
        self.hide_slides_execute();
    }

    /// The slide list's « Modifier l'arrière-plan » (`handleEditBackground`): the slide becomes the active one,
    /// its background is edited in the side panel.
    fn background_execute(&mut self) {
        self.refresh();
    }

    /// The slide ground's « Modifier l'arrière-plan »: the background popup.
    fn bg_dialog_execute(&mut self) {
        let Some(doc) = self.doc.clone() else { return };
        super::background_dialog::BackgroundDialog::edit(self, doc);
        self.repaint_all();
        self.refresh();
    }

    fn bg_color_changed(&mut self, e: &TextChangedEventArgs) {
        let c = e.new.trim().to_string();
        if c.is_empty() {
            return;
        }
        let json = json!({ "type": "color", "color": c }).to_string();
        self.act(|ws| ws.set_background(&json));
    }

    fn bg_gradient_changed(&mut self, e: &TextChangedEventArgs) {
        let Some(grad) = gradient_css::from_css(&e.new) else { return };
        let json = json!({ "type": "gradient", "grad": grad }).to_string();
        self.act(|ws| ws.set_background(&json));
    }

    fn bg_image_click(&mut self) {
        self.pick_image(ImageUse::Background);
    }

    fn bg_all_click(&mut self) {
        self.act(|ws| ws.apply_background_to_all());
    }

    fn trans_type_changed(&mut self, e: &TextChangedEventArgs) {
        let kind = e.new.clone();
        self.act(|ws| ws.set_transition(&kind, None));
    }

    fn trans_duration_changed(&mut self, e: &TextChangedEventArgs) {
        let Ok(ms) = e.new.parse::<f64>() else { return };
        self.act(|ws| {
            let kind = ws.active_slide().map(|s| s.data.transition().0).filter(|k| !k.is_empty() && k != "none").unwrap_or_else(|| "fade".into());
            ws.set_transition(&kind, Some(ms));
        });
    }

    // ── Layouts and guides (the slide ground's menu) ────────────────────────

    fn layout(&mut self, id: &str) {
        let id = id.to_string();
        self.act(|ws| ws.apply_layout(now_ms(), &id));
    }

    fn guides_show_click(&mut self) {
        self.act(|ws| ws.editor.show_guides = !ws.editor.show_guides);
    }

    fn guides_add_v_click(&mut self) {
        self.act(|ws| {
            let Workspace { editor, ids, .. } = ws;
            editor.add_guide(true, ids);
            editor.show_guides = true;
        });
    }

    fn guides_add_h_click(&mut self) {
        self.act(|ws| {
            let Workspace { editor, ids, .. } = ws;
            editor.add_guide(false, ids);
            editor.show_guides = true;
        });
    }

    fn guides_edit_click(&mut self) {
        self.act(|ws| ws.editor.show_guides = true);
    }

    fn guides_clear_click(&mut self) {
        self.act(|ws| ws.editor.guides.clear());
    }

    // ── Elements ────────────────────────────────────────────────────────────

    fn cut_el_execute(&mut self) {
        self.edit(|els, ed, _| {
            ed.copy(els);
            ed.remove(els)
        });
    }

    fn copy_el_execute(&mut self) {
        self.act(|ws| {
            let els = ws.active_elements().to_vec();
            ws.editor.copy(&els);
        });
    }

    fn paste_el_execute(&mut self) {
        self.edit(|els, ed, ids| ed.paste(els, ids));
    }

    fn delete_el_execute(&mut self) {
        self.edit(|els, ed, _| ed.remove(els));
    }

    fn duplicate_execute(&mut self) {
        self.edit(|els, ed, ids| ed.duplicate(els, ids));
    }

    fn group_execute(&mut self) {
        self.edit(|els, ed, ids| ed.group(els, ids));
    }

    fn ungroup_execute(&mut self) {
        self.edit(|els, ed, _| ed.ungroup(els));
    }

    fn front_execute(&mut self) {
        self.edit(|els, ed, _| ed.arrange(els, "front"));
    }

    fn back_execute(&mut self) {
        self.edit(|els, ed, _| ed.arrange(els, "back"));
    }

    fn forward_execute(&mut self) {
        self.edit(|els, ed, _| ed.arrange(els, "forward"));
    }

    fn backward_execute(&mut self) {
        self.edit(|els, ed, _| ed.arrange(els, "backward"));
    }

    fn align(&mut self, mode: &'static str) {
        self.edit(|els, ed, _| ed.align(els, mode));
    }

    fn al_left_execute(&mut self) {
        self.align("left");
    }

    fn al_center_execute(&mut self) {
        self.align("center");
    }

    fn al_right_execute(&mut self) {
        self.align("right");
    }

    fn al_top_execute(&mut self) {
        self.align("top");
    }

    fn al_middle_execute(&mut self) {
        self.align("middle");
    }

    fn al_bottom_execute(&mut self) {
        self.align("bottom");
    }

    fn dist_h_execute(&mut self) {
        self.edit(|els, ed, _| ed.distribute(els, true));
    }

    fn dist_v_execute(&mut self) {
        self.edit(|els, ed, _| ed.distribute(els, false));
    }

    fn same_w_execute(&mut self) {
        self.edit(|els, ed, _| ed.match_size(els, "w"));
    }

    fn same_h_execute(&mut self) {
        self.edit(|els, ed, _| ed.match_size(els, "h"));
    }

    fn same_both_execute(&mut self) {
        self.edit(|els, ed, _| ed.match_size(els, "both"));
    }

    fn swap_execute(&mut self) {
        self.edit(|els, ed, _| ed.swap(els));
    }

    fn rot_right_execute(&mut self) {
        self.edit(|els, ed, _| ed.rotate_by(els, 90.0));
    }

    fn rot_left_execute(&mut self) {
        self.edit(|els, ed, _| ed.rotate_by(els, -90.0));
    }

    fn rot_reset_execute(&mut self) {
        self.edit(|els, ed, _| ed.reset_rotation(els));
    }

    fn flip_h_execute(&mut self) {
        self.edit(|els, ed, _| ed.flip(els, true));
    }

    fn flip_v_execute(&mut self) {
        self.edit(|els, ed, _| ed.flip(els, false));
    }

    fn center_h_execute(&mut self) {
        self.edit(|els, ed, _| ed.center(els, Some(true)));
    }

    fn center_v_execute(&mut self) {
        self.edit(|els, ed, _| ed.center(els, Some(false)));
    }

    fn center_slide_execute(&mut self) {
        self.edit(|els, ed, _| ed.center(els, None));
    }

    fn stretch_w_execute(&mut self) {
        self.edit(|els, ed, _| ed.stretch(els, true));
    }

    fn stretch_h_execute(&mut self) {
        self.edit(|els, ed, _| ed.stretch(els, false));
    }

    fn same_type_execute(&mut self) {
        self.act(|ws| {
            let Some(id) = ws.editor.selection.first().cloned() else { return };
            let els = ws.active_elements().to_vec();
            ws.editor.select_same_type(&els, &id);
        });
    }

    fn copy_style_execute(&mut self) {
        self.act(|ws| {
            let els = ws.active_elements().to_vec();
            ws.editor.copy_style(&els);
        });
    }

    fn paste_style_execute(&mut self) {
        self.edit(|els, ed, _| ed.paste_style(els));
    }

    fn shadow_execute(&mut self) {
        self.edit(|els, ed, _| ed.toggle_shadow(els));
    }

    fn lock_execute(&mut self) {
        self.edit(|els, ed, _| ed.toggle_lock(els));
    }

    fn hide_el_execute(&mut self) {
        self.edit(|els, ed, _| ed.hide(els));
    }

    fn opacity(&mut self, v: f64) {
        self.edit(|els, ed, _| ed.set_opacity(els, v));
    }

    fn op100_execute(&mut self) {
        self.opacity(1.0);
    }

    fn op75_execute(&mut self) {
        self.opacity(0.75);
    }

    fn op50_execute(&mut self) {
        self.opacity(0.5);
    }

    fn op25_execute(&mut self) {
        self.opacity(0.25);
    }

    fn arrow_end_execute(&mut self) {
        self.menu_action("arrow_end");
    }

    fn arrow_start_execute(&mut self) {
        self.menu_action("arrow_start");
    }

    /// One of the element menu's actions (desktop/common `model/actions.rs`).
    fn menu_action(&mut self, key: &str) {
        let key = key.to_string();
        self.act(|ws| {
            actions::element_action(ws, now_ms(), &key);
        });
    }

    fn element_menu_item_clicked(&mut self, e: &TextChangedEventArgs) {
        // « Modifier la forme »: its rows' keys are `shape_<kind>`.
        self.menu_action(&e.new);
    }

    fn alt_execute(&mut self) {
        let Some((id, cur)) = self.with_ws(|ws| ws.editor.single(ws.active_elements()).map(|e| (e.id().to_string(), e.s("alt").unwrap_or("").to_string()))).flatten() else { return };
        let Some(v) = PromptDialog::ask(self, Resources::cmd_alt(), Resources::alt_msg(), &cur, "", true) else { return };
        self.edit(move |els, _, _| {
            let Some(e) = els.iter_mut().find(|e| e.id() == id) else { return Outcome::default() };
            e.set_s("alt", &v);
            Outcome { changed: true, repaint: true, tool_reset: false }
        });
    }

    fn link_execute(&mut self) {
        let Some((id, cur)) = self.with_ws(|ws| ws.editor.single(ws.active_elements()).map(|e| (e.id().to_string(), e.s("link").unwrap_or("").to_string()))).flatten() else { return };
        let Some(v) = PromptDialog::ask(self, Resources::cmd_link(), Resources::link_msg(), &cur, "https://exemple.com", true) else { return };
        self.edit(move |els, _, _| {
            let Some(e) = els.iter_mut().find(|e| e.id() == id) else { return Outcome::default() };
            if v.is_empty() {
                e.remove("link");
            } else {
                e.set_s("link", &v);
            }
            Outcome { changed: true, repaint: true, tool_reset: false }
        });
    }

    fn geometry_execute(&mut self) {
        let Some(el) = self.with_ws(|ws| ws.editor.single(ws.active_elements()).cloned()).flatten() else { return };
        if el.kind() == "line" {
            return;
        }
        let Some(g) = super::geometry_dialog::GeometryDialog::ask(self, &el) else { return };
        let id = el.id().to_string();
        self.edit(move |els, _, _| {
            let Some(e) = els.iter_mut().find(|e| e.id() == id) else { return Outcome::default() };
            e.set_f("x", g.x);
            e.set_f("y", g.y);
            e.set_f("w", g.w);
            e.set_f("h", g.h);
            e.set_f("rotation", g.rotation);
            e.set_f("opacity", g.opacity);
            Outcome { changed: true, repaint: true, tool_reset: false }
        });
    }

    // ── Shape tab ───────────────────────────────────────────────────────────

    fn shape_fill_changed(&mut self, e: &TextChangedEventArgs) {
        let c = e.new.trim().to_string();
        self.patch_shape(move |e| {
            if c.is_empty() {
                e.remove("fill");
            } else {
                e.set("fill", json!({ "type": "color", "color": c }));
            }
        });
    }

    fn shape_stroke_changed(&mut self, e: &TextChangedEventArgs) {
        let c = e.new.trim().to_string();
        self.patch_shape(move |e| {
            let mut st = e.get("stroke").and_then(Json::as_object).cloned().unwrap_or_else(|| {
                let mut m = serde_json::Map::new();
                m.insert("color".into(), json!("#000000"));
                m.insert("width".into(), json!(1));
                m.insert("style".into(), json!("solid"));
                m
            });
            if c.is_empty() {
                st.insert("width".into(), json!(0));
            } else {
                let w = st.get("width").and_then(Json::as_f64).unwrap_or(0.0).max(1.0);
                st.insert("color".into(), json!(c));
                st.insert("width".into(), kubuno_office_slides_core::model::num(w));
            }
            e.set("stroke", Json::Object(st));
        });
    }

    fn shape_no_fill_click(&mut self) {
        self.patch_shape(|e| e.remove("fill"));
    }

    fn shape_no_stroke_click(&mut self) {
        self.patch_shape(|e| {
            let mut st = e.get("stroke").and_then(Json::as_object).cloned().unwrap_or_default();
            st.insert("width".into(), json!(0));
            e.set("stroke", Json::Object(st));
        });
    }

    /// `updateSelectedEl` on the selected shape.
    fn patch_shape(&mut self, f: impl Fn(&mut Element)) {
        self.edit(|els, ed, _| {
            ed.update_sel(els, |e| {
                let mut n = e.clone();
                if n.kind() == "shape" {
                    f(&mut n);
                }
                n
            })
        });
    }

    /// `updateSelectedEl` on the single selected element.
    fn patch_selected(&mut self, f: impl FnOnce(&mut Element)) {
        self.edit(|els, ed, _| {
            let Some(id) = ed.selection.first().cloned() else { return Outcome::default() };
            let Some(e) = els.iter_mut().find(|e| e.id() == id) else { return Outcome::default() };
            f(e);
            Outcome { changed: true, repaint: true, tool_reset: false }
        });
    }

    // ── Insertion ───────────────────────────────────────────────────────────

    fn tool_select_execute(&mut self) {
        self.act(|ws| ws.editor.tool = Tool::Select);
    }

    fn tool_text_execute(&mut self) {
        self.act(|ws| ws.editor.tool = Tool::Text);
    }

    fn shape_gallery_click(&mut self, e: &TextChangedEventArgs) {
        let kind = e.new.clone();
        if kind.is_empty() {
            return;
        }
        // « Formes » arms the drawing tool: the shape is drawn on the slide (the web's `onPick`).
        self.act(|ws| ws.editor.tool = Tool::Shape(kind));
        self.canvas.with(|c| c.focus());
    }

    fn line_tool(&mut self, kind: &str) {
        let kind = kind.to_string();
        self.act(|ws| ws.editor.tool = Tool::Line(kind));
        self.canvas.with(|c| c.focus());
    }

    fn image_execute(&mut self) {
        self.pick_image(ImageUse::Insert);
    }

    /// Asks for an image file, reduces it like `loadImageFile` (1600 px on the long side) and hands it to
    /// [`Self::use_image`]: uploaded first as an asset of a server presentation (`uploadImageRef`, `kbfile:`), so
    /// it never weighs on the slide's 2 MiB; a `data:` URL for a local file, or when the upload fails.
    fn pick_image(&mut self, usage: ImageUse) {
        let Some(path) = crate::platform::file_dialog::open_image() else { return };
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                kubuno_desktop::tracing::warn!("[presentations] {}: {e}", path.display());
                return;
            }
        };
        let is_png = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png"));
        let Some((data, mime, w, h)) = crate::platform::images::prepare_upload(&bytes, is_png, 1600) else {
            self.sync_status = Resources::image_unreadable().to_string();
            self.has_sync = true;
            return;
        };
        // What it applies to is fixed now: the selection may change while the image uploads.
        let usage = match usage {
            ImageUse::Replace(_) => match self.with_ws(|ws| ws.editor.single(ws.active_elements()).filter(|e| e.kind() == "image").map(|e| e.id().to_string())).flatten() {
                Some(id) => ImageUse::Replace(id),
                None => return,
            },
            other => other,
        };
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "image".into());
        let pres = self.live.as_ref().map(|l| l.id.clone());
        match (pres, self.dispatcher()) {
            (Some(pres), Some(dispatcher)) => {
                self.sync_status = Resources::image_uploading().to_string();
                self.has_sync = true;
                let spawned = std::thread::Builder::new().name("presentations-upload".into()).spawn(move || {
                    let result = live::upload_asset(&pres, &data, mime, &name);
                    drop(dispatcher.begin_invoke(move |win: &mut PresentationWindow| match result {
                        Ok(reference) => {
                            win.sync_status = Resources::sync_unsaved().to_string();
                            win.use_image(usage, reference, data, w, h);
                        }
                        Err(why) => {
                            kubuno_desktop::tracing::warn!("[presentations] image upload: {why}");
                            win.sync_status = Resources::image_upload_failed().to_string();
                            win.notify(Resources::cmd_image(), Resources::image_upload_failed());
                            let src = format!("data:{mime};base64,{}", crate::platform::images::base64_encode(&data));
                            win.use_image(usage, src, data, w, h);
                        }
                    }));
                });
                if let Err(e) = spawned {
                    kubuno_desktop::tracing::error!("[presentations] upload thread: {e}");
                }
            }
            _ => {
                let src = format!("data:{mime};base64,{}", crate::platform::images::base64_encode(&data));
                self.use_image(usage, src, data, w, h);
            }
        }
    }

    /// Places an image whose reference is `src` (its bytes already known: no fetch back).
    fn use_image(&mut self, usage: ImageUse, src: String, data: Vec<u8>, w: u32, h: u32) {
        if let Some(doc) = &self.doc {
            doc.store.put(&src, data);
        }
        match usage {
            ImageUse::Insert => self.act(|ws| ws.insert(now_ms(), |ids, z| vec![insert::image(ids, &src, w as f64, h as f64, 0.5, 0.5, z)])),
            ImageUse::Replace(id) => self.edit(move |els, _, _| {
                let Some(e) = els.iter_mut().find(|e| e.id() == id && e.kind() == "image") else { return Outcome::default() };
                e.set_s("storagePath", &src);
                Outcome { changed: true, repaint: true, tool_reset: false }
            }),
            ImageUse::Background => {
                let json = background_image_json(&src);
                self.act(|ws| ws.set_background(&json));
            }
        }
    }

    fn replace_img_execute(&mut self) {
        self.pick_image(ImageUse::Replace(String::new()));
    }

    fn crop_execute(&mut self) {
        self.act(|ws| {
            let Some(id) = ws.editor.selection.first().cloned() else { return };
            let els = ws.active_elements().to_vec();
            let scale = 1.0;
            ws.editor.enter_crop(&els, &id, scale);
        });
        self.canvas.with(|c| c.focus());
    }

    fn img_reset_execute(&mut self) {
        self.canvas.with(|c| c.reset_image_aspect());
        self.refresh();
    }

    fn chart(&mut self, kind: &'static str) {
        self.act(|ws| ws.insert(now_ms(), |ids, z| vec![insert::chart(ids, kind, z)]));
    }

    fn table(&mut self, rows: usize, cols: usize) {
        self.act(|ws| ws.insert(now_ms(), |ids, z| vec![insert::table(ids, rows, cols, z)]));
    }

    fn smartart(&mut self, kind: SmartArtKind) {
        let label = Resources::smartart_item().to_string();
        self.act(|ws| ws.insert(now_ms(), |ids, z| insert::smartart(ids, kind, None, z, &|i| label.replace("{n}", &(i + 1).to_string()))));
    }

    fn insert_text(&mut self, text: &str, o: TextBoxOpts) {
        let text = text.to_string();
        self.act(|ws| ws.insert(now_ms(), |ids, z| vec![insert::text_box(ids, &text, &o, z)]));
    }

    fn textbox_execute(&mut self) {
        self.tool_text_execute();
    }

    fn separator_execute(&mut self) {
        self.act(|ws| ws.insert(now_ms(), |ids, z| vec![insert::separator(ids, z)]));
    }

    fn field(&mut self, kind: &str) {
        let text = match kind {
            "number" => self.with_ws(|ws| ws.active_index().map(|i| i + 1).unwrap_or(1).to_string()).unwrap_or_default(),
            "date" => crate::platform::local_date(),
            _ => crate::platform::local_time(),
        };
        self.insert_text(&text, TextBoxOpts { x: Some(0.78), y: Some(0.9), w: Some(0.18), h: Some(0.07), font_size: Some(14.0), align: Some("right"), color: Some("#5f6368"), ..Default::default() });
    }

    fn field_number_execute(&mut self) {
        self.field("number");
    }

    fn field_date_execute(&mut self) {
        self.field("date");
    }

    fn field_time_execute(&mut self) {
        self.field("time");
    }

    fn extras_item_clicked(&mut self, e: &TextChangedEventArgs) {
        if let Some(emoji) = e.new.strip_prefix("e:") {
            self.insert_text(emoji, TextBoxOpts { font_size: Some(64.0), w: Some(0.14), h: Some(0.18), ..Default::default() });
        } else if let Some(sym) = e.new.strip_prefix("s:") {
            self.insert_text(sym, TextBoxOpts { font_size: Some(48.0), w: Some(0.12), h: Some(0.16), ..Default::default() });
        }
    }

    fn pr_title_click(&mut self) {
        self.insert_text(Resources::pr_title_text(), TextBoxOpts { font_size: Some(44.0), w: Some(0.7), h: Some(0.18), y: Some(0.18), bold: Some(true), ..Default::default() });
    }

    fn pr_text_click(&mut self) {
        self.insert_text(Resources::pr_text_text(), TextBoxOpts { font_size: Some(20.0), w: Some(0.7), h: Some(0.3), y: Some(0.4), align: Some("left"), ..Default::default() });
    }

    fn pr_quote_click(&mut self) {
        self.insert_text(Resources::pr_quote_text(), TextBoxOpts { font_size: Some(28.0), w: Some(0.7), h: Some(0.2), y: Some(0.4), align: Some("center"), color: Some("#5f6368"), ..Default::default() });
    }

    fn pr_footer_click(&mut self) {
        self.insert_text(Resources::pr_footer_text(), TextBoxOpts { font_size: Some(13.0), w: Some(0.5), h: Some(0.06), x: Some(0.05), y: Some(0.91), align: Some("left"), color: Some("#9aa0a6"), ..Default::default() });
    }

    fn pr_two_col_click(&mut self) {
        self.insert_text(Resources::pr_two_col_text(), TextBoxOpts { font_size: Some(18.0), w: Some(0.76), h: Some(0.3), y: Some(0.4), align: Some("left"), columns: Some(2.0), ..Default::default() });
    }

    fn pr_bullets_click(&mut self) {
        self.insert_text(Resources::pr_bullets_text(), TextBoxOpts { font_size: Some(22.0), w: Some(0.7), h: Some(0.3), y: Some(0.38), align: Some("left"), ..Default::default() });
    }

    // ── Chart and table bars ────────────────────────────────────────────────

    fn chart_data_click(&mut self) {
        let Some(el) = self.with_ws(|ws| ws.editor.single(ws.active_elements()).cloned()).flatten() else { return };
        if el.kind() != "chart" {
            return;
        }
        let Some(edit) = super::chart_dialog::ChartDialog::ask(self, &el) else { return };
        self.patch_selected(move |e| {
            e.set_s("title", &edit.title);
            e.set("categories", edit.categories);
            e.set("series", edit.series);
        });
    }

    fn chart_type_changed(&mut self, e: &TextChangedEventArgs) {
        let kind = e.new.trim().to_string();
        if !kind.is_empty() && kind != self.chart_type {
            self.patch_selected(move |e| e.set_s("chartType", &kind));
        }
    }

    fn chart_legend_changed(&mut self, e: &CheckedChangedEventArgs) {
        let on = e.new;
        self.patch_selected(move |e| e.set_b("showLegend", on));
    }

    fn table_flag(&mut self, key: &'static str, on: bool) {
        self.patch_selected(move |e| e.set_b(key, on));
    }

    fn table_header_changed(&mut self, e: &CheckedChangedEventArgs) {
        self.table_flag("headerRow", e.new);
    }

    fn table_banded_changed(&mut self, e: &CheckedChangedEventArgs) {
        self.table_flag("banded", e.new);
    }

    fn table_firstcol_changed(&mut self, e: &CheckedChangedEventArgs) {
        self.table_flag("firstCol", e.new);
    }

    /// The table bar's `+L −L +C −C` (`addRow(el)` at the end, `delRow(el, rows - 1)`…).
    fn table_op(&mut self, op: &'static str) {
        self.patch_selected(move |e| {
            if e.kind() != "table" {
                return;
            }
            let rows = e.f("rows").unwrap_or(1.0).max(1.0) as usize;
            let cols = e.f("cols").unwrap_or(1.0).max(1.0) as usize;
            let keys = match op {
                "row+" => kubuno_office_slides_core::table::add_row(e, None),
                "row-" => kubuno_office_slides_core::table::del_row(e, rows - 1),
                "col+" => kubuno_office_slides_core::table::add_col(e, None),
                _ => kubuno_office_slides_core::table::del_col(e, cols - 1),
            };
            for (k, v) in keys {
                match v {
                    Some(v) => e.set(k, v),
                    None => e.remove(k),
                }
            }
        });
    }

    fn row_add_click(&mut self) {
        self.table_op("row+");
    }

    fn row_del_click(&mut self) {
        self.table_op("row-");
    }

    fn col_add_click(&mut self) {
        self.table_op("col+");
    }

    fn col_del_click(&mut self) {
        self.table_op("col-");
    }

    fn table_style(&mut self, i: usize) {
        let Some(&(_, h, b, bd)) = kubuno_office_slides_core::table::TABLE_STYLES.get(i) else { return };
        self.patch_selected(move |e| {
            e.set_s("headerBg", h);
            e.set_s("bandBg", b);
            e.set_s("borderColor", bd);
        });
    }

    fn table_border_changed(&mut self, e: &TextChangedEventArgs) {
        let c = e.new.trim().to_string();
        if !c.is_empty() {
            self.patch_selected(move |e| e.set_s("borderColor", &c));
        }
    }

    fn table_font_changed(&mut self, e: &TextChangedEventArgs) {
        let Ok(v) = e.new.trim().parse::<f64>() else { return };
        self.patch_selected(move |e| e.set_f("fontSize", v));
    }

    /// « Réinitialiser le style ».
    fn table_reset_click(&mut self) {
        let (_, h, b, bd) = kubuno_office_slides_core::table::TABLE_STYLES[0];
        self.patch_selected(move |e| {
            e.set_b("headerRow", true);
            e.set_b("banded", true);
            e.set_b("firstCol", false);
            e.set_s("headerBg", h);
            e.set_s("bandBg", b);
            e.set_s("borderColor", bd);
            e.set_f("fontSize", 14.0);
        });
    }

    // ── Text format ─────────────────────────────────────────────────────────

    /// `applyTextFormat`: paragraph attributes on every paragraph; marks over the edited selection; else the
    /// whole box.
    fn text_format(&mut self, kind: &str, value: Option<&str>) {
        let (kind, value) = (kind.to_string(), value.map(str::to_string));
        self.act(move |ws| {
            let now = now_ms();
            let Some(id) = ws.editor.selection.first().cloned() else { return };
            if format::is_para_kind(&kind) {
                if ws.text.as_ref().is_some_and(|t| t.id == id) {
                    ws.text_op(now, |te| {
                        te.paras = format::apply_para_attr(&te.paras, &kind, value.as_deref());
                    });
                    if kind == "align" {
                        if let Some(v @ ("left" | "center" | "right")) = value.as_deref() {
                            let v = v.to_string();
                            ws.edit(now, |els, _, _| {
                                let Some(e) = els.iter_mut().find(|e| e.id() == id) else { return Outcome::default() };
                                e.set_s("align", &v);
                                Outcome { changed: true, repaint: true, tool_reset: false }
                            });
                        }
                    }
                } else {
                    ws.edit(now, |els, _, _| {
                        let Some(e) = els.iter_mut().find(|e| e.id() == id) else { return Outcome::default() };
                        *e = format::element_para_attr(e, &kind, value.as_deref());
                        Outcome { changed: true, repaint: true, tool_reset: false }
                    });
                }
                return;
            }
            let editing = ws.text.as_ref().is_some_and(|t| t.id == id && !t.is_collapsed());
            if editing && kind != "align" && kind != "fontFamily" {
                if kind == "clear" {
                    ws.text_op(now, |te| {
                        te.clear_marks();
                    });
                } else if let Some(mark) = format::mark_of(&kind, value.as_deref()) {
                    ws.text_op(now, |te| {
                        te.apply_mark(&mark);
                    });
                }
                return;
            }
            ws.edit(now, |els, _, _| {
                let Some(e) = els.iter_mut().find(|e| e.id() == id) else { return Outcome::default() };
                match format::element_format(e, &kind, value.as_deref()) {
                    Some(n) => {
                        *e = n;
                        Outcome { changed: true, repaint: true, tool_reset: false }
                    }
                    None => Outcome::default(),
                }
            });
        });
        self.canvas.with(|c| c.focus());
    }

    fn bold_execute(&mut self) {
        self.text_format("bold", None);
    }

    fn italic_execute(&mut self) {
        self.text_format("italic", None);
    }

    fn underline_execute(&mut self) {
        self.text_format("underline", None);
    }

    fn strike_execute(&mut self) {
        self.text_format("strike", None);
    }

    fn sup_execute(&mut self) {
        self.text_format("superscript", None);
    }

    fn sub_execute(&mut self) {
        self.text_format("subscript", None);
    }

    fn tal_left_execute(&mut self) {
        self.text_format("align", Some("left"));
    }

    fn tal_center_execute(&mut self) {
        self.text_format("align", Some("center"));
    }

    fn tal_right_execute(&mut self) {
        self.text_format("align", Some("right"));
    }

    fn tal_justify_execute(&mut self) {
        self.text_format("justify", None);
    }

    fn bullets_execute(&mut self) {
        self.text_format("bullet", None);
    }

    fn numbers_execute(&mut self) {
        self.text_format("number", None);
    }

    fn indent_in_execute(&mut self) {
        self.text_format("indentIn", None);
    }

    fn indent_out_execute(&mut self) {
        self.text_format("indentOut", None);
    }

    fn clear_fmt_execute(&mut self) {
        self.text_format("clear", None);
    }

    fn font_family_changed(&mut self, e: &TextChangedEventArgs) {
        let f = e.new.trim().to_string();
        if !f.is_empty() && f != self.font_family {
            self.text_format("fontFamily", Some(&f));
        }
    }

    fn font_size_changed(&mut self, e: &TextChangedEventArgs) {
        let s = e.new.trim().to_string();
        if s.parse::<f64>().is_ok_and(|v| (6.0..=400.0).contains(&v)) && s != self.font_size {
            self.text_format("size", Some(&s));
        }
    }

    fn text_color_changed(&mut self, e: &TextChangedEventArgs) {
        let c = e.new.trim().to_string();
        if !c.is_empty() {
            self.text_format("color", Some(&c));
        }
    }

    fn highlight_changed(&mut self, e: &TextChangedEventArgs) {
        let c = e.new.trim().to_string();
        if !c.is_empty() {
            self.text_format("highlight", Some(&c));
        }
    }

    fn line_height(&mut self, v: &str) {
        self.text_format("lineHeight", Some(v));
    }

    fn fit(&mut self, mode: &str) {
        let Some(fonts) = self.canvas.with(|c| c.fonts()).flatten() else { return };
        let mode = mode.to_string();
        self.act(|ws| ws.autofit(now_ms(), &mode, &*fonts));
    }

    // ── Animations ──────────────────────────────────────────────────────────

    fn anim_entry_changed(&mut self, e: &TextChangedEventArgs) {
        let kind = e.new.clone();
        self.edit(|els, ed, _| ed.set_anim(els, (kind != "none").then_some(kind.as_str())));
    }

    fn anim_exit_changed(&mut self, e: &TextChangedEventArgs) {
        let kind = e.new.clone();
        self.edit(|els, ed, _| ed.set_anim_exit(els, (kind != "none").then_some(kind.as_str())));
    }

    fn anim_duration_changed(&mut self, e: &TextChangedEventArgs) {
        let Ok(ms) = e.new.parse::<f64>() else { return };
        self.edit(|els, ed, _| ed.set_anim_field(els, "duration", ms));
    }

    fn anim_delay_changed(&mut self, e: &TextChangedEventArgs) {
        let Ok(ms) = e.new.parse::<f64>() else { return };
        self.edit(|els, ed, _| ed.set_anim_field(els, "delay", ms));
    }

    // ── Zoom ────────────────────────────────────────────────────────────────

    fn zoom(&mut self, n: f64) {
        self.canvas.with(|c| c.zoom_to_percent(n));
    }


    // ── Legacy images ───────────────────────────────────────────────────────

    /// The web's one-off migration on open: every `data:` image of the presentation is uploaded as an asset and
    /// rewritten as its `kbfile:` reference (desktop/common `model/migrate.rs`); the slides that changed are then
    /// saved like any edit. Off the UI thread; an image that fails to upload keeps its `data:` URL.
    fn migrate_legacy_images(&mut self) {
        let (Some(live), Some(doc), Some(dispatcher)) = (self.live.as_ref(), self.doc.as_ref(), self.dispatcher()) else { return };
        let legacy = kubuno_office_presentations_common::model::migrate::legacy_images(&doc.ws.borrow().deck);
        if legacy.is_empty() {
            return;
        }
        let pres = live.id.clone();
        let spawned = std::thread::Builder::new().name("presentations-migrate".into()).spawn(move || {
            let mut done: Vec<(String, String, Vec<u8>)> = Vec::new();
            for l in legacy {
                match live::upload_asset(&pres, &l.bytes, &l.mime, l.name) {
                    Ok(reference) => done.push((l.url, reference, l.bytes)),
                    Err(why) => kubuno_desktop::tracing::warn!("[presentations] legacy image not migrated: {why}"),
                }
            }
            if done.is_empty() {
                return;
            }
            drop(dispatcher.begin_invoke(move |w: &mut PresentationWindow| w.apply_migrated(done)));
        });
        if let Err(e) = spawned {
            kubuno_desktop::tracing::error!("[presentations] migration thread: {e}");
        }
    }

    fn apply_migrated(&mut self, done: Vec<(String, String, Vec<u8>)>) {
        let Some(doc) = self.doc.clone() else { return };
        let refs: std::collections::HashMap<String, String> = done.iter().map(|(url, r, _)| (url.clone(), r.clone())).collect();
        for (_, r, bytes) in done {
            doc.store.put(&r, bytes);
        }
        let changed = {
            let mut ws = doc.ws.borrow_mut();
            let changed = kubuno_office_presentations_common::model::migrate::rewrite(&mut ws.deck, &refs);
            if !changed.is_empty() {
                ws.revision += 1;
            }
            changed
        };
        kubuno_desktop::tracing::info!("[presentations] migrated legacy images on {} slide(s)", changed.len());
        if !changed.is_empty() {
            self.repaint_all();
            self.refresh();
        }
    }

    /// The ribbon's tab changed; « Fichier » (the Backstage) reports its name too.
    fn ribbon_tab_changed(&mut self, e: &TextChangedEventArgs) {
        kubuno_desktop::tracing::info!("[presentations] ribbon tab: {:?} -> {:?}", e.old, e.new);
    }

    // ── The title band's bell, presence and sharing ─────────────────────────

    /// Files a notification (once: the same title and text are not announced twice, like `pushKeyed`).
    fn notify(&mut self, title: &str, body: &str) {
        if self.notices.iter().any(|n| n.title == title && n.body == body) {
            return;
        }
        self.notices.insert(0, Notice { title: title.to_string(), body: body.to_string(), read: false });
        self.notices.truncate(50);
        self.update_notices();
    }

    fn update_notices(&mut self) {
        self.unread_count = self.notices.iter().filter(|n| !n.read).count() as f64;
        self.notification_rows = Rows::from(notice_rows(&self.notices));
    }

    fn notifications_clicked(&mut self) {
        self.update_notices();
        self.header_menu("notif_menu");
    }

    /// A notice clicked is read; « Tout marquer comme lu » reads them all.
    fn notification_clicked(&mut self, e: &TextChangedEventArgs) {
        match e.new.as_str() {
            "all" => self.notices.iter_mut().for_each(|n| n.read = true),
            k => {
                if let Some(n) = k.strip_prefix('n').and_then(|i| i.parse::<usize>().ok()).and_then(|i| self.notices.get_mut(i)) {
                    n.read = true;
                }
            }
        }
        self.update_notices();
    }

    /// The other people editing the presentation (the session's `editors`), as the web's presence avatars.
    fn set_presence(&mut self, editors: &[String]) {
        self.presence_users = editors.iter().map(|n| format!("{n}|{}", crate::controls::presence_avatars::color_for(n))).collect::<Vec<_>>().join("\n");
        self.has_presence = !editors.is_empty();
    }

    /// « Partager » (`CollaboratorsDialog`).
    fn share_click(&mut self) {
        let Some(id) = self.live.as_ref().map(|l| l.id.clone()) else { return };
        super::share_dialog::ShareDialog::open_for(self, &id);
    }

    /// Opens title band menu `name` where the pointer is (the bell, the gear or the help button that was clicked:
    /// they live in the framework's header cluster, which has no `x:Name` of this view).
    fn header_menu(&mut self, name: &str) {
        use kubuno_desktop::views::window::{show_context_menu, MenuAnchor};
        let anchor = self.form().handle().and_then(crate::platform::fullscreen::pointer_dip).map(|(x, y)| MenuAnchor::Point(x, y + 12.0));
        show_context_menu(name, anchor.unwrap_or_else(|| MenuAnchor::Element("share_button".into())));
    }

    /// The gear (`SettingsMenu`).
    fn settings_clicked(&mut self) {
        let dark = kubuno_desktop::Application::theme().mode == kubuno_desktop::ui::ThemeMode::Dark;
        self.dark_theme = dark;
        self.light_theme = !dark;
        self.header_menu("settings_menu");
    }

    /// The help button (`HeaderActions` Part3).
    fn help_clicked(&mut self) {
        self.header_menu("help_menu");
    }

    /// « Paramètres »: the office module's settings page on the server (`ModuleSettingsRegistry.register('office')`).
    fn hdr_settings_execute(&mut self) {
        open_on_server("/office/settings");
    }

    /// « Apparence »: the light or the dark theme, live in every window of the app.
    fn hdr_light_execute(&mut self) {
        kubuno_desktop::Application::set_theme(kubuno_desktop::ui::Theme::light());
    }

    fn hdr_dark_execute(&mut self) {
        kubuno_desktop::Application::set_theme(kubuno_desktop::ui::Theme::dark());
    }

    /// « Télécharger des modules complémentaires »: the marketplace, filtered on the modules related to office.
    fn hdr_addons_execute(&mut self) {
        open_on_server("/admin/marketplace?related=office");
    }

    /// « Aide »: the Kubuno wiki.
    fn hdr_help_execute(&mut self) {
        kubuno_desktop_header_data::services::open_url(HELP_URL, false);
    }

    /// « À propos »: the server's about page.
    fn hdr_about_execute(&mut self) {
        open_on_server("/about");
    }
    // ── The title band ──────────────────────────────────────────────────────

    /// The title field (`onTitleChange` / `onTitleCommit`): the deck's title at once; the server's at the next
    /// publish tick (one `PATCH` for a burst of keystrokes).
    fn title_changed(&mut self, e: &TextChangedEventArgs) {
        let title = e.new.trim().to_string();
        let changed = self.with_ws(|ws| {
            if ws.deck.title == title {
                return false;
            }
            ws.deck.title = title.clone();
            true
        });
        if changed == Some(true) {
            if self.live.is_some() && !title.is_empty() {
                self.pending_title = Some(title);
                self.publish_pending = true;
            }
            self.refresh();
        }
    }

    fn nav_to(&mut self, delta: isize) {
        self.act(|ws| {
            let Some(i) = ws.active_index() else { return };
            let j = i as isize + delta;
            if j < 0 {
                return;
            }
            if let Some(id) = ws.deck.slides.get(j as usize).map(|s| s.id.clone()) {
                ws.select_slide(&id, false, false);
            }
        });
        self.slides.with(|p| p.reveal_active());
    }

    fn nav_prev_click(&mut self) {
        self.nav_to(-1);
    }

    fn nav_next_click(&mut self) {
        self.nav_to(1);
    }

    /// The star of the title band (`update({ is_starred })`).
    fn star_execute(&mut self) {
        let Some(live) = &self.live else { return };
        let id = live.id.clone();
        // From the presentation, not from `starred`: a checkable command flips its bound state before `OnExecute`.
        let on = !self.with_ws(|ws| ws.deck.meta.get("is_starred").and_then(Json::as_bool).unwrap_or(false)).unwrap_or(false);
        kubuno_desktop::tracing::info!("[presentations] star -> {on} (meta {:?})", self.with_ws(|ws| ws.deck.meta.get("is_starred").cloned()));
        self.with_ws(|ws| {
            if let Json::Object(m) = &mut ws.deck.meta {
                m.insert("is_starred".into(), Json::Bool(on));
            }
        });
        self.refresh();
        self.with_server(move |c| c.set_starred(&id, on).map_err(|e| format!("{e:?}")));
    }

    /// Runs `call` with the shell's account on a worker thread; a failure lands in the status bar.
    fn with_server(&mut self, call: impl FnOnce(&kubuno_office_presentations_common::api::client::Client) -> Result<(), String> + Send + 'static) {
        let dispatcher = self.dispatcher();
        let spawned = std::thread::Builder::new().name("presentations-meta".into()).spawn(move || {
            let result = match kubuno_desktop_sync::current_account() {
                Ok(Some(account)) => call(&kubuno_office_presentations_common::api::client::Client::for_account(account)),
                Ok(None) => Err(Resources::start_signed_out().to_string()),
                Err(e) => Err(e.to_string()),
            };
            if let (Err(why), Some(d)) = (result, dispatcher) {
                kubuno_desktop::tracing::warn!("[presentations] server call: {why}");
                drop(d.begin_invoke(move |w: &mut PresentationWindow| {
                    w.sync_status = Resources::save_failed().replace("{why}", &why);
                    w.has_sync = true;
                }));
            }
        });
        if let Err(e) = spawned {
            kubuno_desktop::tracing::error!("[presentations] worker thread: {e}");
        }
    }

    /// « Affichage › Plein écran » (`ensureFullscreenView`): the window without border over the whole screen, and
    /// back.
    fn fullscreen_execute(&mut self) {
        // After the command's dispatch: a window resized from inside an event handler is not laid out again.
        match self.dispatcher() {
            Some(d) => drop(d.begin_invoke(|w: &mut PresentationWindow| w.toggle_fullscreen())),
            None => self.toggle_fullscreen(),
        }
    }

    fn toggle_fullscreen(&mut self) {
        // The window's own state decides (the checkable command has already flipped `full_screen`).
        let Some(hwnd) = self.form().handle() else { return };
        match self.fullscreen_saved.take() {
            Some(saved) => crate::platform::fullscreen::leave(hwnd, &saved),
            None => self.fullscreen_saved = crate::platform::fullscreen::enter(hwnd),
        }
        self.full_screen = self.fullscreen_saved.is_some();
    }

    // ── The Backstage: « Accueil » and « Imprimer » ─────────────────────────

    /// Reads the start page's lists in the background (`list({ recent: true, limit: 20 })` and every
    /// presentation); without an account both are `None`.
    fn refresh_start(&mut self) {
        let Some(dispatcher) = self.dispatcher() else { return };
        let spawned = std::thread::Builder::new().name("presentations-start".into()).spawn(move || {
            use kubuno_office_presentations_common::api::client::{Client, PresentationMeta};
            let to_items = |v: Vec<PresentationMeta>| -> Vec<StartItem> { v.into_iter().map(|p| StartItem { id: p.id, title: p.title, updated_at: p.updated_at }).collect() };
            let lists = match kubuno_desktop_sync::current_account() {
                Ok(Some(account)) => {
                    let client = Client::for_account(account);
                    let recent = client.list(true).map_err(|e| kubuno_desktop::tracing::warn!("[presentations] recent: {e:?}")).ok().map(to_items);
                    let all = client.list(false).map_err(|e| kubuno_desktop::tracing::warn!("[presentations] list: {e:?}")).ok().map(to_items);
                    (recent.or(Some(Vec::new())), all.or(Some(Vec::new())))
                }
                _ => (None, None),
            };
            drop(dispatcher.begin_invoke(move |w: &mut PresentationWindow| {
                w.start_page.with(|p| p.set_lists(lists.0, lists.1));
            }));
        });
        if let Err(e) = spawned {
            kubuno_desktop::tracing::error!("[presentations] start page thread: {e}");
        }
    }

    fn start_command(&mut self, e: &StartCommandEventArgs) {
        match e.command.as_str() {
            "open" => self.open_server(&e.id),
            "new" => self.start_new(),
            "menu" => {
                self.start_target = e.id.clone();
                kubuno_desktop::views::window::show_context_menu("start_menu", kubuno_desktop::views::window::MenuAnchor::Point(e.x, e.y));
            }
            _ => {}
        }
    }

    /// Opens server presentation `id` (`onOpen`): in another window, unless it is this one.
    fn open_server(&mut self, id: &str) {
        if id.is_empty() || self.live.as_ref().is_some_and(|l| l.id == id) {
            return;
        }
        crate::platform::launch::new_window(Some(id));
    }

    /// « Nouvelle présentation » (`create({ title })` then `onOpen`); without an account, a new local one.
    fn start_new(&mut self) {
        match kubuno_desktop_sync::current_account() {
            Ok(Some(account)) => match kubuno_office_presentations_common::api::client::Client::for_account(account).create(Resources::start_default_title()) {
                Ok(p) => {
                    crate::platform::launch::new_window(Some(&p.id));
                    self.refresh_start();
                }
                Err(e) => self.sync_status = format!("{e:?}"),
            },
            _ => crate::platform::launch::new_window(None),
        }
    }

    fn start_open_execute(&mut self) {
        let id = self.start_target.clone();
        self.open_server(&id);
    }

    fn start_dup_execute(&mut self) {
        let id = self.start_target.clone();
        if let Ok(Some(account)) = kubuno_desktop_sync::current_account() {
            match kubuno_office_presentations_common::api::client::Client::for_account(account).duplicate(&id) {
                Ok(p) => {
                    crate::platform::launch::new_window(Some(&p.id));
                    self.refresh_start();
                }
                Err(e) => self.sync_status = format!("{e:?}"),
            }
        }
    }

    fn start_trash_execute(&mut self) {
        let id = self.start_target.clone();
        if ConfirmDialog::choose(self, Resources::trash_title(), Resources::trash_text(), Resources::start_trash(), Resources::cancel()) != Answer::First {
            return;
        }
        if let Ok(Some(account)) = kubuno_desktop_sync::current_account() {
            match kubuno_office_presentations_common::api::client::Client::for_account(account).trash(&id) {
                Ok(()) if self.live.as_ref().is_some_and(|l| l.id == id) => self.close(),
                Ok(()) => self.refresh_start(),
                Err(e) => self.sync_status = format!("{e:?}"),
            }
        }
    }

    /// « Imprimer » (Ctrl+P): the visible slides through the print preview.
    fn print_execute(&mut self) {
        let Some(doc) = self.doc.clone() else { return };
        let fonts = self.canvas.with(|c| c.fonts()).flatten();
        match crate::platform::print::print_slides(self, &doc, fonts) {
            Ok(true) => {}
            Ok(false) => {
                ConfirmDialog::choose(self, Resources::bs_print(), Resources::print_empty(), "OK", Resources::cancel());
            }
            Err(e) => {
                ConfirmDialog::choose(self, Resources::print_failed_title(), &e, "OK", Resources::cancel());
            }
        }
    }

    // ── Slideshow ───────────────────────────────────────────────────────────

    fn slideshow_execute(&mut self) {
        let Some(doc) = self.doc.clone() else { return };
        let start = doc.ws.borrow().active_index().unwrap_or(0);
        let fonts = self.canvas.with(|c| c.fonts()).flatten();
        self.status_mode = Resources::status_mode_show().to_string();
        let last = super::slideshow_window::SlideshowWindow::present(self, doc.clone(), start, fonts);
        self.status_mode = Resources::status_mode_edit().to_string();
        let id = doc.ws.borrow().deck.slides.get(last).map(|s| s.id.clone());
        if let Some(id) = id {
            self.act(|ws| ws.select_slide(&id, false, false));
            self.slides.with(|p| p.reveal_active());
        }
    }
}

/// One handler per menu item: the views name a method per item; each runs one action of its family.
macro_rules! menu_items {
    ($($name:ident => |$s:ident| $body:expr;)*) => {
        impl PresentationWindow {
            $(fn $name(&mut self) { let $s = self; $body; })*
        }
    };
}

menu_items! {
    anim_none_click => |w| w.menu_action("anim_none");
    anim_fade_click => |w| w.menu_action("anim_fade");
    anim_flyl_click => |w| w.menu_action("anim_flyl");
    anim_flyr_click => |w| w.menu_action("anim_flyr");
    anim_flyt_click => |w| w.menu_action("anim_flyt");
    anim_flyb_click => |w| w.menu_action("anim_flyb");
    anim_zoom_click => |w| w.menu_action("anim_zoom");
    anim_rise_click => |w| w.menu_action("anim_rise");
    anim_spin_click => |w| w.menu_action("anim_spin");
    anim_flip_click => |w| w.menu_action("anim_flip");
    anim_swivel_click => |w| w.menu_action("anim_swivel");
    anim_growturn_click => |w| w.menu_action("anim_growturn");
    anim_expand_click => |w| w.menu_action("anim_expand");
    anim_pulse_click => |w| w.menu_action("anim_pulse");
    anim_bounce_click => |w| w.menu_action("anim_bounce");
    anim_dropin_click => |w| w.menu_action("anim_dropin");
    anim_wiper_click => |w| w.menu_action("anim_wiper");
    anim_wipeu_click => |w| w.menu_action("anim_wipeu");
    anim_flytl_click => |w| w.menu_action("anim_flytl");
    anim_flybr_click => |w| w.menu_action("anim_flybr");
    qs_0_click => |w| w.menu_action("qs_0");
    qs_1_click => |w| w.menu_action("qs_1");
    qs_2_click => |w| w.menu_action("qs_2");
    qs_3_click => |w| w.menu_action("qs_3");
    qs_4_click => |w| w.menu_action("qs_4");
    qs_5_click => |w| w.menu_action("qs_5");
    qs_6_click => |w| w.menu_action("qs_6");
    qs_7_click => |w| w.menu_action("qs_7");
    cr_8_click => |w| w.menu_action("cr_8");
    cr_16_click => |w| w.menu_action("cr_16");
    cr_24_click => |w| w.menu_action("cr_24");
    cr_40_click => |w| w.menu_action("cr_40");
    flt_none_click => |w| w.menu_action("flt_none");
    flt_gray_click => |w| w.menu_action("flt_gray");
    flt_sepia_click => |w| w.menu_action("flt_sepia");
    flt_bright_click => |w| w.menu_action("flt_bright");
    flt_dark_click => |w| w.menu_action("flt_dark");
    flt_contrast_click => |w| w.menu_action("flt_contrast");
    flt_saturate_click => |w| w.menu_action("flt_saturate");
    flt_blur_click => |w| w.menu_action("flt_blur");
    flt_cold_click => |w| w.menu_action("flt_cold");
    bd_none_click => |w| w.menu_action("bd_none");
    bd_thin_click => |w| w.menu_action("bd_thin");
    bd_white_click => |w| w.menu_action("bd_white");
    bd_blue_click => |w| w.menu_action("bd_blue");
    ir_0_click => |w| w.menu_action("ir_0");
    ir_8_click => |w| w.menu_action("ir_8");
    ir_16_click => |w| w.menu_action("ir_16");
    ir_32_click => |w| w.menu_action("ir_32");
    tint_none_click => |w| w.menu_action("tint_none");
    tint_blue_click => |w| w.menu_action("tint_blue");
    tint_warm_click => |w| w.menu_action("tint_warm");
    tint_green_click => |w| w.menu_action("tint_green");
    fx_shadow_click => |w| w.menu_action("fx_shadow");
    fx_outline_click => |w| w.menu_action("fx_outline");
    fx_wa_blue_click => |w| w.menu_action("fx_wa_blue");
    fx_wa_gold_click => |w| w.menu_action("fx_wa_gold");
    fx_wa_none_click => |w| w.menu_action("fx_wa_none");
    case_none_click => |w| w.menu_action("case_none");
    case_upper_click => |w| w.menu_action("case_upper");
    case_lower_click => |w| w.menu_action("case_lower");
    case_capitalize_click => |w| w.menu_action("case_capitalize");
    ls_0_click => |w| w.menu_action("ls_0");
    ls_1_click => |w| w.menu_action("ls_1");
    ls_2_click => |w| w.menu_action("ls_2");
    ls_4_click => |w| w.menu_action("ls_4");
    col_1_click => |w| w.menu_action("col_1");
    col_2_click => |w| w.menu_action("col_2");
    as_8_click => |w| w.menu_action("as_8");
    as_12_click => |w| w.menu_action("as_12");
    as_18_click => |w| w.menu_action("as_18");
    as_26_click => |w| w.menu_action("as_26");
    lw_1_click => |w| w.menu_action("lw_1");
    lw_2_click => |w| w.menu_action("lw_2");
    lw_4_click => |w| w.menu_action("lw_4");
    lw_6_click => |w| w.menu_action("lw_6");
    lw_10_click => |w| w.menu_action("lw_10");
    dash_solid_click => |w| w.menu_action("dash_solid");
    dash_dashed_click => |w| w.menu_action("dash_dashed");
    dash_dotted_click => |w| w.menu_action("dash_dotted");
    lay_title_click => |w| w.layout("title");
    lay_section_click => |w| w.layout("section");
    lay_title_body_click => |w| w.layout("title_body");
    lay_two_col_click => |w| w.layout("two_col");
    lay_title_only_click => |w| w.layout("title_only");
    lay_one_col_click => |w| w.layout("one_col");
    lay_main_click => |w| w.layout("main");
    lay_section_desc_click => |w| w.layout("section_desc");
    lay_caption_click => |w| w.layout("caption");
    lay_number_click => |w| w.layout("number");
    lay_blank_click => |w| w.layout("blank");
    mfit_none_click => |w| w.fit("none");
    mfit_shape_click => |w| w.fit("shape");
    mfit_shrink_click => |w| w.fit("shrink");
    zoom_50_click => |w| w.zoom(50.0);
    zoom_75_click => |w| w.zoom(75.0);
    zoom_100_click => |w| w.zoom(100.0);
    zoom_150_click => |w| w.zoom(150.0);
    zoom_200_click => |w| w.zoom(200.0);
    tl_straight_click => |w| w.line_tool("straight");
    tl_arrow_click => |w| w.line_tool("arrow");
    tl_elbow_click => |w| w.line_tool("elbow");
    tl_curved_click => |w| w.line_tool("curved");
    tl_arc_click => |w| w.line_tool("arc");
    tl_polyline_click => |w| w.line_tool("polyline");
    tl_freehand_click => |w| w.line_tool("freehand");
    tc_column_click => |w| w.chart("column");
    tc_bar_click => |w| w.chart("bar");
    tc_line_click => |w| w.chart("line");
    tc_area_click => |w| w.chart("area");
    tc_pie_click => |w| w.chart("pie");
    tc_donut_click => |w| w.chart("donut");
    tt_2x2_click => |w| w.table(2, 2);
    tt_3x3_click => |w| w.table(3, 3);
    tt_3x4_click => |w| w.table(3, 4);
    tt_4x4_click => |w| w.table(4, 4);
    tt_5x5_click => |w| w.table(5, 5);
    tt_6x6_click => |w| w.table(6, 6);
    tt_8x8_click => |w| w.table(8, 8);
    ts_process_click => |w| w.smartart(SmartArtKind::Process);
    ts_list_click => |w| w.smartart(SmartArtKind::List);
    ts_cycle_click => |w| w.smartart(SmartArtKind::Cycle);
    ts_hierarchy_click => |w| w.smartart(SmartArtKind::Hierarchy);
    ts_pyramid_click => |w| w.smartart(SmartArtKind::Pyramid);
    ts_matrix_click => |w| w.smartart(SmartArtKind::Matrix);
    lh_1_click => |w| w.line_height("1.0");
    lh_115_click => |w| w.line_height("1.15");
    lh_15_click => |w| w.line_height("1.5");
    lh_2_click => |w| w.line_height("2.0");
    ts_0_click => |w| w.table_style(0);
    ts_1_click => |w| w.table_style(1);
    ts_2_click => |w| w.table_style(2);
    ts_3_click => |w| w.table_style(3);
    ts_4_click => |w| w.table_style(4);
}

/// The fonts of the toolbar: the web's list, then the system's.
fn font_rows() -> Rows {
    let mut names: Vec<String> = FONT_FAMILIES.iter().map(|s| s.to_string()).collect();
    for f in kubuno_desktop::ui::editors::system_font_families().iter() {
        if !names.iter().any(|n| n.eq_ignore_ascii_case(f)) {
            names.push(f.clone());
        }
    }
    Rows::from(names.into_iter().map(|f| Row::new().with("Text", Value::Str(f))).collect::<Vec<_>>())
}

/// « Modifier la forme »: every kind of the shared catalogue, keyed `shape_<kind>`.
fn shape_rows() -> Rows {
    let mut rows = Vec::new();
    for cat in kubuno_office_slides_core::shapes::catalog::SHAPE_CATALOG.iter().filter(|c| c.id != "lines") {
        rows.push(Row::new().with("Text", Value::Str(cat.title.to_string())).with("Kind", Value::Str("Header".into())));
        for s in cat.shapes {
            rows.push(Row::new().with("Text", Value::Str(s.label.to_string())).with("Key", Value::Str(format!("shape_{}", s.kind))));
        }
    }
    Rows::from(rows)
}

/// The placeholder texts, in the UI's language.
fn placeholders() -> kubuno_office_slides_core::insert::Placeholders {
    kubuno_office_slides_core::insert::Placeholders { title: Resources::ph_title().to_string(), subtitle: Resources::ph_subtitle().to_string(), body: Resources::ph_body().to_string() }
}


/// A notification of the bell (`AppNotification`): what happened and whether it was seen.
#[derive(Debug, Clone, PartialEq)]
struct Notice {
    title: String,
    body: String,
    read: bool,
}

/// The bell's menu rows: a header, the notices (unread ones checked), the « mark all read » row; or the empty
/// line.
/// The help menu's « Aide » (`HeaderActions` Part3).
const HELP_URL: &str = "https://github.com/kubuno/kubuno/wiki";

/// Opens `path` of the signed-in account's server in the browser (nothing without an account).
fn open_on_server(path: &str) {
    match kubuno_desktop_header_data::current().map(|s| s.server).filter(|s| !s.is_empty()) {
        Some(server) => kubuno_desktop_header_data::services::open_url(&format!("{}{path}", server.trim_end_matches('/')), false),
        None => kubuno_desktop::tracing::info!("[presentations] {path}: no server"),
    }
}

fn notice_rows(notices: &[Notice]) -> Vec<Row> {
    let mut rows = vec![Row::new().with("Text", Value::Str(Resources::notifications().to_string())).with("Kind", Value::Str("Header".into()))];
    if notices.is_empty() {
        rows.push(Row::new().with("Text", Value::Str(Resources::no_notifications().to_string())).with("Enabled", Value::Bool(false)));
    } else {
        for (i, n) in notices.iter().enumerate() {
            let text = if n.body.is_empty() { n.title.clone() } else { format!("{} — {}", n.title, n.body) };
            rows.push(Row::new().with("Text", Value::Str(text)).with("Key", Value::Str(format!("n{i}"))).with("Checked", Value::Bool(!n.read)));
        }
        rows.push(Row::new().with("Kind", Value::Str("Separator".into())));
        rows.push(Row::new().with("Text", Value::Str(Resources::mark_all_read().to_string())).with("Key", Value::Str("all".into())));
    }
    rows
}

/// What a picked image is for.
#[derive(Debug, Clone, PartialEq)]
enum ImageUse {
    Insert,
    /// The image element (its id, fixed when the file is picked).
    Replace(String),
    Background,
}

/// The slide background of an image (`setSlideBg({ type: 'image', imagePath })`).
fn background_image_json(src: &str) -> String {
    json!({ "type": "image", "imagePath": src }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kubuno_office_slides_core::model::Background;

    #[test]
    fn the_bell_lists_the_notices_and_offers_to_read_them_all() {
        assert_eq!(notice_rows(&[]).len(), 2);
        let rows = notice_rows(&[Notice { title: "a".into(), body: "b".into(), read: false }, Notice { title: "c".into(), body: String::new(), read: true }]);
        // Header, two notices, a separator, the « mark all read » row.
        assert_eq!(rows.len(), 5);
    }

    #[test]
    fn a_background_image_is_stored_where_the_renderer_reads_it() {
        // The background used to be written under `storagePath` (the element key): nothing was drawn.
        let v: Json = serde_json::from_str(&background_image_json("kbfile:abc")).unwrap_or_default();
        assert_eq!(Background::parse(Some(&v)), Background::Image("kbfile:abc".into()));
    }
}
