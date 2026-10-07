//! Real-time collaboration of an open server workbook, as the web editor does it (`SA:643-680`,
//! `SA:870-897`, `SA:3159-3301`, `web/src/collab/collabProvider.ts`).
//!
//! [`Collab`] is what the window holds on its thread: the Y.Doc and the presence
//! (`kubuno_office_sheets_core::collab`) and the rules that tie them to the editor — first synchronisation
//! (seeding), collaborators' changes mirrored without undo entries, local edits published as one diff per
//! sheet, presence published and read back. It produces the frames to send and never touches the network:
//! the room's thread (`kubuno_office_desktop_shared::collab::Room`) carries them. [`room_url`] builds the
//! room's URL for the account the shell shows.
//!
//! The REST save keeps running beside it, as on the web (which both saves through REST and mirrors to Yjs).

use kubuno_office_sheets_core::addr;
use kubuno_office_sheets_core::collab::awareness::{self, awareness_frame, parse_text, TextFrame};
use kubuno_office_sheets_core::collab::{room, user_color, Awareness, Peer, Seed, SheetsDoc};
use kubuno_office_sheets_core::editor::Editor;
use serde_json::{json, Map, Value as Json};

/// After the greeting, the room sends its persisted parts at once; when nothing more has arrived for this
/// long the first synchronisation is complete (the web seeds from a React effect, after the parts).
pub const SETTLE_MS: u64 = 300;
/// The pointer is published at most this often (`usePublishCursor`, ~40 ms).
pub const CURSOR_MS: u64 = 40;

/// A frame to send to the room.
#[derive(Debug, Clone, PartialEq)]
pub enum Outgoing {
    Binary(Vec<u8>),
    Text(String),
}

/// What a received frame changed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Changed {
    /// The workbook (cells, names): the window repaints and recomputes.
    pub content: bool,
    /// The peers (selections, pointers, avatars).
    pub presence: bool,
}

/// The collaboration state of one open workbook.
pub struct Collab {
    doc: SheetsDoc,
    presence: Awareness,
    /// `{"type":"sync"}` received: `Some(empty)`.
    greeted: Option<bool>,
    last_frame: u64,
    seeded: bool,
    last_cursor: u64,
    /// The room id, `office-spreadsheet:<id>`.
    pub room: String,
}

impl Collab {
    /// The state for workbook `spreadsheet_id`, as client `client_id` (a random 32-bit number).
    pub fn new(spreadsheet_id: &str, client_id: u64, now: u64) -> Collab {
        Collab { doc: SheetsDoc::new(client_id), presence: Awareness::new(client_id, now), greeted: None, last_frame: now, seeded: false, last_cursor: 0, room: room(spreadsheet_id) }
    }

    /// Whether the first synchronisation is done (local edits are published from then on).
    pub fn is_synced(&self) -> bool {
        self.seeded
    }

    /// The socket opened (`onopen`): the whole local state and our presence.
    pub fn on_connected(&mut self) -> Vec<Outgoing> {
        let mut out = Vec::new();
        let state = self.doc.state_as_update();
        if state.len() > 2 {
            out.push(Outgoing::Binary(state));
        }
        if self.presence.local_state().is_some() {
            out.push(Outgoing::Text(awareness_frame(&self.presence.encode_local())));
        }
        out
    }

    /// A Yjs update from the room.
    pub fn on_binary(&mut self, bytes: &[u8], now: u64) -> Result<(), String> {
        self.last_frame = now;
        self.doc.apply_update(bytes).map_err(|e| e.to_string())
    }

    /// A text frame from the room: the greeting or presence. Returns the frames to answer with.
    pub fn on_text(&mut self, text: &str, now: u64) -> (Vec<Outgoing>, Changed) {
        self.last_frame = now;
        let mut out = Vec::new();
        let mut changed = Changed::default();
        match parse_text(text) {
            Some(TextFrame::Sync { empty }) => {
                if self.greeted.is_none() {
                    self.greeted = Some(empty);
                }
            }
            Some(TextFrame::Awareness(bytes)) => match self.presence.apply(&bytes, now) {
                Ok((peer_changed, resend)) => {
                    changed.presence = peer_changed;
                    if let Some(r) = resend {
                        out.push(Outgoing::Text(awareness_frame(&r)));
                    }
                    // A new peer appeared: publish our state so it sees us (`onAwarenessUpdate`).
                    if peer_changed && self.presence.local_state().is_some() {
                        out.push(Outgoing::Text(awareness_frame(&self.presence.encode_local())));
                    }
                }
                Err(e) => tracing::warn!("[spreadsheets] collab: {e}"),
            },
            _ => {}
        }
        (out, changed)
    }

    /// Whether the first synchronisation can run now.
    pub fn ready_to_seed(&self, now: u64) -> bool {
        match self.greeted {
            None => false,
            Some(_) if self.seeded => false,
            Some(true) => true,
            Some(false) => now.saturating_sub(self.last_frame) >= SETTLE_MS,
        }
    }

    /// The first synchronisation (`SA:3277-3296`, `SA:889-897`): a sheet the room has cells for takes them;
    /// a sheet it has none for gives it the workbook's; the names are seeded when the room has none and then
    /// always read from it.
    pub fn seed(&mut self, ed: &mut Editor) -> (Vec<Outgoing>, bool) {
        let mut out = Vec::new();
        let mut changed = false;
        let sheets: Vec<(String, Map<String, Json>)> = ed.wb.sheets.iter().map(|s| (s.meta.id.clone(), s.cells_json().as_object().cloned().unwrap_or_default())).collect();
        for (id, cells) in sheets {
            match self.doc.seed_cells(&id, &cells) {
                Seed::Seeded(u) => out.push(Outgoing::Binary(u)),
                Seed::Adopt(room_cells) => changed |= ed.apply_remote_cells(&id, &room_cells),
                Seed::Nothing => {}
            }
        }
        if let Some(u) = self.doc.seed_names(&ed.wb.names) {
            out.push(Outgoing::Binary(u));
        }
        changed |= ed.apply_remote_names(&self.doc.names());
        self.seeded = true;
        (out, changed)
    }

    /// Mirrors the room into the editor after updates (`mirror()`, `SA:3253-3268`; the names observer,
    /// `SA:879-883`). Only after the first synchronisation.
    pub fn mirror(&mut self, ed: &mut Editor) -> bool {
        if !self.seeded {
            return false;
        }
        let ids: Vec<String> = ed.wb.sheets.iter().map(|s| s.meta.id.clone()).collect();
        let mut changed = false;
        for id in ids {
            let room_cells = self.doc.cells(&id);
            changed |= ed.apply_remote_cells(&id, &room_cells);
        }
        changed |= ed.apply_remote_names(&self.doc.names());
        changed
    }

    /// Publishes this user's edits (`commitData`, `SA:3189-3199`): one diff per sheet that changed, and the
    /// names.
    pub fn publish(&mut self, ed: &Editor) -> Vec<Outgoing> {
        if !self.seeded {
            return Vec::new();
        }
        let mut out = Vec::new();
        for s in &ed.wb.sheets {
            let cells = s.cells_json().as_object().cloned().unwrap_or_default();
            if let Some(u) = self.doc.write_cells(&s.meta.id, &cells) {
                out.push(Outgoing::Binary(u));
            }
        }
        if let Some(u) = self.doc.write_names(&ed.wb.names) {
            out.push(Outgoing::Binary(u));
        }
        out
    }

    fn field(&mut self, key: &str, value: Json, now: u64) -> Option<Outgoing> {
        self.presence.set_local_field(key, value, now).map(|u| Outgoing::Text(awareness_frame(&u)))
    }

    /// `awareness.setLocalStateField('user', …)` (`SA:658-664`).
    pub fn set_user(&mut self, id: &str, name: &str, avatar: Option<&str>, now: u64) -> Option<Outgoing> {
        self.field("user", json!({ "id": id, "name": name, "color": user_color(id), "avatar": avatar }), now)
    }

    /// The active cell for the peers (`SA:3298-3301`): column letters, 1-based row, sheet id.
    pub fn set_selection(&mut self, cell: Option<(u32, u32)>, sheet_id: &str, now: u64) -> Option<Outgoing> {
        let v = match cell {
            Some((col, row)) => json!({ "col": addr::index_to_col(col as i64), "row": row, "sheet": sheet_id }),
            None => Json::Null,
        };
        self.field("sel", v, now)
    }

    /// The pointer in the grid's content coordinates (`usePublishCursor`), at most every [`CURSOR_MS`];
    /// `None` clears it (the pointer left the grid).
    pub fn set_cursor(&mut self, at: Option<(f64, f64)>, now: u64) -> Option<Outgoing> {
        match at {
            Some((x, y)) => {
                if now.saturating_sub(self.last_cursor) < CURSOR_MS {
                    return None;
                }
                self.last_cursor = now;
                self.field("cursor", json!({ "x": x, "y": y }), now)
            }
            None => self.field("cursor", Json::Null, now),
        }
    }

    /// The periodic check: our presence renewal, silent peers dropped.
    pub fn tick(&mut self, now: u64) -> (Vec<Outgoing>, bool) {
        let (renew, dropped) = self.presence.tick(now);
        (renew.map(|u| Outgoing::Text(awareness_frame(&u))).into_iter().collect(), dropped)
    }

    /// Leaving (`removeAwarenessStates`, `destroy()`): the frame telling the peers.
    pub fn leave(&mut self, now: u64) -> Outgoing {
        Outgoing::Text(awareness_frame(&self.presence.leave(now)))
    }

    /// The peers present.
    pub fn peers(&self) -> Vec<Peer> {
        self.presence.peers()
    }

    /// Our client id (`awareness.clientID`).
    pub fn client_id(&self) -> u64 {
        self.presence.client()
    }
}

/// A random 32-bit client id, as Yjs draws one.
pub fn new_client_id() -> u64 {
    let id = kubuno_office_desktop_shared::ids::fresh_uuid_v4().replace('-', "");
    u64::from_str_radix(&id[..8], 16).unwrap_or(1) & 0xffff_ffff
}

/// The user as the web publishes it (`id`, `display_name || username || email`).
pub fn current_user() -> Option<(String, String)> {
    let account = kubuno_desktop_sync::current_account().ok().flatten()?;
    let name = account.display_name.clone().filter(|n| !n.trim().is_empty()).or_else(|| account.email.clone()).unwrap_or_else(|| account.user_id.clone());
    Some((account.user_id, name))
}

/// The room's URL for the account the shell shows, with a current access token (run on the room's thread
/// before every connection).
pub fn room_url(room_name: &str) -> Result<String, String> {
    let account = kubuno_desktop_sync::current_account().map_err(|e| format!("compte : {e}"))?.ok_or_else(|| "aucun compte connecté".to_string())?;
    let token = kubuno_desktop_sync::account_access_token(&account).map_err(|e| format!("jeton : {e}"))?;
    Ok(kubuno_office_desktop_shared::collab::room_url(&account.server_url, room_name, &token))
}

/// Whether a workbook may join its room: an encrypted workbook never does (`SA:652-656`).
pub fn allowed(ed: &Editor) -> bool {
    !ed.wb.sheets.iter().any(|s| s.is_encrypted())
}

/// Encodes a presence update for a test peer (used by the tests and the probes).
pub fn presence_frame(client: u64, clock: u64, state: Json) -> String {
    awareness_frame(&awareness::encode(&[awareness::Entry { client, clock, state: Some(state) }]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kubuno_office_sheets_core::model::workbook::Workbook;
    use std::rc::Rc;

    fn editor() -> Editor {
        let wb = Workbook::from_bytes(crate::sample::SAMPLE.as_bytes(), "x").expect("the sample");
        Editor::new(wb, Rc::new(crate::platform::SystemEnv::default()))
    }

    fn binaries(out: &[Outgoing]) -> Vec<Vec<u8>> {
        out.iter().filter_map(|o| if let Outgoing::Binary(b) = o { Some(b.clone()) } else { None }).collect()
    }

    #[test]
    fn two_desktops_edit_together() {
        let (mut ea, mut eb) = (editor(), editor());
        let mut a = Collab::new("w", 1, 0);
        let mut b = Collab::new("w", 2, 0);
        // A arrives in an empty room: it seeds it.
        a.on_text(r#"{"type":"sync","empty":true}"#, 0);
        assert!(a.ready_to_seed(0));
        let (seed, _) = a.seed(&mut ea);
        assert!(!seed.is_empty());
        // B arrives later: the greeting, then the room's parts; it adopts them once settled.
        b.on_text(r#"{"type":"sync","empty":false}"#, 10);
        for u in binaries(&seed) {
            b.on_binary(&u, 20).expect("applies");
        }
        assert!(!b.ready_to_seed(100));
        assert!(b.ready_to_seed(20 + SETTLE_MS));
        let (out, _) = b.seed(&mut eb);
        assert!(binaries(&out).is_empty(), "nothing to seed: the room had everything");
        // A types a value: one update; B mirrors it, without an undo entry.
        let first = ea.wb.sheets[0].meta.id.clone();
        ea.update_cell(kubuno_office_sheets_core::addr::Cell::new(1, 20), "=B3*2").expect("writes");
        let sent = binaries(&a.publish(&ea));
        assert_eq!(sent.len(), 1);
        for u in &sent {
            b.on_binary(u, 30).expect("applies");
        }
        assert!(b.mirror(&mut eb));
        assert_eq!(eb.wb.sheets[0].cells.get("B20").and_then(|c| c.formula()).map(str::to_string), Some("=B3*2".to_string()));
        assert!(!eb.can_undo());
        assert!(b.publish(&eb).is_empty(), "no echo");
        // B defines a name; A's formulas see it.
        let mut names = eb.wb.names.clone();
        names.insert("DOUBLE".into(), json!("=2"));
        eb.apply_remote_names(&names);
        for u in binaries(&b.publish(&eb)) {
            a.on_binary(&u, 40).expect("applies");
        }
        a.mirror(&mut ea);
        assert!(ea.wb.names.contains_key("DOUBLE"));
        let _ = first;
    }

    #[test]
    fn presence_travels() {
        let mut a = Collab::new("w", 1, 0);
        let mut b = Collab::new("w", 2, 0);
        let u = a.set_user("u1", "Ana", None, 1).expect("published");
        let s = a.set_selection(Some((2, 7)), "sheet-1", 2).expect("published");
        for f in [u, s] {
            if let Outgoing::Text(t) = f {
                b.on_text(&t, 3);
            }
        }
        let p = b.peers();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].name, "Ana");
        assert_eq!(p[0].color, user_color("u1"));
        assert_eq!(p[0].sel, Some(("C".into(), 7, Some("sheet-1".into()))));
        assert!(a.set_cursor(Some((1.0, 2.0)), 100).is_some());
        assert!(a.set_cursor(Some((1.0, 3.0)), 110).is_none(), "throttled");
        assert_eq!(a.set_selection(Some((2, 7)), "sheet-1", 200), None, "unchanged");
        assert!(new_client_id() <= 0xffff_ffff);
    }
}
