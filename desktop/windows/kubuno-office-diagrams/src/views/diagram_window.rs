//! Code-behind of Kubuno Diagrams' window (`diagram_window.kbview`) - Windows Forms' `Form1.cs`.
//!
//! The canvas owns the open diagram (the core's editor); the window opens the diagram into it, binds what
//! it reports (`StateChanged`: the ribbon's enabled and checked states, the panels, the status bar, the
//! title's « modifié ») and turns the ribbon's commands, the context menus, the panels and the page bar
//! into the editor's edits. A diagram of the server is kept in step by the shared office save session
//! (`api::live`, the Documents worker) through the diagram routes (`api::remote`).

use kubuno_desktop::prelude::*;
use kubuno_desktop::views::component::Control as _;
use kubuno_office_diagrams_common::api::live::{Event as LiveEvent, Live, Sink};
use kubuno_office_diagrams_common::api::remote::{Protection, Resolution, State};
use kubuno_office_diagrams_common::doc::{canonical, pages_from_canonical, OpenDiagram, Source};
use kubuno_office_diagrams_core::editor::{obj, Align, Editor, Order};
use kubuno_office_diagrams_core::file::Page;
use kubuno_office_diagrams_core::layout::LayoutKind;
use kubuno_office_diagrams_core::model::{js_number, Routing};
use serde_json::Value;
use kubuno_desktop::views::binding::Value as BValue;

use crate::controls::diagram_canvas::{DiagramCanvas, DiagramContextEventArgs, DiagramStateEventArgs};
use crate::controls::layer_list::{LayerList, LayersCommandEventArgs};
use crate::controls::page_tabs::{PageTabEventArgs, PageTabs};
use crate::controls::start_page::{StartCommandEventArgs, StartItem, StartPage};
use crate::controls::stencil_palette::{StencilClickedEventArgs, StencilPalette};
use crate::Resources;

/// The dock panels' ids: `panel<N>` in declaration order. They carry no `x:Name`: the view composition binds a named
/// element's `Title`, and `<DockPanel>` reads its title as a literal (framework desktop-v0.1.1-alpha).
const PANEL_FORMAT: &str = "panel2";
const PANEL_LAYERS: &str = "panel3";

/// How the window starts: desktop/common's launch options.
pub use kubuno_office_diagrams_common::app::Options;

/// Kubuno Diagrams' main window.
#[kubuno_desktop::view("diagram_window.kbview")]
pub struct DiagramWindow {
    #[control]
    canvas: Custom<DiagramCanvas>,
    #[control]
    page_tabs: Custom<PageTabs>,
    #[control]
    layer_list: Custom<LayerList>,
    #[control]
    palette: Custom<StencilPalette>,
    #[control]
    format_panel: Custom<crate::FormatPanel>,
    #[control]
    start_page: Custom<StartPage>,
    #[bind]
    window_title: String,
    /// Whether the ribbon's content tabs are shown: hidden until a diagram is open, like the web's
    /// `ModuleHome` (`kubuno_office_desktop_shared::ribbon`).
    #[bind]
    show_content_tabs: bool,
    /// The ribbon's active tab: the backstage with no diagram, the editing tab once one is open.
    #[bind]
    active_ribbon_tab: String,
    // ── The commands' states ──
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
    has_clipboard: bool,
    #[bind]
    has_selection: bool,
    #[bind]
    has_shapes: bool,
    #[bind]
    has_any_shape: bool,
    #[bind]
    armed_text: bool,
    #[bind]
    armed_shape: String,
    #[bind]
    multi: bool,
    #[bind]
    three: bool,
    #[bind]
    two_shapes: bool,
    #[bind]
    has_group: bool,
    #[bind]
    show_grid: bool,
    #[bind]
    snap: bool,
    #[bind]
    show_minimap: bool,
    #[bind]
    show_rulers: bool,
    // ── The connector ──
    #[bind]
    conn_selected: bool,
    #[bind]
    route_straight: bool,
    #[bind]
    route_ortho: bool,
    #[bind]
    route_curved: bool,
    #[bind]
    line_solid: bool,
    #[bind]
    line_dashed: bool,
    #[bind]
    line_dotted: bool,
    #[bind]
    arrow_end: bool,
    #[bind]
    arrow_start: bool,
    #[bind]
    conn_has_nodes: bool,
    #[bind]
    width1: bool,
    #[bind]
    width2: bool,
    #[bind]
    width3: bool,
    #[bind]
    width4: bool,
    // ── The shape ──
    #[bind]
    shape_solid: bool,
    #[bind]
    shape_dashed: bool,
    #[bind]
    shape_dotted: bool,
    #[bind]
    shape_shadow: bool,
    #[bind]
    fill_color: String,
    #[bind]
    outline_color: String,
    #[bind]
    many_layers: bool,
    #[bind]
    layer_items: Rows,
    // ── The Backstage and the status bar ──
    #[bind]
    doc_title: String,
    #[bind]
    modified_text: String,
    #[bind]
    shapes_text: String,
    #[bind]
    connectors_text: String,
    #[bind]
    pages_text: String,
    #[bind]
    zoom_percent: f32,
    #[bind]
    shapes_status: String,
    #[bind]
    connectors_status: String,
    #[bind]
    selection_status: String,
    #[bind]
    has_selection_status: bool,
    #[bind]
    sync_status: String,
    #[bind]
    has_sync: bool,
    /// The last state the canvas reported.
    state: DiagramStateEventArgs,
    /// What the last right click was on (its target and point), for the menus' commands.
    context: DiagramContextEventArgs,
    /// The diagram to open at load.
    pending: Option<OpenDiagram>,
    /// Where the diagram comes from.
    source: Source,
    /// The server diagram's session.
    live: Option<Live>,
    /// The revision last published to the session.
    published: u64,
    in_conflict: bool,
    open_zoom: Option<f32>,
    header_sample: bool,
    /// The diagram a start page row's menu was opened on.
    start_target: String,
}

impl DiagramWindow {
    pub fn new(options: Options, diagram: OpenDiagram) -> Self {
        let source = match &options.doc {
            Some(id) => Source::Server(id.clone()),
            None => diagram.source.clone(),
        };
        // No file and no `--doc` = the empty default placeholder: the ribbon shows only « Fichier ».
        let has_document = options.file.is_some() || options.doc.is_some();
        let tabs = kubuno_office_desktop_shared::ribbon::DocumentTabs::for_document(has_document, "tab_home", "backstage");
        let mut window = Self {
            window_title: format!("{} - {}", diagram.title, Resources::app_title()),
            doc_title: diagram.title.clone(),
            is_server: matches!(source, Source::Server(_)),
            source,
            pending: Some(diagram),
            show_content_tabs: tabs.show_content,
            active_ribbon_tab: tabs.active_tab,
            open_zoom: options.zoom,
            header_sample: options.sample,
            show_grid: true,
            show_minimap: true,
            zoom_percent: 100.0,
            ..Self::default()
        };
        window.initialize_component();
        window
    }

    fn diagram_window_load(&mut self) {
        let stencil_name: Box<dyn Fn(&kubuno_office_diagrams_core::stencils::StencilDef) -> String> = Box::new(stencil_label);
        if let Some(d) = self.pending.take() {
            let server = matches!(self.source, Source::Server(_));
            let zoom = self.open_zoom.map(|z| (z / 100.0) as f64);
            self.canvas.with(|c| {
                if !server {
                    c.open(d.pages);
                }
                c.edit(|e| {
                    e.set_stencil_namer(stencil_name);
                    if let Some(z) = zoom {
                        e.set_zoom(z);
                    }
                });
                c.focus();
            });
        }
        if let Source::Server(id) = self.source.clone() {
            self.start_live(id);
        }
        self.refresh_start();
        let mut header = kubuno_desktop_header_data::FeedConfig::new("kubuno-diagrams");
        if !self.header_sample {
            header.proxy = kubuno_desktop_sync::get_proxy();
        }
        kubuno_desktop_header_data::start(kubuno_desktop_header_data::HeaderOptions::for_app(&["office-diagrams"]), header, self.header_sample, self.dispatcher());
    }

    // ── A diagram of the server ─────────────────────────────────────────────

    fn start_live(&mut self, id: String) {
        let Some(dispatcher) = self.dispatcher() else {
            kubuno_desktop::tracing::warn!("[diagrams] no dispatcher: the server diagram cannot open");
            return;
        };
        self.sync_status = Resources::sync_opening().to_string();
        self.has_sync = true;
        let post = dispatcher.clone();
        let sink: Sink = std::sync::Arc::new(move |event| {
            drop(post.begin_invoke(move |w: &mut DiagramWindow| w.on_live(event)));
        });
        let doc = id.clone();
        self.live = Some(Live::open(id, sink, move || kubuno_office_diagrams_common::api::remote::connect(&doc)));
    }

    fn load_canonical(&mut self, bytes: &[u8], title: &str) -> bool {
        match pages_from_canonical(bytes) {
            Some(pages) if !pages.is_empty() => {
                self.doc_title = title.to_string();
                self.canvas.with(|c| {
                    c.open(pages);
                    c.edit(|e| e.set_stencil_namer(Box::new(stencil_label)));
                    c.focus();
                });
                self.published = self.canvas.with(|c| c.editor().revision()).unwrap_or(0);
                true
            }
            _ => {
                self.sync_status = Resources::open_failed().replace("{why}", "contenu illisible");
                false
            }
        }
    }

    fn on_live(&mut self, event: LiveEvent) {
        match event {
            LiveEvent::Opened { title, content, recovered, protection, offline } => {
                self.load_canonical(&content, &title);
                if let Some(copy) = recovered {
                    let restore = super::confirm_dialog::ConfirmDialog::ask(self, Resources::recover_title(), Resources::recover_text(), Resources::recover_yes(), Resources::recover_no());
                    if restore && self.load_canonical(&copy, &title) {
                        if let Some(live) = &self.live {
                            live.recovered(true);
                            live.edited(copy, true);
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
                self.refresh_meta();
            }
            LiveEvent::Failed(why) => self.sync_status = Resources::open_failed().replace("{why}", &why),
            LiveEvent::Status(s) => {
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
                self.in_conflict = s.state == State::Conflict;
                if s.state == State::Clean && !s.unsaved {
                    self.canvas.with(|c| c.edit(Editor::mark_saved));
                }
            }
            LiveEvent::Conflict => self.ask_conflict(),
            LiveEvent::Reload(content) => {
                let title = self.doc_title.clone();
                self.load_canonical(&content, &title);
            }
            LiveEvent::Closed => {}
        }
    }

    fn ask_conflict(&mut self) {
        let mine = super::confirm_dialog::ConfirmDialog::ask(self, Resources::conflict_title(), Resources::conflict_text(), Resources::conflict_mine(), Resources::conflict_theirs());
        if let Some(live) = &self.live {
            live.resolve(if mine { Resolution::KeepMine } else { Resolution::TakeTheirs });
        }
    }

    /// The star, the title and the modification date (the diagram's row), read in the background.
    fn refresh_meta(&mut self) {
        let Source::Server(id) = self.source.clone() else { return };
        let Some(dispatcher) = self.dispatcher() else { return };
        std::thread::spawn(move || {
            let Ok(Some(account)) = kubuno_desktop_sync::current_account() else { return };
            let client = kubuno_office_diagrams_common::api::client::DiagramClient::for_account(account);
            if let Ok((meta, _)) = client.get(&id) {
                drop(dispatcher.begin_invoke(move |w: &mut DiagramWindow| {
                    w.starred = meta.is_starred;
                    w.doc_title = meta.title.clone();
                    w.modified_text = meta.updated_at.chars().take(10).collect();
                    w.update_title();
                }));
            }
        });
    }

    fn diagram_window_closing(&mut self, e: &mut kubuno_desktop::views::events::FormClosingEventArgs) {
        // A local file or the sample has no autosave: unsaved changes are asked about, never dropped.
        let dirty = self.canvas.with(|c| c.editor().is_dirty()).unwrap_or(false);
        if must_ask_before_close(self.live.is_some(), dirty) {
            match super::confirm_dialog::ConfirmDialog::ask3(
                self,
                Resources::close_title(),
                &Resources::close_text().replace("{name}", &self.doc_title),
                Resources::close_save(),
                Resources::close_discard(),
                Resources::cancel(),
            ) {
                Some(true) => {
                    self.save_execute();
                    if self.canvas.with(|c| c.editor().is_dirty()).unwrap_or(false) {
                        // « Enregistrer sous » cancelled or the write failed: stay open.
                        e.cancel = true;
                        return;
                    }
                }
                Some(false) => {}
                None => {
                    e.cancel = true;
                    return;
                }
            }
        }
        if let Some(live) = self.live.take() {
            let bytes = self.canonical_bytes();
            if !live.close(bytes) {
                kubuno_desktop::tracing::warn!("[diagrams] the server session did not finish closing in time; the local copy is kept");
            }
        }
    }

    fn canonical_bytes(&mut self) -> Option<Vec<u8>> {
        self.canvas.with(|c| canonical(c.editor().pages()))
    }

    fn update_title(&mut self) {
        let dirty = self.state.dirty;
        let title = if dirty { Resources::title_modified().replace("{title}", &self.doc_title) } else { self.doc_title.clone() };
        self.window_title = format!("{title} - {}", Resources::app_title());
    }

    // ── What the canvas reports ─────────────────────────────────────────────

    fn canvas_state_changed(&mut self, e: &DiagramStateEventArgs) {
        let s = e.clone();
        self.can_undo = s.can_undo;
        self.can_redo = s.can_redo;
        self.has_clipboard = s.has_clipboard;
        self.has_selection = s.selected + s.selected_conns > 0;
        self.has_shapes = s.selected > 0;
        self.has_any_shape = s.shapes > 0;
        self.multi = s.selected >= 2;
        self.three = s.selected >= 3;
        self.two_shapes = s.shapes >= 2;
        self.has_group = s.has_group;
        self.armed_text = s.armed == "text";
        self.armed_shape = s.armed.clone();
        self.show_grid = s.show_grid;
        self.snap = s.snap;
        self.show_minimap = s.show_minimap;
        self.show_rulers = s.show_rulers;
        self.zoom_percent = (s.zoom * 100.0).round();
        self.can_save = s.dirty;
        self.shapes_status = Resources::status_shapes().replace("{count}", &s.shapes.to_string());
        self.connectors_status = Resources::status_connectors().replace("{count}", &s.connectors.to_string());
        self.shapes_text = s.shapes.to_string();
        self.connectors_text = s.connectors.to_string();
        self.pages_text = s.pages.to_string();
        let total = s.selected + s.selected_conns;
        self.selection_status = if !s.shape_id.is_empty() {
            let kind = stencil_name_of(&s.shape_kind);
            if s.shape_label.is_empty() {
                kind
            } else {
                format!("{} ({kind})", s.shape_label)
            }
        } else if total > 1 {
            Resources::status_selected().replace("{count}", &total.to_string())
        } else {
            String::new()
        };
        self.has_selection_status = !self.selection_status.is_empty();
        self.read_selection();
        let page_changed = self.state.page != s.page
            || self.state.pages != s.pages
            || self.state.revision != s.revision
            || self.state.opened != s.opened;
        self.state = s;
        if page_changed {
            self.refresh_pages_and_layers();
        }
        self.refresh_format();
        self.update_title();
        self.publish();
    }

    /// The connector's and the shape's states (the contextual tabs, the menus, the format panel).
    fn read_selection(&mut self) {
        let snapshot = self.canvas.with(|c| {
            let e = c.editor();
            let conn = e.selected_conn().map(|c| (c.style(), !c.waypoints().is_empty()));
            let shape = e.active_shape().map(|s| s.style());
            let layers: Vec<(String, String)> = e.layers().iter().map(|l| (l.id().to_string(), l.name().to_string())).collect();
            let layer_of = e.selected_shape().map(|s| s.layer_id().map(str::to_string).unwrap_or_else(|| layers.first().map(|l| l.0.clone()).unwrap_or_default()));
            (conn, shape, layers, layer_of)
        });
        let Some((conn, shape, layers, layer_of)) = snapshot else { return };
        self.conn_selected = conn.is_some();
        if let Some((st, nodes)) = conn {
            self.route_straight = st.routing == Routing::Straight;
            self.route_ortho = st.routing == Routing::Orthogonal;
            self.route_curved = st.routing == Routing::Curved;
            self.line_solid = st.stroke_style == "solid";
            self.line_dashed = st.stroke_style == "dashed";
            self.line_dotted = st.stroke_style == "dotted";
            self.arrow_end = st.arrow_end != "none";
            self.arrow_start = st.arrow_start != "none";
            self.conn_has_nodes = nodes;
            let w = st.stroke_width.round() as i64;
            (self.width1, self.width2, self.width3, self.width4) = (w == 1, w == 2, w == 3, w == 4);
        }
        if let Some(st) = shape {
            self.shape_solid = st.stroke_style == "solid";
            self.shape_dashed = st.stroke_style == "dashed";
            self.shape_dotted = st.stroke_style == "dotted";
            self.shape_shadow = st.shadow;
            self.fill_color = if st.fill_color == "none" { String::new() } else { st.fill_color.clone() };
            self.outline_color = if st.stroke_color == "none" { String::new() } else { st.stroke_color.clone() };
        }
        self.many_layers = layers.len() > 1;
        self.layer_items = Rows::from(
            layers
                .iter()
                .map(|(id, name)| Row::new().with("Text", BValue::Str(name.clone())).with("Key", BValue::Str(format!("layer:{id}"))).with("Checked", BValue::Bool(layer_of.as_deref() == Some(id.as_str()))))
                .collect::<Vec<_>>(),
        );
    }

    /// The page bar and the layers panel follow the editor.
    fn refresh_pages_and_layers(&mut self) {
        let data = self.canvas.with(|c| {
            let e = c.editor();
            let pages: Vec<String> = e.pages().iter().map(|p| p.name.clone()).collect();
            let layers: Vec<crate::controls::layer_list::LayerRow> = e
                .layers()
                .iter()
                // The panel lists the top layer first (`layers.map(...).reverse()`).
                .rev()
                .map(|l| crate::controls::layer_list::LayerRow { id: l.id().to_string(), name: l.name().to_string(), visible: l.visible(), locked: l.locked() })
                .collect();
            (pages, e.current_index(), layers, e.active_layer().to_string(), e.has_selection())
        });
        let Some((pages, current, layers, active, has_sel)) = data else { return };
        self.page_tabs.with(|p| p.set_pages(pages, current));
        self.layer_list.with(|l| l.set_layers(layers, active, has_sel));
    }

    /// A server diagram: the canonical bytes go to the session when the editor changed.
    fn publish(&mut self) {
        if self.live.is_none() || !self.state.dirty || self.state.revision == self.published {
            return;
        }
        self.published = self.state.revision;
        if let (Some(bytes), Some(live)) = (self.canonical_bytes(), &self.live) {
            // Diagrams save on the web's 1.5 s debounce after each change: the session's structural cadence.
            live.edited(bytes, true);
        }
    }

    fn canvas_context_menu(&mut self, e: &DiagramContextEventArgs) {
        self.context = e.clone();
        let menu = match e.kind.as_str() {
            "shape" => "shape_menu",
            "connector" => "conn_menu",
            _ => "canvas_menu",
        };
        self.read_selection();
        kubuno_desktop::views::window::show_context_menu(menu, kubuno_desktop::views::window::MenuAnchor::Point(e.x, e.y));
    }

    fn shape_menu_item_clicked(&mut self, e: &TextChangedEventArgs) {
        if let Some(lid) = e.new.strip_prefix("layer:") {
            let lid = lid.to_string();
            self.edit(|ed| ed.move_selection_to_layer(&lid));
        }
    }

    // ── Helpers ─────────────────────────────────────────────────────────────

    fn edit(&mut self, f: impl FnOnce(&mut Editor)) {
        self.canvas.with(|c| c.edit(f));
    }

    fn sel_conn(&self) -> Option<String> {
        (!self.state.conn_id.is_empty()).then(|| self.state.conn_id.clone())
    }

    fn conn_style(&mut self, key: &str, v: Value) {
        if let Some(cid) = self.sel_conn() {
            let patch = obj(&[(key, v)]);
            self.edit(|e| e.update_conn_style(&cid, &patch));
        }
    }

    fn shape_style(&mut self, key: &str, v: Value) {
        let patch = obj(&[(key, v)]);
        self.edit(|e| e.apply_style_to_selection(&patch));
    }

    // ── File ────────────────────────────────────────────────────────────────

    fn save_execute(&mut self) {
        if self.live.is_some() {
            if self.in_conflict {
                self.ask_conflict();
                return;
            }
            if let (Some(bytes), Some(live)) = (self.canonical_bytes(), &self.live) {
                live.save_now(bytes);
            }
            return;
        }
        let path = match &self.source {
            Source::File(p) => Some(p.clone()),
            _ => crate::platform::file_dialog::save_as(&self.doc_title, "kbdia"),
        };
        let Some(path) = path else { return };
        let bytes = self.canvas.with(|c| OpenDiagram::to_kbdia(c.editor().pages())).unwrap_or_default();
        match std::fs::write(&path, bytes) {
            Ok(()) => {
                self.source = Source::File(path.clone());
                self.doc_title = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                self.canvas.with(|c| c.edit(Editor::mark_saved));
            }
            Err(e) => {
                self.sync_status = Resources::msg_save_failed().replace("{why}", &e.to_string());
                self.has_sync = true;
            }
        }
    }

    fn bs_close_click(&mut self) {
        self.close();
    }

    fn star_execute(&mut self) {
        let Source::Server(id) = self.source.clone() else { return };
        let to = !self.starred;
        self.starred = to;
        std::thread::spawn(move || {
            if let Ok(Some(account)) = kubuno_desktop_sync::current_account() {
                let client = kubuno_office_diagrams_common::api::client::DiagramClient::for_account(account);
                if let Err(e) = client.update(&id, None, Some(to)) {
                    kubuno_desktop::tracing::warn!("[diagrams] star: {e}");
                }
            }
        });
    }

    fn trash_execute(&mut self) {
        let Source::Server(id) = self.source.clone() else { return };
        if !super::confirm_dialog::ConfirmDialog::ask(self, Resources::trash_title(), Resources::trash_text(), Resources::cmd_delete(), Resources::cancel()) {
            return;
        }
        if let Ok(Some(account)) = kubuno_desktop_sync::current_account() {
            let client = kubuno_office_diagrams_common::api::client::DiagramClient::for_account(account);
            match client.trash(&id) {
                Ok(()) => self.close(),
                Err(e) => self.sync_status = e.to_string(),
            }
        }
    }

    fn new_execute(&mut self) {
        crate::platform::launch::new_window(None);
    }

    fn dup_doc_execute(&mut self) {
        let Source::Server(id) = self.source.clone() else { return };
        if let Ok(Some(account)) = kubuno_desktop_sync::current_account() {
            let client = kubuno_office_diagrams_common::api::client::DiagramClient::for_account(account);
            match client.duplicate(&id) {
                Ok(d) => crate::platform::launch::new_window(Some(&d.id)),
                Err(e) => self.sync_status = e.to_string(),
            }
        }
    }

    // ── The Backstage's « Accueil » and « Imprimer » ─────────────────────────

    /// Reads the start page's lists in the background (`diagramsApi.list({ recent: true, limit: 20 })` and the
    /// « Parcourir » list); without an account both are `None`.
    fn refresh_start(&mut self) {
        let Some(dispatcher) = self.dispatcher() else { return };
        std::thread::spawn(move || {
            let to_items = |v: Vec<kubuno_office_diagrams_common::api::client::DiagramMeta>| -> Vec<StartItem> {
                v.into_iter().map(|d| StartItem { id: d.id, title: d.title, updated_at: d.updated_at }).collect()
            };
            let lists = match kubuno_desktop_sync::current_account() {
                Ok(Some(account)) => {
                    let client = kubuno_office_diagrams_common::api::client::DiagramClient::for_account(account);
                    let recent = client.recent().map_err(|e| kubuno_desktop::tracing::warn!("[diagrams] recent: {e}")).ok().map(to_items);
                    let all = client.list().map_err(|e| kubuno_desktop::tracing::warn!("[diagrams] list: {e}")).ok().map(to_items);
                    (recent.or(Some(Vec::new())), all.or(Some(Vec::new())))
                }
                _ => (None, None),
            };
            drop(dispatcher.begin_invoke(move |w: &mut DiagramWindow| {
                w.start_page.with(|p| p.set_lists(lists.0, lists.1));
            }));
        });
    }

    fn start_command(&mut self, e: &StartCommandEventArgs) {
        match e.command.as_str() {
            "open" => self.open_server(&e.id),
            "new" => self.start_new(),
            "menu" => {
                self.start_target = e.id.clone();
                // The menu opens at the pointer, in the control's coordinates (the Backstage view's origin).
                kubuno_desktop::views::window::show_context_menu("start_menu", kubuno_desktop::views::window::MenuAnchor::Point(e.x, e.y));
            }
            _ => {}
        }
    }

    /// Opens server diagram `id` (`onOpen`): in another window, unless it is this one.
    fn open_server(&mut self, id: &str) {
        if id.is_empty() || matches!(&self.source, Source::Server(open) if open == id) {
            return;
        }
        crate::platform::launch::new_window(Some(id));
    }

    /// « Nouveau diagramme » (`diagramsApi.create({ title: t('diagrams_default_title') })` then `onOpen`); without an
    /// account, a new local diagram as « Nouveau » does.
    fn start_new(&mut self) {
        match kubuno_desktop_sync::current_account() {
            Ok(Some(account)) => {
                let client = kubuno_office_diagrams_common::api::client::DiagramClient::for_account(account);
                match client.create(Resources::start_default_title()) {
                    Ok(d) => {
                        crate::platform::launch::new_window(Some(&d.id));
                        self.refresh_start();
                    }
                    Err(e) => self.sync_status = e.to_string(),
                }
            }
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
            let client = kubuno_office_diagrams_common::api::client::DiagramClient::for_account(account);
            match client.duplicate(&id) {
                Ok(d) => {
                    crate::platform::launch::new_window(Some(&d.id));
                    self.refresh_start();
                }
                Err(e) => self.sync_status = e.to_string(),
            }
        }
    }

    fn start_trash_execute(&mut self) {
        let id = self.start_target.clone();
        if !super::confirm_dialog::ConfirmDialog::ask(self, Resources::trash_title(), Resources::trash_text(), Resources::cmd_delete(), Resources::cancel()) {
            return;
        }
        if let Ok(Some(account)) = kubuno_desktop_sync::current_account() {
            let client = kubuno_office_diagrams_common::api::client::DiagramClient::for_account(account);
            match client.trash(&id) {
                // Trashing the open diagram closes it, as the title bar's corbeille does.
                Ok(()) if matches!(&self.source, Source::Server(open) if *open == id) => self.close(),
                Ok(()) => self.refresh_start(),
                Err(e) => self.sync_status = e.to_string(),
            }
        }
    }

    /// « Imprimer »: every page of the diagram, through the print preview.
    fn bs_print_click(&mut self) {
        let Some(pages) = self.canvas.with(|c| c.editor().pages().to_vec()) else { return };
        let title = self.doc_title.clone();
        if let Err(e) = crate::platform::print::print_pages(self, &title, &pages) {
            super::confirm_dialog::ConfirmDialog::ask(self, Resources::print_failed_title(), &e, "OK", Resources::cancel());
        }
    }

    fn import_execute(&mut self) {
        let Some(path) = crate::platform::file_dialog::open_import() else { return };
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        let is_csv = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("csv"));
        let io = if is_csv {
            let mut ids = kubuno_office_diagrams_core::model::IdSource::new(kubuno_desktop::controls::host::now_ms());
            Some(kubuno_office_diagrams_core::io::from_csv(&text, &mut ids))
        } else {
            kubuno_office_diagrams_core::io::from_drawio_xml(&text)
        };
        match io.filter(|io| !io.shapes.is_empty()) {
            Some(io) => self.edit(|e| e.import(&io.shapes, &io.connectors)),
            None => {
                super::confirm_dialog::ConfirmDialog::ask(self, Resources::import_failed_title(), Resources::import_failed_text(), "OK", Resources::cancel());
            }
        }
    }

    fn export_raster(&mut self, jpeg: bool) -> Option<(Vec<u8>, u32, u32)> {
        let scene = self.canvas.with(|c| {
            let e = c.editor();
            let (shapes, conns) = e.visible_data();
            (shapes, conns, e.page().bg_color.clone())
        })?;
        crate::platform::raster::render_export(&scene.0, &scene.1, &scene.2, jpeg)
    }

    fn exp_png_execute(&mut self) {
        let Some((bytes, _, _)) = self.export_raster(false) else { return };
        if let Some(path) = crate::platform::file_dialog::save_as(&self.doc_title, "png") {
            let _ = std::fs::write(path, bytes);
        }
    }

    fn exp_jpg_execute(&mut self) {
        let Some((bytes, _, _)) = self.export_raster(true) else { return };
        if let Some(path) = crate::platform::file_dialog::save_as(&self.doc_title, "jpg") {
            let _ = std::fs::write(path, bytes);
        }
    }

    fn exp_pdf_execute(&mut self) {
        let Some((jpeg, w, h)) = self.export_raster(true) else { return };
        let pdf = kubuno_office_diagrams_core::pdf::build_jpeg_pdf(&jpeg, w, h);
        if let Some(path) = crate::platform::file_dialog::save_as(&self.doc_title, "pdf") {
            let _ = std::fs::write(path, pdf);
        }
    }

    fn exp_drawio_execute(&mut self) {
        let Some((shapes, connectors)) = self.canvas.with(|c| c.editor().visible_data()) else { return };
        let xml = kubuno_office_diagrams_core::io::to_drawio_xml(&kubuno_office_diagrams_core::io::IoData { shapes, connectors }, &self.doc_title);
        if let Some(path) = crate::platform::file_dialog::save_as(&self.doc_title, "drawio") {
            let _ = std::fs::write(path, xml);
        }
    }

    fn exp_json_execute(&mut self) {
        // The web's « Export JSON » is the server's export (`GET /diagrams/:id/export/json`); a local diagram
        // writes the same shape from its pages.
        let Some(pages) = self.canvas.with(|c| c.editor().pages().to_vec()) else { return };
        let list: Vec<Value> = pages
            .iter()
            .enumerate()
            .map(|(i, p)| serde_json::json!({ "id": p.id, "name": p.name, "position": i, "bg_color": p.bg_color, "data": p.data.to_value() }))
            .collect();
        let v = serde_json::json!({ "format": "kubuno-diagram/v1", "diagram": { "title": self.doc_title }, "pages": list });
        if let Some(path) = crate::platform::file_dialog::save_as(&self.doc_title, "json") {
            let _ = std::fs::write(path, serde_json::to_vec_pretty(&v).unwrap_or_default());
        }
    }

    // ── Clipboard and history ───────────────────────────────────────────────

    fn undo_execute(&mut self) {
        self.edit(Editor::undo);
    }

    fn redo_execute(&mut self) {
        self.edit(Editor::redo);
    }

    fn copy_execute(&mut self) {
        self.edit(Editor::copy);
        if let Some(Some(v)) = self.canvas.with(|c| c.editor().clipboard_json()) {
            crate::platform::clipboard::put(&v);
        }
    }

    fn cut_execute(&mut self) {
        self.copy_execute();
        self.edit(Editor::cut);
    }

    fn take_system_clipboard(&mut self) {
        if let Some(v) = crate::platform::clipboard::get() {
            self.edit(|e| e.set_clipboard_json(&v));
        }
    }

    fn paste_execute(&mut self) {
        self.take_system_clipboard();
        self.edit(|e| e.paste(None));
    }

    fn paste_here_execute(&mut self) {
        self.take_system_clipboard();
        let at = kubuno_office_diagrams_core::canvas::Point::new(self.context.world_x, self.context.world_y);
        self.edit(|e| e.paste(Some(at)));
    }

    fn dup_sel_execute(&mut self) {
        self.edit(Editor::duplicate);
    }

    fn delete_execute(&mut self) {
        self.edit(Editor::delete_selected);
    }

    fn select_all_execute(&mut self) {
        self.edit(Editor::select_all);
    }

    fn select_pickable_execute(&mut self) {
        self.edit(Editor::select_all_pickable);
    }

    fn props_execute(&mut self) {
        self.dock.open(PANEL_FORMAT);
    }

    fn layers_execute(&mut self) {
        self.dock.open(PANEL_LAYERS);
    }

    // ── Insertion ───────────────────────────────────────────────────────────

    fn shapes_execute(&mut self) {
        // The gallery is a popup: the pick comes back through the dispatcher.
        let Some(dispatcher) = self.dispatcher() else { return };
        crate::controls::shape_gallery::ShapeGallery::open(&self.armed_shape, move |kind| {
            drop(dispatcher.begin_invoke(move |w: &mut DiagramWindow| w.edit(|e| e.arm_shape(&kind))));
        });
    }

    fn text_execute(&mut self) {
        self.edit(|e| e.arm_shape("text"));
    }

    fn templates_execute(&mut self) {
        if let Some(t) = super::templates_dialog::TemplatesDialog::pick(self) {
            let io = (t.build)();
            if !io.shapes.is_empty() {
                self.edit(|e| e.import(&io.shapes, &io.connectors));
            }
        }
    }

    fn palette_stencil_clicked(&mut self, e: &StencilClickedEventArgs) {
        let id = e.id.clone();
        self.edit(|ed| ed.toggle_armed(&id));
    }

    // ── Disposition ─────────────────────────────────────────────────────────

    fn al_left_execute(&mut self) {
        self.edit(|e| e.align(Align::Left));
    }
    fn al_center_execute(&mut self) {
        self.edit(|e| e.align(Align::Center));
    }
    fn al_right_execute(&mut self) {
        self.edit(|e| e.align(Align::Right));
    }
    fn al_top_execute(&mut self) {
        self.edit(|e| e.align(Align::Top));
    }
    fn al_middle_execute(&mut self) {
        self.edit(|e| e.align(Align::Middle));
    }
    fn al_bottom_execute(&mut self) {
        self.edit(|e| e.align(Align::Bottom));
    }
    fn dist_h_execute(&mut self) {
        self.edit(|e| e.distribute(true));
    }
    fn dist_v_execute(&mut self) {
        self.edit(|e| e.distribute(false));
    }
    fn front_execute(&mut self) {
        self.edit(|e| e.reorder(Order::Front));
    }
    fn forward_execute(&mut self) {
        self.edit(|e| e.reorder(Order::Forward));
    }
    fn backward_execute(&mut self) {
        self.edit(|e| e.reorder(Order::Backward));
    }
    fn back_execute(&mut self) {
        self.edit(|e| e.reorder(Order::Back));
    }
    fn group_execute(&mut self) {
        self.edit(Editor::group);
    }
    fn ungroup_execute(&mut self) {
        self.edit(Editor::ungroup);
    }
    fn flip_h_execute(&mut self) {
        self.edit(|e| e.flip(true));
    }
    fn flip_v_execute(&mut self) {
        self.edit(|e| e.flip(false));
    }
    fn lay_tb_execute(&mut self) {
        self.edit(|e| e.apply_layout(LayoutKind::HierTb));
    }
    fn lay_lr_execute(&mut self) {
        self.edit(|e| e.apply_layout(LayoutKind::HierLr));
    }
    fn lay_circle_execute(&mut self) {
        self.edit(|e| e.apply_layout(LayoutKind::Circle));
    }
    fn lay_grid_execute(&mut self) {
        self.edit(|e| e.apply_layout(LayoutKind::Grid));
    }
    fn rot_left_execute(&mut self) {
        self.edit(|e| e.rotate(-90.0, false));
    }
    fn rot_right_execute(&mut self) {
        self.edit(|e| e.rotate(90.0, false));
    }

    // ── Affichage ───────────────────────────────────────────────────────────

    fn zin_execute(&mut self) {
        self.edit(Editor::zoom_in);
    }
    fn zout_execute(&mut self) {
        self.edit(Editor::zoom_out);
    }
    fn zfit_execute(&mut self) {
        self.edit(Editor::zoom_to_fit);
    }
    fn zreset_execute(&mut self) {
        self.edit(Editor::reset_view);
    }
    fn grid_execute(&mut self) {
        self.edit(|e| e.show_grid = !e.show_grid);
    }
    fn snap_execute(&mut self) {
        self.edit(|e| e.snap = !e.snap);
    }
    fn minimap_execute(&mut self) {
        self.edit(|e| e.show_minimap = !e.show_minimap);
    }
    fn rulers_execute(&mut self) {
        self.edit(|e| e.show_rulers = !e.show_rulers);
    }
    fn th_blue_execute(&mut self) {
        self.edit(|e| e.apply_theme("#dae8fc", "#6c8ebf", "#000000", "#6c8ebf"));
    }
    fn th_green_execute(&mut self) {
        self.edit(|e| e.apply_theme("#d5e8d4", "#82b366", "#000000", "#82b366"));
    }
    fn th_gray_execute(&mut self) {
        self.edit(|e| e.apply_theme("#f5f5f5", "#666666", "#000000", "#666666"));
    }
    fn th_mono_execute(&mut self) {
        self.edit(|e| e.apply_theme("#ffffff", "#000000", "#000000", "#000000"));
    }
    fn th_dark_execute(&mut self) {
        self.edit(|e| e.apply_theme("#2b2b2b", "#b0b0b0", "#ffffff", "#b0b0b0"));
    }

    fn status_zoom_changed(&mut self, e: &NumericValueChangedEventArgs) {
        let z = (e.new / 100.0) as f64;
        self.edit(|ed| ed.set_zoom(z));
    }

    // ── Connecteur ──────────────────────────────────────────────────────────

    fn r_straight_execute(&mut self) {
        self.conn_style("routing", "straight".into());
    }
    fn r_ortho_execute(&mut self) {
        self.conn_style("routing", "orthogonal".into());
    }
    fn r_curved_execute(&mut self) {
        self.conn_style("routing", "curved".into());
    }
    fn l_solid_execute(&mut self) {
        self.conn_style("strokeStyle", "solid".into());
    }
    fn l_dashed_execute(&mut self) {
        self.conn_style("strokeStyle", "dashed".into());
    }
    fn l_dotted_execute(&mut self) {
        self.conn_style("strokeStyle", "dotted".into());
    }
    fn arrow_end_execute(&mut self) {
        let v = if self.arrow_end { "none" } else { "block" };
        self.conn_style("arrowEnd", v.into());
    }
    fn arrow_start_execute(&mut self) {
        let v = if self.arrow_start { "none" } else { "block" };
        self.conn_style("arrowStart", v.into());
    }
    fn w1_execute(&mut self) {
        self.conn_style("strokeWidth", js_number(1.0));
    }
    fn w2_execute(&mut self) {
        self.conn_style("strokeWidth", js_number(2.0));
    }
    fn w3_execute(&mut self) {
        self.conn_style("strokeWidth", js_number(3.0));
    }
    fn w4_execute(&mut self) {
        self.conn_style("strokeWidth", js_number(4.0));
    }
    fn reverse_execute(&mut self) {
        if let Some(cid) = self.sel_conn() {
            self.edit(|e| e.conn_reverse(&cid));
        }
    }
    fn conn_dup_execute(&mut self) {
        if let Some(cid) = self.sel_conn() {
            self.edit(|e| e.conn_duplicate(&cid));
        }
    }
    fn conn_del_execute(&mut self) {
        if let Some(cid) = self.sel_conn() {
            self.edit(|e| e.delete_connector(&cid));
        }
    }
    fn conn_front_execute(&mut self) {
        if let Some(cid) = self.sel_conn() {
            self.edit(|e| e.conn_reorder(&cid, true));
        }
    }
    fn conn_back_execute(&mut self) {
        if let Some(cid) = self.sel_conn() {
            self.edit(|e| e.conn_reorder(&cid, false));
        }
    }
    fn edit_label_execute(&mut self) {
        if let Some(cid) = self.sel_conn() {
            self.edit(|e| e.start_label_edit(&cid, true));
        }
    }
    fn add_node_execute(&mut self) {
        let (cid, seg, x, y) = (self.context.id.clone(), self.context.seg, self.context.world_x, self.context.world_y);
        self.edit(|e| e.conn_add_node(&cid, seg, x, y));
    }
    fn clear_nodes_execute(&mut self) {
        if let Some(cid) = self.sel_conn() {
            self.edit(|e| e.conn_clear_nodes(&cid));
        }
    }

    // ── Forme ───────────────────────────────────────────────────────────────

    fn edit_text_execute(&mut self) {
        let id = self.context.id.clone();
        self.edit(|e| e.start_label_edit(&id, false));
    }
    fn sh_solid_execute(&mut self) {
        self.shape_style("strokeStyle", "solid".into());
    }
    fn sh_dashed_execute(&mut self) {
        self.shape_style("strokeStyle", "dashed".into());
    }
    fn sh_dotted_execute(&mut self) {
        self.shape_style("strokeStyle", "dotted".into());
    }
    fn sh_shadow_execute(&mut self) {
        let v = !self.shape_shadow;
        self.shape_style("shadow", Value::Bool(v));
    }
    fn fill_picked(&mut self, e: &TextChangedEventArgs) {
        let c = if e.new.is_empty() { "none".to_string() } else { e.new.clone() };
        self.shape_style("fillColor", c.into());
    }
    fn outline_picked(&mut self, e: &TextChangedEventArgs) {
        let c = if e.new.is_empty() { "none".to_string() } else { e.new.clone() };
        self.shape_style("strokeColor", c.into());
    }
    fn no_fill_execute(&mut self) {
        self.shape_style("fillColor", "none".into());
    }
    fn no_outline_execute(&mut self) {
        self.shape_style("strokeColor", "none".into());
    }

    // ── The page bar ────────────────────────────────────────────────────────

    fn page_selected(&mut self, e: &PageTabEventArgs) {
        let i = e.index;
        self.edit(|ed| ed.set_current_page(i));
    }

    fn page_added(&mut self) {
        let Some(n) = self.canvas.with(|c| c.editor().pages().len()) else { return };
        let name = Resources::page_n().replace("{n}", &(n + 1).to_string());
        let id = new_page_id();
        self.edit(|e| {
            e.add_page(Page::new(&id, &name, Default::default()));
        });
        self.refresh_pages_and_layers();
    }

    fn page_renamed(&mut self, e: &PageTabEventArgs) {
        let (i, current) = (e.index, e.name.clone());
        let Some(name) = super::prompt_dialog::PromptDialog::ask(self, Resources::rename_page_title(), &current) else { return };
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        self.edit(|ed| {
            let id = ed.pages().get(i).map(|p| p.id.clone());
            if let Some(id) = id {
                ed.rename_page(&id, &name);
            }
        });
        self.refresh_pages_and_layers();
    }

    fn page_deleted(&mut self, e: &PageTabEventArgs) {
        let (i, name) = (e.index, e.name.clone());
        let title = Resources::delete_page_title().replace("{name}", &name);
        if !super::confirm_dialog::ConfirmDialog::ask(self, &title, Resources::delete_page_text(), Resources::cmd_delete(), Resources::cancel()) {
            return;
        }
        self.edit(|ed| {
            let id = ed.pages().get(i).map(|p| p.id.clone());
            if let Some(id) = id {
                ed.remove_page(&id);
            }
        });
        self.refresh_pages_and_layers();
    }

    // ── The layers panel ────────────────────────────────────────────────────

    fn layers_command(&mut self, e: &LayersCommandEventArgs) {
        let (cmd, id) = (e.command.clone(), e.id.clone());
        match cmd.as_str() {
            "activate" => self.edit(|ed| ed.set_active_layer(&id)),
            "add" => self.edit(Editor::add_layer),
            "delete" => self.edit(|ed| ed.delete_layer(&id)),
            "visible" => self.edit(|ed| ed.toggle_layer_visible(&id)),
            "lock" => self.edit(|ed| ed.toggle_layer_locked(&id)),
            "up" => self.edit(|ed| ed.move_layer(&id, true)),
            "down" => self.edit(|ed| ed.move_layer(&id, false)),
            "move_here" => self.edit(|ed| ed.move_selection_to_layer(&id)),
            "rename" => {
                let current = e.name.clone();
                if let Some(name) = super::prompt_dialog::PromptDialog::ask(self, Resources::rename_layer_title(), &current) {
                    let name = if name.trim().is_empty() { current } else { name.trim().to_string() };
                    self.edit(|ed| ed.rename_layer(&id, &name));
                }
            }
            _ => {}
        }
        self.refresh_pages_and_layers();
    }
}

/// Whether closing must first ask about unsaved changes: only without a server session (a server diagram is
/// saved by the session and its journal; a local file or the sample is not).
fn must_ask_before_close(has_session: bool, dirty: bool) -> bool {
    !has_session && dirty
}

#[cfg(test)]
mod close_tests {
    #[test]
    fn a_modified_local_diagram_is_never_closed_silently() {
        assert!(super::must_ask_before_close(false, true));
        assert!(!super::must_ask_before_close(false, false));
        assert!(!super::must_ask_before_close(true, true));
    }
}

/// A page id minted here (the server honours it: `CreatePageDto.id`): a version-4 UUID.
fn new_page_id() -> String {
    kubuno_office_documents_common::api::client::fresh_uuid_v4()
}

/// The label of a placed stencil: its translation (`stencil_<id>`), else its French name (the web's
/// `t('stencil_' + id, { defaultValue: name })`).
pub fn stencil_label(st: &kubuno_office_diagrams_core::stencils::StencilDef) -> String {
    let s = kubuno_desktop::resources::string(&format!("stencil_{}", st.id));
    if s.is_empty() {
        st.name.to_string()
    } else {
        s
    }
}

/// The name of a stencil id (the status bar's type), the id itself when unknown.
pub fn stencil_name_of(kind: &str) -> String {
    match kubuno_office_diagrams_core::stencils::stencil(kind) {
        Some(st) => stencil_label(st),
        None => kind.to_string(),
    }
}

// The Propriétés panel's handlers live beside this file (the panel's own work: `diagram_window_format.rs`).
#[path = "diagram_window_format.rs"]
mod format_glue;
