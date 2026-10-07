//! The project routes of the office module (`server/src/router/mod.rs:107-222`), as requests of the shared
//! transport (`kubuno_office_desktop_common::api::client`).
//!
//! The editing calls come from the core (`Change::request` → [`HttpCall`]); the reads are built here. Pure
//! functions, so paths and bodies are testable without a network.

use kubuno_office_desktop_common::api::client::{urlencode, Method, Request, CORE_CONFIG_PATH, OFFICE_PREFIX};
use kubuno_office_projects_core::change::HttpCall;

/// The `User-Agent` of this app.
pub fn user_agent() -> String {
    format!("Kubuno-Projects/{}", env!("CARGO_PKG_VERSION"))
}

fn req(method: Method, path: String) -> Request {
    Request { method, path, headers: vec![("User-Agent".to_string(), user_agent())], body: None }
}

/// `/api/v1/office/projects/<id><suffix>`.
pub fn path(id: &str, suffix: &str) -> String {
    format!("{OFFICE_PREFIX}/projects/{}{suffix}", urlencode(id))
}

/// A call of the editor's outbox. `None` for a verb the transport does not speak.
pub fn call(c: &HttpCall) -> Option<Request> {
    let method = match c.method {
        "GET" => Method::Get,
        "POST" => Method::Post,
        "PATCH" => Method::Patch,
        "DELETE" => Method::Delete,
        _ => return None,
    };
    let mut r = req(method, format!("{OFFICE_PREFIX}{}", c.path));
    r.body = c.body.clone();
    Some(r)
}

/// `GET /projects/:id` — `{ project, tasks, dependencies, resources, assignments }`.
pub fn get_project(id: &str) -> Request {
    req(Method::Get, path(id, ""))
}

/// `GET /projects/:id/calendars`.
pub fn calendars(id: &str) -> Request {
    req(Method::Get, path(id, "/calendars"))
}

/// `GET /projects/:id/settings`.
pub fn settings(id: &str) -> Request {
    req(Method::Get, path(id, "/settings"))
}

/// `GET /projects/:id/baselines`.
pub fn baselines(id: &str) -> Request {
    req(Method::Get, path(id, "/baselines"))
}

/// `GET /projects/:id/versions`.
pub fn versions(id: &str) -> Request {
    req(Method::Get, path(id, "/versions"))
}

/// `GET /projects/:id/org-members` — the directory members a resource can be (the resources panel).
pub fn org_members(id: &str) -> Request {
    req(Method::Get, path(id, "/org-members"))
}

/// `GET /projects/:id/costs` — `{ config: { currency, default_hourly_rate, … }, … }` (the cost per resource).
pub fn costs(id: &str) -> Request {
    req(Method::Get, path(id, "/costs"))
}

/// `GET /projects/:id/resources/:rid/time-off` — `{ time_off: [...] }`.
pub fn time_off(id: &str, resource: &str) -> Request {
    req(Method::Get, path(id, &format!("/resources/{}/time-off", urlencode(resource))))
}

/// `POST /projects/:id/duplicate` — `{ id }` of the copy.
pub fn duplicate(id: &str) -> Request {
    req(Method::Post, path(id, "/duplicate"))
}

/// `POST /projects` `{ title }` — a new management project.
pub fn create(title: &str) -> Request {
    let mut r = req(Method::Post, format!("{OFFICE_PREFIX}/projects"));
    r.body = Some(serde_json::json!({ "title": title }));
    r
}

/// `POST /projects/:id/trash`.
pub fn trash(id: &str) -> Request {
    req(Method::Post, path(id, "/trash"))
}

/// `GET /api/v1/config` — the instance settings (`office.project_default_include_weekends`).
pub fn core_config() -> Request {
    req(Method::Get, CORE_CONFIG_PATH.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kubuno_office_projects_core::change::Change;

    #[test]
    fn calls_and_reads() {
        let c = Change::DeleteTask { id: "t1".into() }.request("p 1");
        let r = call(&c).expect("request");
        assert_eq!(r.method, Method::Delete);
        assert_eq!(r.path, "/api/v1/office/projects/p%201/tasks/t1");
        assert_eq!(get_project("x").path, "/api/v1/office/projects/x");
        assert!(get_project("x").header_value("user-agent").is_some_and(|u| u.starts_with("Kubuno-Projects/")));
        assert!(call(&HttpCall { method: "PUT", path: "/x".into(), body: None }).is_none());
    }
}
