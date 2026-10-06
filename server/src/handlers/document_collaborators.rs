//! Partage utilisateur-à-utilisateur des documents.
//!
//! Un propriétaire peut donner accès (`view`/`comment`/`edit`) à d'autres
//! utilisateurs Kubuno. Les ACL de lecture/écriture sont appliquées dans
//! `documents::get` / `documents::update` (owner OU collaborateur). Ici on gère
//! la liste des collaborateurs et la recherche de destinataires (`core.users`).

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    state::AppState,
};

const PERMISSIONS: [&str; 3] = ["view", "comment", "edit"];

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct RecipientHit {
    pub id:           Uuid,
    pub display_name: Option<String>,
    pub email:        String,
    pub avatar_url:   Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Collaborator {
    pub user_id:      Uuid,
    pub permission:   String,
    pub display_name: Option<String>,
    pub email:        String,
    pub avatar_url:   Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddCollaboratorDto {
    pub user_id:    Uuid,
    pub permission: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCollaboratorDto {
    pub permission: String,
}

/// Vrai si `user` est propriétaire du document.
async fn is_owner(state: &AppState, doc_id: Uuid, user_id: Uuid) -> Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM documents WHERE id = $1 AND owner_id = $2)",
    )
    .bind(doc_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await?;
    Ok(exists)
}

/// `GET /office/recipients?q=` — recherche d'utilisateurs avec qui partager.
pub async fn search_recipients(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Value>> {
    let query = q.q.unwrap_or_default();
    let query = query.trim();
    if query.is_empty() {
        return Ok(Json(json!({ "recipients": [] })));
    }
    let pattern = format!("%{query}%");
    let hits = sqlx::query_as::<_, RecipientHit>(
        r#"SELECT id, display_name, email::text AS email, avatar_url
           FROM core.users
           WHERE is_active = TRUE
             AND id <> $1
             AND (email::text ILIKE $2 OR username ILIKE $2 OR display_name ILIKE $2)
           ORDER BY display_name NULLS LAST, email
           LIMIT 20"#,
    )
    .bind(user.id)
    .bind(&pattern)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "recipients": hits })))
}

/// `GET /office/:doc_id/collaborators` — liste des collaborateurs (owner ou collaborateur).
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(doc_id): Path<Uuid>,
) -> Result<Json<Value>> {
    // Le demandeur doit avoir accès au document (owner OU collaborateur).
    let has_access: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
               SELECT 1 FROM documents WHERE id = $1 AND owner_id = $2
               UNION
               SELECT 1 FROM document_collaborators WHERE document_id = $1 AND user_id = $2
           )"#,
    )
    .bind(doc_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;
    if !has_access {
        return Err(OfficeError::NotFound(format!("Document {doc_id}")));
    }

    // Propriétaire (pour l'affichage).
    let owner = sqlx::query_as::<_, RecipientHit>(
        r#"SELECT u.id, u.display_name, u.email::text AS email, u.avatar_url
           FROM documents d JOIN core.users u ON u.id = d.owner_id
           WHERE d.id = $1"#,
    )
    .bind(doc_id)
    .fetch_optional(&state.db)
    .await?;

    let collaborators = sqlx::query_as::<_, Collaborator>(
        r#"SELECT c.user_id, c.permission,
                  u.display_name, u.email::text AS email, u.avatar_url
           FROM document_collaborators c
           JOIN core.users u ON u.id = c.user_id
           WHERE c.document_id = $1
           ORDER BY u.display_name NULLS LAST, u.email"#,
    )
    .bind(doc_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "owner": owner, "collaborators": collaborators })))
}

/// `POST /office/:doc_id/collaborators` — ajoute/maj un collaborateur (owner uniquement).
pub async fn add(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(doc_id): Path<Uuid>,
    Json(dto): Json<AddCollaboratorDto>,
) -> Result<Json<Value>> {
    if !is_owner(&state, doc_id, user.id).await? {
        return Err(OfficeError::Forbidden);
    }
    let permission = dto.permission.unwrap_or_else(|| "edit".to_string());
    if !PERMISSIONS.contains(&permission.as_str()) {
        return Err(OfficeError::Validation(format!("Permission invalide : {permission}")));
    }
    if dto.user_id == user.id {
        return Err(OfficeError::Validation("Le propriétaire a déjà accès".into()));
    }
    // Le destinataire doit exister et être actif.
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM core.users WHERE id = $1 AND is_active = TRUE)",
    )
    .bind(dto.user_id)
    .fetch_one(&state.db)
    .await?;
    if !exists {
        return Err(OfficeError::NotFound("Utilisateur introuvable".into()));
    }

    sqlx::query(
        r#"INSERT INTO document_collaborators (document_id, user_id, permission)
           VALUES ($1, $2, $3)
           ON CONFLICT (document_id, user_id) DO UPDATE SET permission = EXCLUDED.permission"#,
    )
    .bind(doc_id)
    .bind(dto.user_id)
    .bind(&permission)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({ "ok": true, "user_id": dto.user_id, "permission": permission })))
}

/// `PATCH /office/:doc_id/collaborators/:user_id` — change la permission (owner uniquement).
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((doc_id, target_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateCollaboratorDto>,
) -> Result<Json<Value>> {
    if !is_owner(&state, doc_id, user.id).await? {
        return Err(OfficeError::Forbidden);
    }
    if !PERMISSIONS.contains(&dto.permission.as_str()) {
        return Err(OfficeError::Validation(format!("Permission invalide : {}", dto.permission)));
    }
    let rows = sqlx::query(
        "UPDATE document_collaborators SET permission = $3 WHERE document_id = $1 AND user_id = $2",
    )
    .bind(doc_id)
    .bind(target_id)
    .bind(&dto.permission)
    .execute(&state.db)
    .await?
    .rows_affected();
    if rows == 0 {
        return Err(OfficeError::NotFound("Collaborateur introuvable".into()));
    }
    Ok(Json(json!({ "ok": true })))
}

/// `DELETE /office/:doc_id/collaborators/:user_id` — retire un collaborateur.
/// Autorisé au propriétaire, ou au collaborateur lui-même (quitter le partage).
pub async fn remove(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((doc_id, target_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    if target_id != user.id && !is_owner(&state, doc_id, user.id).await? {
        return Err(OfficeError::Forbidden);
    }
    sqlx::query(
        "DELETE FROM document_collaborators WHERE document_id = $1 AND user_id = $2",
    )
    .bind(doc_id)
    .bind(target_id)
    .execute(&state.db)
    .await?;
    Ok(Json(json!({ "ok": true })))
}
