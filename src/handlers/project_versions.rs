//! Project versions (release milestones): named containers that group tasks for a
//! roadmap. CRUD; tasks are attached via their `version_id` (see update_task).
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    handlers::project_authz::{require_permission, Level},
    middleware::OfficeUser,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct CreateVersionDto {
    pub name:        Option<String>,
    pub description: Option<String>,
    pub start_date:  Option<NaiveDate>,
    pub due_date:    Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateVersionDto {
    pub name:        Option<String>,
    pub description: Option<String>,
    pub start_date:  Option<NaiveDate>,
    pub due_date:    Option<NaiveDate>,
    pub status:      Option<String>,
}

const COLS: &str = "id, project_id, name, description, start_date, due_date, status, position, created_at";

/// GET /projects/:id/versions
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let rows: Vec<Version> = sqlx::query_as::<_, Version>(
        &format!("SELECT {COLS} FROM project_versions WHERE project_id = $1 ORDER BY position ASC, name ASC"),
    ).bind(project_id).fetch_all(&state.db).await?;
    Ok(Json(json!({ "versions": rows })))
}

/// POST /projects/:id/versions
pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    body: Option<Json<CreateVersionDto>>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let dto = body.map(|Json(d)| d).unwrap_or(CreateVersionDto {
        name: None, description: None, start_date: None, due_date: None,
    });
    let name = dto.name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Nouvelle version".to_string());
    let version = sqlx::query_as::<_, Version>(
        &format!(
            "INSERT INTO project_versions (project_id, name, description, start_date, due_date) \
             VALUES ($1, $2, $3, $4, $5) RETURNING {COLS}"
        ),
    )
    .bind(project_id).bind(&name)
    .bind(dto.description.as_deref().unwrap_or(""))
    .bind(dto.start_date).bind(dto.due_date)
    .fetch_one(&state.db).await?;
    Ok(Json(json!({ "version": version })))
}

/// PATCH /projects/:id/versions/:vid
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, version_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateVersionDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    if let Some(ref s) = dto.status {
        if !["open", "locked", "closed"].contains(&s.as_str()) {
            return Err(OfficeError::Validation(format!("Statut de version invalide : {s}")));
        }
    }
    let version = sqlx::query_as::<_, Version>(
        &format!(
            "UPDATE project_versions SET \
                name = COALESCE($3, name), \
                description = COALESCE($4, description), \
                start_date = COALESCE($5, start_date), \
                due_date = COALESCE($6, due_date), \
                status = COALESCE($7, status) \
             WHERE id = $1 AND project_id = $2 RETURNING {COLS}"
        ),
    )
    .bind(version_id).bind(project_id)
    .bind(dto.name.as_deref()).bind(dto.description.as_deref())
    .bind(dto.start_date).bind(dto.due_date).bind(dto.status.as_deref())
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Version introuvable".into()))?;
    Ok(Json(json!({ "version": version })))
}

/// DELETE /projects/:id/versions/:vid — tasks keep their data, version_id set NULL.
pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, version_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    sqlx::query("DELETE FROM project_versions WHERE id = $1 AND project_id = $2")
        .bind(version_id).bind(project_id)
        .execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
struct Version {
    id:          Uuid,
    project_id:  Uuid,
    name:        String,
    description: String,
    start_date:  Option<NaiveDate>,
    due_date:    Option<NaiveDate>,
    status:      String,
    position:    i32,
    created_at:  chrono::DateTime<chrono::Utc>,
}
