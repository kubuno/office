//! Working calendars of a project: which days are worked, and the days that
//! depart from that pattern.
//!
//! The schedule reads them through `services::working_calendar`, which knows
//! nothing about projects — these endpoints are only the storage and the wiring.
use axum::{extract::{Path, State}, Extension, Json};
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

const COLS: &str = "id, project_id, name, mon, tue, wed, thu, fri, sat, sun, created_at";

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct Calendar {
    id:         Uuid,
    project_id: Uuid,
    name:       String,
    mon: bool, tue: bool, wed: bool, thu: bool, fri: bool, sat: bool, sun: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct CalendarException {
    id:          Uuid,
    calendar_id: Uuid,
    day:         NaiveDate,
    is_working:  bool,
    note:        String,
}

#[derive(Debug, Deserialize)]
pub struct UpsertCalendarDto {
    pub name: Option<String>,
    pub mon: Option<bool>, pub tue: Option<bool>, pub wed: Option<bool>,
    pub thu: Option<bool>, pub fri: Option<bool>, pub sat: Option<bool>, pub sun: Option<bool>,
    /// Make this the calendar the project schedules against.
    pub set_as_project_default: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ExceptionDto {
    pub day:        NaiveDate,
    pub is_working: Option<bool>,
    pub note:       Option<String>,
}

/// GET /projects/:id/calendars — the calendars plus, for each, its exceptions.
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;

    let calendars = sqlx::query_as::<_, Calendar>(
        &format!("SELECT {COLS} FROM pm_calendars WHERE project_id = $1 ORDER BY created_at ASC"),
    ).bind(project_id).fetch_all(&state.db).await?;

    let exceptions = sqlx::query_as::<_, CalendarException>(
        "SELECT e.id, e.calendar_id, e.day, e.is_working, e.note \
         FROM pm_calendar_exceptions e \
         JOIN pm_calendars c ON c.id = e.calendar_id \
         WHERE c.project_id = $1 ORDER BY e.day ASC",
    ).bind(project_id).fetch_all(&state.db).await?;

    let active: Option<Uuid> = sqlx::query_scalar("SELECT calendar_id FROM projects WHERE id = $1")
        .bind(project_id).fetch_one(&state.db).await?;

    Ok(Json(json!({
        "calendars": calendars,
        "exceptions": exceptions,
        "project_calendar_id": active,
    })))
}

/// POST /projects/:id/calendars
pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    body: Option<Json<UpsertCalendarDto>>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let dto = body.map(|Json(d)| d);

    let name = dto.as_ref().and_then(|d| d.name.clone())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Calendrier".to_string());

    // Seed the working week from the instance default the administrator chose
    // (weekends counted or not) — Monday–Friday stays the shipped fallback.
    let include_weekends = state.instance().project_default_include_weekends;
    let mut tx = state.db.begin().await?;
    let cal = sqlx::query_as::<_, Calendar>(
        &format!("INSERT INTO pm_calendars (project_id, name, sat, sun) VALUES ($1, $2, $3, $3) RETURNING {COLS}"),
    ).bind(project_id).bind(&name).bind(include_weekends).fetch_one(&mut *tx).await?;

    // A project with no calendar yet adopts its first one: otherwise creating a
    // calendar would appear to do nothing at all.
    sqlx::query("UPDATE projects SET calendar_id = $2 WHERE id = $1 AND calendar_id IS NULL")
        .bind(project_id).bind(cal.id)
        .execute(&mut *tx).await?;
    tx.commit().await?;

    Ok(Json(json!({ "calendar": cal })))
}

/// PATCH /projects/:id/calendars/:cid
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, calendar_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpsertCalendarDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;

    let cal = sqlx::query_as::<_, Calendar>(&format!(
        "UPDATE pm_calendars SET \
            name = COALESCE($3, name), \
            mon = COALESCE($4, mon), tue = COALESCE($5, tue), wed = COALESCE($6, wed), \
            thu = COALESCE($7, thu), fri = COALESCE($8, fri), sat = COALESCE($9, sat), \
            sun = COALESCE($10, sun) \
         WHERE id = $1 AND project_id = $2 RETURNING {COLS}"
    ))
    .bind(calendar_id).bind(project_id).bind(dto.name.as_deref())
    .bind(dto.mon).bind(dto.tue).bind(dto.wed).bind(dto.thu)
    .bind(dto.fri).bind(dto.sat).bind(dto.sun)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Calendrier introuvable".into()))?;

    // A week with no worked day would leave the scheduler nowhere to put the work.
    if !(cal.mon || cal.tue || cal.wed || cal.thu || cal.fri || cal.sat || cal.sun) {
        return Err(OfficeError::Validation(
            "Un calendrier doit garder au moins un jour travaillé.".into(),
        ));
    }

    if dto.set_as_project_default.unwrap_or(false) {
        sqlx::query("UPDATE projects SET calendar_id = $2 WHERE id = $1")
            .bind(project_id).bind(calendar_id)
            .execute(&state.db).await?;
    }
    mark_schedule_stale(&state, project_id).await?;
    Ok(Json(json!({ "calendar": cal })))
}

/// DELETE /projects/:id/calendars/:cid
pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, calendar_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    // The foreign keys set `projects.calendar_id` and `tasks.calendar_id` back to
    // NULL, which falls back to the Monday-to-Friday default — no orphan pointers.
    let rows = sqlx::query("DELETE FROM pm_calendars WHERE id = $1 AND project_id = $2")
        .bind(calendar_id).bind(project_id)
        .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Calendrier introuvable".into())); }
    mark_schedule_stale(&state, project_id).await?;
    Ok(Json(json!({ "ok": true })))
}

/// PUT /projects/:id/calendars/:cid/exceptions — add or update one day.
pub async fn set_exception(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, calendar_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<ExceptionDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    if !calendar_in_project(&state, project_id, calendar_id).await? {
        return Err(OfficeError::NotFound("Calendrier introuvable".into()));
    }

    let ex = sqlx::query_as::<_, CalendarException>(
        "INSERT INTO pm_calendar_exceptions (calendar_id, day, is_working, note) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (calendar_id, day) DO UPDATE \
           SET is_working = EXCLUDED.is_working, note = EXCLUDED.note \
         RETURNING id, calendar_id, day, is_working, note",
    )
    .bind(calendar_id).bind(dto.day)
    .bind(dto.is_working.unwrap_or(false))
    .bind(dto.note.as_deref().unwrap_or(""))
    .fetch_one(&state.db).await?;

    mark_schedule_stale(&state, project_id).await?;
    Ok(Json(json!({ "exception": ex })))
}

/// DELETE /projects/:id/calendars/:cid/exceptions/:day
pub async fn delete_exception(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, calendar_id, day)): Path<(Uuid, Uuid, NaiveDate)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    if !calendar_in_project(&state, project_id, calendar_id).await? {
        return Err(OfficeError::NotFound("Calendrier introuvable".into()));
    }
    sqlx::query("DELETE FROM pm_calendar_exceptions WHERE calendar_id = $1 AND day = $2")
        .bind(calendar_id).bind(day)
        .execute(&state.db).await?;
    mark_schedule_stale(&state, project_id).await?;
    Ok(Json(json!({ "ok": true })))
}

/// Changing when work happens changes every date in the plan — but the database
/// triggers only watch task durations and links, so nothing would mark the
/// schedule as stale. Say it explicitly.
async fn mark_schedule_stale(state: &AppState, project_id: Uuid) -> Result<()> {
    sqlx::query("UPDATE tasks SET cpm_dirty = TRUE WHERE project_id = $1")
        .bind(project_id)
        .execute(&state.db).await?;
    Ok(())
}

async fn calendar_in_project(state: &AppState, project_id: Uuid, calendar_id: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM pm_calendars WHERE id = $1 AND project_id = $2)",
    ).bind(calendar_id).bind(project_id).fetch_one(&state.db).await?)
}
