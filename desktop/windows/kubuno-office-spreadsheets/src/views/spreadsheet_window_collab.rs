//! Real-time collaboration in the spreadsheet window (a child module of `spreadsheet_window`): the room of a
//! server workbook (`useCollab(\`office-spreadsheet:${ssId}\`)`, `SA:643-680`), collaborators' edits mirrored
//! into the grid, this user's edits and presence published, the peers drawn on the grid and as avatars.
//!
//! The rules are `kubuno_office_spreadsheets_common::collab::Collab`'s; the socket is
//! `kubuno_office_desktop_shared::collab::Room`'s thread; this file only wires them to the window.

use std::sync::Arc;

use kubuno_desktop::prelude::*;
use kubuno_desktop::views::component::Control as _;
use kubuno_office_desktop_shared::collab::{Event as RoomEvent, Room, Sink, Status};
use kubuno_office_sheets_core::addr;
use kubuno_office_spreadsheets_common::collab::{self as sc, Collab, Outgoing};

use super::SpreadsheetWindow;
use crate::controls::grid::presence::PeerMark;
use crate::controls::grid::GridPointerEventArgs;
use crate::Resources;

/// The window's collaboration: the state and the room's thread.
pub struct CollabLink {
    state: Collab,
    room: Room,
    connected: bool,
}

fn now() -> u64 {
    kubuno_desktop::controls::host::now_ms()
}

impl SpreadsheetWindow {
    /// Joins the workbook's room (after the workbook is open). An encrypted workbook never joins
    /// (`SA:652-656`).
    pub(super) fn start_collab(&mut self, spreadsheet_id: &str) {
        if self.collab.is_some() {
            return;
        }
        let allowed = self.grid.with(|g| g.editor().map(sc::allowed)).flatten().unwrap_or(false);
        if !allowed {
            return;
        }
        let Some(dispatcher) = self.dispatcher() else { return };
        let post = dispatcher.clone();
        let sink: Sink = Arc::new(move |event| {
            drop(post.begin_invoke(move |w: &mut SpreadsheetWindow| w.on_collab(event)));
        });
        let mut state = Collab::new(spreadsheet_id, sc::new_client_id(), now());
        if let Some((id, name)) = sc::current_user() {
            let _ = state.set_user(&id, &name, None, now());
        }
        let room_name = state.room.clone();
        let room = Room::open("spreadsheet", move || sc::room_url(&room_name), sink);
        self.collab = Some(CollabLink { state, room, connected: false });
        self.collab_active = true;
        self.collab_status = Resources::collab_connecting().to_string();
        self.has_collab = true;
    }

    fn send(&self, out: Vec<Outgoing>) {
        if let Some(link) = &self.collab {
            for o in out {
                match o {
                    Outgoing::Binary(b) => link.room.send_binary(b),
                    Outgoing::Text(t) => link.room.send_text(t),
                }
            }
        }
    }

    pub(super) fn on_collab(&mut self, event: RoomEvent) {
        let t = now();
        match event {
            RoomEvent::Status(Status::Connected) => {
                let out = match self.collab.as_mut() {
                    Some(link) => {
                        link.connected = true;
                        link.state.on_connected()
                    }
                    None => return,
                };
                self.send(out);
                self.collab_status = Resources::collab_live().to_string();
            }
            RoomEvent::Status(Status::Connecting) => {
                if self.collab.as_ref().is_some_and(|l| !l.connected) {
                    self.collab_status = Resources::collab_connecting().to_string();
                }
            }
            RoomEvent::Status(Status::Disconnected(why)) => {
                kubuno_desktop::tracing::info!("[spreadsheets] collab: {why}");
                if let Some(link) = self.collab.as_mut() {
                    link.connected = false;
                }
                self.collab_status = Resources::collab_offline().to_string();
            }
            RoomEvent::Binary(bytes) => {
                // This user's edits not yet published go out first: mirroring the room afterwards would otherwise
                // replace them with the room's older state (the web writes its Y.Map on every commit, synchronously).
                self.collab_state_changed();
                let Some(link) = self.collab.as_mut() else { return };
                if let Err(e) = link.state.on_binary(&bytes, t) {
                    kubuno_desktop::tracing::warn!("[spreadsheets] collab: {e}");
                    return;
                }
                let state = &mut link.state;
                self.grid.with(|g| {
                    if g.editor_mut().is_some_and(|ed| state.mirror(ed)) {
                        g.invalidate();
                    }
                });
            }
            RoomEvent::Text(text) => {
                let Some(link) = self.collab.as_mut() else { return };
                let (out, changed) = link.state.on_text(&text, t);
                self.send(out);
                if changed.presence {
                    self.refresh_peers();
                }
            }
        }
    }

    /// `collab_timer` (every 250 ms): the first synchronisation once the room's parts have arrived, the
    /// presence renewal and the silent peers.
    pub(super) fn collab_tick(&mut self) {
        let t = now();
        let Some(link) = self.collab.as_mut() else { return };
        let mut out = Vec::new();
        if link.state.ready_to_seed(t) {
            let state = &mut link.state;
            let seeded = self.grid.with(|g| g.editor_mut().map(|ed| state.seed(ed))).flatten();
            if let Some((o, changed)) = seeded {
                out.extend(o);
                if changed {
                    self.grid.with(|g| g.invalidate());
                }
            }
            // What this user typed before the synchronisation goes out now.
            self.collab_state_changed();
        }
        let Some(link) = self.collab.as_mut() else { return };
        let (renew, dropped) = link.state.tick(t);
        out.extend(renew);
        self.send(out);
        if dropped {
            self.refresh_peers();
        }
    }

    /// After every change the grid reports: this user's edits and active cell go to the room.
    pub(super) fn collab_state_changed(&mut self) {
        let t = now();
        let Some(link) = self.collab.as_mut() else { return };
        let state = &mut link.state;
        let out = self
            .grid
            .with(|g| {
                g.editor().map(|ed| {
                    let mut out = state.publish(ed);
                    let cell = ed.selected.then_some((ed.sel.active.col, ed.sel.active.row));
                    out.extend(state.set_selection(cell, &ed.sheet().meta.id, t));
                    out
                })
            })
            .flatten()
            .unwrap_or_default();
        self.send(out);
        // The sheet shown may have changed: the peers on it.
        self.refresh_peers();
    }

    /// The pointer over the grid, for the peers (`usePublishCursor`, ~40 ms).
    pub(super) fn grid_pointer_moved(&mut self, e: &GridPointerEventArgs) {
        let Some(link) = self.collab.as_mut() else { return };
        let out = link.state.set_cursor(e.inside.then_some((e.x, e.y)), now());
        self.send(out.into_iter().collect());
    }

    /// The peers on the grid and in the avatars (`usePresenceUsers`: deduplicated by name and colour).
    fn refresh_peers(&mut self) {
        let Some(link) = self.collab.as_ref() else { return };
        let peers = link.state.peers();
        let sheet = self.grid.with(|g| g.editor().map(|e| e.sheet().meta.id.clone())).flatten().unwrap_or_default();
        let marks: Vec<PeerMark> = peers
            .iter()
            .map(|p| PeerMark {
                name: p.name.clone(),
                color: p.color.clone(),
                cell: p.sel.as_ref().filter(|(_, _, s)| s.as_deref().is_none_or(|s| s == sheet)).and_then(|(col, row, _)| {
                    let c = addr::col_to_index(col);
                    (!col.is_empty() && col.chars().all(|ch| ch.is_ascii_uppercase()) && (0..addr::MAX_COLS as i64).contains(&c)).then_some((c as u32, *row))
                }),
                cursor: p.cursor,
            })
            .collect();
        self.grid.with(|g| g.set_peers(marks));
        let mut seen = Vec::new();
        for p in &peers {
            let line = format!("{}|{}", p.name, p.color);
            if !seen.contains(&line) {
                seen.push(line);
            }
        }
        self.presence_users = seen.join("\n");
        self.has_presence = !seen.is_empty();
    }

    /// The workbook was reloaded (the server's copy taken): the room's content goes back into it.
    pub(super) fn collab_reloaded(&mut self) {
        let Some(link) = self.collab.as_mut() else { return };
        let state = &mut link.state;
        self.grid.with(|g| {
            if let Some(ed) = g.editor_mut() {
                state.mirror(ed);
            }
            g.invalidate();
        });
    }

    /// While the room is live, the saved copy is the merge of everyone's edits (Yjs) and every client saves
    /// it, last writer wins, as the web does: a digest conflict is not a question for the user then.
    pub(super) fn collab_resolves_conflicts(&self) -> bool {
        self.collab.as_ref().is_some_and(|l| l.connected && l.state.is_synced())
    }

    /// Leaves the room (`destroy()`): our presence is withdrawn, the socket closed.
    pub(super) fn stop_collab(&mut self) {
        if let Some(mut link) = self.collab.take() {
            let bye = link.state.leave(now());
            if let Outgoing::Text(t) = bye {
                link.room.send_text(t);
            }
            link.room.close();
        }
        self.collab_active = false;
    }
}

impl SpreadsheetWindow {
    /// A desktop addition, Excel's: a new valid name typed in the Name Box names the selection
    /// (`=A1:B3`, what the web's Name Manager proposes: `selectionRef={cellAddressLabel}`, `SA:8022`) — the web
    /// defines names only in its Name Manager. The name joins the
    /// workbook's names, shared through the room like the web's (`setDefinedName`, `SA:885`).
    pub(super) fn define_name_from_box(&mut self, raw: &str) -> bool {
        let name = raw.trim();
        if !kubuno_office_sheets_core::formula::parser::is_valid_defined_name(name) || addr::Rect::parse(name).is_some() {
            return false;
        }
        let defined = self
            .grid
            .with(|g| {
                g.with_editor(|ed| {
                    if !ed.selected {
                        return false;
                    }
                    let r = ed.sel.rect();
                    let a = |c: u32, row: u32| format!("{}{}", addr::index_to_col(c as i64), row);
                    let target = if r.c1 == r.c2 && r.r1 == r.r2 { a(r.c1, r.r1) } else { format!("{}:{}", a(r.c1, r.r1), a(r.c2, r.r2)) };
                    ed.wb.names.insert(name.to_uppercase(), serde_json::Value::String(format!("={target}")));
                    ed.touch();
                    true
                })
            })
            .flatten()
            .unwrap_or(false);
        if defined {
            self.collab_state_changed();
        }
        defined
    }
}
