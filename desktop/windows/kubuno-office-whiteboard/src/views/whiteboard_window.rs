//! Code-behind of Kubuno Whiteboard's window (`whiteboard_window.kbview`) — Windows Forms' `Form1.cs`.
//!
//! The canvas owns the open board (the shared engine's Yjs document, viewport and undo); the window opens
//! the board into it, binds what it reports (`StateChanged`: the zoom, the counts, the active tool, the
//! selection and the undo state) and turns the ribbon's commands into the canvas' edits. A board of the
//! server (`--doc`) is kept in step by the real-time collaboration room
//! (`kubuno_office_desktop_shared::collab::Room`, the Yjs WebSocket): incoming updates are applied to the
//! document; the local updates the canvas produces are relayed back to the room (increment C).

use kubuno_desktop::prelude::*;
use kubuno_desktop::views::component::Control as _;
use kubuno_office_desktop_shared::collab::{Event as RoomEvent, Room, Sink, Status};
use kubuno_office_whiteboard_common::collab::{self, Collab};
use kubuno_office_whiteboard_common::doc::{OpenBoard, Source};

use crate::controls::board_canvas::{BoardCanvas, BoardStateEventArgs};
use crate::controls::layer_list::{LayerList, LayerSelectEventArgs};
use crate::{PropRow, PropertiesPanel, Resources};
use serde_json::Value as Json;

/// How the window starts: desktop/common's launch options.
pub use kubuno_office_whiteboard_common::app::Options;

fn now() -> u64 {
    kubuno_desktop::controls::host::now_ms()
}

/// Kubuno Whiteboard's main window.
#[kubuno_desktop::view("whiteboard_window.kbview")]
pub struct WhiteboardWindow {
    #[control]
    canvas: Custom<BoardCanvas>,
    #[control]
    layer_list: Custom<LayerList>,
    #[control]
    properties_panel: Custom<PropertiesPanel>,
    #[bind]
    window_title: String,
    #[bind]
    zoom_percent: f32,
    // ── Tools ──
    #[bind]
    tool_select: bool,
    #[bind]
    tool_hand: bool,
    #[bind]
    tool_sticky: bool,
    #[bind]
    tool_text: bool,
    #[bind]
    tool_shape: bool,
    #[bind]
    tool_arrow: bool,
    #[bind]
    tool_pen: bool,
    #[bind]
    tool_eraser: bool,
    #[bind]
    tool_frame: bool,
    // ── Editing / arrange ──
    #[bind]
    can_undo: bool,
    #[bind]
    can_redo: bool,
    #[bind]
    has_selection: bool,
    #[bind]
    multi: bool,
    #[bind]
    has_group: bool,
    // ── Background ──
    #[bind]
    bg_white: bool,
    #[bind]
    bg_grid: bool,
    #[bind]
    bg_dots: bool,
    #[bind]
    bg_lines: bool,
    // ── The Backstage and the status bar ──
    #[bind]
    doc_title: String,
    #[bind]
    background: String,
    #[bind]
    elements_text: String,
    #[bind]
    strokes_text: String,
    #[bind]
    elements_status: String,
    #[bind]
    strokes_status: String,
    #[bind]
    sync_status: String,
    #[bind]
    has_sync: bool,
    /// The board to open at load.
    pending: Option<OpenBoard>,
    source: Source,
    room: Option<Room>,
    collab: Option<Collab>,
}

impl WhiteboardWindow {
    pub fn new(options: Options, board: OpenBoard) -> Self {
        let (board, source) = match &options.doc {
            Some(id) => (OpenBoard::for_server(id, "Tableau blanc", "dots"), Source::Server(id.clone())),
            None => {
                let src = board.source.clone();
                (board, src)
            }
        };
        let background = board.background.clone();
        let mut window = Self {
            window_title: format!("{} — {}", board.title, Resources::app_title()),
            doc_title: board.title.clone(),
            background: background.clone(),
            zoom_percent: 100.0,
            tool_select: true,
            source,
            pending: Some(board),
            ..Self::default()
        };
        window.set_background_flags(&background);
        window.initialize_component();
        window
    }

    fn whiteboard_window_load(&mut self) {
        if let Some(board) = self.pending.take() {
            self.canvas.with(|c| {
                c.open(board);
                c.focus();
            });
        }
        // The Propriétés / Calques panels are open by default (the DockArea's declared Side="Right"
        // arrangement); fill them with the current board state.
        self.update_panels();
        if let Source::Server(id) = self.source.clone() {
            self.start_live(id);
        }
        let mut header = kubuno_desktop_header_data::FeedConfig::new("kubuno-whiteboard");
        header.proxy = kubuno_desktop_sync::get_proxy();
        kubuno_desktop_header_data::start(kubuno_desktop_header_data::HeaderOptions::for_app(&["office-whiteboard"]), header, false, self.dispatcher());
    }

    fn whiteboard_window_closing(&mut self, _e: &mut kubuno_desktop::views::events::FormClosingEventArgs) {
        if let Some(room) = self.room.take() {
            room.close();
        }
    }

    // ── A board of the server: the Yjs room (bidirectional) ──────────────────

    fn start_live(&mut self, id: String) {
        let Some(dispatcher) = self.dispatcher() else {
            kubuno_desktop::tracing::warn!("[whiteboard] no dispatcher: the server board cannot open");
            return;
        };
        self.sync_status = "Connexion…".into();
        self.has_sync = true;
        self.collab = Some(Collab::new(&id, now()));
        let post = dispatcher.clone();
        let sink: Sink = std::sync::Arc::new(move |event| {
            drop(post.begin_invoke(move |w: &mut WhiteboardWindow| w.on_room(event)));
        });
        let url_id = id.clone();
        self.room = Some(Room::open("whiteboard", move || collab::sync_url(&url_id), sink));
    }

    fn on_room(&mut self, event: RoomEvent) {
        match event {
            RoomEvent::Status(s) => {
                self.sync_status = match s {
                    Status::Connecting => "Connexion…".into(),
                    Status::Connected => "Connecté".into(),
                    Status::Disconnected(_) => "Hors ligne".into(),
                };
            }
            RoomEvent::Binary(bytes) => {
                self.canvas.with(|c| {
                    let _ = c.board().doc.apply_update(&bytes);
                    c.refresh();
                });
            }
            RoomEvent::Text(text) => {
                if let Some(collab) = self.collab.as_mut() {
                    collab.on_text(&text, now());
                }
            }
        }
    }

    /// Publishes to the room the first state and any local update, once the room is connected.
    fn publish(&mut self, update: Vec<u8>) {
        if update.is_empty() {
            return;
        }
        if let Some(room) = self.room.as_ref() {
            room.send_binary(update);
        }
    }

    // ── Tools ─────────────────────────────────────────────────────────────────

    fn set_tool(&mut self, tool: &str) {
        self.canvas.with(|c| c.set_tool(tool));
        self.tool_select = tool == "select";
        self.tool_hand = tool == "hand";
        self.tool_sticky = tool == "sticky";
        self.tool_text = tool == "text";
        self.tool_shape = tool == "shape";
        self.tool_arrow = tool == "arrow";
        self.tool_pen = tool == "pen";
        self.tool_eraser = tool == "eraser";
        self.tool_frame = tool == "frame";
    }
    fn tool_select_execute(&mut self) {
        self.set_tool("select");
    }
    fn tool_hand_execute(&mut self) {
        self.set_tool("hand");
    }
    fn tool_sticky_execute(&mut self) {
        self.set_tool("sticky");
    }
    fn tool_text_execute(&mut self) {
        self.set_tool("text");
    }
    fn tool_shape_execute(&mut self) {
        self.set_tool("shape");
    }
    fn tool_arrow_execute(&mut self) {
        self.set_tool("arrow");
    }
    fn tool_pen_execute(&mut self) {
        self.set_tool("pen");
    }
    fn tool_eraser_execute(&mut self) {
        self.set_tool("eraser");
    }
    fn tool_frame_execute(&mut self) {
        self.set_tool("frame");
    }

    // ── Editing / arrange ───────────────────────────────────────────────────

    fn undo_execute(&mut self) {
        self.canvas.with(|c| c.undo());
    }
    fn redo_execute(&mut self) {
        self.canvas.with(|c| c.redo());
    }
    fn delete_execute(&mut self) {
        self.canvas.with(|c| c.delete_selection());
    }
    fn front_execute(&mut self) {
        self.canvas.with(|c| c.bring_to_front());
    }
    fn back_execute(&mut self) {
        self.canvas.with(|c| c.send_to_back());
    }
    fn group_execute(&mut self) {
        self.canvas.with(|c| c.group_selection());
    }
    fn ungroup_execute(&mut self) {
        self.canvas.with(|c| c.ungroup_selection());
    }

    // ── View ────────────────────────────────────────────────────────────────

    fn zin_execute(&mut self) {
        self.canvas.with(|c| c.zoom_in());
    }
    fn zout_execute(&mut self) {
        self.canvas.with(|c| c.zoom_out());
    }
    fn zfit_execute(&mut self) {
        self.canvas.with(|c| c.zoom_fit());
    }
    fn zreset_execute(&mut self) {
        self.canvas.with(|c| c.zoom_reset());
    }
    fn status_zoom_changed(&mut self, e: &NumericValueChangedEventArgs) {
        self.canvas.with(|c| c.set_zoom_percent(e.new));
    }

    // ── Background ────────────────────────────────────────────────────────────

    fn bg_white_execute(&mut self) {
        self.set_background("white");
    }
    fn bg_grid_execute(&mut self) {
        self.set_background("grid");
    }
    fn bg_dots_execute(&mut self) {
        self.set_background("dots");
    }
    fn bg_lines_execute(&mut self) {
        self.set_background("lines");
    }
    fn set_background(&mut self, bg: &str) {
        self.canvas.with(|c| c.set_background(bg));
        self.set_background_flags(bg);
        self.background = bg.to_string();
    }
    fn set_background_flags(&mut self, bg: &str) {
        self.bg_white = bg == "white";
        self.bg_grid = bg == "grid";
        self.bg_dots = bg == "dots";
        self.bg_lines = bg == "lines";
    }

    fn bs_close_click(&mut self) {
        self.close();
    }

    // ── What the canvas reports ──────────────────────────────────────────────

    fn canvas_state_changed(&mut self, e: &BoardStateEventArgs) {
        self.zoom_percent = e.zoom;
        self.elements_text = e.elements.to_string();
        self.strokes_text = e.strokes.to_string();
        self.elements_status = format!("{} éléments", e.elements);
        self.strokes_status = format!("{} tracés", e.strokes);
        self.can_undo = e.can_undo;
        self.can_redo = e.can_redo;
        self.has_selection = e.selected > 0;
        self.multi = e.selected > 1;
        self.has_group = e.has_group;
        if self.background != e.background {
            let bg = e.background.clone();
            self.background = bg.clone();
            self.set_background_flags(&bg);
        }
        self.update_panels();
        // Relay a local edit to the collaboration room (increment C).
        if !e.update.is_empty() {
            self.publish(e.update.clone());
        }
    }

    // ── Panels ────────────────────────────────────────────────────────────────

    fn layer_selected(&mut self, e: &LayerSelectEventArgs) {
        let id = e.id.clone();
        self.canvas.with(|c| c.select_only(&id));
    }

    fn update_panels(&mut self) {
        let rows = self.canvas.with(|c| c.element_rows()).unwrap_or_default();
        let selected = self.canvas.with(|c| c.primary_selection()).flatten().unwrap_or_default();
        self.layer_list.with(|l| l.set_rows(rows, selected));
        let json = self.canvas.with(|c| c.selected_element_json()).flatten();
        self.has_selection = json.is_some();
        let prop_rows = json.map(|obj| Self::property_rows(&obj)).unwrap_or_default();
        self.properties_panel.with(|p| p.set_rows(prop_rows));
    }

    /// The Propriétés rows for the selected element's JSON (the web's property inspector).
    fn property_rows(obj: &Json) -> Vec<PropRow> {
        let s = |k: &str| obj.get(k).and_then(Json::as_str).unwrap_or("").to_string();
        let n = |k: &str| obj.get(k).and_then(Json::as_f64).unwrap_or(0.0);
        let ty = s("type");
        let kind = match ty.as_str() {
            "sticky" => "Post-it",
            "text" => "Texte",
            "shape" => "Forme",
            "arrow" => "Flèche",
            "frame" => "Cadre",
            "image" => "Image",
            _ => "Élément",
        };
        let mut rows = vec![PropRow::new("Type", kind)];
        if ty == "arrow" {
            rows.push(PropRow::new("Position", format!("{:.0}, {:.0} → {:.0}, {:.0}", n("startX"), n("startY"), n("endX"), n("endY"))));
        } else {
            rows.push(PropRow::new("Position", format!("{:.0}, {:.0}", n("x"), n("y"))));
            rows.push(PropRow::new("Taille", format!("{:.0} × {:.0}", n("width"), n("height"))));
        }
        // Colour row: a shape shows its fill, everything else its colour.
        let color = match ty.as_str() {
            "shape" => s("fill"),
            "sticky" | "text" | "arrow" | "frame" => s("color"),
            _ => String::new(),
        };
        if !color.is_empty() {
            rows.push(PropRow::new("Couleur", color));
        }
        // Outline (shapes only).
        if ty == "shape" {
            let stroke = s("stroke");
            if !stroke.is_empty() {
                rows.push(PropRow::new("Contour", stroke));
            }
        }
        // Text (sticky / text box).
        if ty == "sticky" || ty == "text" {
            rows.push(PropRow::new("Texte", s("text")));
        }
        rows
    }
}
