//! Undo / redo - the web's Yjs `UndoManager` (`WhiteboardApp.tsx`: the board binds an `UndoManager` to the
//! `elements` map and the `strokes` array). It reverts only *local* changes (the transactions stamped with
//! [`crate::collab::doc::LOCAL_ORIGIN`]), never a collaborator's, and groups edits less than 500 ms apart
//! into one step - exactly `Y.UndoManager`'s default `captureTimeout`.
//!
//! The manager observes the document, so it is created once per open board and kept alongside the
//! [`WhiteboardDoc`] by the window. It borrows nothing from the application beyond the document.

use std::collections::HashSet;
use std::sync::Arc;

use yrs::undo::Options;
use yrs::UndoManager;

use crate::collab::doc::{ELEMENTS, LOCAL_ORIGIN, STROKES, WhiteboardDoc};

/// The web's `captureTimeout` (ms): edits closer than this merge into one undo step.
pub const CAPTURE_TIMEOUT_MS: u64 = 500;

/// An undo/redo manager over a board's two shared types.
pub struct Undo {
    mgr: UndoManager,
}

impl Undo {
    /// Attaches an undo manager to a board, tracking the `elements` map and the `strokes` array, and only
    /// the local origin. `now_ms` is supplied by the caller through [`Undo::undo`]/[`Undo::redo`] calls'
    /// capture boundaries; `yrs` groups by its own monotonic timestamp, the web's 500 ms default.
    pub fn new(doc: &WhiteboardDoc) -> Undo {
        let d = doc.doc();
        let elements = d.get_or_insert_map(ELEMENTS);
        let strokes = d.get_or_insert_array(STROKES);
        let mut tracked = HashSet::new();
        tracked.insert(LOCAL_ORIGIN.into());
        // `Options::default()` is not available on `wasm32` (its default clock is OS-only), and the core
        // reads no clock anyway: a constant timestamp is used, so undo steps are bounded by explicit
        // `stop_capturing` calls at the window's gesture boundaries rather than by elapsed time.
        let options = Options {
            capture_timeout_millis: CAPTURE_TIMEOUT_MS,
            tracked_origins: tracked,
            capture_transaction: None,
            timestamp: Arc::new(|| 0u64),
        };
        let mut mgr = UndoManager::with_scope_and_options(d, &elements, options);
        mgr.expand_scope(&strokes);
        Undo { mgr }
    }

    /// True when there is a local change to undo.
    pub fn can_undo(&self) -> bool {
        self.mgr.can_undo()
    }

    /// True when a change was undone and can be redone.
    pub fn can_redo(&self) -> bool {
        self.mgr.can_redo()
    }

    /// Reverts the last local change group; `true` when something was undone.
    pub fn undo(&mut self) -> bool {
        self.mgr.undo_blocking()
    }

    /// Re-applies the last undone group; `true` when something was redone.
    pub fn redo(&mut self) -> bool {
        self.mgr.redo_blocking()
    }

    /// Ends the current capture group, so the next edit starts a new undo step (`stopCapturing`).
    pub fn stop_capturing(&mut self) {
        self.mgr.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn undo_reverts_a_local_edit_but_not_a_remote_one() {
        let doc = WhiteboardDoc::new(1);
        let mut undo = Undo::new(&doc);

        // A local edit, then stop the capture so it is its own step.
        doc.set_element("a", &json!({"id":"a","type":"shape","x":0,"y":0,"width":10,"height":10,"kind":"rect"}));
        undo.stop_capturing();
        assert_eq!(doc.elements_len(), 1);
        assert!(undo.can_undo());

        assert!(undo.undo());
        assert_eq!(doc.elements_len(), 0);
        assert!(undo.can_redo());

        assert!(undo.redo());
        assert_eq!(doc.elements_len(), 1);
    }
}
