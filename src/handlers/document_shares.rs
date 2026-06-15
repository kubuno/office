use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use axum::Extension;
use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    models::{document::Document, document_share::*},
    state::AppState,
};

fn generate_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..32).map(|_| format!("{:02x}", rng.gen::<u8>())).collect()
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(doc_id): Path<Uuid>,
    Json(dto): Json<CreateShareDto>,
) -> Result<Json<Value>> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM documents WHERE id = $1 AND owner_id = $2)",
    )
    .bind(doc_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    if !exists {
        return Err(OfficeError::NotFound(format!("Document {doc_id}")));
    }

    let permission = dto.permission.unwrap_or_else(|| "view".to_string());
    if !["view", "comment", "edit"].contains(&permission.as_str()) {
        return Err(OfficeError::Validation("permission invalide".into()));
    }

    let token = generate_token();

    let share = sqlx::query_as::<_, Share>(
        r#"INSERT INTO document_shares (document_id, token, permission, expires_at, created_by)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, document_id, token, permission, expires_at, created_by, created_at, revoked_at"#,
    )
    .bind(doc_id)
    .bind(token)
    .bind(permission)
    .bind(dto.expires_at)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "share": share })))
}

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(doc_id): Path<Uuid>,
) -> Result<Json<Value>> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM documents WHERE id = $1 AND owner_id = $2)",
    )
    .bind(doc_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    if !exists {
        return Err(OfficeError::NotFound(format!("Document {doc_id}")));
    }

    let shares = sqlx::query_as::<_, Share>(
        r#"SELECT id, document_id, token, permission, expires_at, created_by, created_at, revoked_at
           FROM document_shares
           WHERE document_id = $1 AND revoked_at IS NULL
           ORDER BY created_at DESC"#,
    )
    .bind(doc_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "shares": shares })))
}

pub async fn revoke(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((_doc_id, share_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "UPDATE document_shares SET revoked_at = NOW()
         WHERE id = $1 AND created_by = $2 AND revoked_at IS NULL",
    )
    .bind(share_id)
    .bind(user.id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(OfficeError::NotFound(format!("Partage {share_id}")));
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn get_public(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<Value>> {
    let share = sqlx::query_as::<_, Share>(
        r#"SELECT id, document_id, token, permission, expires_at, created_by, created_at, revoked_at
           FROM document_shares
           WHERE token = $1 AND revoked_at IS NULL
             AND (expires_at IS NULL OR expires_at > NOW())"#,
    )
    .bind(token)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound("Lien de partage introuvable ou expiré".into()))?;

    let doc = sqlx::query_as::<_, Document>(
        r#"SELECT id, owner_id, title, icon, cover_url, content_json, content_text,
                  word_count, is_starred, is_trashed, trashed_at, parent_id,
                  position, last_editor_id, created_at, updated_at
           FROM documents WHERE id = $1 AND is_trashed = FALSE"#,
    )
    .bind(share.document_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound("Document introuvable".into()))?;

    Ok(Json(json!({
        "document": doc,
        "permission": share.permission,
    })))
}
