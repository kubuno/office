use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    script::{models::run::ScriptRun, services::execution_service},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct ListRunsQuery {
    pub limit:  Option<i64>,
    pub offset: Option<i64>,
}

// ── List runs for a script ────────────────────────────────────────────────────

pub async fn list_for_script(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(script_id): Path<Uuid>,
    Query(q): Query<ListRunsQuery>,
) -> Result<Json<Value>> {
    // Verify ownership
    let owns: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM office_script.scripts WHERE id = $1 AND owner_id = $2)",
    )
    .bind(script_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    if !owns {
        return Err(OfficeError::NotFound(format!("Script {script_id} introuvable")));
    }

    let limit  = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);

    let runs: Vec<ScriptRun> = sqlx::query_as::<_, ScriptRun>(
        r#"SELECT id, script_id, owner_id, trigger_id, run_source, status, duration_ms,
                  memory_used_kb, console_output, return_value, error_message, error_stack,
                  trigger_data, started_at, finished_at
           FROM office_script.runs
           WHERE script_id = $1 AND owner_id = $2
           ORDER BY started_at DESC LIMIT $3 OFFSET $4"#,
    )
    .bind(script_id)
    .bind(user.id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "runs": runs, "total": runs.len() })))
}

// ── Get single run ────────────────────────────────────────────────────────────

pub async fn get(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<Value>> {
    let run = execution_service::get_run(&state.db, run_id, user.id).await?;
    Ok(Json(json!({ "run": run })))
}
