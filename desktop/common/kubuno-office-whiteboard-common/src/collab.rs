//! Real-time collaboration of an open server board, as the web editor does it (`WhiteboardApp.tsx:314-358`,
//! `web/src/collab/collabProvider.ts`).
//!
//! [`Collab`] is what the window holds on its thread: the Yjs document (`WhiteboardDoc`) and the rules that
//! tie it to the board - the greeting, the first synchronisation (adopting the room's content, or seeding
//! it from a local board), and collaborators' updates mirrored without local-undo entries. It produces the
//! frames to send and never touches the network: the room's thread
//! (`kubuno_office_desktop_shared::collab::Room`) carries them, and [`sync_url`] builds the board's own
//! WebSocket URL for the account the shell shows.
//!
//! Presence (awareness) is relayed by the room as text frames; it is a follow-up here - this module tracks
//! and syncs the board's *content* (the `elements` map and the `strokes` array).

use kubuno_office_whiteboard_core::collab::{sync_url as core_sync_url, WhiteboardDoc};
use serde_json::Value as Json;

/// After the greeting, the room sends its persisted parts at once; when nothing more has arrived for this
/// long the first synchronisation is complete (the web seeds from a React effect, after the parts).
pub const SETTLE_MS: u64 = 300;

/// A frame to send to the room.
#[derive(Debug, Clone, PartialEq)]
pub enum Outgoing {
    Binary(Vec<u8>),
    Text(String),
}

/// What a received frame changed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Changed {
    /// The board's content (elements, strokes): the window rebuilds the scene and repaints.
    pub content: bool,
}

/// The greeting the server sends first (`{"type":"sync","empty":bool}`), or a presence frame we ignore.
fn parse_greeting(text: &str) -> Option<bool> {
    let v: Json = serde_json::from_str(text).ok()?;
    if v.get("type").and_then(Json::as_str) == Some("sync") {
        Some(v.get("empty").and_then(Json::as_bool).unwrap_or(false))
    } else {
        None
    }
}

/// The collaboration state of one open board.
pub struct Collab {
    /// `{"type":"sync"}` received: `Some(empty)`.
    greeted: Option<bool>,
    last_frame: u64,
    seeded: bool,
    /// The room id, `office-whiteboard:<id>` (for the window's bookkeeping).
    pub room: String,
}

impl Collab {
    /// The state for board `board_id`.
    pub fn new(board_id: &str, now: u64) -> Collab {
        Collab { greeted: None, last_frame: now, seeded: false, room: kubuno_office_whiteboard_core::collab::room(board_id) }
    }

    /// Whether the first synchronisation is done (local edits are published from then on).
    pub fn is_synced(&self) -> bool {
        self.seeded
    }

    /// The socket opened (`onopen`): the whole local state, when there is any (more than the 2-byte empty
    /// update), so a board seeded locally reaches the room.
    pub fn on_connected(&self, doc: &WhiteboardDoc) -> Vec<Outgoing> {
        let state = doc.state_as_update();
        if state.len() > 2 {
            vec![Outgoing::Binary(state)]
        } else {
            Vec::new()
        }
    }

    /// A Yjs update from the room: applied to the document (origin `remote`, so it is never undoable).
    pub fn on_binary(&mut self, doc: &WhiteboardDoc, bytes: &[u8], now: u64) -> Result<Changed, String> {
        self.last_frame = now;
        doc.apply_update(bytes).map_err(|e| e.to_string())?;
        Ok(Changed { content: true })
    }

    /// A text frame from the room: the greeting (presence is ignored for now).
    pub fn on_text(&mut self, text: &str, now: u64) {
        self.last_frame = now;
        if let Some(empty) = parse_greeting(text) {
            if self.greeted.is_none() {
                self.greeted = Some(empty);
            }
        }
    }

    /// Whether the first synchronisation can run now: after the greeting, at once when the room said it was
    /// empty, otherwise once the parts have settled.
    pub fn ready_to_seed(&self, now: u64) -> bool {
        match self.greeted {
            None => false,
            Some(_) if self.seeded => false,
            Some(true) => true,
            Some(false) => now.saturating_sub(self.last_frame) >= SETTLE_MS,
        }
    }

    /// The first synchronisation: if the room held no content and the local board has some, seed the room
    /// with it; otherwise adopt what the room sent. Returns the update to publish, if any.
    pub fn seed(&mut self, doc: &WhiteboardDoc, local_elements: &serde_json::Map<String, Json>, local_strokes: &[Json]) -> Option<Outgoing> {
        self.seeded = true;
        let room_empty = doc.elements_len() == 0 && doc.strokes_len() == 0;
        if room_empty && !(local_elements.is_empty() && local_strokes.is_empty()) {
            let mut update: Option<Vec<u8>> = doc.write_elements(local_elements);
            if let Some(u) = doc.write_strokes(local_strokes) {
                update = Some(match update {
                    Some(mut first) => {
                        first.extend_from_slice(&u);
                        first
                    }
                    None => u,
                });
            }
            return update.map(Outgoing::Binary);
        }
        None
    }
}

/// A random 32-bit client id, as Yjs draws one.
pub fn new_client_id() -> u64 {
    let id = kubuno_office_desktop_shared::ids::fresh_uuid_v4().replace('-', "");
    u64::from_str_radix(&id[..8], 16).unwrap_or(1) & 0xffff_ffff
}

/// The user as the web publishes it (`id`, `display_name || email || id`).
pub fn current_user() -> Option<(String, String)> {
    let account = kubuno_desktop_sync::current_account().ok().flatten()?;
    let name = account.display_name.clone().filter(|n| !n.trim().is_empty()).or_else(|| account.email.clone()).unwrap_or_else(|| account.user_id.clone());
    Some((account.user_id, name))
}

/// The board's WebSocket URL for the account the shell shows, with a current access token (run on the
/// room's thread before every connection).
pub fn sync_url(board_id: &str) -> Result<String, String> {
    let account = kubuno_desktop_sync::current_account().map_err(|e| format!("compte : {e}"))?.ok_or_else(|| "aucun compte connecté".to_string())?;
    let token = kubuno_desktop_sync::account_access_token(&account).map_err(|e| format!("jeton : {e}"))?;
    Ok(core_sync_url(&account.server_url, board_id, &token))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_greeting_is_recognised_and_presence_ignored() {
        assert_eq!(parse_greeting(r#"{"type":"sync","empty":true}"#), Some(true));
        assert_eq!(parse_greeting(r#"{"type":"sync","empty":false}"#), Some(false));
        assert_eq!(parse_greeting(r#"{"kbAwareness":"..."}"#), None);
    }

    #[test]
    fn an_empty_room_is_seeded_from_the_local_board() {
        let doc = WhiteboardDoc::new(1);
        let mut c = Collab::new("b1", 0);
        c.on_text(r#"{"type":"sync","empty":true}"#, 0);
        assert!(c.ready_to_seed(0));
        let mut els = serde_json::Map::new();
        els.insert("e1".into(), json!({"id":"e1","type":"shape","x":0.0,"y":0.0,"width":10.0,"height":10.0,"kind":"rect"}));
        let out = c.seed(&doc, &els, &[]);
        assert!(matches!(out, Some(Outgoing::Binary(_))));
        assert_eq!(doc.elements_len(), 1);
        assert!(c.is_synced());
    }

    #[test]
    fn a_non_empty_room_is_adopted_not_seeded() {
        let doc = WhiteboardDoc::new(1);
        doc.set_element("x", &json!({"id":"x","type":"frame","x":0.0,"y":0.0,"width":5.0,"height":5.0,"title":"F","color":"#000"}));
        let mut c = Collab::new("b1", 0);
        c.on_text(r#"{"type":"sync","empty":false}"#, 0);
        assert!(!c.ready_to_seed(100));
        assert!(c.ready_to_seed(1000));
        let els = serde_json::Map::new();
        assert!(c.seed(&doc, &els, &[]).is_none());
        assert_eq!(doc.elements_len(), 1);
    }
}
