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
    models::document_template::*,
    state::AppState,
};

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
) -> Result<Json<Value>> {
    let templates = sqlx::query_as::<_, Template>(
        r#"SELECT id, name, description, category, icon, content_json, is_builtin, created_by, created_at
           FROM document_templates
           WHERE is_builtin = TRUE OR created_by = $1
           ORDER BY is_builtin DESC, name ASC"#,
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "templates": templates })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<CreateTemplateDto>,
) -> Result<Json<Value>> {
    if dto.name.trim().is_empty() {
        return Err(OfficeError::Validation("Le nom du modèle est requis".into()));
    }

    let template = sqlx::query_as::<_, Template>(
        r#"INSERT INTO document_templates (name, description, category, icon, content_json, created_by)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, name, description, category, icon, content_json, is_builtin, created_by, created_at"#,
    )
    .bind(dto.name.trim())
    .bind(dto.description)
    .bind(dto.category.unwrap_or_else(|| "general".to_string()))
    .bind(dto.icon)
    .bind(dto.content_json)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "template": template })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "DELETE FROM document_templates WHERE id = $1 AND created_by = $2 AND is_builtin = FALSE",
    )
    .bind(id)
    .bind(user.id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(OfficeError::NotFound(format!("Modèle {id}")));
    }
    Ok(Json(json!({ "ok": true })))
}
