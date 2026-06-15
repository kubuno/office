use std::sync::{Arc, OnceLock};

use serde_json::json;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    script::{
        models::run::ScriptRun,
        runtime::{
            sandbox::{Sandbox, SandboxConfig},
            transpiler::strip_typescript,
        },
    },
    state::AppState,
};

/// Nombre maximum d'exécutions de scripts simultanées sur toute l'instance.
/// Chaque exécution consomme un thread du pool `spawn_blocking` ; on borne la
/// concurrence pour éviter l'épuisement du pool (DoS).
const MAX_CONCURRENT_RUNS: usize = 8;

/// Nombre maximum de runs `running` simultanés autorisés par utilisateur.
const MAX_RUNNING_PER_USER: i64 = 5;

fn run_semaphore() -> &'static Arc<Semaphore> {
    static SEM: OnceLock<Arc<Semaphore>> = OnceLock::new();
    SEM.get_or_init(|| Arc::new(Semaphore::new(MAX_CONCURRENT_RUNS)))
}

/// Orchestrates a full script execution:
/// 1. Load script from DB
/// 2. Strip TypeScript
/// 3. Execute in sandbox
/// 4. Persist the ScriptRun
/// Returns the newly created ScriptRun ID.
pub async fn execute_script(
    state: &AppState,
    script_id: Uuid,
    owner_id: Uuid,
    trigger_id: Option<Uuid>,
    run_source: &str,
    trigger_data: Option<serde_json::Value>,
) -> Result<Uuid> {
    // Load script metadata (la source vit dans le fichier .kbscr)
    let (file_id, timeout_secs, memory_limit_mb): (Option<Uuid>, i32, i32) =
        sqlx::query_as(
            "SELECT file_id, timeout_secs, memory_limit_mb FROM office_script.scripts WHERE id = $1 AND owner_id = $2",
        )
        .bind(script_id)
        .bind(owner_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| OfficeError::NotFound(format!("Script {script_id} introuvable")))?;

    let source_code = match file_id {
        Some(fid) => crate::services::content_files::read_script_source(state, owner_id, fid).await?,
        None => String::new(),
    };

    // Anti-DoS : refuser si l'utilisateur a déjà trop de runs en cours.
    let running: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM office_script.runs WHERE owner_id = $1 AND status = 'running'",
    )
    .bind(owner_id)
    .fetch_one(&state.db)
    .await?;

    if running >= MAX_RUNNING_PER_USER {
        return Err(OfficeError::Validation(format!(
            "Trop d'exécutions simultanées ({MAX_RUNNING_PER_USER} max). Attendez la fin des scripts en cours."
        )));
    }

    // Create a pending run record
    let run_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO office_script.runs
               (script_id, owner_id, trigger_id, run_source, status, trigger_data)
           VALUES ($1, $2, $3, $4, 'running', $5)
           RETURNING id"#,
    )
    .bind(script_id)
    .bind(owner_id)
    .bind(trigger_id)
    .bind(run_source)
    .bind(&trigger_data)
    .fetch_one(&state.db)
    .await?;

    // Execute asynchronously (spawn so caller gets run_id immediately)
    let db = state.db.clone();
    let js_code = strip_typescript(&source_code);

    tokio::spawn(async move {
        // Borne la concurrence globale : on attend un créneau avant d'exécuter.
        // Le permis est relâché automatiquement à la fin de la tâche.
        let _permit = run_semaphore().clone().acquire_owned().await;

        let sandbox = match Sandbox::new(SandboxConfig {
            timeout_secs:    timeout_secs as u64,
            memory_limit_mb: memory_limit_mb as u32,
        }) {
            Ok(s) => s,
            Err(e) => {
                let _ = finalize_run(&db, run_id, script_id, "error", 0, None, None, Some(e.to_string()), None).await;
                return;
            }
        };

        let result = sandbox.execute(&js_code, owner_id).await;

        let status = result.status.as_str();
        let console_json = serde_json::to_value(&result.console_output).unwrap_or(json!([]));

        let _ = finalize_run(
            &db,
            run_id,
            script_id,
            status,
            result.duration_ms as i32,
            Some(console_json),
            result.return_value,
            result.error_message,
            result.error_stack,
        ).await;
    });

    Ok(run_id)
}

async fn finalize_run(
    db: &sqlx::PgPool,
    run_id: Uuid,
    script_id: Uuid,
    status: &str,
    duration_ms: i32,
    console_output: Option<serde_json::Value>,
    return_value: Option<serde_json::Value>,
    error_message: Option<String>,
    error_stack: Option<String>,
) -> std::result::Result<(), sqlx::Error> {
    let console = console_output.unwrap_or(json!([]));

    sqlx::query(
        r#"UPDATE office_script.runs
           SET status = $2, duration_ms = $3, console_output = $4,
               return_value = $5, error_message = $6, error_stack = $7,
               finished_at = NOW()
           WHERE id = $1"#,
    )
    .bind(run_id)
    .bind(status)
    .bind(duration_ms)
    .bind(&console)
    .bind(&return_value)
    .bind(&error_message)
    .bind(&error_stack)
    .execute(db)
    .await?;

    // Update script stats
    let _ = sqlx::query(
        r#"UPDATE office_script.scripts
           SET run_count = run_count + 1, last_run_at = NOW(), last_run_status = $2
           WHERE id = $1"#,
    )
    .bind(script_id)
    .bind(status)
    .execute(db)
    .await;

    Ok(())
}

/// Load a ScriptRun by id
pub async fn get_run(db: &sqlx::PgPool, run_id: Uuid, owner_id: Uuid) -> Result<ScriptRun> {
    sqlx::query_as::<_, ScriptRun>(
        r#"SELECT id, script_id, owner_id, trigger_id, run_source, status, duration_ms,
                  memory_used_kb, console_output, return_value, error_message, error_stack,
                  trigger_data, started_at, finished_at
           FROM office_script.runs WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(run_id)
    .bind(owner_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Run {run_id} introuvable")))
}
