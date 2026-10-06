//! Project baselines: a saved snapshot of the planned schedule (task offsets +
//! durations) at a point in time, for planned-vs-actual comparison on the Gantt.
use axum::{
    extract::{Path, State},
    Extension, Json,
};
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
pub struct CaptureDto {
    pub name: Option<String>,
}

/// POST /projects/:id/baselines — snapshot the current planned schedule.
pub async fn capture(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    body: Option<Json<CaptureDto>>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Owner).await?;
    // Snapshot every schedulable task (summaries are computed, excluded like the CPM).
    let snapshot = sqlx::query_as::<_, (Uuid, String, Option<i32>, Option<i32>, i32, Option<f64>)>(
        r#"SELECT id, name, early_start, early_finish, duration_days, estimated_hours
           FROM tasks WHERE project_id = $1 AND task_type != 'summary' ORDER BY position ASC"#,
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await?;
    let tasks: Vec<Value> = snapshot
        .iter()
        .map(|(task_id, name, es, ef, dur, _work)| {
            json!({ "task_id": task_id, "name": name, "early_start": es, "early_finish": ef, "duration_days": dur })
        })
        .collect();

    let project_start: Option<chrono::NaiveDate> =
        sqlx::query_scalar("SELECT start_date FROM projects WHERE id = $1")
            .bind(project_id)
            .fetch_one(&state.db)
            .await?;

    let name = body
        .and_then(|Json(d)| d.name)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Plan de référence".to_string());

    // The JSONB column is kept for now — older clients still read it — but the row
    // table is what every calculation queries. Both are written in one transaction
    // so they can never disagree.
    let mut tx = state.db.begin().await?;

    let baseline = sqlx::query_as::<_, (Uuid, chrono::DateTime<chrono::Utc>)>(
        r#"INSERT INTO project_baselines (project_id, name, project_start, tasks, captured_by)
           VALUES ($1, $2, $3, $4, $5) RETURNING id, captured_at"#,
    )
    .bind(project_id)
    .bind(&name)
    .bind(project_start)
    .bind(json!(tasks))
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;

    for row in &snapshot {
        sqlx::query(
            "INSERT INTO pm_baseline_task_snapshot                  (baseline_id, task_id, name, planned_start, planned_finish, planned_duration, planned_work)              VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(baseline.0).bind(row.0).bind(&row.1)
        .bind(row.2).bind(row.3).bind(row.4).bind(row.5)
        .execute(&mut *tx).await?;
    }

    // The first baseline of a project becomes the one it is judged against;
    // otherwise capturing one would change nothing until someone picked it.
    sqlx::query(
        "UPDATE project_baselines SET is_primary = TRUE WHERE id = $1          AND NOT EXISTS (SELECT 1 FROM project_baselines WHERE project_id = $2 AND is_primary)",
    ).bind(baseline.0).bind(project_id).execute(&mut *tx).await?;

    tx.commit().await?;

    Ok(Json(json!({
        "id": baseline.0, "name": name, "captured_at": baseline.1, "task_count": tasks.len(),
    })))
}

/// GET /projects/:id/baselines — list baselines (metadata + full task snapshots).
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let rows = sqlx::query_as::<_, (Uuid, String, Option<chrono::NaiveDate>, Value, chrono::DateTime<chrono::Utc>, bool)>(
        r#"SELECT id, name, project_start, tasks, captured_at, is_primary
           FROM project_baselines WHERE project_id = $1 ORDER BY captured_at DESC"#,
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await?;
    let baselines: Vec<Value> = rows
        .into_iter()
        .map(|(id, name, project_start, tasks, captured_at, is_primary)| {
            json!({ "id": id, "name": name, "project_start": project_start, "tasks": tasks,
                    "captured_at": captured_at, "is_primary": is_primary })
        })
        .collect();
    Ok(Json(json!({ "baselines": baselines })))
}

/// DELETE /projects/:id/baselines/:bid — remove a baseline.
pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, baseline_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Owner).await?;
    sqlx::query("DELETE FROM project_baselines WHERE id = $1 AND project_id = $2")
        .bind(baseline_id)
        .bind(project_id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateBaselineDto {
    pub name:       Option<String>,
    /// Make this the baseline the project is judged against.
    pub is_primary: Option<bool>,
}

/// PATCH /projects/:id/baselines/:bid — rename, or promote to primary.
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, baseline_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateBaselineDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Owner).await?;

    let mut tx = state.db.begin().await?;
    if dto.is_primary.unwrap_or(false) {
        // Demote the incumbent first: a partial unique index guarantees one primary
        // per project, so promoting without demoting would simply fail.
        sqlx::query("UPDATE project_baselines SET is_primary = FALSE WHERE project_id = $1 AND is_primary")
            .bind(project_id).execute(&mut *tx).await?;
    }
    let row = sqlx::query_as::<_, (Uuid, String, bool)>(
        "UPDATE project_baselines SET \
             name = COALESCE($3, name), \
             is_primary = COALESCE($4, is_primary) \
         WHERE id = $1 AND project_id = $2 RETURNING id, name, is_primary",
    )
    .bind(baseline_id).bind(project_id)
    .bind(dto.name.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.is_primary)
    .fetch_optional(&mut *tx).await?
    .ok_or_else(|| OfficeError::NotFound("Plan de référence introuvable".into()))?;
    tx.commit().await?;

    Ok(Json(json!({ "id": row.0, "name": row.1, "is_primary": row.2 })))
}

/// GET /projects/:id/baselines/:bid/variance — what the plan promised against
/// what it now says, task by task.
///
/// Computed here rather than in the interface: the same numbers feed the Gantt,
/// a report and, later, earned value — they must not be re-derived three times.
pub async fn variance(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, baseline_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;

    #[derive(sqlx::FromRow)]
    struct Row {
        task_id:          Uuid,
        name:             String,
        planned_start:    Option<i32>,
        planned_finish:   Option<i32>,
        planned_duration: i32,
        current_start:    Option<i32>,
        current_finish:   Option<i32>,
        current_duration: Option<i32>,
    }
    // A LEFT JOIN on purpose: a task that has since been deleted still appears,
    // with no current dates — dropping it would hide scope that was removed.
    let rows = sqlx::query_as::<_, Row>(
        "SELECT s.task_id, s.name, s.planned_start, s.planned_finish, s.planned_duration, \
                t.early_start AS current_start, t.early_finish AS current_finish, \
                t.duration_days AS current_duration \
         FROM pm_baseline_task_snapshot s \
         LEFT JOIN tasks t ON t.id = s.task_id AND t.project_id = $2 \
         WHERE s.baseline_id = $1 \
         ORDER BY s.planned_start NULLS LAST, s.name",
    )
    .bind(baseline_id).bind(project_id)
    .fetch_all(&state.db).await?;

    if rows.is_empty() {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM project_baselines WHERE id = $1 AND project_id = $2)",
        ).bind(baseline_id).bind(project_id).fetch_one(&state.db).await?;
        if !exists {
            return Err(OfficeError::NotFound("Plan de référence introuvable".into()));
        }
    }

    let mut items = Vec::with_capacity(rows.len());
    let (mut late, mut early, mut gone) = (0usize, 0usize, 0usize);
    for r in &rows {
        let start_variance = match (r.current_start, r.planned_start) {
            (Some(c), Some(p)) => Some(c - p),
            _ => None,
        };
        let finish_variance = match (r.current_finish, r.planned_finish) {
            (Some(c), Some(p)) => Some(c - p),
            _ => None,
        };
        if r.current_start.is_none() { gone += 1; }
        match finish_variance {
            Some(v) if v > 0 => late += 1,
            Some(v) if v < 0 => early += 1,
            _ => {}
        }
        items.push(json!({
            "task_id": r.task_id,
            "name": r.name,
            "planned_start": r.planned_start,
            "planned_finish": r.planned_finish,
            "planned_duration": r.planned_duration,
            "current_start": r.current_start,
            "current_finish": r.current_finish,
            "current_duration": r.current_duration,
            "start_variance": start_variance,
            "finish_variance": finish_variance,
        }));
    }

    Ok(Json(json!({
        "baseline_id": baseline_id,
        "tasks": items,
        "summary": { "total": rows.len(), "late": late, "early": early, "removed": gone },
    })))
}
