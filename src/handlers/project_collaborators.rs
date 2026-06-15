//! Partage utilisateur-à-utilisateur des projets (même modèle que
//! `presentation_collaborators`). Recherche de destinataires via `/office/recipients`.

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

async fn is_owner(state: &AppState, project_id: Uuid, user_id: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1 AND owner_id = $2)",
    ).bind(project_id).bind(user_id).fetch_one(&state.db).await?)
}

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    let has_access: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
               SELECT 1 FROM projects WHERE id = $1 AND owner_id = $2
               UNION
               SELECT 1 FROM project_collaborators WHERE project_id = $1 AND user_id = $2
           )"#,
    ).bind(project_id).bind(user.id).fetch_one(&state.db).await?;
    if !has_access { return Err(OfficeError::NotFound(format!("Projet {project_id}"))); }

    let owner = sqlx::query_as::<_, RecipientHit>(
        r#"SELECT u.id, u.display_name, u.email::text AS email, u.avatar_url
           FROM projects p JOIN core.users u ON u.id = p.owner_id WHERE p.id = $1"#,
    ).bind(project_id).fetch_optional(&state.db).await?;

    let collaborators = sqlx::query_as::<_, Collaborator>(
        r#"SELECT c.user_id, c.permission, u.display_name, u.email::text AS email, u.avatar_url
           FROM project_collaborators c JOIN core.users u ON u.id = c.user_id
           WHERE c.project_id = $1 ORDER BY u.display_name NULLS LAST, u.email"#,
    ).bind(project_id).fetch_all(&state.db).await?;

    Ok(Json(json!({ "owner": owner, "collaborators": collaborators })))
}

pub async fn add(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<AddDto>,
) -> Result<Json<Value>> {
    if !is_owner(&state, project_id, user.id).await? { return Err(OfficeError::Forbidden); }
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
        r#"INSERT INTO project_collaborators (project_id, user_id, permission)
           VALUES ($1, $2, $3)
           ON CONFLICT (project_id, user_id) DO UPDATE SET permission = EXCLUDED.permission"#,
    ).bind(project_id).bind(dto.user_id).bind(&permission).execute(&state.db).await?;

    Ok(Json(json!({ "ok": true, "user_id": dto.user_id, "permission": permission })))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, target_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateDto>,
) -> Result<Json<Value>> {
    if !is_owner(&state, project_id, user.id).await? { return Err(OfficeError::Forbidden); }
    if !PERMISSIONS.contains(&dto.permission.as_str()) {
        return Err(OfficeError::Validation(format!("Permission invalide : {}", dto.permission)));
    }
    let rows = sqlx::query(
        "UPDATE project_collaborators SET permission = $3 WHERE project_id = $1 AND user_id = $2",
    ).bind(project_id).bind(target_id).bind(&dto.permission).execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Collaborateur introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, target_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    if target_id != user.id && !is_owner(&state, project_id, user.id).await? {
        return Err(OfficeError::Forbidden);
    }
    sqlx::query("DELETE FROM project_collaborators WHERE project_id = $1 AND user_id = $2")
        .bind(project_id).bind(target_id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}
