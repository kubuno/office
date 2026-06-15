use axum::{
    extract::{Path, State},
    Extension, Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    script::{
        models::macro_::{CreateMacroDto, ScriptMacro},
        services::execution_service,
    },
    state::AppState,
};

// ── List all macros for user ──────────────────────────────────────────────────

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
) -> Result<Json<Value>> {
    let macros: Vec<ScriptMacro> = sqlx::query_as::<_, ScriptMacro>(
        r#"SELECT id, script_id, owner_id, document_type, document_id,
                  button_label, button_icon, position, created_at
           FROM office_script.macros WHERE owner_id = $1 ORDER BY position, created_at"#,
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "macros": macros })))
}

// ── List macros for a specific document ──────────────────────────────────────

pub async fn list_for_document(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((doc_type, doc_id)): Path<(String, Uuid)>,
) -> Result<Json<Value>> {
    let macros: Vec<ScriptMacro> = sqlx::query_as::<_, ScriptMacro>(
        r#"SELECT id, script_id, owner_id, document_type, document_id,
                  button_label, button_icon, position, created_at
           FROM office_script.macros
           WHERE owner_id = $1 AND document_type = $2 AND document_id = $3
           ORDER BY position, created_at"#,
    )
    .bind(user.id)
    .bind(&doc_type)
    .bind(doc_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "macros": macros })))
}

// ── Create ────────────────────────────────────────────────────────────────────

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<CreateMacroDto>,
) -> Result<Json<Value>> {
    // Verify script ownership
    let owns: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM office_script.scripts WHERE id = $1 AND owner_id = $2)",
    )
    .bind(dto.script_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    if !owns {
        return Err(OfficeError::NotFound(format!("Script {} introuvable", dto.script_id)));
    }

    let button_label = dto.button_label.unwrap_or_else(|| "Exécuter".to_string());
    let button_icon  = dto.button_icon.unwrap_or_else(|| "⚡".to_string());
    let position     = dto.position.unwrap_or(0);

    let macro_: ScriptMacro = sqlx::query_as::<_, ScriptMacro>(
        r#"INSERT INTO office_script.macros
               (script_id, owner_id, document_type, document_id, button_label, button_icon, position)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           RETURNING id, script_id, owner_id, document_type, document_id,
                     button_label, button_icon, position, created_at"#,
    )
    .bind(dto.script_id)
    .bind(user.id)
    .bind(&dto.document_type)
    .bind(dto.document_id)
    .bind(&button_label)
    .bind(&button_icon)
    .bind(position)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "macro": macro_ })))
}

// ── Delete ────────────────────────────────────────────────────────────────────

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(macro_id): Path<Uuid>,
) -> Result<Json<Value>> {
    let affected = sqlx::query(
        "DELETE FROM office_script.macros WHERE id = $1 AND owner_id = $2",
    )
    .bind(macro_id)
    .bind(user.id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(OfficeError::NotFound(format!("Macro {macro_id} introuvable")));
    }
    Ok(Json(json!({ "deleted": true })))
}

// ── Run macro ─────────────────────────────────────────────────────────────────

pub async fn run_macro(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(macro_id): Path<Uuid>,
) -> Result<Json<Value>> {
    let macro_: ScriptMacro = sqlx::query_as::<_, ScriptMacro>(
        r#"SELECT id, script_id, owner_id, document_type, document_id,
                  button_label, button_icon, position, created_at
           FROM office_script.macros WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(macro_id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Macro {macro_id} introuvable")))?;

    let trigger_data = serde_json::json!({
        "macro_id": macro_id,
        "document_type": macro_.document_type,
        "document_id": macro_.document_id,
    });

    let run_id = execution_service::execute_script(
        &state,
        macro_.script_id,
        user.id,
        None,
        "macro",
        Some(trigger_data),
    )
    .await?;

    Ok(Json(json!({ "run_id": run_id })))
}
