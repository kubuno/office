//! Talking to the server. Data is server-coupled, so the client is thin: [`routes`] builds the Data REST
//! requests of the shared transport, and [`connect_account`] opens a transport bound to the account the
//! Kubuno shell shows (its token broker). There is no outbox worker or crash journal like Documents - each
//! edit is one REST call (`widgets.*`, `pages.*`, `reports.update`), issued straight away.

pub mod direct;
pub mod routes;

use kubuno_office_documents_common::api::client::{Request, Response, SyncTransport, Transport};

/// A blocking transport to the current account, or an error when no one is signed in.
pub fn connect_account() -> Result<Box<dyn Transport>, String> {
    let account = kubuno_desktop_sync::current_account()
        .map_err(|e| format!("{e}"))?
        .ok_or_else(|| "no account signed in: sign in to Kubuno Desktop".to_string())?;
    Ok(Box::new(SyncTransport::for_account(account)))
}

/// A transport to the server: the dev core directly when `KUBUNO_DEV_BASE` is set (development `--live`
/// without a signed-in shell), otherwise the shell's token broker. Credentials are read from the environment.
pub fn connect() -> Result<Box<dyn Transport>, String> {
    if let Ok(base) = std::env::var("KUBUNO_DEV_BASE") {
        let login = std::env::var("KUBUNO_DEV_LOGIN").map_err(|_| "KUBUNO_DEV_LOGIN not set".to_string())?;
        let password = std::env::var("KUBUNO_DEV_PASSWORD").map_err(|_| "KUBUNO_DEV_PASSWORD not set".to_string())?;
        return direct::DirectTransport::login(&base, &login, &password).map(|t| Box::new(t) as Box<dyn Transport>);
    }
    connect_account()
}

/// Sends a request, returning the parsed JSON body on a 2xx, or a short error otherwise.
pub fn send(transport: &dyn Transport, request: &Request) -> Result<serde_json::Value, String> {
    let resp: Response = transport.send(request).map_err(|e| format!("{e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status));
    }
    resp.parse().map_err(|e| format!("{e}"))
}
