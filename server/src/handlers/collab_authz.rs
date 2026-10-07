//! Autorisation des rooms de collaboration temps réel (appelé par le CORE).
//!
//! Le service collab du core est générique et ne connaît pas les ACL métier. Avant
//! d'admettre un utilisateur dans une room `office-<type>:<uuid>`, il interroge ce
//! endpoint interne (`POST /internal/collab/authorize`, protégé par X-Internal-Secret).
//! On répond 200 (autorisé), 403 (refusé) ou 401 (secret invalide). Le core
//! n'interdit l'accès QUE sur un 403 explicite.

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    services::collab_epoch::{self as ce, CollabEntity},
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

    // 2) Parse `office-<type>:<uuid>[~<epoch>]`.
    let room = ce::parse_room(&dto.room)?;

    // 3) ACL par type d'entité — même règle que les WebSockets du module.
    if !may_access(&state, &room.entity_type, room.id, dto.user_id).await? {
        return Err(OfficeError::Forbidden);
    }

    // 4) A room of an abandoned epoch is closed for good: its state predates a
    //    REST replacement of the content, and a client rejoining it would write
    //    that state back. Entities without epochs never carry one.
    match CollabEntity::parse(&room.entity_type) {
        Some(entity) => {
            let current = ce::current(&state.db, entity, room.id).await?
                .ok_or(OfficeError::Forbidden)?;
            if !epoch_admits(room.epoch, current) {
                tracing::info!(room = %dto.room, current, "collab: join refused, stale epoch");
                return Err(OfficeError::Forbidden);
            }
        }
        None if room.epoch.is_some() => return Err(OfficeError::Forbidden),
        None => {}
    }
    Ok(Json(json!({ "ok": true })))
}

/// Does a room named with `room_epoch` (none = epoch 0) belong to the current epoch?
fn epoch_admits(room_epoch: Option<i64>, current: i64) -> bool {
    room_epoch.unwrap_or(0) == current
}

/// `GET /collab/:entity_type/:entity_id/epoch` — the current epoch and room of
/// an entity, for a web session that lost its socket and must find out whether
/// its room was abandoned in the meantime (without downloading the content).
pub async fn current_epoch(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((entity_type, entity_id)): Path<(String, Uuid)>,
) -> Result<Json<Value>> {
    let entity = CollabEntity::parse(&entity_type)
        .ok_or_else(|| OfficeError::Validation(format!("no collaboration epoch for {entity_type}")))?;
    if !may_access(&state, entity.as_str(), entity_id, user.id).await? {
        // Same answer as a missing entity: the route does not reveal what exists.
        return Err(OfficeError::NotFound(format!("{entity_type} {entity_id}")));
    }
    let epoch = ce::current(&state.db, entity, entity_id).await?
        .ok_or_else(|| OfficeError::NotFound(format!("{entity_type} {entity_id}")))?;
    Ok(Json(json!({ "collab_epoch": epoch, "room": ce::room_name(entity, entity_id, epoch) })))
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
    // A macro, not a closure: the three names are spelled out once per entity
    // below and folded in at compile time, so every arm of the match hands the
    // driver one `&'static str` and no run-time SQL text exists at all.
    macro_rules! shared {
        ($owner_table:literal, $link_table:literal, $link_column:literal) => {
            concat!(
                "SELECT EXISTS(
                     SELECT 1 FROM ", $owner_table, " WHERE id = $1 AND owner_id = $2
                     UNION
                     SELECT 1 FROM ", $link_table, " WHERE ", $link_column, " = $1 AND user_id = $2
                 )"
            )
        };
    }
    let sql: &'static str = match entity_type {
        "document"     => shared!("documents",     "document_collaborators",     "document_id"),
        "spreadsheet"  => shared!("spreadsheets",  "spreadsheet_collaborators",  "spreadsheet_id"),
        "presentation" => shared!("presentations", "presentation_collaborators", "presentation_id"),
        "project"      => shared!("projects",      "project_collaborators",      "project_id"),
        "whiteboard"   => shared!("office_wb.boards", "office_wb.board_collaborators", "board_id"),
        "diagram"      => "SELECT EXISTS(SELECT 1 FROM diagrams WHERE id = $1 AND owner_id = $2)",
        _              => return Ok(false),
    };
    Ok(sqlx::query_scalar::<_, bool>(sql)
        .bind(entity_id).bind(user_id)
        .fetch_one(&state.db).await?)
}

#[cfg(test)]
mod tests {
    use super::epoch_admits;

    #[test]
    fn only_the_current_epoch_is_admitted() {
        assert!(epoch_admits(None, 0));
        assert!(epoch_admits(Some(3), 3));
        assert!(!epoch_admits(None, 1), "the historical room is closed after a replacement");
        assert!(!epoch_admits(Some(2), 3));
        assert!(!epoch_admits(Some(4), 3));
    }
}
