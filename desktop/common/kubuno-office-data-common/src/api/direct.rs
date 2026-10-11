//! A direct blocking HTTP transport for **development** `--live` runs against a dev core, used when the
//! Kubuno Desktop shell's token broker is not signed in (so `SyncTransport` has no account). It logs in with
//! credentials read from the environment and sends every request with a bearer token. The production path is
//! always the broker (`crate::api::connect_account`); this is only reached when `KUBUNO_DEV_BASE` is set.
//!
//! Environment: `KUBUNO_DEV_BASE` (e.g. `http://127.0.0.1:18900`), `KUBUNO_DEV_LOGIN`, `KUBUNO_DEV_PASSWORD`.
//! Credentials are never hardcoded.

use kubuno_office_desktop_common::api::client::{ApiResult, Error, Method, Request, Response, Transport};
use serde_json::json;

/// A blocking transport that holds a bearer token for a dev core.
pub struct DirectTransport {
    client: reqwest::blocking::Client,
    base: String,
    token: String,
}

impl DirectTransport {
    /// Log in to `base` and keep the access token (`POST /api/v1/auth/login`).
    pub fn login(base: &str, login: &str, password: &str) -> Result<Self, String> {
        let base = base.trim_end_matches('/').to_string();
        let client = reqwest::blocking::Client::builder()
            .user_agent("Kubuno-Data-dev")
            .build()
            .map_err(|e| format!("http client: {e}"))?;
        let resp = client
            .post(format!("{base}/api/v1/auth/login"))
            .json(&json!({ "login": login, "password": password, "client_type": "native", "device_name": "kubuno-data-dev" }))
            .send()
            .map_err(|e| format!("login: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("login HTTP {}", resp.status().as_u16()));
        }
        let body: serde_json::Value = resp.json().map_err(|e| format!("login body: {e}"))?;
        let token = body
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "login: no access_token".to_string())?
            .to_string();
        Ok(Self { client, base, token })
    }
}

impl Transport for DirectTransport {
    fn send(&self, request: &Request) -> ApiResult<Response> {
        let url = format!("{}{}", self.base, request.path);
        let mut rb = match request.method {
            Method::Get => self.client.get(&url),
            Method::Post => self.client.post(&url),
            Method::Patch => self.client.patch(&url),
            Method::Delete => self.client.delete(&url),
        };
        rb = rb.bearer_auth(&self.token);
        for (k, v) in &request.headers {
            rb = rb.header(k, v);
        }
        rb = match (&request.body, request.method) {
            (Some(body), _) => rb.json(body),
            // A POST without a body still sends `{}` (the handlers run through axum extractors).
            (None, Method::Post) => rb.json(&json!({})),
            (None, _) => rb,
        };
        let resp = rb.send().map_err(|e| Error::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        let body = resp.bytes().map_err(|e| Error::Transport(e.to_string()))?.to_vec();
        Ok(Response { status, body })
    }
}
