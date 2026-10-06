//! Partage utilisateur-à-utilisateur des tableurs (même modèle que
//! `document_collaborators`). La recherche de destinataires réutilise le endpoint
//! générique `/office/recipients` (cf. `document_collaborators::search_recipients`).

use axum::{extract::{Path, State}, Extension, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    handlers::document_collaborators::{Collaborator, RecipientHit},
    middleware::OfficeUser,
    state::AppState,
};

const PERMISSIONS: [&str; 3] = ["view", "comment", "edit"];

#[derive(Debug, Deserialize)]
pub struct AddDto { pub user_id: Uuid, pub permission: Option<String> }

#[derive(Debug, Deserialize)]
pub struct UpdateDto { pub permission: String }

async fn is_owner(state: &AppState, ss_id: Uuid, user_id: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM spreadsheets WHERE id = $1 AND owner_id = $2)",
    ).bind(ss_id).bind(user_id).fetch_one(&state.db).await?)
}

/// `GET /office/spreadsheets/:id/collaborators` — owner ou collaborateur.
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(ss_id): Path<Uuid>,
) -> Result<Json<Value>> {
    let has_access: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
               SELECT 1 FROM spreadsheets WHERE id = $1 AND owner_id = $2
               UNION
               SELECT 1 FROM spreadsheet_collaborators WHERE spreadsheet_id = $1 AND user_id = $2
           )"#,
    ).bind(ss_id).bind(user.id).fetch_one(&state.db).await?;
    if !has_access { return Err(OfficeError::NotFound(format!("Tableur {ss_id}"))); }

    let owner = sqlx::query_as::<_, RecipientHit>(
        r#"SELECT u.id, u.display_name, u.email::text AS email, u.avatar_url
           FROM spreadsheets s JOIN core.users u ON u.id = s.owner_id WHERE s.id = $1"#,
    ).bind(ss_id).fetch_optional(&state.db).await?;

    let collaborators = sqlx::query_as::<_, Collaborator>(
        r#"SELECT c.user_id, c.permission, u.display_name, u.email::text AS email, u.avatar_url
           FROM spreadsheet_collaborators c JOIN core.users u ON u.id = c.user_id
           WHERE c.spreadsheet_id = $1 ORDER BY u.display_name NULLS LAST, u.email"#,
    ).bind(ss_id).fetch_all(&state.db).await?;

    Ok(Json(json!({ "owner": owner, "collaborators": collaborators })))
}

/// `POST /office/spreadsheets/:id/collaborators` — owner uniquement.
pub async fn add(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(ss_id): Path<Uuid>,
    Json(dto): Json<AddDto>,
) -> Result<Json<Value>> {
    if !is_owner(&state, ss_id, user.id).await? { return Err(OfficeError::Forbidden); }
    let permission = dto.permission.unwrap_or_else(|| "edit".to_string());
    if !PERMISSIONS.contains(&permission.as_str()) {
        return Err(OfficeError::Validation(format!("Permission invalide : {permission}")));
    }
    if dto.user_id == user.id { return Err(OfficeError::Validation("Le propriétaire a déjà accès".into())); }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM core.users WHERE id = $1 AND is_active = TRUE)",
    ).bind(dto.user_id).fetch_one(&state.db).await?;
    if !exists { return Err(OfficeError::NotFound("Utilisateur introuvable".into())); }

    sqlx::query(
        r#"INSERT INTO spreadsheet_collaborators (spreadsheet_id, user_id, permission)
           VALUES ($1, $2, $3)
           ON CONFLICT (spreadsheet_id, user_id) DO UPDATE SET permission = EXCLUDED.permission"#,
    ).bind(ss_id).bind(dto.user_id).bind(&permission).execute(&state.db).await?;

    Ok(Json(json!({ "ok": true, "user_id": dto.user_id, "permission": permission })))
}

/// `PATCH /office/spreadsheets/:id/collaborators/:user_id` — owner uniquement.
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((ss_id, target_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateDto>,
) -> Result<Json<Value>> {
    if !is_owner(&state, ss_id, user.id).await? { return Err(OfficeError::Forbidden); }
    if !PERMISSIONS.contains(&dto.permission.as_str()) {
        return Err(OfficeError::Validation(format!("Permission invalide : {}", dto.permission)));
    }
    let rows = sqlx::query(
        "UPDATE spreadsheet_collaborators SET permission = $3 WHERE spreadsheet_id = $1 AND user_id = $2",
    ).bind(ss_id).bind(target_id).bind(&dto.permission).execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Collaborateur introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}

/// `DELETE /office/spreadsheets/:id/collaborators/:user_id` — owner ou soi-même.
pub async fn remove(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((ss_id, target_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    if target_id != user.id && !is_owner(&state, ss_id, user.id).await? {
        return Err(OfficeError::Forbidden);
    }
    sqlx::query("DELETE FROM spreadsheet_collaborators WHERE spreadsheet_id = $1 AND user_id = $2")
        .bind(ss_id).bind(target_id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}
