//! Workbook encryption on the server path (`wbEncryption.ts` + `api.ts`'s transparent encryption): the window
//! and the session host share a [`Crypt`]. While the workbook is encrypted, every sheet's cells leave as an
//! Agile envelope (`data.cells = {}` + `enc`), never in clear; the sheets read back are decrypted with the
//! session's key; removing the encryption sends the cells in clear with `clear_enc`. The key lives in memory
//! only, and nothing of an encrypted workbook goes to the crash journal.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use kubuno_office_sheets_core::crypto::{self, SecretKey, WorkbookKeyDescriptor};
use kubuno_office_sheets_core::model::workbook::Workbook;
use serde_json::{Map, Value as Json};

use super::diff::Op;

/// What the window decided about the workbook's encryption.
#[derive(Debug, Default)]
pub struct Crypt {
    /// The workbook is (to be) stored encrypted.
    pub encrypted: bool,
    /// The session key once unlocked (`None`: locked, or never encrypted).
    pub key: Option<(WorkbookKeyDescriptor, SecretKey)>,
}

/// The window's and the host's shared state.
pub type SharedCrypt = Arc<Mutex<Crypt>>;

impl Crypt {
    /// Encrypted but not unlocked: nothing of the cells may be saved.
    pub fn locked(&self) -> bool {
        self.encrypted && self.key.is_none()
    }
}

/// Decrypts the sheets read from the server with `key` (their envelopes stay as markers); returns the ids of
/// the sheets the server holds encrypted.
pub fn read_back(wb: &mut Workbook, key: Option<&SecretKey>) -> HashSet<String> {
    let ids: HashSet<String> = wb.sheets.iter().filter(|s| s.is_encrypted()).map(|s| s.meta.id.clone()).collect();
    if let Some(k) = key {
        for s in &mut wb.sheets {
            if let Err(e) = crypto::unlock_sheet(s, k) {
                tracing::warn!("[spreadsheets] a sheet does not decrypt with the session key: {e}");
            }
        }
    }
    ids
}

/// Rewrites a save plan for the encryption state: `mine` is the edited workbook, `server_encrypted` the
/// sheets the server holds encrypted.
pub fn transform(ops: Vec<Op>, mine: &Workbook, server_encrypted: &HashSet<String>, c: &Crypt) -> Vec<Op> {
    let mut out = Vec::with_capacity(ops.len());
    let mut sent_cells: HashSet<String> = HashSet::new();
    for op in ops {
        match op {
            Op::Patch { sheet, body } => {
                let mut body = body.as_object().cloned().unwrap_or_default();
                if body.contains_key("data") {
                    sent_cells.insert(sheet.clone());
                }
                if !shape_body(&sheet, &mut body, mine, server_encrypted, c) {
                    continue;
                }
                if !body.is_empty() {
                    out.push(Op::Patch { sheet, body: Json::Object(body) });
                }
            }
            other => out.push(other),
        }
    }
    // A sheet whose stored form must change although its cells did not: encrypting, decrypting.
    for s in &mine.sheets {
        if sent_cells.contains(&s.meta.id) {
            continue;
        }
        let must = (c.encrypted && c.key.is_some() && !server_encrypted.contains(&s.meta.id)) || (!c.encrypted && server_encrypted.contains(&s.meta.id));
        if must {
            let mut body = Map::new();
            body.insert("data".into(), serde_json::json!({ "cells": s.cells_json() }));
            if shape_body(&s.meta.id, &mut body, mine, server_encrypted, c) {
                out.push(Op::Patch { sheet: s.meta.id.clone(), body: Json::Object(body) });
            }
        }
    }
    out
}

/// Encrypts (or clears) the cells of one PATCH body; `false` drops the request (a locked workbook).
fn shape_body(sheet: &str, body: &mut Map<String, Json>, mine: &Workbook, server_encrypted: &HashSet<String>, c: &Crypt) -> bool {
    let Some(cells) = body.get("data").and_then(|d| d.get("cells")).cloned() else { return true };
    if c.locked() {
        // Never in clear, never what a locked editor shows (nothing).
        body.remove("data");
        return true;
    }
    match (&c.key, c.encrypted) {
        (Some((d, k)), true) => match crypto::encrypt_cells(d, k, &cells) {
            Ok(env) => {
                if let Some(data) = body.get_mut("data").and_then(Json::as_object_mut) {
                    data.insert("cells".into(), Json::Object(Map::new()));
                }
                body.insert("enc".into(), serde_json::to_value(env).unwrap_or(Json::Null));
                true
            }
            Err(e) => {
                tracing::error!("[spreadsheets] cannot encrypt the sheet {sheet}: {e}");
                false
            }
        },
        _ => {
            if server_encrypted.contains(sheet) && mine.sheet(sheet).is_some() {
                body.insert("clear_enc".into(), Json::Bool(true));
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn wb() -> Workbook {
        Workbook::from_api(&json!({"spreadsheet": {"id": "w", "title": "T"}, "sheets": [{"id": "s1", "name": "F", "position": 0}]}), &[json!({"sheet": {"id": "s1"}, "data": {"cells": {"A1": {"v": "x"}}}})]).expect("a workbook")
    }

    #[test]
    fn encrypting_sends_every_sheet_as_an_envelope() {
        let (d, k) = crypto::create_workbook_key("pw", 5).expect("random");
        let c = Crypt { encrypted: true, key: Some((d, k.clone())) };
        let ops = transform(Vec::new(), &wb(), &HashSet::new(), &c);
        let Op::Patch { body, .. } = &ops[0] else { panic!("a patch") };
        assert_eq!(body["data"]["cells"], json!({}), "never in clear");
        let env: crypto::SheetEncEnvelope = serde_json::from_value(body["enc"].clone()).expect("an envelope");
        assert_eq!(crypto::decrypt_cells(&k, &env).expect("decrypts"), json!({"A1": {"v": "x"}}));
    }

    #[test]
    fn decrypting_clears_and_a_locked_workbook_never_sends_cells() {
        let enc: HashSet<String> = ["s1".to_string()].into();
        let ops = transform(Vec::new(), &wb(), &enc, &Crypt::default());
        let Op::Patch { body, .. } = &ops[0] else { panic!("a patch") };
        assert_eq!(body["clear_enc"], true);
        assert_eq!(body["data"]["cells"]["A1"]["v"], "x");
        let locked = Crypt { encrypted: true, key: None };
        let ops = transform(vec![Op::Patch { sheet: "s1".into(), body: json!({"data": {"cells": {"A1": {"v": "typed"}}}, "merges": []}) }], &wb(), &enc, &locked);
        let Op::Patch { body, .. } = &ops[0] else { panic!("a patch") };
        assert!(body.get("data").is_none());
        assert_eq!(body["merges"], json!([]));
    }

    #[test]
    fn sheets_read_back_are_decrypted_with_the_key() {
        let (d, k) = crypto::create_workbook_key("pw", 5).expect("random");
        let env = crypto::encrypt_cells(&d, &k, &json!({"B2": {"v": 3}})).expect("encrypts");
        let mut w = Workbook::from_api(&json!({"spreadsheet": {"id": "w"}, "sheets": [{"id": "s1", "name": "F", "position": 0}]}), &[json!({"sheet": {"id": "s1"}, "data": {"cells": {}, "enc": env}})]).expect("a workbook");
        assert!(read_back(&mut w.clone(), None).contains("s1"));
        read_back(&mut w, Some(&k));
        assert!(w.sheets[0].cells.contains_key("B2"));
    }
}
