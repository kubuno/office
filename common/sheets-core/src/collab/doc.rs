//! The spreadsheet's Y.Doc (`SA:643-680`): `cells:<sheetId>` maps and the `names` map.

use serde_json::{Map, Value as Json};
use yrs::updates::decoder::Decode;
use yrs::{Doc, Map as _, MapRef, ReadTxn, StateVector, Transact, Update};

use super::json::{from_out, to_any};
use super::CollabError;

/// The `names` map (`ydoc.getMap('names')`, `SA:876`).
pub const NAMES: &str = "names";

/// The cells map of a sheet (`ydoc.getMap(`cells:${activeSheetId}`)`, `SA:674`).
pub fn cells_map_name(sheet_id: &str) -> String {
    format!("cells:{sheet_id}")
}

/// What the first synchronisation does with a sheet (`SA:3277-3296`).
#[derive(Debug, Clone, PartialEq)]
pub enum Seed {
    /// The room had no cells for the sheet and the workbook has some: they were written to the room
    /// (this update carries them).
    Seeded(Vec<u8>),
    /// The room has cells for the sheet: they replace the workbook's (the room is newer than the file).
    Adopt(Map<String, Json>),
    /// Nothing to do (both empty).
    Nothing,
}

/// A spreadsheet's collaborative document.
pub struct SheetsDoc {
    doc: Doc,
}

impl SheetsDoc {
    /// A new, empty document for client `client_id` (a random 32-bit number, as Yjs picks it: the caller
    /// draws it, the core never reads a random source).
    pub fn new(client_id: u64) -> SheetsDoc {
        SheetsDoc { doc: Doc::with_client_id(client_id) }
    }

    pub fn client_id(&self) -> u64 {
        self.doc.client_id()
    }

    fn map(&self, name: &str) -> MapRef {
        self.doc.get_or_insert_map(name)
    }

    /// Applies an update received from the room (`Y.applyUpdate(doc, data, 'remote')`). The room sends the
    /// persisted parts one by one and echoes our own updates back: applying twice is harmless (CRDT).
    pub fn apply_update(&self, bytes: &[u8]) -> Result<(), CollabError> {
        let update = Update::decode_v1(bytes).map_err(|e| CollabError::BadUpdate(e.to_string()))?;
        let mut txn = self.doc.transact_mut();
        txn.apply_update(update).map_err(|e| CollabError::BadUpdate(e.to_string()))
    }

    /// The whole document as one update (`Y.encodeStateAsUpdate(doc)`, sent when the socket opens if it is
    /// longer than 2 bytes, `collabProvider.ts` `onopen`).
    pub fn state_as_update(&self) -> Vec<u8> {
        self.doc.transact().encode_state_as_update_v1(&StateVector::default())
    }

    /// Runs `f` in one transaction and returns the update it produced (`None`: nothing changed).
    fn change(&self, f: impl FnOnce(&mut yrs::TransactionMut)) -> Option<Vec<u8>> {
        let before = self.doc.transact().state_vector();
        {
            let mut txn = self.doc.transact_mut();
            f(&mut txn);
        }
        let txn = self.doc.transact();
        if txn.state_vector() == before {
            return None;
        }
        Some(txn.encode_state_as_update_v1(&before))
    }

    fn read(&self, name: &str) -> Map<String, Json> {
        let map = self.map(name);
        let txn = self.doc.transact();
        let mut out = Map::new();
        for (k, v) in map.iter(&txn) {
            if let Some(j) = from_out(&v) {
                out.insert(k.to_string(), j);
            }
        }
        out
    }

    /// A sheet's cells as the room holds them.
    pub fn cells(&self, sheet_id: &str) -> Map<String, Json> {
        self.read(&cells_map_name(sheet_id))
    }

    /// How many cells the room holds for a sheet (`cellsMap.size`).
    pub fn cells_len(&self, sheet_id: &str) -> u32 {
        let map = self.map(&cells_map_name(sheet_id));
        let txn = self.doc.transact();
        map.len(&txn)
    }

    /// The defined names (UPPERCASE name → definition).
    pub fn names(&self) -> Map<String, Json> {
        self.read(NAMES)
    }

    /// Writes `current` to a map: sets the keys whose value differs, deletes the keys `current` lacks - the
    /// web's diff in one transaction (`SA:3189-3199`, `ydoc.transact(…, 'local')`).
    fn write(&self, name: &str, current: &Map<String, Json>) -> Option<Vec<u8>> {
        let held = self.read(name);
        if &held == current {
            return None;
        }
        let map = self.map(name);
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

    /// Publishes a sheet's cells: the update for what changed, or `None`.
    pub fn write_cells(&self, sheet_id: &str, current: &Map<String, Json>) -> Option<Vec<u8>> {
        self.write(&cells_map_name(sheet_id), current)
    }

    /// Publishes the defined names (`setDefinedName`/`deleteDefinedName`, `SA:885-886`): keys are written
    /// in upper case, as the web does.
    pub fn write_names(&self, current: &Map<String, Json>) -> Option<Vec<u8>> {
        let upper: Map<String, Json> = current.iter().map(|(k, v)| (k.trim().to_uppercase(), v.clone())).collect();
        self.write(NAMES, &upper)
    }

    /// The first synchronisation of a sheet (`SA:3277-3296`): the room's cells win when it has some;
    /// otherwise the workbook's are written to it.
    pub fn seed_cells(&self, sheet_id: &str, backend: &Map<String, Json>) -> Seed {
        if self.cells_len(sheet_id) == 0 {
            if backend.is_empty() {
                Seed::Nothing
            } else {
                match self.write_cells(sheet_id, backend) {
                    Some(u) => Seed::Seeded(u),
                    None => Seed::Nothing,
                }
            }
        } else {
            Seed::Adopt(self.cells(sheet_id))
        }
    }

    /// Seeds the names from the workbook's when the room has none (`SA:889-897`); returns the update, if any.
    pub fn seed_names(&self, backend: &Map<String, Json>) -> Option<Vec<u8>> {
        let empty = {
            let map = self.map(NAMES);
            let txn = self.doc.transact();
            map.len(&txn) == 0
        };
        if empty && !backend.is_empty() {
            self.write_names(backend)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Json) -> Map<String, Json> {
        v.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn two_peers_converge_through_updates() {
        let a = SheetsDoc::new(1);
        let b = SheetsDoc::new(2);
        let u = a.write_cells("s1", &obj(json!({"A1": {"v": 1}, "B2": {"f": "=A1*2"}}))).expect("a change");
        b.apply_update(&u).expect("applies");
        assert_eq!(b.cells("s1"), obj(json!({"A1": {"v": 1}, "B2": {"f": "=A1*2"}})));
        // b deletes A1 and edits B2: only those travel.
        let u2 = b.write_cells("s1", &obj(json!({"B2": {"f": "=3"}}))).expect("a change");
        a.apply_update(&u2).expect("applies");
        assert_eq!(a.cells("s1"), obj(json!({"B2": {"f": "=3"}})));
        assert_eq!(a.write_cells("s1", &obj(json!({"B2": {"f": "=3"}}))), None, "nothing new to send");
    }

    #[test]
    fn seeding_follows_the_web() {
        let d = SheetsDoc::new(7);
        assert_eq!(d.seed_cells("s1", &Map::new()), Seed::Nothing);
        let backend = obj(json!({"A1": {"v": "x"}}));
        assert!(matches!(d.seed_cells("s1", &backend), Seed::Seeded(_)));
        // Once the room has cells, they win.
        assert_eq!(d.seed_cells("s1", &obj(json!({"A1": {"v": "older"}}))), Seed::Adopt(backend));
    }

    #[test]
    fn names_are_upper_case_and_seeded_once() {
        let d = SheetsDoc::new(3);
        assert!(d.seed_names(&obj(json!({"taux": "=0.2"}))).is_some());
        assert_eq!(d.names(), obj(json!({"TAUX": "=0.2"})));
        assert!(d.seed_names(&obj(json!({"OTHER": "=1"}))).is_none());
    }

    #[test]
    fn the_state_update_rebuilds_the_document() {
        let a = SheetsDoc::new(11);
        a.write_cells("s1", &obj(json!({"C3": {"v": 0.5}})));
        a.write_names(&obj(json!({"N": "=1"})));
        let b = SheetsDoc::new(12);
        b.apply_update(&a.state_as_update()).expect("applies");
        assert_eq!(b.cells("s1"), obj(json!({"C3": {"v": 0.5}})));
        assert_eq!(b.names(), obj(json!({"N": "=1"})));
        assert!(b.apply_update(&[1, 2, 3]).is_err());
    }
}
