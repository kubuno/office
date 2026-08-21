//! Which artifacts a project shows, and how it is run.
//!
//! The module keeps growing: schedule, board, roadmap, workload, baselines, time
//! log — and more to come. Showing all of them to every project would bury a
//! three-task plan under a ribbon built for a construction programme. So each
//! project switches on what it uses, and the interface only offers the rest.
//!
//! Defaults come from the project's methodology and are never written to the
//! database: a row exists only where someone made a deliberate choice, which keeps
//! the table empty for the vast majority of projects and lets defaults evolve
//! without a migration.
use axum::{extract::{Path, State}, Extension, Json};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    handlers::project_authz::{require_permission, Level},
    middleware::OfficeUser,
    state::AppState,
};

/// Everything a project can switch on. Adding a capability means adding a key
/// here — an unknown key is refused rather than silently stored.
pub const ARTIFACTS: [&str; 22] = [
    "schedule",   // Gantt + critical path
    "board",      // Kanban
    "calendar",
    "workload",   // resource load
    "network",    // PERT
    "roadmap",    // release versions
    "baselines",
    "timelog",
    "charter",       // project charter
    "wbs",           // breakdown + dictionary
    "deliverables",
    "requirements",  // + traceability matrix
    "risks",         // register + probability/impact matrix
    "issues",        // what is happening now
    "costs",         // budget + earned value
    "stakeholders",  // register + power/interest grid + RACI
    "quality",       // metrics, checks, cost of quality
    "communications",
    "decisions",
    "changes",       // change requests, assessment, decision
    "closure",       // closure checks + lessons learned
    "procurement",   // contracts, suppliers, payments
];

const METHODOLOGIES: [&str; 3] = ["predictive", "agile", "hybrid"];

/// Whether an artifact is offered by default for a given methodology.
///
/// Everything that exists today is on by default, whatever the methodology: these
/// views were always visible, and a project that already relies on one must not
/// lose it because tailoring arrived. Only the *starting point* differs — an agile
/// project opens on its board. Capabilities added later start off unless the
/// methodology calls for them.
fn default_enabled(_methodology: &str, artifact: &str) -> bool {
    ARTIFACTS.contains(&artifact)
}

/// The view a project opens on, derived from how it is run.
fn default_view(methodology: &str) -> &'static str {
    match methodology {
        "agile" => "board",
        _ => "schedule",
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateSettingsDto {
    pub methodology: Option<String>,
    /// Only the artifacts named here change; the others keep their current state.
    pub artifacts:   Option<Map<String, Value>>,
}

#[derive(Debug, sqlx::FromRow)]
struct SettingRow {
    artifact_key: String,
    enabled:      bool,
    config:       Value,
}

/// GET /projects/:id/settings
pub async fn get(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;

    let methodology: String =
        sqlx::query_scalar("SELECT methodology FROM projects WHERE id = $1")
            .bind(project_id)
            .fetch_optional(&state.db).await?
            .ok_or_else(|| OfficeError::NotFound("Projet introuvable".into()))?;

    let rows = sqlx::query_as::<_, SettingRow>(
        "SELECT artifact_key, enabled, config FROM pm_project_settings WHERE project_id = $1",
    ).bind(project_id).fetch_all(&state.db).await?;

    // Defaults first, stored choices on top — so a project created before an
    // artifact existed still answers for it.
    let mut artifacts = Map::new();
    for key in ARTIFACTS {
        artifacts.insert(key.to_string(), json!({
            "enabled": default_enabled(&methodology, key),
            "config":  json!({}),
        }));
    }
    for row in rows {
        artifacts.insert(row.artifact_key, json!({
            "enabled": row.enabled,
            "config":  row.config,
        }));
    }

    Ok(Json(json!({
        "methodology":  methodology,
        "default_view": default_view(&methodology),
        "artifacts":    artifacts,
    })))
}

/// PUT /projects/:id/settings
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<UpdateSettingsDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;

    if let Some(ref m) = dto.methodology {
        if !METHODOLOGIES.contains(&m.as_str()) {
            return Err(OfficeError::Validation(format!("Méthode inconnue : {m}")));
        }
    }
    if let Some(ref map) = dto.artifacts {
        for key in map.keys() {
            if !ARTIFACTS.contains(&key.as_str()) {
                return Err(OfficeError::Validation(format!("Artefact inconnu : {key}")));
            }
        }
    }

    let mut tx = state.db.begin().await?;

    if let Some(m) = dto.methodology.as_deref() {
        sqlx::query("UPDATE projects SET methodology = $2 WHERE id = $1")
            .bind(project_id).bind(m)
            .execute(&mut *tx).await?;
    }

    if let Some(map) = dto.artifacts {
        for (key, value) in map {
            let enabled = value.get("enabled").and_then(Value::as_bool).unwrap_or(true);
            let config  = value.get("config").cloned().unwrap_or_else(|| json!({}));
            if !config.is_object() {
                return Err(OfficeError::Validation(format!(
                    "La configuration de « {key} » doit être un objet"
                )));
            }
            sqlx::query(
                "INSERT INTO pm_project_settings (project_id, artifact_key, enabled, config) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (project_id, artifact_key) DO UPDATE \
                   SET enabled = EXCLUDED.enabled, config = EXCLUDED.config, updated_at = now()",
            )
            .bind(project_id).bind(&key).bind(enabled).bind(&config)
            .execute(&mut *tx).await?;
        }
    }

    tx.commit().await?;
    get(State(state), Extension(user), Path(project_id)).await
}
