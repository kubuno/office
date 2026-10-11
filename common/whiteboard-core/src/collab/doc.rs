//! The whiteboard's Y.Doc (`WhiteboardApp.tsx:314-315`): the `elements` map (id → element object) and the
//! `strokes` array (`WhiteboardApp.tsx:314` `getMap('elements')`, `:315` `getArray('strokes')`).
//!
//! This is the authoritative stored content: the server persists Yjs v1 binary updates and consolidates
//! them into a gzipped `.kbwbd` Drive file (`server/src/whiteboard/services/yjs_service.rs`); there is no
//! JSON content blob. [`WhiteboardDoc`] is the `yrs` port of that document — the same implementation and
//! wire format the core's collaboration service and the spreadsheet engine use. It holds the elements and
//! strokes as plain JSON (lossless: a field or element kind the desktop does not model survives here even
//! when the typed [`crate::model`] view drops it).

use serde_json::{Map, Value as Json};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Any, Array, ArrayRef, Doc, Map as _, MapRef, Origin, Out, ReadTxn, StateVector, Transact, Update};

use super::json::{from_out, to_any};
use super::CollabError;

/// The elements map (`ydoc.getMap('elements')`).
pub const ELEMENTS: &str = "elements";
/// The strokes array (`ydoc.getArray('strokes')`).
pub const STROKES: &str = "strokes";

/// The origin stamped on a local edit — the one the undo manager tracks (`ydoc.transact(…, 'local')`).
pub const LOCAL_ORIGIN: &str = "local";
/// The origin stamped on an update received from the room, so undo never reverts someone else's change
/// (`Y.applyUpdate(doc, data, 'remote')`).
pub const REMOTE_ORIGIN: &str = "remote";

/// A whiteboard's collaborative document.
pub struct WhiteboardDoc {
    doc: Doc,
}

impl WhiteboardDoc {
    /// A new, empty document for client `client_id` (a random 32-bit number, as Yjs picks it: the caller
    /// draws it, the core never reads a random source).
    pub fn new(client_id: u64) -> WhiteboardDoc {
        WhiteboardDoc { doc: Doc::with_client_id(client_id) }
    }

    pub fn client_id(&self) -> u64 {
        self.doc.client_id()
    }

    fn elements_map(&self) -> MapRef {
        self.doc.get_or_insert_map(ELEMENTS)
    }

    fn strokes_array(&self) -> ArrayRef {
        self.doc.get_or_insert_array(STROKES)
    }

    /// Applies an update received from the room (`Y.applyUpdate(doc, data, 'remote')`). The server sends the
    /// persisted parts one by one and echoes our own updates back: applying twice is harmless (CRDT).
    pub fn apply_update(&self, bytes: &[u8]) -> Result<(), CollabError> {
        let update = Update::decode_v1(bytes).map_err(|e| CollabError::BadUpdate(e.to_string()))?;
        let mut txn = self.doc.transact_mut_with(Origin::from(REMOTE_ORIGIN));
        txn.apply_update(update).map_err(|e| CollabError::BadUpdate(e.to_string()))
    }

    /// The underlying Yjs document — used by [`crate::undo`] to attach a `yrs` undo manager. The elements
    /// map and the strokes array are reachable through it by their names ([`ELEMENTS`], [`STROKES`]).
    pub fn doc(&self) -> &Doc {
        &self.doc
    }

    /// The whole document as one update (`Y.encodeStateAsUpdate(doc)`): sent when the socket opens, and the
    /// payload written into the `.kbwbd` snapshot.
    pub fn state_as_update(&self) -> Vec<u8> {
        self.doc.transact().encode_state_as_update_v1(&StateVector::default())
    }

    /// The document's state vector (what it already has), so a peer can send only what is missing.
    pub fn state_vector(&self) -> Vec<u8> {
        self.doc.transact().state_vector().encode_v1()
    }

    /// Runs `f` in one transaction and returns the update it produced (`None`: nothing changed).
    fn change(&self, f: impl FnOnce(&mut yrs::TransactionMut)) -> Option<Vec<u8>> {
        let before = self.doc.transact().state_vector();
        {
            let mut txn = self.doc.transact_mut_with(Origin::from(LOCAL_ORIGIN));
            f(&mut txn);
        }
        let txn = self.doc.transact();
        if txn.state_vector() == before {
            return None;
        }
        Some(txn.encode_state_as_update_v1(&before))
    }

    // ── Elements (a map keyed by id) ────────────────────────────────────────

    /// Every element the room holds, id → the element object as JSON.
    pub fn elements(&self) -> Map<String, Json> {
        let map = self.elements_map();
        let txn = self.doc.transact();
        let mut out = Map::new();
        for (k, v) in map.iter(&txn) {
            if let Some(j) = from_out(&v) {
                out.insert(k.to_string(), j);
            }
        }
        out
    }

    /// How many elements the room holds (`elements.size`).
    pub fn elements_len(&self) -> u32 {
        let map = self.elements_map();
        let txn = self.doc.transact();
        map.len(&txn)
    }

    /// Sets (or inserts) one element (`elements.set(id, obj)`); returns the update, or `None` when the room
    /// already holds exactly this value.
    pub fn set_element(&self, id: &str, value: &Json) -> Option<Vec<u8>> {
        let map = self.elements_map();
        {
            let txn = self.doc.transact();
            if map.get(&txn, id).as_ref().and_then(from_out).as_ref() == Some(value) {
                return None;
            }
        }
        self.change(|txn| {
            map.insert(txn, id, to_any(value));
        })
    }

    /// Removes one element (`elements.delete(id)`); returns the update, or `None` when it was absent.
    pub fn remove_element(&self, id: &str) -> Option<Vec<u8>> {
        let map = self.elements_map();
        {
            let txn = self.doc.transact();
            map.get(&txn, id)?;
        }
        self.change(|txn| {
            map.remove(txn, id);
        })
    }

    /// Replaces the whole element set with `current` (the web's diff, in one transaction): sets the keys
    /// whose value differs, deletes the keys `current` lacks. Returns the update, or `None` when unchanged.
    pub fn write_elements(&self, current: &Map<String, Json>) -> Option<Vec<u8>> {
        let held = self.elements();
        if &held == current {
            return None;
        }
        let map = self.elements_map();
        self.change(|txn| {
            for (k, v) in current {
                if held.get(k) != Some(v) {
                    map.insert(txn, k.as_str(), to_any(v));
                }
            }
            for k in held.keys() {
                if !current.contains_key(k) {
                    map.remove(txn, k);
                }
            }
        })
    }

    // ── Strokes (an append-mostly array) ────────────────────────────────────

    /// Every stroke the room holds, in array order.
    pub fn strokes(&self) -> Vec<Json> {
        let arr = self.strokes_array();
        let txn = self.doc.transact();
        arr.iter(&txn).filter_map(|v| from_out(&v)).collect()
    }

    /// How many strokes the room holds (`strokes.length`).
    pub fn strokes_len(&self) -> u32 {
        let arr = self.strokes_array();
        let txn = self.doc.transact();
        arr.len(&txn)
    }

    /// Appends one stroke (`strokes.push([stroke])`); returns the update.
    pub fn push_stroke(&self, value: &Json) -> Option<Vec<u8>> {
        let arr = self.strokes_array();
        self.change(|txn| {
            let any = to_any(value);
            arr.push_back(txn, any);
        })
    }

    /// Replaces the whole stroke array with `current` (used by the eraser and undo, which rewrite the
    /// list). Returns the update, or `None` when unchanged.
    pub fn write_strokes(&self, current: &[Json]) -> Option<Vec<u8>> {
        let held = self.strokes();
        if held == current {
            return None;
        }
        let arr = self.strokes_array();
        self.change(|txn| {
            let len = arr.len(txn);
            if len > 0 {
                arr.remove_range(txn, 0, len);
            }
            for v in current {
                arr.push_back(txn, to_any(v));
            }
        })
    }
}

impl Default for WhiteboardDoc {
    fn default() -> Self {
        WhiteboardDoc::new(1)
    }
}

/// Helper used by the elements map: a prelim value is an [`Any`]. Kept to document the single place a stroke
/// or element crosses into Yjs.
#[allow(dead_code)]
fn any_is_map(a: &Any) -> bool {
    matches!(a, Any::Map(_))
}

/// Reads the element id from a stored object (`obj.id`), or `None`.
pub fn element_id(obj: &Json) -> Option<&str> {
    obj.get("id").and_then(Json::as_str)
}

/// The out value of a map entry, exposed for callers that need the raw [`Out`].
pub fn out_to_json(o: &Out) -> Option<Json> {
    from_out(o)
}
