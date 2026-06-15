use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::{Path, State},
    response::sse::{Event, Sse},
    Extension, Json,
};
use futures::Stream;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    script::services::execution_service,
    state::AppState,
};

/// POST /script/scripts/:id/run
/// Starts a script execution, returns the run_id immediately.
pub async fn run_script(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(script_id): Path<Uuid>,
) -> Result<Json<Value>> {
    let run_id = execution_service::execute_script(
        &state,
        script_id,
        user.id,
        None,
        "manual",
        None,
    )
    .await?;

    Ok(Json(json!({ "run_id": run_id })))
}

/// GET /script/runs/:run_id/stream
/// SSE stream: polls the run until finished and streams console entries + final result.
pub async fn stream_run(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(run_id): Path<Uuid>,
) -> Result<Sse<impl Stream<Item = std::result::Result<Event, Infallible>>>> {
    // Verify ownership
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM office_script.runs WHERE id = $1 AND owner_id = $2)",
    )
    .bind(run_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    if !exists {
        return Err(OfficeError::NotFound(format!("Run {run_id} introuvable")));
    }

    let db = state.db.clone();

    let stream = async_stream::stream! {
        let mut sent_count = 0usize;
        let mut finished = false;

        // Poll every 200ms for up to 5 minutes
        for _ in 0..1500 {
            if finished { break; }

            tokio::time::sleep(Duration::from_millis(200)).await;

            let row: Option<(String, Value, Option<Value>, Option<String>, Option<i32>)> = sqlx::query_as(
                r#"SELECT status, console_output, return_value, error_message, duration_ms
                   FROM office_script.runs WHERE id = $1"#,
            )
            .bind(run_id)
            .fetch_optional(&db)
            .await
            .unwrap_or(None);

            let (status, console_json, return_value, error_message, duration_ms) = match row {
                Some(r) => r,
                None => break,
            };

            // Stream new console entries
            if let Value::Array(entries) = &console_json {
                for entry in entries.iter().skip(sent_count) {
                    let data = serde_json::to_string(&json!({
                        "type": "console_log",
                        "entry": entry
                    })).unwrap_or_default();
                    yield Ok(Event::default().event("console_log").data(data));
                    sent_count += 1;
                }
            }

            // Check if finished
            if status != "running" {
                finished = true;
                let data = serde_json::to_string(&json!({
                    "type": "finished",
                    "status": status,
                    "return_value": return_value,
                    "error_message": error_message,
                    "duration_ms": duration_ms,
                    "run_id": run_id,
                })).unwrap_or_default();
                yield Ok(Event::default().event("finished").data(data));
            }
        }

        // If timed out without finish event
        if !finished {
            let data = serde_json::to_string(&json!({
                "type": "finished",
                "status": "timeout",
                "error_message": "Le stream a expiré",
                "run_id": run_id,
            })).unwrap_or_default();
            yield Ok(Event::default().event("finished").data(data));
        }
    };

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}
