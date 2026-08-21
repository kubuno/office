//! Autorisation des rooms de collaboration temps réel (appelé par le CORE).
//!
//! Le service collab du core est générique et ne connaît pas les ACL métier. Avant
//! d'admettre un utilisateur dans une room `office-<type>:<uuid>`, il interroge ce
//! endpoint interne (`POST /internal/collab/authorize`, protégé par X-Internal-Secret).
//! On répond 200 (autorisé), 403 (refusé) ou 401 (secret invalide). Le core
//! n'interdit l'accès QUE sur un 403 explicite.

use axum::{extract::State, http::HeaderMap, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct AuthorizeDto {
    pub room:    String,
    pub user_id: Uuid,
}

/// Vérifie l'accès d'un utilisateur à l'entité désignée par une room office.
/// `room` = `office-<entity_type>:<uuid>` (ex. `office-document:…`).
pub async fn authorize(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(dto): Json<AuthorizeDto>,
) -> Result<Json<Value>> {
    // 1) Secret interne obligatoire (sinon le core retombe en fail-open avec un warn).
    let secret = headers.get("x-internal-secret").and_then(|v| v.to_str().ok()).unwrap_or("");
    let expected = state.settings.core.internal_secret.as_str();
    if expected.is_empty() || secret != expected {
        return Err(OfficeError::Unauthorized);
    }

    // 2) Parse `office-<type>:<uuid>`.
    let rest = dto.room.strip_prefix("office-").or_else(|| dto.room.strip_prefix("office:"))
        .unwrap_or(&dto.room);
    let (entity_type, id_str) = rest.split_once(':')
        .ok_or_else(|| OfficeError::Validation(format!("room invalide : {}", dto.room)))?;
    let entity_id = Uuid::parse_str(id_str)
        .map_err(|_| OfficeError::Validation(format!("uuid invalide dans : {}", dto.room)))?;

    // 3) ACL par type d'entité — même règle que les WebSockets du module.
    if may_access(&state, entity_type, entity_id, dto.user_id).await? {
        Ok(Json(json!({ "ok": true })))
    } else {
        Err(OfficeError::Forbidden)
    }
}

/// [`may_access`] reduced to a yes/no for the WebSocket handlers, which have no way
/// to report an error mid-handshake. A database failure answers "no" and is logged:
/// a check that cannot run must refuse, never wave the caller through.
pub async fn authorized(state: &AppState, entity_type: &str, entity_id: Uuid, user_id: Uuid) -> bool {
    match may_access(state, entity_type, entity_id, user_id).await {
        Ok(ok) => ok,
        Err(e) => {
            tracing::error!(error = %e, %entity_id, entity_type, "collab: contrôle d'accès impossible — connexion refusée");
            false
        }
    }
}

/// May this person reach that entity? The single answer shared by the core's room
/// authorization and by the module's own WebSocket handlers, which used to accept
/// the upgrade without asking anything at all.
///
/// An unknown entity type answers `false` — a new editor must opt in here rather
/// than inherit access by accident.
pub async fn may_access(
    state:       &AppState,
    entity_type: &str,
    entity_id:   Uuid,
    user_id:     Uuid,
) -> Result<bool> {
    let shared = |owner_table: &'static str, link_table: &'static str, link_column: &'static str| {
        // All three names are internal constants, never user input — no injection.
        format!(
            "SELECT EXISTS(
                 SELECT 1 FROM {owner_table} WHERE id = $1 AND owner_id = $2
                 UNION
                 SELECT 1 FROM {link_table} WHERE {link_column} = $1 AND user_id = $2
             )"
        )
    };
    let sql = match entity_type {
        "document"     => shared("documents",     "document_collaborators",     "document_id"),
        "spreadsheet"  => shared("spreadsheets",  "spreadsheet_collaborators",  "spreadsheet_id"),
        "presentation" => shared("presentations", "presentation_collaborators", "presentation_id"),
        "project"      => shared("projects",      "project_collaborators",      "project_id"),
        "whiteboard"   => shared("office_wb.boards", "office_wb.board_collaborators", "board_id"),
        "diagram"      => "SELECT EXISTS(SELECT 1 FROM diagrams WHERE id = $1 AND owner_id = $2)".to_string(),
        _              => return Ok(false),
    };
    Ok(sqlx::query_scalar::<_, bool>(&sql)
        .bind(entity_id).bind(user_id)
        .fetch_one(&state.db).await?)
}
