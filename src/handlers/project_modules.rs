//! Cloud-project resources: which Kubuno modules a project "uses" (the sovereign
//! counterpart of a cloud console's enabled services). Stored as bare module-id
//! strings — modules are discovered dynamically, so no cross-schema FK.
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
pub struct AttachDto {
    pub module_id: String,
}

/// GET /projects/:id/modules — the modules attached to the project.
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let rows = sqlx::query_as::<_, (String, chrono::DateTime<chrono::Utc>)>(
        "SELECT module_id, added_at FROM project_modules WHERE project_id = $1 ORDER BY added_at ASC",
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await?;
    let modules: Vec<Value> = rows
        .into_iter()
        .map(|(module_id, added_at)| json!({ "module_id": module_id, "added_at": added_at }))
        .collect();
    Ok(Json(json!({ "modules": modules })))
}

/// POST /projects/:id/modules — attach a module (idempotent).
pub async fn attach(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<AttachDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let module_id = dto.module_id.trim();
    if module_id.is_empty() || module_id.len() > 100 {
        return Err(OfficeError::Validation("module_id invalide".into()));
    }
    sqlx::query(
        r#"INSERT INTO project_modules (project_id, module_id, added_by)
           VALUES ($1, $2, $3)
           ON CONFLICT (project_id, module_id) DO NOTHING"#,
    )
    .bind(project_id)
    .bind(module_id)
    .bind(user.id)
    .execute(&state.db)
    .await?;
    Ok(Json(json!({ "ok": true, "module_id": module_id })))
}

/// DELETE /projects/:id/modules/:mid — detach a module.
pub async fn detach(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, module_id)): Path<(Uuid, String)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    sqlx::query("DELETE FROM project_modules WHERE project_id = $1 AND module_id = $2")
        .bind(project_id)
        .bind(&module_id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}
