//! The save session's outside world for a workbook: [`SheetsHost`] answers `session::Server` through the
//! spreadsheet routes and keeps the crash journal (`session::Journal`).
//!
//! The session (shared with Kubuno Documents) only knows "a document's bytes": here they are the workbook's
//! bundle (`Workbook::to_bundle_bytes`: title, sheets' ids/names/positions, content), read whole — the
//! workbook head, then every sheet — so its digest changes when anyone changes any sheet. A save re-reads
//! that bundle, compares the digest with the one it opened from (the session's rule), then sends only what
//! differs ([`super::diff`]).

use std::path::PathBuf;

use kubuno_office_desktop_common::api::client::{self, Transport};
use kubuno_office_desktop_common::api::session::{self, SaveJob, Snapshot};
use kubuno_office_sheets_core::model::workbook::Workbook;
use serde_json::{Map, Value as Json};

use super::diff::{self, Op};
use super::routes;

/// The spreadsheet's autosave setting (`office.spreadsheet_autosave_s`, the web's `useOfficeInstance.ts`).
pub const SPREADSHEET_AUTOSAVE_KEY: &str = "office.spreadsheet_autosave_s";

/// Maps a REST failure onto the four shapes the session tells apart.
fn map(e: client::Error) -> session::Error {
    match e {
        client::Error::Precondition => session::Error::Precondition,
        client::Error::TooLarge => session::Error::TooLarge,
        client::Error::Forbidden | client::Error::NotFound => session::Error::NotOwner,
        other => session::Error::Transport(other.to_string()),
    }
}

/// The crash journal: one file per workbook in the user's data folder
/// (`<data>/spreadsheets/journal/<id>.json`, the sandbox's under `KUBUNO_SANDBOX_DIR`).
pub struct FileJournal {
    dir: PathBuf,
}

impl FileJournal {
    pub fn for_user() -> Option<Self> {
        let dir = kubuno_desktop_account::paths::user_data_dir().ok()?.join("spreadsheets").join("journal");
        Some(Self { dir })
    }

    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, id: &str) -> PathBuf {
        let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        self.dir.join(format!("{safe}.json"))
    }
}

impl session::Journal for FileJournal {
    fn write(&mut self, id: &str, bytes: &[u8]) -> Result<(), session::Error> {
        let path = self.path(id);
        std::fs::create_dir_all(&self.dir).map_err(|e| session::Error::Transport(format!("journal : {e}")))?;
        // Written beside, then renamed: a crash mid-write never leaves a torn journal.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, bytes).map_err(|e| session::Error::Transport(format!("journal : {e}")))?;
        std::fs::rename(&tmp, &path).map_err(|e| session::Error::Transport(format!("journal : {e}")))
    }

    fn read(&mut self, id: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(id)).ok()
    }

    fn clear(&mut self, id: &str) {
        let _ = std::fs::remove_file(self.path(id));
    }
}

/// The session host of a server workbook.
pub struct SheetsHost<J: session::Journal = FileJournal> {
    transport: Box<dyn Transport + Send>,
    pub journal: J,
    /// The workbook as the last read found it (the base a save diffs against).
    server_copy: Option<Workbook>,
    /// What the last save could not carry (`diff::Plan::unsaved`).
    pub unsaved: Vec<String>,
    /// The signed-in user (the editors list excludes us).
    me: Option<String>,
}

impl<J: session::Journal> SheetsHost<J> {
    pub fn new(transport: Box<dyn Transport + Send>, journal: J, me: Option<String>) -> Self {
        Self { transport, journal, server_copy: None, unsaved: Vec::new(), me }
    }

    fn send(&self, r: &client::Request) -> Result<Json, client::Error> {
        let resp = self.transport.send(r)?;
        if !resp.ok() {
            return Err(client::Error::from_response(resp.status, &resp.body));
        }
        resp.parse()
    }

    /// Reads the whole workbook: the head, then each sheet.
    pub fn read(&self, id: &str) -> Result<Workbook, client::Error> {
        let head = self.send(&routes::get_workbook(id))?;
        let ids: Vec<String> = head.get("sheets").and_then(Json::as_array).map(|a| a.iter().filter_map(|s| s.get("id").and_then(Json::as_str).map(str::to_string)).collect()).unwrap_or_default();
        let mut bodies = Vec::with_capacity(ids.len());
        for sid in ids {
            bodies.push(self.send(&routes::get_sheet(id, &sid))?);
        }
        Workbook::from_api(&head, &bodies).map_err(|e| client::Error::Malformed(e.to_string()))
    }

    /// The editors `join` lists, us excluded, by display name.
    fn editors(&self, joined: &Json) -> Vec<String> {
        joined
            .get("editors")
            .and_then(Json::as_array)
            .map(|a| {
                a.iter()
                    .filter(|e| e.get("user_id").and_then(Json::as_str) != self.me.as_deref())
                    .map(|e| e.get("display_name").and_then(Json::as_str).or_else(|| e.get("user_id").and_then(Json::as_str)).unwrap_or("").to_string())
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl<J: session::Journal> session::Server for SheetsHost<J> {
    fn fetch(&mut self, id: &str) -> Result<Snapshot, session::Error> {
        let wb = self.read(id).map_err(map)?;
        let content = wb.to_bundle_bytes();
        self.server_copy = Some(wb);
        Ok(Snapshot { content, etag: None, editors: Vec::new() })
    }

    fn patch(&mut self, job: &SaveJob) -> Result<Option<String>, session::Error> {
        let mine = Workbook::from_bytes(&job.content, "").map_err(|e| session::Error::Transport(format!("classeur illisible : {e}")))?;
        let base = match self.server_copy.clone() {
            Some(b) => b,
            None => self.read(&job.id).map_err(map)?,
        };
        let plan = diff::plan(&base, &mine);
        self.unsaved = plan.unsaved.clone();
        if !plan.unsaved.is_empty() {
            tracing::warn!("[spreadsheets] not saved through the API: {}", plan.unsaved.join(", "));
        }
        if mine.title != base.title && !mine.title.is_empty() {
            self.send(&routes::rename(&job.id, &mine.title)).map_err(map)?;
        }
        for op in plan.ops {
            let r = match &op {
                Op::Create { sheet, name } => routes::create_sheet(&job.id, sheet, name),
                Op::Patch { sheet, body } => routes::patch_sheet(&job.id, sheet, body.clone()),
                Op::Delete { sheet } => routes::delete_sheet(&job.id, sheet),
            };
            self.send(&r).map_err(map)?;
        }
        // The server copy is re-read by the session after the save.
        self.server_copy = None;
        Ok(None)
    }

    fn join(&mut self, id: &str) -> Result<Snapshot, session::Error> {
        let joined = self.send(&routes::join(id)).map_err(map)?;
        let editors = self.editors(&joined);
        let mut snap = self.fetch(id)?;
        snap.editors = editors;
        Ok(snap)
    }

    fn ping(&mut self, id: &str) -> Result<(), session::Error> {
        self.send(&routes::ping(id)).map(|_| ()).map_err(map)
    }

    fn leave(&mut self, id: &str) -> Result<(), session::Error> {
        self.send(&routes::leave(id)).map(|_| ()).map_err(map)
    }

    /// The core's public settings, with the spreadsheet's autosave under the key the shared session reads.
    fn config(&mut self) -> Result<Map<String, Json>, session::Error> {
        let v = self.send(&routes::core_config()).map_err(map)?;
        let mut cfg = v.get("config").and_then(Json::as_object).cloned().unwrap_or_default();
        match cfg.get(SPREADSHEET_AUTOSAVE_KEY).cloned() {
            Some(s) => {
                cfg.insert(session::AUTOSAVE_KEY.into(), s);
            }
            None => {
                cfg.remove(session::AUTOSAVE_KEY);
            }
        }
        Ok(cfg)
    }
}

impl<J: session::Journal> session::Journal for SheetsHost<J> {
    fn write(&mut self, id: &str, bytes: &[u8]) -> Result<(), session::Error> {
        self.journal.write(id, bytes)
    }
    fn read(&mut self, id: &str) -> Option<Vec<u8>> {
        self.journal.read(id)
    }
    fn clear(&mut self, id: &str) {
        self.journal.clear(id)
    }
}

/// Connects to workbook `id` as the account the shell shows (its access token is borrowed from the shell's
/// broker). Returns the host and the title; offline, the title is the id and the session opens from the
/// journal.
pub fn connect(id: &str) -> Result<(SheetsHost, String), String> {
    use session::Journal as _;
    let journal = FileJournal::for_user().ok_or_else(|| "dossier de données introuvable".to_string())?;
    let account = kubuno_desktop_sync::current_account()
        .map_err(|e| format!("compte : {e}"))?
        .ok_or_else(|| "aucun compte connecté : connectez-vous dans Kubuno Desktop".to_string())?;
    let me = account.user_id.clone();
    let transport = client::SyncTransport::for_account(account);
    let mut host = SheetsHost::new(Box::new(transport), journal, Some(me));
    let title = match host.send(&routes::get_workbook(id)) {
        Ok(head) => head.get("spreadsheet").and_then(|s| s.get("title")).and_then(Json::as_str).unwrap_or("").to_string(),
        Err(client::Error::Transport(_)) if host.journal.read(id).is_some() => id.to_string(),
        Err(e) => return Err(e.to_string()),
    };
    Ok((host, title))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kubuno_office_desktop_common::api::client::{Method, Request, Response};
    use kubuno_office_desktop_common::api::session::{Journal, Server};
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    /// An in-memory office server: the workbook as the API serves it, every request logged.
    #[derive(Clone, Default)]
    struct Fake {
        state: Arc<Mutex<(Json, Vec<String>)>>,
    }

    impl Fake {
        fn new() -> Self {
            let wb = json!({
                "spreadsheet": {"id": "w1", "title": "Budget", "updated_at": "x"},
                "sheets": [{"id": "s1", "name": "A", "position": 0, "spreadsheet_id": "w1"}],
                "data": {"s1": {"cells": {"A1": {"v": 1}}, "col_widths": {}, "row_heights": {}, "frozen_rows": 0, "frozen_cols": 0}},
                "names": {}
            });
            Self { state: Arc::new(Mutex::new((wb, Vec::new()))) }
        }
        fn log(&self) -> Vec<String> {
            self.state.lock().unwrap().1.clone()
        }
    }

    impl Transport for Fake {
        fn send(&self, r: &Request) -> client::ApiResult<Response> {
            let mut st = self.state.lock().unwrap();
            st.1.push(format!("{} {}", r.method.as_str(), r.path));
            let path = r.path.trim_start_matches("/api/v1/office/spreadsheets/w1");
            let wb = &mut st.0;
            let body = match (r.method, path) {
                (Method::Get, "") => json!({"spreadsheet": wb["spreadsheet"], "sheets": wb["sheets"]}),
                (Method::Get, p) if p.starts_with("/sheets/") => {
                    let sid = &p[8..];
                    json!({"sheet": {"id": sid}, "data": wb["data"][sid], "names": wb["names"]})
                }
                (Method::Patch, p) if p.starts_with("/sheets/") => {
                    let sid = p[8..].to_string();
                    let b = r.body.clone().unwrap_or(Json::Null);
                    let d = &mut wb["data"][&sid];
                    if let Some(data) = b.get("data") {
                        d["cells"] = data["cells"].clone();
                    }
                    for (k, v) in b.as_object().into_iter().flatten() {
                        if k != "data" && k != "name" && k != "position" {
                            d[k] = v.clone();
                        }
                    }
                    json!({})
                }
                (Method::Post, "/editing/join") => json!({"content": {}, "editors": [{"user_id": "me"}, {"user_id": "u2", "display_name": "Alice"}]}),
                (_, _) => json!({"ok": true}),
            };
            Ok(Response::json(200, &body))
        }
    }

    fn host(fake: &Fake) -> SheetsHost<FileJournal> {
        let dir = std::env::temp_dir().join(format!("kubuno-sheets-journal-{}", std::process::id()));
        SheetsHost::new(Box::new(fake.clone()), FileJournal::at(dir), Some("me".into()))
    }

    #[test]
    fn a_save_reads_then_sends_only_the_changed_sheet_fields() {
        let fake = Fake::new();
        let mut h = host(&fake);
        let snap = h.join("w1").expect("joined");
        assert_eq!(snap.editors, vec!["Alice".to_string()]);
        let mut wb = Workbook::from_bytes(&snap.content, "").expect("bundle");
        wb.sheets[0].cells.insert("B1".into(), kubuno_office_sheets_core::model::cell::Cell::value(json!(2)));
        let content = wb.to_bundle_bytes();
        let job = SaveJob {
            id: "w1".into(),
            content_digest: session::Digest::of(&content),
            content,
            expect: Some(session::Digest::of(&snap.content)),
            if_match: None,
            idempotency_key: "k".into(),
        };
        let outcome = job.run(&mut h);
        assert!(matches!(outcome, session::Outcome::Saved { .. }), "{outcome:?}");
        let log = fake.log();
        assert!(log.iter().any(|l| l == "PATCH /api/v1/office/spreadsheets/w1/sheets/s1"), "{log:?}");
        let again = h.read("w1").expect("read");
        assert_eq!(again.sheets[0].cells["B1"].v, Some(json!(2)));
    }

    #[test]
    fn a_change_on_the_server_is_a_conflict() {
        let fake = Fake::new();
        let mut h = host(&fake);
        let snap = h.fetch("w1").expect("fetched");
        fake.state.lock().unwrap().0["data"]["s1"]["cells"]["A1"]["v"] = json!(99);
        let job = SaveJob {
            id: "w1".into(),
            content_digest: String::new(),
            content: snap.content.clone(),
            expect: Some(session::Digest::of(&snap.content)),
            if_match: None,
            idempotency_key: "k".into(),
        };
        assert!(matches!(job.run(&mut h), session::Outcome::Conflict { .. }));
        assert!(!fake.log().iter().any(|l| l.starts_with("PATCH")));
    }

    #[test]
    fn the_config_maps_the_spreadsheet_autosave() {
        struct Cfg;
        impl Transport for Cfg {
            fn send(&self, _r: &Request) -> client::ApiResult<Response> {
                Ok(Response::json(200, &json!({"config": {"office.spreadsheet_autosave_s": 5, "office.autosave_interval_s": 60}})))
            }
        }
        let dir = std::env::temp_dir().join("kubuno-sheets-journal-cfg");
        let mut h = SheetsHost::new(Box::new(Cfg), FileJournal::at(dir), None);
        let cfg = h.config().expect("config");
        assert_eq!(cfg.get(session::AUTOSAVE_KEY), Some(&json!(5)));
    }

    #[test]
    fn the_journal_round_trips() {
        let dir = std::env::temp_dir().join(format!("kubuno-sheets-journal-rt-{}", std::process::id()));
        let mut j = FileJournal::at(dir);
        j.write("a/b", b"{}").expect("written");
        assert_eq!(j.read("a/b").as_deref(), Some(&b"{}"[..]));
        j.clear("a/b");
        assert!(j.read("a/b").is_none());
    }
}
