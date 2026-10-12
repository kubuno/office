//! The typed client for `/api/v1/office/whiteboard/boards/*` (`office/web/src/whiteboard-api.ts`,
//! `office/server/src/whiteboard/handlers/boards.rs`), through the shell's token broker
//! (`kubuno_desktop_sync::account_api`): this app never holds a password or a refresh token.
//!
//! Only the board *metadata* goes through here (title, background, stars, open-by-file). The board's
//! content (elements and strokes) is the Yjs document synced over the WebSocket (`crate::collab`); there
//! is no content REST endpoint and no digest precondition.

use kubuno_office_documents_common::api::client::{urlencode, ApiResult, Error, Response};
use serde_json::{json, Value};

/// How the client reaches the server: one method, so a test double is a closure.
pub trait Wire: Send + Sync {
    fn send(&self, method: &str, path: &str, body: Option<&Value>) -> ApiResult<Response>;
}

/// The shipping wire: `kubuno_desktop_sync::account_api` (the bearer token borrowed from the shell's broker)
/// for one account.
pub struct SyncWire {
    account: kubuno_desktop_sync::AccountRef,
}

impl SyncWire {
    pub fn new(account: kubuno_desktop_sync::AccountRef) -> Self {
        Self { account }
    }
}

/// The `User-Agent` of this app.
pub fn user_agent() -> String {
    format!("Kubuno-Whiteboard/{}", env!("CARGO_PKG_VERSION"))
}

impl Wire for SyncWire {
    fn send(&self, method: &str, path: &str, body: Option<&Value>) -> ApiResult<Response> {
        let headers = vec![("User-Agent".to_string(), user_agent())];
        // A POST without a body still sends `{}` (axum's `Json` extractors refuse an empty body).
        let bytes = match (body, method) {
            (Some(v), _) => Some(serde_json::to_vec(v).unwrap_or_default()),
            (None, "POST") => Some(b"{}".to_vec()),
            (None, _) => None,
        };
        match kubuno_desktop_sync::account_api(&self.account).request(method, path, &headers, bytes) {
            Ok(r) => Ok(Response { status: r.status, body: r.body }),
            Err(e) => Err(Error::Transport(e.to_string())),
        }
    }
}

/// A board's row (`Board` of `whiteboard-api.ts`), the fields the desktop needs.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardMeta {
    pub id: String,
    pub title: String,
    /// `white | grid | dots | lines`.
    pub background: String,
    pub is_starred: bool,
    pub updated_at: String,
}

impl BoardMeta {
    pub fn from_value(v: &Value) -> Option<BoardMeta> {
        Some(BoardMeta {
            id: v.get("id")?.as_str()?.to_string(),
            title: v.get("title").and_then(Value::as_str).unwrap_or("").to_string(),
            background: v.get("background").and_then(Value::as_str).unwrap_or("dots").to_string(),
            is_starred: v.get("is_starred").and_then(Value::as_bool).unwrap_or(false),
            updated_at: v.get("updated_at").and_then(Value::as_str).unwrap_or("").to_string(),
        })
    }
}

/// `/api/v1/office/whiteboard/boards<suffix>`.
pub fn boards_path(suffix: &str) -> String {
    format!("/api/v1/office/whiteboard/boards{suffix}")
}

/// `/api/v1/office/whiteboard/boards/<id><suffix>`, the id escaped.
pub fn board_path(id: &str, suffix: &str) -> String {
    format!("/api/v1/office/whiteboard/boards/{}{suffix}", urlencode(id))
}

/// The board routes.
pub struct BoardClient {
    wire: Box<dyn Wire>,
}

impl BoardClient {
    /// Over the shell's token broker, as `account`.
    pub fn for_account(account: kubuno_desktop_sync::AccountRef) -> Self {
        Self { wire: Box::new(SyncWire::new(account)) }
    }

    pub fn with_wire(wire: Box<dyn Wire>) -> Self {
        Self { wire }
    }

    fn call(&self, method: &str, path: &str, body: Option<Value>) -> ApiResult<Value> {
        let r: Response = self.wire.send(method, path, body.as_ref())?;
        if r.ok() {
            r.parse()
        } else {
            Err(Error::from_response(r.status, &r.body))
        }
    }

    /// `GET /boards/:id` → the board row.
    pub fn get(&self, id: &str) -> ApiResult<BoardMeta> {
        let v = self.call("GET", &board_path(id, ""), None)?;
        BoardMeta::from_value(&v["board"]).ok_or_else(|| Error::Malformed("board".into()))
    }

    /// `POST /boards/open-by-file { file_id }` → the board of a Drive file (the desktop's « ouvrir »).
    pub fn open_by_file(&self, file_id: &str) -> ApiResult<BoardMeta> {
        let v = self.call("POST", &boards_path("/open-by-file"), Some(json!({ "file_id": file_id })))?;
        BoardMeta::from_value(&v["board"]).ok_or_else(|| Error::Malformed("board".into()))
    }

    /// `GET /boards?limit=200` → every board not in the trash, latest first (the start-page list).
    pub fn list(&self) -> ApiResult<Vec<BoardMeta>> {
        let v = self.call("GET", &boards_path("?limit=200"), None)?;
        Ok(v["boards"].as_array().map(|a| a.iter().filter_map(BoardMeta::from_value).collect()).unwrap_or_default())
    }

    /// `POST /boards { title, background }` → the new board.
    pub fn create(&self, title: &str, background: &str) -> ApiResult<BoardMeta> {
        let v = self.call("POST", &boards_path(""), Some(json!({ "title": title, "background": background })))?;
        BoardMeta::from_value(&v["board"]).ok_or_else(|| Error::Malformed("board".into()))
    }

    /// `PATCH /boards/:id` with the title, background and/or star.
    pub fn update(&self, id: &str, title: Option<&str>, background: Option<&str>, starred: Option<bool>) -> ApiResult<BoardMeta> {
        let mut body = serde_json::Map::new();
        if let Some(t) = title {
            body.insert("title".into(), t.into());
        }
        if let Some(b) = background {
            body.insert("background".into(), b.into());
        }
        if let Some(s) = starred {
            body.insert("is_starred".into(), s.into());
        }
        let v = self.call("PATCH", &board_path(id, ""), Some(Value::Object(body)))?;
        BoardMeta::from_value(&v["board"]).ok_or_else(|| Error::Malformed("board".into()))
    }

    /// The core's public configuration (`GET /api/v1/config`, `config` object).
    pub fn core_config(&self) -> ApiResult<serde_json::Map<String, Value>> {
        let v = self.call("GET", kubuno_office_documents_common::api::session::CONFIG_PATH, None)?;
        Ok(v.get("config").and_then(Value::as_object).cloned().unwrap_or_default())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    pub type Seen = Arc<Mutex<Vec<(String, String, Option<Value>)>>>;
    pub type Answer = Box<dyn Fn(&str, &str) -> Response + Send + Sync>;

    pub struct Fake {
        pub seen: Seen,
        pub answer: Answer,
    }

    impl Wire for Fake {
        fn send(&self, method: &str, path: &str, body: Option<&Value>) -> ApiResult<Response> {
            self.seen.lock().unwrap_or_else(|p| p.into_inner()).push((method.to_string(), path.to_string(), body.cloned()));
            Ok((self.answer)(method, path))
        }
    }

    #[test]
    fn get_reads_the_board_row() {
        let seen: Seen = Arc::default();
        let body = json!({ "board": { "id": "b1", "title": "Plan", "background": "grid", "is_starred": true, "updated_at": "2026-10-12T06:00:00Z" } });
        let c = BoardClient::with_wire(Box::new(Fake { seen: seen.clone(), answer: Box::new(move |_, _| Response::json(200, &body)) }));
        let b = c.get("b1").expect("get");
        assert_eq!((b.title.as_str(), b.background.as_str(), b.is_starred), ("Plan", "grid", true));
        assert_eq!(seen.lock().unwrap()[0].1, "/api/v1/office/whiteboard/boards/b1");
    }

    #[test]
    fn open_by_file_and_update_send_the_web_bodies() {
        let seen: Seen = Arc::default();
        let c = BoardClient::with_wire(Box::new(Fake { seen: seen.clone(), answer: Box::new(|_, _| Response::json(200, &json!({ "board": { "id": "b", "title": "", "background": "dots" } }))) }));
        c.open_by_file("file-9").expect("open");
        c.update("b/2", Some("New"), Some("lines"), Some(false)).expect("update");
        let seen = seen.lock().expect("seen");
        assert_eq!(seen[0].2, Some(json!({ "file_id": "file-9" })));
        assert_eq!(seen[1].1, "/api/v1/office/whiteboard/boards/b%2F2");
        assert_eq!(seen[1].2, Some(json!({ "title": "New", "background": "lines", "is_starred": false })));
    }

    #[test]
    fn a_413_is_too_large() {
        let c = BoardClient::with_wire(Box::new(Fake { seen: Arc::default(), answer: Box::new(|_, _| Response { status: 413, body: Vec::new() }) }));
        assert_eq!(c.get("b"), Err(Error::TooLarge));
    }
}
