//! The Maths routes of the office module (`server/src/maths/handlers/formulas.rs`, base `/office/maths`), as
//! requests of the shared transport (`kubuno_office_desktop_common::api::client`). Pure functions, so paths
//! and bodies are testable without a network.
//!
//! A formula's content is a single `latex` string; a save is one `PATCH` of it (no file-save session).

use kubuno_office_desktop_common::api::client::{urlencode, Method, Request, OFFICE_PREFIX};
use serde_json::{json, Value};

/// The `User-Agent` of this app.
pub fn user_agent() -> String {
    format!("Kubuno-Maths/{}", env!("CARGO_PKG_VERSION"))
}

fn req(method: Method, path: String) -> Request {
    Request { method, path, headers: vec![("User-Agent".to_string(), user_agent())], body: None }
}

fn req_body(method: Method, path: String, body: Value) -> Request {
    let mut r = req(method, path);
    r.body = Some(body);
    r
}

fn maths(suffix: &str) -> String {
    format!("{OFFICE_PREFIX}/maths{suffix}")
}

// ── Reads ───────────────────────────────────────────────────────────────────

/// `GET /office/maths/formulas/:id` — `{ formula }` (its `latex` read from the `.kbmath` file).
pub fn get_formula(id: &str) -> Request {
    req(Method::Get, maths(&format!("/formulas/{}", urlencode(id))))
}

/// `POST /office/maths/formulas/open-by-file` `{ file_id }` — resolve a `.kbmath` to its formula.
pub fn open_by_file(file_id: &str) -> Request {
    req_body(Method::Post, maths("/formulas/open-by-file"), json!({ "file_id": file_id }))
}

/// `GET /office/maths/formulas` — the account's formulas.
pub fn list_formulas() -> Request {
    req(Method::Get, maths("/formulas"))
}

// ── Mutations ─────────────────────────────────────────────────────────────────

/// `PATCH /office/maths/formulas/:id` — the content (`latex`), and optionally the name / star. The server
/// writes `latex` to the `.kbmath` Drive file; there is no `If-Match`.
pub fn update_formula(id: &str, latex: Option<&str>, name: Option<&str>, starred: Option<bool>) -> Request {
    let mut body = serde_json::Map::new();
    if let Some(l) = latex {
        body.insert("latex".into(), json!(l));
    }
    if let Some(n) = name {
        body.insert("name".into(), json!(n));
    }
    if let Some(s) = starred {
        body.insert("is_starred".into(), json!(s));
    }
    req_body(Method::Patch, maths(&format!("/formulas/{}", urlencode(id))), Value::Object(body))
}

/// `POST /office/maths/formulas/:id/duplicate` — `{ formula }`.
pub fn duplicate_formula(id: &str) -> Request {
    req_body(Method::Post, maths(&format!("/formulas/{}/duplicate", urlencode(id))), json!({}))
}

/// `POST /office/maths/formulas/:id/trash`.
pub fn trash_formula(id: &str) -> Request {
    req_body(Method::Post, maths(&format!("/formulas/{}/trash", urlencode(id))), json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert_eq!(get_formula("abc").path, format!("{OFFICE_PREFIX}/maths/formulas/abc"));
        let u = update_formula("id1", Some("x^2"), None, Some(true));
        assert_eq!(u.method, Method::Patch);
        let body = u.body.unwrap();
        assert_eq!(body.get("latex").unwrap(), "x^2");
        assert_eq!(body.get("is_starred").unwrap(), true);
        assert!(body.get("name").is_none());
    }
}
