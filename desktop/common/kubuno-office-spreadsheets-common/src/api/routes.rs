//! The spreadsheet routes of the office module (`server/src/router/mod.rs`), as requests.
//!
//! Built by pure functions so the paths, the bodies and the escaping are testable without a network; sent
//! through the transport shared with Kubuno Documents (`kubuno_office_desktop_common::api::client`).

use kubuno_office_desktop_common::api::client::{urlencode, Method, Request, OFFICE_PREFIX};
use serde_json::Value as Json;

/// The `User-Agent` of this app.
pub fn user_agent() -> String {
    format!("Kubuno-Spreadsheets/{}", env!("CARGO_PKG_VERSION"))
}

fn req(method: Method, path: String) -> Request {
    Request { method, path, headers: vec![("User-Agent".to_string(), user_agent())], body: None }
}

/// `/api/v1/office/spreadsheets/<id><suffix>`.
pub fn path(id: &str, suffix: &str) -> String {
    format!("{OFFICE_PREFIX}/spreadsheets/{}{suffix}", urlencode(id))
}

/// `GET /spreadsheets/:id` — `{ spreadsheet, sheets }`.
pub fn get_workbook(id: &str) -> Request {
    req(Method::Get, path(id, ""))
}

/// `GET /spreadsheets/:id/sheets/:sid` — `{ sheet, data, names }`.
pub fn get_sheet(id: &str, sheet: &str) -> Request {
    req(Method::Get, path(id, &format!("/sheets/{}", urlencode(sheet))))
}

/// `PATCH /spreadsheets/:id/sheets/:sid` with the given fields.
pub fn patch_sheet(id: &str, sheet: &str, body: Json) -> Request {
    let mut r = req(Method::Patch, path(id, &format!("/sheets/{}", urlencode(sheet))));
    r.body = Some(body);
    r
}

/// `POST /spreadsheets/:id/sheets` `{ id, name }` (the client-minted id is honoured).
pub fn create_sheet(id: &str, sheet: &str, name: &str) -> Request {
    let mut r = req(Method::Post, path(id, "/sheets"));
    r.body = Some(serde_json::json!({ "id": sheet, "name": name }));
    r
}

/// `DELETE /spreadsheets/:id/sheets/:sid`.
pub fn delete_sheet(id: &str, sheet: &str) -> Request {
    req(Method::Delete, path(id, &format!("/sheets/{}", urlencode(sheet))))
}

/// `PATCH /spreadsheets/:id` `{ title }`.
pub fn rename(id: &str, title: &str) -> Request {
    let mut r = req(Method::Patch, path(id, ""));
    r.body = Some(serde_json::json!({ "title": title }));
    r
}

/// `POST /spreadsheets/:id/editing/join`.
pub fn join(id: &str) -> Request {
    req(Method::Post, path(id, "/editing/join"))
}

/// `POST /spreadsheets/:id/editing/ping`.
pub fn ping(id: &str) -> Request {
    req(Method::Post, path(id, "/editing/ping"))
}

/// `DELETE /spreadsheets/:id/editing/leave`.
pub fn leave(id: &str) -> Request {
    req(Method::Delete, path(id, "/editing/leave"))
}

/// `GET /spreadsheets/:id/export/xlsx|ods`.
pub fn export(id: &str, format: &str) -> Request {
    req(Method::Get, path(id, &format!("/export/{format}")))
}

/// `GET /api/v1/config` (the core's public settings).
pub fn core_config() -> Request {
    req(Method::Get, kubuno_office_desktop_common::api::client::CORE_CONFIG_PATH.to_string())
}

/// `POST /spreadsheets` `{ title }` — `{ spreadsheet, sheets }` (« Nouveau »: the web names it `common_untitled`).
pub fn create_workbook(title: &str) -> Request {
    let mut r = req(Method::Post, format!("{OFFICE_PREFIX}/spreadsheets"));
    r.body = Some(serde_json::json!({ "title": title }));
    r
}

/// `POST /spreadsheets/:id/duplicate` — `{ id }` of the copy, titled « … (copie) » (« Dupliquer »).
pub fn duplicate(id: &str) -> Request {
    req(Method::Post, path(id, "/duplicate"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_escaped() {
        assert_eq!(get_sheet("a b", "s/1").path, "/api/v1/office/spreadsheets/a%20b/sheets/s%2F1");
        assert_eq!(leave("x").method, Method::Delete);
        assert!(get_workbook("x").header_value("user-agent").is_some_and(|u| u.starts_with("Kubuno-Spreadsheets/")));
    }
}
