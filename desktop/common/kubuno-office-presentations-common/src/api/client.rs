//! The typed client for `/api/v1/office/presentations/*` (server `handlers/presentations.rs`).
//!
//! # Facts this is built on (read in the server, 2026-10-07)
//!
//! * The content is not in the database: the slides' JSON lives in a gzipped `.kbsld` file in Drive; the order
//!   and the hidden flags are rows of `office.slides`. Every slide write re-reads and rewrites the whole file.
//! * No route has a concurrency token (no etag, no `If-Match`): the save path compares content digests itself
//!   (api/session.rs).
//! * Every route here accepts the owner OR a collaborator (`require_pres_access`).
//! * Bodies are capped at axum's default 2 MiB per request: a 413 is terminal for that slide.
//!
//! The routes, the headers and the error mapping only; when to call them is the session's business. The error
//! type and the helpers are Documents' (`kubuno_office_desktop_common::api::client`): the office API answers
//! both apps the same way.

use serde_json::Value;

pub use kubuno_office_desktop_common::api::client::{urlencode, ApiResult, Error, Response, OFFICE_PREFIX};

use kubuno_office_slides_core::obj::{quote, Obj, Slot};

/// The HTTP verbs the presentation routes use (`PUT` is the elements save, which Documents never needs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }
}

/// One request: method, path, body as JSON TEXT (the slide's elements are written byte for byte, in their
/// stored key order — a `serde_json::Value` would reorder them).
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl Request {
    pub fn new(method: Method, path: impl Into<String>) -> Self {
        Self { method, path: path.into(), headers: vec![("User-Agent".into(), user_agent())], body: None }
    }

    pub fn body(mut self, json: String) -> Self {
        self.body = Some(json);
        self
    }
}

/// How a [`Client`] reaches the server (a test double is three lines).
pub trait Transport {
    fn send(&self, request: &Request) -> ApiResult<Response>;
}

/// The shipping transport: `kubuno_desktop_sync` as the shell's current account (the access token borrowed from
/// its broker; refresh, rotation and the instance URL are the sync client's).
pub struct SyncTransport {
    account: kubuno_desktop_sync::AccountRef,
}

impl SyncTransport {
    pub fn for_account(account: kubuno_desktop_sync::AccountRef) -> Self {
        Self { account }
    }
}

impl Transport for SyncTransport {
    fn send(&self, request: &Request) -> ApiResult<Response> {
        // A body-less POST still sends `{}`: axum's `Json<…>` extractors refuse an empty body.
        let body = match (&request.body, request.method) {
            (Some(b), _) => Some(b.clone().into_bytes()),
            (None, Method::Post) => Some(b"{}".to_vec()),
            (None, _) => None,
        };
        match kubuno_desktop_sync::account_api(&self.account).request(request.method.as_str(), &request.path, &request.headers, body) {
            Ok(r) => Ok(Response { status: r.status, body: r.body }),
            Err(e) => Err(Error::Transport(e.to_string())),
        }
    }
}

/// `Kubuno-Presentations/<version>`.
pub fn user_agent() -> String {
    format!("Kubuno-Presentations/{}", env!("CARGO_PKG_VERSION"))
}

/// `/api/v1/office/presentations/<id><suffix>`.
pub fn pres_path(id: &str, suffix: &str) -> String {
    format!("{OFFICE_PREFIX}/presentations/{}{suffix}", urlencode(id))
}

/// `/api/v1/office/presentations/<id>/slides/<sid><suffix>`.
pub fn slide_path(id: &str, sid: &str, suffix: &str) -> String {
    pres_path(id, &format!("/slides/{}{suffix}", urlencode(sid)))
}

/// `GET /presentations/:id`: the metadata and the ordered slide summaries.
#[derive(Clone, Debug, PartialEq)]
pub struct Meta {
    /// `{presentation, slides}` as received.
    pub value: Value,
    pub title: String,
}

/// The typed client.
pub struct Client {
    transport: Box<dyn Transport + Send + Sync>,
}

impl Client {
    pub fn new(transport: Box<dyn Transport + Send + Sync>) -> Self {
        Self { transport }
    }

    pub fn for_account(account: kubuno_desktop_sync::AccountRef) -> Self {
        Self::new(Box::new(SyncTransport::for_account(account)))
    }

    /// Sends and maps a non-2xx onto [`Error`].
    pub fn call(&self, req: &Request) -> ApiResult<Response> {
        let r = self.transport.send(req)?;
        if r.ok() {
            Ok(r)
        } else {
            Err(Error::from_response(r.status, &r.body))
        }
    }

    fn text(r: &Response) -> ApiResult<String> {
        String::from_utf8(r.body.clone()).map_err(|e| Error::Malformed(format!("UTF-8 : {e}")))
    }

    pub fn meta(&self, id: &str) -> ApiResult<Meta> {
        let r = self.call(&Request::new(Method::Get, pres_path(id, "")))?;
        let value = r.parse()?;
        let title = value.get("presentation").and_then(|p| p.get("title")).and_then(Value::as_str).unwrap_or_default().to_string();
        if value.get("slides").and_then(Value::as_array).is_none() {
            return Err(Error::Malformed("no `slides` in the presentation".into()));
        }
        Ok(Meta { value, title })
    }

    /// `POST :id/editing/join`: the whole content file as JSON TEXT (key order and bytes kept) and the editors.
    pub fn join(&self, id: &str) -> ApiResult<(String, Value)> {
        let r = self.call(&Request::new(Method::Post, pres_path(id, "/editing/join")))?;
        let top = Obj::parse(&Self::text(&r)?).map_err(|e| Error::Malformed(e.to_string()))?;
        let content = top.slot("content").map(Slot::text).ok_or_else(|| Error::Malformed("no `content` in join".into()))?;
        Ok((content, top.get("editors").cloned().unwrap_or(Value::Null)))
    }

    pub fn ping(&self, id: &str) -> ApiResult<()> {
        self.call(&Request::new(Method::Post, pres_path(id, "/editing/ping"))).map(|_| ())
    }

    pub fn leave(&self, id: &str) -> ApiResult<()> {
        self.call(&Request::new(Method::Delete, pres_path(id, "/editing/leave"))).map(|_| ())
    }

    /// `GET :id/slides/:sid`: the slide's content (`data`) as JSON text.
    pub fn slide(&self, id: &str, sid: &str) -> ApiResult<String> {
        let r = self.call(&Request::new(Method::Get, slide_path(id, sid, "")))?;
        let top = Obj::parse(&Self::text(&r)?).map_err(|e| Error::Malformed(e.to_string()))?;
        top.slot("data").map(Slot::text).ok_or_else(|| Error::Malformed("no `data` in the slide".into()))
    }

    /// `PUT :id/slides/:sid {elements, notes}` (elements as stored JSON text).
    pub fn put_slide(&self, id: &str, sid: &str, elements_json: Option<&str>, notes: Option<&str>) -> ApiResult<()> {
        let mut parts = Vec::new();
        if let Some(e) = elements_json {
            parts.push(format!("\"elements\":{e}"));
        }
        if let Some(n) = notes {
            parts.push(format!("\"notes\":{}", quote(n)));
        }
        let body = format!("{{{}}}", parts.join(","));
        self.call(&Request::new(Method::Put, slide_path(id, sid, "")).body(body)).map(|_| ())
    }

    /// `PATCH :id/slides/:sid {background?, transition?, is_hidden?}` (values as JSON text).
    pub fn patch_slide(&self, id: &str, sid: &str, background: Option<&str>, transition: Option<&str>, hidden: Option<bool>) -> ApiResult<()> {
        let mut parts = Vec::new();
        if let Some(b) = background {
            parts.push(format!("\"background\":{b}"));
        }
        if let Some(t) = transition {
            parts.push(format!("\"transition\":{t}"));
        }
        if let Some(h) = hidden {
            parts.push(format!("\"is_hidden\":{h}"));
        }
        let body = format!("{{{}}}", parts.join(","));
        self.call(&Request::new(Method::Patch, slide_path(id, sid, "")).body(body)).map(|_| ())
    }

    /// `POST :id/slides {id, position}`: a slide with OUR id (the server honours client-minted ids), its
    /// content the server's empty slide.
    pub fn create_slide(&self, id: &str, sid: &str, position: usize) -> ApiResult<()> {
        let body = format!("{{\"id\":{},\"position\":{position}}}", quote(sid));
        self.call(&Request::new(Method::Post, pres_path(id, "/slides")).body(body)).map(|_| ())
    }

    pub fn delete_slide(&self, id: &str, sid: &str) -> ApiResult<()> {
        self.call(&Request::new(Method::Delete, slide_path(id, sid, ""))).map(|_| ())
    }

    /// `PATCH :id/slides/reorder {slides:[{id, position}]}`.
    pub fn reorder(&self, id: &str, order: &[String]) -> ApiResult<()> {
        let items: Vec<String> = order.iter().enumerate().map(|(i, s)| format!("{{\"id\":{},\"position\":{i}}}", quote(s))).collect();
        let body = format!("{{\"slides\":[{}]}}", items.join(","));
        self.call(&Request::new(Method::Patch, pres_path(id, "/slides/reorder")).body(body)).map(|_| ())
    }

    /// `PATCH :id {title}` (renames the `.kbsld` too).
    pub fn rename(&self, id: &str, title: &str) -> ApiResult<()> {
        let body = format!("{{\"title\":{}}}", quote(title));
        self.call(&Request::new(Method::Patch, pres_path(id, "")).body(body)).map(|_| ())
    }

    pub fn set_starred(&self, id: &str, starred: bool) -> ApiResult<()> {
        self.call(&Request::new(Method::Patch, pres_path(id, "")).body(format!("{{\"is_starred\":{starred}}}"))).map(|_| ())
    }

    /// `GET :id/assets/:file_id`: an image's bytes.
    pub fn asset(&self, id: &str, file_id: &str) -> ApiResult<Vec<u8>> {
        self.call(&Request::new(Method::Get, pres_path(id, &format!("/assets/{}", urlencode(file_id))))).map(|r| r.body)
    }

    /// `GET /presentations?recent=true`: the recent list for the backstage.
    pub fn recent(&self) -> ApiResult<Vec<Value>> {
        let r = self.call(&Request::new(Method::Get, format!("{OFFICE_PREFIX}/presentations?recent=true")))?;
        Ok(r.parse()?.get("presentations").and_then(Value::as_array).cloned().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Fake {
        sent: Mutex<Vec<Request>>,
        answer: Response,
    }

    impl Transport for std::sync::Arc<Fake> {
        fn send(&self, request: &Request) -> ApiResult<Response> {
            self.sent.lock().unwrap_or_else(|p| p.into_inner()).push(request.clone());
            Ok(self.answer.clone())
        }
    }

    fn client(status: u16, body: &str) -> (Client, std::sync::Arc<Fake>) {
        let fake = std::sync::Arc::new(Fake { sent: Mutex::new(Vec::new()), answer: Response { status, body: body.as_bytes().to_vec() } });
        (Client::new(Box::new(fake.clone())), fake)
    }

    #[test]
    fn a_slide_save_is_a_put_of_the_stored_elements() {
        let (c, fake) = client(200, "{}");
        c.put_slide("p 1", "s1", Some(r#"[{"id":"a","x":0.5}]"#), Some("n\"")).expect("put");
        let sent = fake.sent.lock().unwrap_or_else(|p| p.into_inner())[0].clone();
        assert_eq!(sent.method, Method::Put);
        assert_eq!(sent.path, "/api/v1/office/presentations/p%201/slides/s1");
        assert_eq!(sent.body.as_deref(), Some(r#"{"elements":[{"id":"a","x":0.5}],"notes":"n\""}"#));
    }

    #[test]
    fn the_slide_content_keeps_its_bytes_and_errors_map() {
        let (c, _) = client(200, r#"{"slide":{"id":"s"},"data":{"notes":"","elements":[{"y":1.50,"x":2}]}}"#);
        assert_eq!(c.slide("p", "s").ok().as_deref(), Some(r#"{"notes":"","elements":[{"y":1.50,"x":2}]}"#));
        let (c, _) = client(413, "");
        assert_eq!(c.put_slide("p", "s", Some("[]"), None), Err(Error::TooLarge));
        let (c, fake) = client(200, "{}");
        c.reorder("p", &["b".into(), "a".into()]).expect("reorder");
        assert_eq!(fake.sent.lock().unwrap_or_else(|p| p.into_inner())[0].body.as_deref(), Some(r#"{"slides":[{"id":"b","position":0},{"id":"a","position":1}]}"#));
    }
}
