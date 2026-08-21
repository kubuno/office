//! Time entries: a dated log of hours worked on a task. Any project member can log
//! time; the sum of a task's entries is rolled up into `tasks.spent_hours` (kept
//! distinct from the estimate). Modelled on OpenProject's time tracking.
use axum::{
    extract::{Path, Query, State},
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

// Fixed activity vocabulary (OpenProject makes these admin-configurable per project;
// a fixed set keeps this self-contained without an extra referential).
const ACTIVITIES: [&str; 6] = [
    "development", "design", "coordination", "testing", "documentation", "other",
];

#[derive(Debug, Deserialize)]
pub struct CreateEntryDto {
    pub task_id:  Uuid,
    pub spent_on: Option<NaiveDate>,
    pub hours:    Option<f64>,
    pub activity: Option<String>,
    pub comment:  Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEntryDto {
    pub spent_on: Option<NaiveDate>,
    pub hours:    Option<f64>,
    pub activity: Option<String>,
    pub comment:  Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    // Optional filter: only entries for this task.
    pub task_id: Option<Uuid>,
}

fn normalize_activity(a: Option<String>) -> Result<String> {
    let a = a.map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty())
        .unwrap_or_else(|| "development".to_string());
    if !ACTIVITIES.contains(&a.as_str()) {
        return Err(OfficeError::Validation(format!("Activité inconnue : {a}")));
    }
    Ok(a)
}

fn validate_hours(h: f64) -> Result<f64> {
    if !h.is_finite() || !(0.0..=1000.0).contains(&h) {
        return Err(OfficeError::Validation("Nombre d'heures invalide".into()));
    }
    Ok(h)
}

// Recompute the task's rolled-up spent_hours from its entries (NULL when none).
async fn rollup_spent<'e, E>(exec: E, task_id: Uuid) -> Result<()>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query(
        "UPDATE tasks SET spent_hours = \
             (SELECT NULLIF(COALESCE(SUM(hours), 0), 0) FROM time_entries WHERE task_id = $1) \
         WHERE id = $1",
    ).bind(task_id).execute(exec).await?;
    Ok(())
}

const COLS: &str = "id, project_id, task_id, user_id, spent_on, hours, activity, comment, created_at";

/// GET /projects/:id/time-entries[?task_id=…]
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let rows: Vec<Entry> = match q.task_id {
        Some(tid) => sqlx::query_as::<_, Entry>(&format!(
            "SELECT {COLS} FROM time_entries WHERE project_id = $1 AND task_id = $2 \
             ORDER BY spent_on DESC, created_at DESC"
        )).bind(project_id).bind(tid).fetch_all(&state.db).await?,
        None => sqlx::query_as::<_, Entry>(&format!(
            "SELECT {COLS} FROM time_entries WHERE project_id = $1 \
             ORDER BY spent_on DESC, created_at DESC"
        )).bind(project_id).fetch_all(&state.db).await?,
    };
    Ok(Json(json!({ "entries": rows })))
}

/// POST /projects/:id/time-entries
pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<CreateEntryDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    // The task must belong to this project.
    let ok = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE id = $1 AND project_id = $2)",
    ).bind(dto.task_id).bind(project_id).fetch_one(&state.db).await?;
    if !ok {
        return Err(OfficeError::NotFound("Tâche introuvable dans ce projet".into()));
    }
    let hours = validate_hours(dto.hours.unwrap_or(0.0))?;
    let activity = normalize_activity(dto.activity)?;

    let mut tx = state.db.begin().await?;
    let entry = sqlx::query_as::<_, Entry>(&format!(
        "INSERT INTO time_entries (project_id, task_id, user_id, spent_on, hours, activity, comment) \
         VALUES ($1, $2, $3, COALESCE($4, CURRENT_DATE), $5, $6, $7) RETURNING {COLS}"
    ))
    .bind(project_id).bind(dto.task_id).bind(user.id)
    .bind(dto.spent_on).bind(hours).bind(&activity)
    .bind(dto.comment.as_deref().unwrap_or(""))
    .fetch_one(&mut *tx).await?;
    rollup_spent(&mut *tx, dto.task_id).await?;
    tx.commit().await?;
    Ok(Json(json!({ "entry": entry })))
}

/// PATCH /projects/:id/time-entries/:eid — author or project owner.
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, entry_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateEntryDto>,
) -> Result<Json<Value>> {
    // Editing time needs write access to the project, and on top of that the entry
    // must be your own — unless you own the project.
    let owner = require_permission(&state, project_id, user.id, Level::Edit).await? == Level::Owner;
    let existing = sqlx::query_as::<_, Entry>(&format!(
        "SELECT {COLS} FROM time_entries WHERE id = $1 AND project_id = $2"
    )).bind(entry_id).bind(project_id).fetch_optional(&state.db).await?
        .ok_or_else(|| OfficeError::NotFound("Saisie introuvable".into()))?;
    if !owner && existing.user_id != user.id {
        return Err(OfficeError::Forbidden);
    }
    if let Some(h) = dto.hours { validate_hours(h)?; }
    let activity = match dto.activity {
        Some(a) => Some(normalize_activity(Some(a))?),
        None => None,
    };

    let mut tx = state.db.begin().await?;
    let entry = sqlx::query_as::<_, Entry>(&format!(
        "UPDATE time_entries SET \
            spent_on = COALESCE($3, spent_on), \
            hours    = COALESCE($4, hours), \
            activity = COALESCE($5, activity), \
            comment  = COALESCE($6, comment) \
         WHERE id = $1 AND project_id = $2 RETURNING {COLS}"
    ))
    .bind(entry_id).bind(project_id)
    .bind(dto.spent_on).bind(dto.hours).bind(activity.as_deref()).bind(dto.comment.as_deref())
    .fetch_one(&mut *tx).await?;
    rollup_spent(&mut *tx, entry.task_id).await?;
    tx.commit().await?;
    Ok(Json(json!({ "entry": entry })))
}

/// DELETE /projects/:id/time-entries/:eid — author or project owner.
pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, entry_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    // Same rule as update: write access to the project, plus authorship (or ownership).
    let owner = require_permission(&state, project_id, user.id, Level::Edit).await? == Level::Owner;
    let existing = sqlx::query_as::<_, Entry>(&format!(
        "SELECT {COLS} FROM time_entries WHERE id = $1 AND project_id = $2"
    )).bind(entry_id).bind(project_id).fetch_optional(&state.db).await?
        .ok_or_else(|| OfficeError::NotFound("Saisie introuvable".into()))?;
    if !owner && existing.user_id != user.id {
        return Err(OfficeError::Forbidden);
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM time_entries WHERE id = $1 AND project_id = $2")
        .bind(entry_id).bind(project_id).execute(&mut *tx).await?;
    rollup_spent(&mut *tx, existing.task_id).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
struct Entry {
    id:         Uuid,
    project_id: Uuid,
    task_id:    Uuid,
    user_id:    Uuid,
    spent_on:   NaiveDate,
    hours:      f64,
    activity:   String,
    comment:    String,
    created_at: chrono::DateTime<chrono::Utc>,
}
