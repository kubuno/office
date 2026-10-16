//! Real-time collaboration: the web editor's Y.Doc, platform-neutral.
//!
//! The web binds a board to a Yjs document synced over its own WebSocket - **not** the core's generic
//! `/collab/<entity>/<id>/sync` room, but the office module's `/api/v1/office/whiteboard/boards/<id>/sync`
//! (`WhiteboardApp.tsx:352-358`, server `whiteboard/handlers/websocket.rs`). The document has exactly two
//! shared types (`WhiteboardApp.tsx:314-315`): a `Y.Map` `elements` keyed by element id, and a `Y.Array`
//! `strokes`. The board's background and title are metadata (a REST column), not content.
//!
//! [`doc::WhiteboardDoc`] is that document (the `yrs` port of Yjs, the implementation the core's
//! collaboration service and the spreadsheet engine use too). The transport (a WebSocket) is the caller's:
//! the desktop reuses `kubuno-office-desktop-shared::collab`, whose binary-frame protocol matches the
//! whiteboard endpoint, with the URL built by [`sync_url`]. Nothing here reads a clock or a random source:
//! the client id and the time are passed in.

pub mod doc;
pub mod json;

pub use doc::{element_id, WhiteboardDoc};

/// The whiteboard WebSocket URL on a server (the office module's own endpoint; the token in the query, as
/// native clients do - `WhiteboardApp.tsx:355`). `server_url` is the core's base (`https://host`).
pub fn sync_url(server_url: &str, board_id: &str, token: &str) -> String {
    let base = server_url.trim_end_matches('/');
    let base = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        base.to_string()
    };
    format!("{base}/api/v1/office/whiteboard/boards/{}/sync?token={}", encode(board_id), encode(token))
}

/// The room name the desktop uses for its local bookkeeping (`useCollab(\`office-whiteboard:${id}\`)`,
/// `WhiteboardApp.tsx:352`). The board's own WS endpoint does not take a room in the path, but the window
/// keys its state by this string, like the other editors.
pub fn room(board_id: &str) -> String {
    format!("office-whiteboard:{board_id}")
}

/// `encodeURIComponent` for the path and query pieces.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A failure to read a frame of the room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollabError {
    /// A binary frame is not a Yjs v1 update.
    BadUpdate(String),
}

impl std::fmt::Display for CollabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CollabError::BadUpdate(e) => write!(f, "invalid Yjs update: {e}"),
        }
    }
}

impl std::error::Error for CollabError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_url_switches_scheme_and_hits_the_office_endpoint() {
        assert_eq!(
            sync_url("https://dev.kubuno.com/", "b 1", "tok/en"),
            "wss://dev.kubuno.com/api/v1/office/whiteboard/boards/b%201/sync?token=tok%2Fen"
        );
        assert_eq!(
            sync_url("http://127.0.0.1:8080", "abc", "t"),
            "ws://127.0.0.1:8080/api/v1/office/whiteboard/boards/abc/sync?token=t"
        );
    }
}
