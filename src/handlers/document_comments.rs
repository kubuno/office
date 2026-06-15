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
    models::document_comment::*,
    state::AppState,
};

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(doc_id): Path<Uuid>,
) -> Result<Json<Value>> {
    let exists: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
            SELECT 1 FROM documents
            WHERE id = $1 AND (owner_id = $2 OR EXISTS (
                SELECT 1 FROM document_collaborators WHERE document_id = $1 AND user_id = $2
            ))
        )"#,
    )
    .bind(doc_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    if !exists {
        return Err(OfficeError::NotFound(format!("Document {doc_id}")));
    }

    let comments = sqlx::query_as::<_, Comment>(
        r#"SELECT id, document_id, author_id, parent_id, content, is_resolved, created_at, updated_at
           FROM document_comments
           WHERE document_id = $1
           ORDER BY created_at ASC"#,
    )
    .bind(doc_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "comments": comments })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(doc_id): Path<Uuid>,
    Json(dto): Json<CreateCommentDto>,
) -> Result<Json<Value>> {
    if dto.content.trim().is_empty() {
        return Err(OfficeError::Validation("Le contenu du commentaire est requis".into()));
    }

    let comment = sqlx::query_as::<_, Comment>(
        r#"INSERT INTO document_comments (document_id, author_id, parent_id, content)
           VALUES ($1, $2, $3, $4)
           RETURNING id, document_id, author_id, parent_id, content, is_resolved, created_at, updated_at"#,
    )
    .bind(doc_id)
    .bind(user.id)
    .bind(dto.parent_id)
    .bind(dto.content.trim())
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "comment": comment })))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((_doc_id, comment_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateCommentDto>,
) -> Result<Json<Value>> {
    if dto.content.trim().is_empty() {
        return Err(OfficeError::Validation("Le contenu est requis".into()));
    }

    let updated = sqlx::query_as::<_, Comment>(
        r#"UPDATE document_comments SET content = $2
           WHERE id = $1 AND author_id = $3
           RETURNING id, document_id, author_id, parent_id, content, is_resolved, created_at, updated_at"#,
    )
    .bind(comment_id)
    .bind(dto.content.trim())
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Commentaire {comment_id}")))?;

    Ok(Json(json!({ "comment": updated })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((_doc_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "DELETE FROM document_comments WHERE id = $1 AND author_id = $2",
    )
    .bind(comment_id)
    .bind(user.id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(OfficeError::NotFound(format!("Commentaire {comment_id}")));
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn resolve(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((doc_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    let is_owner: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM documents WHERE id = $1 AND owner_id = $2)",
    )
    .bind(doc_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    if !is_owner {
        return Err(OfficeError::Forbidden);
    }

    sqlx::query(
        "UPDATE document_comments SET is_resolved = NOT is_resolved WHERE id = $1",
    )
    .bind(comment_id)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({ "ok": true })))
}
