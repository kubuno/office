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

    // 3) ACL par type d'entité. Les documents ont un partage user-à-user
    //    (collaborateurs) ; les autres restent réservés au propriétaire.
    let allowed = match entity_type {
        "document" => {
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS(
                       SELECT 1 FROM documents WHERE id = $1 AND owner_id = $2
                       UNION
                       SELECT 1 FROM document_collaborators WHERE document_id = $1 AND user_id = $2
                   )"#,
            )
            .bind(entity_id).bind(dto.user_id).fetch_one(&state.db).await?
        }
        "spreadsheet"  => {
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS(
                       SELECT 1 FROM spreadsheets WHERE id = $1 AND owner_id = $2
                       UNION
                       SELECT 1 FROM spreadsheet_collaborators WHERE spreadsheet_id = $1 AND user_id = $2
                   )"#,
            )
            .bind(entity_id).bind(dto.user_id).fetch_one(&state.db).await?
        }
        "presentation" => {
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS(
                       SELECT 1 FROM presentations WHERE id = $1 AND owner_id = $2
                       UNION
                       SELECT 1 FROM presentation_collaborators WHERE presentation_id = $1 AND user_id = $2
                   )"#,
            )
            .bind(entity_id).bind(dto.user_id).fetch_one(&state.db).await?
        }
        "diagram"      => owner_check(&state, "diagrams",      entity_id, dto.user_id).await?,
        "project"      => {
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS(
                       SELECT 1 FROM projects WHERE id = $1 AND owner_id = $2
                       UNION
                       SELECT 1 FROM project_collaborators WHERE project_id = $1 AND user_id = $2
                   )"#,
            )
            .bind(entity_id).bind(dto.user_id).fetch_one(&state.db).await?
        }
        _ => false,
    };

    if allowed {
        Ok(Json(json!({ "ok": true })))
    } else {
        Err(OfficeError::Forbidden)
    }
}

async fn owner_check(state: &AppState, table: &str, id: Uuid, user_id: Uuid) -> Result<bool> {
    // `table` est une constante interne (jamais une entrée utilisateur) → pas d'injection.
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = $1 AND owner_id = $2)");
    Ok(sqlx::query_scalar::<_, bool>(&sql).bind(id).bind(user_id).fetch_one(&state.db).await?)
}
