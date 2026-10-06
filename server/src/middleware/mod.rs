use axum::{extract::{Request, State}, middleware::Next, response::Response};
use uuid::Uuid;
use crate::{errors::OfficeError, state::AppState};

#[derive(Debug, Clone)]
pub struct OfficeUser {
    pub id:    Uuid,
    pub role:  String,
    pub email: String,
}

pub type OfficeUserExt = axum::Extension<OfficeUser>;

/// Comparaison à temps constant pour éviter une fuite par timing sur le secret.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> std::result::Result<Response, OfficeError> {
    // Défense en profondeur : exiger l'`X-Internal-Secret` injecté par le proxy
    // du core lorsqu'un secret est configuré, afin qu'un accès direct au module
    // (hors proxy) ne puisse pas usurper d'identité via les en-têtes `X-Kubuno-*`.
    let expected = state.settings.core.internal_secret.as_bytes();
    if !expected.is_empty() {
        let provided = req
            .headers()
            .get("x-internal-secret")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !constant_time_eq(provided.as_bytes(), expected) {
            return Err(OfficeError::Unauthorized);
        }
    }

    let user_id = req
        .headers()
        .get("x-kubuno-user-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or(OfficeError::Unauthorized)?;

    let role = req
        .headers()
        .get("x-kubuno-user-role")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("user")
        .to_string();

    let email = req
        .headers()
        .get("x-kubuno-user-email")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    req.extensions_mut().insert(OfficeUser { id: user_id, role, email });
    Ok(next.run(req).await)
}
