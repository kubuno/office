use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    models::font::*,
    state::AppState,
};

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
) -> Result<Json<serde_json::Value>> {
    let fonts = sqlx::query_as::<_, UserFont>(
        "SELECT id, user_id, name, css_family, source, import_url, created_at
         FROM fonts
         WHERE user_id = $1
         ORDER BY name",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "fonts": fonts })))
}

pub async fn add(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<AddFontDto>,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
    let name       = dto.name.trim();
    let css_family = dto.css_family.trim();
    let import_url = dto.import_url.trim();
    let source     = dto.source.as_deref().unwrap_or("google");

    if name.is_empty() || css_family.is_empty() || import_url.is_empty() {
        return Err(OfficeError::Validation("Tous les champs sont requis".into()));
    }
    if !["google", "url"].contains(&source) {
        return Err(OfficeError::Validation("Source invalide".into()));
    }

    let font = sqlx::query_as::<_, UserFont>(
        "INSERT INTO fonts (user_id, name, css_family, source, import_url)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (user_id, css_family) DO UPDATE
             SET name       = EXCLUDED.name,
                 import_url = EXCLUDED.import_url,
                 source     = EXCLUDED.source
         RETURNING id, user_id, name, css_family, source, import_url, created_at",
    )
    .bind(user.id)
    .bind(name)
    .bind(css_family)
    .bind(source)
    .bind(import_url)
    .fetch_one(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "font": font }))))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let result = sqlx::query("DELETE FROM fonts WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(OfficeError::NotFound("Police introuvable".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}
