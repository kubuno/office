//! The Data (BI) routes of the office module (`server/src/handlers/data_*.rs`, base `/office/data`), as
//! requests of the shared transport (`kubuno_office_documents_common::api::client`). Pure functions, so paths
//! and bodies are testable without a network.
//!
//! Data is server-coupled: reads load a report and run queries, each edit is a single mutation. There is no
//! file-save session (unlike Documents).

use kubuno_office_documents_common::api::client::{urlencode, Method, Request, OFFICE_PREFIX};
use serde_json::Value;

/// The `User-Agent` of this app.
pub fn user_agent() -> String {
    format!("Kubuno-Data/{}", env!("CARGO_PKG_VERSION"))
}

fn req(method: Method, path: String) -> Request {
    Request { method, path, headers: vec![("User-Agent".to_string(), user_agent())], body: None }
}

fn req_body(method: Method, path: String, body: Value) -> Request {
    let mut r = req(method, path);
    r.body = Some(body);
    r
}

fn data(suffix: &str) -> String {
    format!("{OFFICE_PREFIX}/data{suffix}")
}

// ── Reads ───────────────────────────────────────────────────────────────────

/// `GET /office/data/reports/:id` - `{ report, pages, widgets }`.
pub fn get_report(id: &str) -> Request {
    req(Method::Get, data(&format!("/reports/{}", urlencode(id))))
}

/// `POST /office/data/reports/open-by-file` `{ file_id }` - resolve a `.kbdrp` to its report.
pub fn open_by_file(file_id: &str) -> Request {
    req_body(Method::Post, data("/reports/open-by-file"), serde_json::json!({ "file_id": file_id }))
}

/// `GET /office/data/datasets` - the account's datasets (the Fields pane).
pub fn list_datasets() -> Request {
    req(Method::Get, data("/datasets"))
}

/// `GET /office/data/datasets/:id/preview` - a dataset's cached columns + rows.
pub fn preview_dataset(id: &str) -> Request {
    req(Method::Get, data(&format!("/datasets/{}/preview", urlencode(id))))
}

/// `POST /office/data/execute` - a widget's aggregated query (`{ dataset_id, dimensions, metrics, filters,
/// sort, limit }` → `{ columns, rows, total }`).
pub fn execute(query: Value) -> Request {
    req_body(Method::Post, data("/execute"), query)
}

// ── Mutations (one REST call per edit) ────────────────────────────────────────

/// `PATCH /office/data/reports/:id` - title, theme, dataset_ids, star.
pub fn update_report(id: &str, patch: Value) -> Request {
    req_body(Method::Patch, data(&format!("/reports/{}", urlencode(id))), patch)
}

/// `POST /office/data/reports/:id/trash` and `/restore`.
pub fn trash_report(id: &str) -> Request {
    req(Method::Post, data(&format!("/reports/{}/trash", urlencode(id))))
}
pub fn restore_report(id: &str) -> Request {
    req(Method::Post, data(&format!("/reports/{}/restore", urlencode(id))))
}

/// `POST /office/data/reports/:id/pages` - add a page.
pub fn create_page(report_id: &str, body: Value) -> Request {
    req_body(Method::Post, data(&format!("/reports/{}/pages", urlencode(report_id))), body)
}
/// `PATCH /office/data/reports/:id/pages/:pid` - rename, resize, recolour, reposition.
pub fn update_page(report_id: &str, page_id: &str, patch: Value) -> Request {
    req_body(Method::Patch, data(&format!("/reports/{}/pages/{}", urlencode(report_id), urlencode(page_id))), patch)
}
/// `DELETE /office/data/reports/:id/pages/:pid`.
pub fn delete_page(report_id: &str, page_id: &str) -> Request {
    req(Method::Delete, data(&format!("/reports/{}/pages/{}", urlencode(report_id), urlencode(page_id))))
}

/// `POST /office/data/pages/:pid/widgets` - add a widget.
pub fn create_widget(page_id: &str, body: Value) -> Request {
    req_body(Method::Post, data(&format!("/pages/{}/widgets", urlencode(page_id))), body)
}
/// `PATCH /office/data/widgets/:id` - move, resize, config, z-index, type.
pub fn update_widget(id: &str, patch: Value) -> Request {
    req_body(Method::Patch, data(&format!("/widgets/{}", urlencode(id))), patch)
}
/// `DELETE /office/data/widgets/:id`.
pub fn delete_widget(id: &str) -> Request {
    req(Method::Delete, data(&format!("/widgets/{}", urlencode(id))))
}
/// `PATCH /office/data/widgets/batch` - bulk position moves `{ widgets: [{ id, x, y, width, height }] }`.
pub fn batch_update_widgets(widgets: Value) -> Request {
    req_body(Method::Patch, data("/widgets/batch"), serde_json::json!({ "widgets": widgets }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_built() {
        assert!(get_report("r1").path.ends_with("/office/data/reports/r1"));
        assert_eq!(execute(serde_json::json!({})).method, Method::Post);
        assert!(update_widget("w 1", serde_json::json!({ "x": 1 })).path.contains("/office/data/widgets/w%201"));
        assert!(create_page("r", serde_json::json!({})).path.ends_with("/reports/r/pages"));
    }
}
