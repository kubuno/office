//! Code-behind of Kubuno Whiteboard's window (`whiteboard_window.kbview`) — Windows Forms' `Form1.cs`.
//!
//! The canvas owns the open board (the shared engine's Yjs document and viewport); the window opens the
//! board into it, binds what it reports (`StateChanged`: the zoom, the counts, the background) and turns
//! the ribbon's commands into view changes. A board of the server (`--doc`) is filled by the real-time
//! collaboration room (`kubuno_office_desktop_shared::collab::Room`, the Yjs WebSocket): increment A applies
//! the room's updates read-only; sending local edits is increment C.

use kubuno_desktop::prelude::*;
use kubuno_desktop::views::component::Control as _;
use kubuno_office_desktop_shared::collab::{Event as RoomEvent, Room, Sink, Status};
use kubuno_office_whiteboard_common::collab::{self, Collab};
use kubuno_office_whiteboard_common::doc::{OpenBoard, Source};

use crate::controls::board_canvas::{BoardCanvas, BoardStateEventArgs};
use crate::Resources;

/// How the window starts: desktop/common's launch options.
pub use kubuno_office_whiteboard_common::app::Options;

/// A monotonic clock in milliseconds.
fn now() -> u64 {
    kubuno_desktop::controls::host::now_ms()
}

/// Kubuno Whiteboard's main window.
#[kubuno_desktop::view("whiteboard_window.kbview")]
pub struct WhiteboardWindow {
    #[control]
    canvas: Custom<BoardCanvas>,
    #[bind]
    window_title: String,
    #[bind]
    zoom_percent: f32,
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
    /// Where the board comes from.
    source: Source,
    /// The server board's collaboration room and its sync bookkeeping.
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

    // ── A board of the server: the Yjs room (read-only in increment A) ──────

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
                // Read-only: apply the room's update to the board's document and repaint.
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

    // ── View commands ───────────────────────────────────────────────────────

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
        if self.background != e.background {
            let bg = e.background.clone();
            self.background = bg.clone();
            self.set_background_flags(&bg);
        }
    }
}
