//! Talking to the server. A Maths formula's content is a single `latex` string, so the client is thin:
//! [`routes`] builds the Maths REST requests of the shared transport, and [`connect_account`] opens a
//! transport bound to the account the Kubuno shell shows (its token broker). Saving is one `PATCH` of the
//! `latex` (there is no digest/editing-session model like Documents — see vskubuno `docs/MATHS-DESKTOP.md`).

pub mod routes;

use kubuno_office_desktop_common::api::client::{Request, Response, SyncTransport, Transport};

/// A blocking transport to the current account, or an error when no one is signed in.
pub fn connect_account() -> Result<Box<dyn Transport>, String> {
    let account = kubuno_desktop_sync::current_account()
        .map_err(|e| format!("{e}"))?
        .ok_or_else(|| "no account signed in: sign in to Kubuno Desktop".to_string())?;
    Ok(Box::new(SyncTransport::for_account(account)))
}

/// Sends a request, returning the parsed JSON body on a 2xx, or a short error otherwise.
pub fn send(transport: &dyn Transport, request: &Request) -> Result<serde_json::Value, String> {
    let resp: Response = transport.send(request).map_err(|e| format!("{e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status));
    }
    resp.parse().map_err(|e| format!("{e}"))
}
