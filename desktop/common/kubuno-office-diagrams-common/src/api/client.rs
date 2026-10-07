//! The typed client for `/api/v1/office/diagrams/*` (`office/server/src/handlers/diagrams.rs`, routes
//! `office/server/src/router/*.rs:225-259`), through the shell's token broker
//! (`kubuno_desktop_sync::request`): this app never holds a password or a refresh token. The status
//! mapping is the office client's (`Error::from_response`: 413 → `TooLarge`, 403/404…).
//!
//! Server facts the client is built on: every diagram route is owner-only; there is no etag at all; the
//! page save is `PUT …/pages/:pid/data { data }` (2 MiB body limit → 413); `POST …/pages` honours a
//! client-minted `id` (`CreatePageDto.id`).

use kubuno_office_desktop_common::api::client::{urlencode, ApiResult, Error, Response};
use serde_json::{json, Value};

use crate::doc::PageMeta;

/// How the client reaches the server: one method, so a test double is a closure.
pub trait Wire: Send + Sync {
    fn send(&self, method: &str, path: &str, body: Option<&Value>) -> ApiResult<Response>;
}

/// The shipping wire: `kubuno_desktop_sync::request` (the bearer token borrowed from the shell's broker,
/// its refresh and rotation).
pub struct SyncWire {
    instance: String,
}

impl SyncWire {
    pub fn new(instance: String) -> Self {
        Self { instance }
    }
}

/// The `User-Agent` of this app.
pub fn user_agent() -> String {
    format!("Kubuno-Diagrams/{}", env!("CARGO_PKG_VERSION"))
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
        match kubuno_desktop_sync::request(&self.instance, method, path, &headers, bytes) {
            Ok(r) => Ok(Response { status: r.status, body: r.body }),
            Err(e) => Err(Error::Transport(e.to_string())),
        }
    }
}

/// A diagram's row (`Diagram`).
#[derive(Debug, Clone, PartialEq)]
pub struct DiagramMeta {
    pub id: String,
    pub title: String,
    pub is_starred: bool,
    pub updated_at: String,
    pub file_id: Option<String>,
}

impl DiagramMeta {
    pub fn from_value(v: &Value) -> Option<DiagramMeta> {
        Some(DiagramMeta {
            id: v.get("id")?.as_str()?.to_string(),
            title: v.get("title").and_then(Value::as_str).unwrap_or("").to_string(),
            is_starred: v.get("is_starred").and_then(Value::as_bool).unwrap_or(false),
            updated_at: v.get("updated_at").and_then(Value::as_str).unwrap_or("").to_string(),
            file_id: v.get("file_id").and_then(Value::as_str).map(str::to_string),
        })
    }
}

/// `/api/v1/office/diagrams<suffix>`.
pub fn diagrams_path(suffix: &str) -> String {
    format!("/api/v1/office/diagrams{suffix}")
}

/// `/api/v1/office/diagrams/<id><suffix>`, the id escaped.
pub fn diagram_path(id: &str, suffix: &str) -> String {
    format!("/api/v1/office/diagrams/{}{suffix}", urlencode(id))
}

/// The diagram routes.
pub struct DiagramClient {
    wire: Box<dyn Wire>,
}

impl DiagramClient {
    /// Over the shell's token broker, as account `instance`.
    pub fn new(instance: String) -> Self {
        Self { wire: Box::new(SyncWire::new(instance)) }
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

    /// `GET /diagrams/:id`: the row and the pages' metadata in display order.
    pub fn get(&self, id: &str) -> ApiResult<(DiagramMeta, Vec<PageMeta>)> {
        let v = self.call("GET", &diagram_path(id, ""), None)?;
        let d = DiagramMeta::from_value(&v["diagram"]).ok_or_else(|| Error::Malformed("diagram".into()))?;
        let mut pages: Vec<PageMeta> = v["pages"].as_array().map(|a| a.iter().filter_map(PageMeta::from_value).collect()).unwrap_or_default();
        pages.sort_by_key(|p| p.position);
        Ok((d, pages))
    }

    /// `GET /diagrams/:id/export/json`.
    pub fn export_json(&self, id: &str) -> ApiResult<Value> {
        self.call("GET", &diagram_path(id, "/export/json"), None)
    }

    /// `GET /diagrams?recent=true&limit=20` (the start page's recent list).
    pub fn recent(&self) -> ApiResult<Vec<DiagramMeta>> {
        let v = self.call("GET", &diagrams_path("?recent=true&limit=20"), None)?;
        Ok(v["diagrams"].as_array().map(|a| a.iter().filter_map(DiagramMeta::from_value).collect()).unwrap_or_default())
    }

    /// `POST /diagrams { title }` → the new diagram.
    pub fn create(&self, title: &str) -> ApiResult<DiagramMeta> {
        let v = self.call("POST", &diagrams_path(""), Some(json!({ "title": title })))?;
        DiagramMeta::from_value(&v["diagram"]).ok_or_else(|| Error::Malformed("diagram".into()))
    }

    /// `PATCH /diagrams/:id` with the title and/or the star.
    pub fn update(&self, id: &str, title: Option<&str>, starred: Option<bool>) -> ApiResult<DiagramMeta> {
        let mut body = serde_json::Map::new();
        if let Some(t) = title {
            body.insert("title".into(), t.into());
        }
        if let Some(s) = starred {
            body.insert("is_starred".into(), s.into());
        }
        let v = self.call("PATCH", &diagram_path(id, ""), Some(Value::Object(body)))?;
        DiagramMeta::from_value(&v["diagram"]).ok_or_else(|| Error::Malformed("diagram".into()))
    }

    /// `POST /diagrams/:id/trash`.
    pub fn trash(&self, id: &str) -> ApiResult<()> {
        self.call("POST", &diagram_path(id, "/trash"), None).map(|_| ())
    }

    /// `POST /diagrams/:id/duplicate` → the copy.
    pub fn duplicate(&self, id: &str) -> ApiResult<DiagramMeta> {
        let v = self.call("POST", &diagram_path(id, "/duplicate"), None)?;
        DiagramMeta::from_value(&v["diagram"]).ok_or_else(|| Error::Malformed("diagram".into()))
    }

    /// `POST /diagrams/:id/pages { id, name }` (the id minted here, so the page keeps it everywhere).
    pub fn create_page(&self, id: &str, page_id: &str, name: &str, bg_color: &str) -> ApiResult<PageMeta> {
        let v = self.call("POST", &diagram_path(id, "/pages"), Some(json!({ "id": page_id, "name": name, "bg_color": bg_color })))?;
        PageMeta::from_value(&v["page"]).ok_or_else(|| Error::Malformed("page".into()))
    }

    /// `PATCH /diagrams/:id/pages/:pid` (name, colour).
    pub fn update_page_meta(&self, id: &str, page_id: &str, name: &str, bg_color: &str) -> ApiResult<()> {
        let path = diagram_path(id, &format!("/pages/{}", urlencode(page_id)));
        self.call("PATCH", &path, Some(json!({ "name": name, "bg_color": bg_color }))).map(|_| ())
    }

    /// `PATCH /diagrams/:id/pages/reorder { pages: [{ id, position }] }`.
    pub fn reorder_pages(&self, id: &str, order: &[String]) -> ApiResult<()> {
        let pages: Vec<Value> = order.iter().enumerate().map(|(i, p)| json!({ "id": p, "position": i })).collect();
        self.call("PATCH", &diagram_path(id, "/pages/reorder"), Some(json!({ "pages": pages }))).map(|_| ())
    }

    /// `DELETE /diagrams/:id/pages/:pid`.
    pub fn delete_page(&self, id: &str, page_id: &str) -> ApiResult<()> {
        self.call("DELETE", &diagram_path(id, &format!("/pages/{}", urlencode(page_id))), None).map(|_| ())
    }

    /// `PUT /diagrams/:id/pages/:pid/data { data }` — the save of one page.
    pub fn put_page_data(&self, id: &str, page_id: &str, data: &Value) -> ApiResult<()> {
        let path = diagram_path(id, &format!("/pages/{}/data", urlencode(page_id)));
        self.call("PUT", &path, Some(json!({ "data": data }))).map(|_| ())
    }

    /// `POST /diagrams/:id/editing/join` → the whole content and the editors.
    pub fn join(&self, id: &str) -> ApiResult<(Value, Vec<String>)> {
        let v = self.call("POST", &diagram_path(id, "/editing/join"), None)?;
        let editors = v["editors"]
            .as_array()
            .map(|a| a.iter().map(|e| e["display_name"].as_str().or(e["user_id"].as_str()).unwrap_or("").to_string()).collect())
            .unwrap_or_default();
        Ok((v["content"].clone(), editors))
    }

    /// `POST /diagrams/:id/editing/ping`.
    pub fn ping(&self, id: &str) -> ApiResult<()> {
        self.call("POST", &diagram_path(id, "/editing/ping"), None).map(|_| ())
    }

    /// `DELETE /diagrams/:id/editing/leave`.
    pub fn leave(&self, id: &str) -> ApiResult<()> {
        self.call("DELETE", &diagram_path(id, "/editing/leave"), None).map(|_| ())
    }

    /// The core's public configuration (`GET /api/v1/config`, `config` object).
    pub fn core_config(&self) -> ApiResult<serde_json::Map<String, Value>> {
        let v = self.call("GET", kubuno_office_desktop_common::api::session::CONFIG_PATH, None)?;
        Ok(v.get("config").and_then(Value::as_object).cloned().unwrap_or_default())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// A transport answering from a closure and recording every request.
    pub struct Fake {
        pub seen: Arc<Mutex<Vec<(String, String, Option<Value>)>>>,
        pub answer: Box<dyn Fn(&str, &str) -> Response + Send + Sync>,
    }

    impl Wire for Fake {
        fn send(&self, method: &str, path: &str, body: Option<&Value>) -> ApiResult<Response> {
            self.seen.lock().unwrap_or_else(|p| p.into_inner()).push((method.to_string(), path.to_string(), body.cloned()));
            Ok((self.answer)(method, path))
        }
    }

    #[test]
    fn routes_escape_ids_and_send_the_web_bodies() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let c = DiagramClient::with_wire(Box::new(Fake { seen: seen.clone(), answer: Box::new(|_, _| Response::json(200, &json!({ "ok": true }))) }));
        c.put_page_data("d/1", "p?2", &json!({ "shapes": [] })).expect("put");
        c.create_page("d", "p3", "Page 3", "#ffffff").err();
        c.reorder_pages("d", &["b".into(), "a".into()]).expect("reorder");
        let seen = seen.lock().expect("seen");
        assert_eq!(seen[0].0, "PUT");
        assert_eq!(seen[0].1, "/api/v1/office/diagrams/d%2F1/pages/p%3F2/data");
        assert_eq!(seen[0].2, Some(json!({ "data": { "shapes": [] } })));
        assert_eq!(seen[1].2, Some(json!({ "id": "p3", "name": "Page 3", "bg_color": "#ffffff" })));
        assert_eq!(seen[2].2, Some(json!({ "pages": [{ "id": "b", "position": 0 }, { "id": "a", "position": 1 }] })));
    }

    #[test]
    fn a_413_is_too_large() {
        let c = DiagramClient::with_wire(Box::new(Fake { seen: Arc::default(), answer: Box::new(|_, _| Response { status: 413, body: Vec::new() }) }));
        assert_eq!(c.put_page_data("d", "p", &json!({})), Err(Error::TooLarge));
    }
}
