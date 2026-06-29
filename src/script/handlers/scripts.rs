use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    script::{
        models::script::{CreateScriptDto, ListScriptsQuery, Script, UpdateScriptDto},
        runtime::transpiler::strip_typescript,
    },
    services::content_files,
    state::AppState,
};

// ── List ──────────────────────────────────────────────────────────────────────

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Query(q): Query<ListScriptsQuery>,
) -> Result<Json<Value>> {
    let trashed = q.trashed.unwrap_or(false);
    let limit   = q.limit.unwrap_or(50).min(200);
    let offset  = q.offset.unwrap_or(0);

    let scripts: Vec<Script> = if let Some(ref search) = q.search {
        sqlx::query_as::<_, Script>(
            r#"SELECT id, owner_id, name, description, file_id, compiled_code, compile_error,
                      timeout_secs, memory_limit_mb, run_count, last_run_at, last_run_status,
                      is_starred, is_trashed, created_at, updated_at
               FROM office_script.scripts
               WHERE owner_id = $1 AND is_trashed = $2 AND name ILIKE $3
               ORDER BY updated_at DESC LIMIT $4 OFFSET $5"#,
        )
        .bind(user.id).bind(trashed).bind(format!("%{search}%")).bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    } else {
        sqlx::query_as::<_, Script>(
            r#"SELECT id, owner_id, name, description, file_id, compiled_code, compile_error,
                      timeout_secs, memory_limit_mb, run_count, last_run_at, last_run_status,
                      is_starred, is_trashed, created_at, updated_at
               FROM office_script.scripts
               WHERE owner_id = $1 AND is_trashed = $2
               ORDER BY updated_at DESC LIMIT $3 OFFSET $4"#,
        )
        .bind(user.id).bind(trashed).bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    };

    Ok(Json(json!({ "scripts": scripts, "total": scripts.len() })))
}

// ── Create ────────────────────────────────────────────────────────────────────

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<CreateScriptDto>,
) -> Result<Json<Value>> {
    let name        = dto.name.unwrap_or_else(|| "Nouveau script".to_string());
    let source_code = dto.source_code.unwrap_or_else(|| "// Kubuno Script\n".to_string());
    let timeout     = dto.timeout_secs.unwrap_or(30);
    let memory      = dto.memory_limit_mb.unwrap_or(64);

    // Contenu (source) → fichier .kbscr.
    let file_id = content_files::create_script_file(&state, user.id, &name, &source_code).await?;

    let mut script: Script = sqlx::query_as::<_, Script>(
        r#"INSERT INTO office_script.scripts
               (owner_id, name, description, file_id, timeout_secs, memory_limit_mb)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, owner_id, name, description, file_id, compiled_code, compile_error,
                     timeout_secs, memory_limit_mb, run_count, last_run_at, last_run_status,
                     is_starred, is_trashed, created_at, updated_at"#,
    )
    .bind(user.id)
    .bind(&name)
    .bind(&dto.description)
    .bind(file_id)
    .bind(timeout)
    .bind(memory)
    .fetch_one(&state.db)
    .await?;

    script.source_code = source_code;
    Ok(Json(json!({ "script": script })))
}

// ── Get ───────────────────────────────────────────────────────────────────────

pub async fn get(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let script = fetch_script(&state, id, user.id).await?;
    Ok(Json(json!({ "script": script })))
}

// ── Update ────────────────────────────────────────────────────────────────────

pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateScriptDto>,
) -> Result<Json<Value>> {
    let existing = fetch_script(&state, id, user.id).await?;

    let name        = dto.name.as_deref().unwrap_or(&existing.name).to_string();
    let source_code = dto.source_code.as_deref().unwrap_or(&existing.source_code).to_string();
    let timeout     = dto.timeout_secs.unwrap_or(existing.timeout_secs);
    let memory      = dto.memory_limit_mb.unwrap_or(existing.memory_limit_mb);
    let description = dto.description.or(existing.description);

    // If source code changed, clear compiled code
    let compiled_code = if dto.source_code.is_some() {
        None::<String>
    } else {
        existing.compiled_code
    };

    // Source modifiée → écrite dans le fichier .kbscr (créé si absent).
    let file_id = match existing.file_id {
        Some(fid) => {
            if dto.source_code.is_some() {
                content_files::write_script_source(&state, user.id, fid, &source_code).await?;
            }
            fid
        }
        None => content_files::create_script_file(&state, user.id, &name, &source_code).await?,
    };

    let mut script: Script = sqlx::query_as::<_, Script>(
        r#"UPDATE office_script.scripts
           SET name = $3, description = $4, file_id = $5, compiled_code = $6,
               compile_error = NULL, timeout_secs = $7, memory_limit_mb = $8,
               is_starred = COALESCE($9, is_starred)
           WHERE id = $1 AND owner_id = $2
           RETURNING id, owner_id, name, description, file_id, compiled_code, compile_error,
                     timeout_secs, memory_limit_mb, run_count, last_run_at, last_run_status,
                     is_starred, is_trashed, created_at, updated_at"#,
    )
    .bind(id)
    .bind(user.id)
    .bind(&name)
    .bind(&description)
    .bind(file_id)
    .bind(&compiled_code)
    .bind(timeout)
    .bind(memory)
    .bind(dto.is_starred)
    .fetch_one(&state.db)
    .await?;

    // Nom modifié → renommer le fichier .kbscr (nom = nom du fichier). Best-effort.
    if let (true, Some(fid)) = (dto.name.is_some(), script.file_id) {
        if !name.trim().is_empty() {
            if let Err(e) = content_files::rename_content_file(&state, user.id, fid, &name, "kbscr").await {
                tracing::warn!(error = %e, script = %id, "rename .kbscr (nom) échoué");
            }
        }
    }

    script.source_code = source_code;
    Ok(Json(json!({ "script": script })))
}

// ── Delete ────────────────────────────────────────────────────────────────────

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let affected = sqlx::query(
        "DELETE FROM office_script.scripts WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(OfficeError::NotFound(format!("Script {id} introuvable")));
    }
    Ok(Json(json!({ "deleted": true })))
}

// ── Trash ─────────────────────────────────────────────────────────────────────

pub async fn trash(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let affected = sqlx::query(
        "UPDATE office_script.scripts SET is_trashed = TRUE WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(OfficeError::NotFound(format!("Script {id} introuvable")));
    }
    Ok(Json(json!({ "trashed": true })))
}

// ── Restore ───────────────────────────────────────────────────────────────────

pub async fn restore(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query(
        "UPDATE office_script.scripts SET is_trashed = FALSE WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({ "restored": true })))
}

// ── Duplicate ─────────────────────────────────────────────────────────────────

pub async fn duplicate(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let existing = fetch_script(&state, id, user.id).await?;

    let new_name = format!("{} (copie)", existing.name);
    let new_file_id = content_files::create_script_file(&state, user.id, &new_name, &existing.source_code).await?;

    let mut script: Script = sqlx::query_as::<_, Script>(
        r#"INSERT INTO office_script.scripts
               (owner_id, name, description, file_id, timeout_secs, memory_limit_mb)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, owner_id, name, description, file_id, compiled_code, compile_error,
                     timeout_secs, memory_limit_mb, run_count, last_run_at, last_run_status,
                     is_starred, is_trashed, created_at, updated_at"#,
    )
    .bind(user.id)
    .bind(&new_name)
    .bind(&existing.description)
    .bind(new_file_id)
    .bind(existing.timeout_secs)
    .bind(existing.memory_limit_mb)
    .fetch_one(&state.db)
    .await?;

    script.source_code = existing.source_code;
    Ok(Json(json!({ "script": script })))
}

// ── Compile ───────────────────────────────────────────────────────────────────

pub async fn compile(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let existing = fetch_script(&state, id, user.id).await?;

    // Strip TypeScript annotations — our "compilation" step
    let compiled = strip_typescript(&existing.source_code);

    sqlx::query(
        r#"UPDATE office_script.scripts
           SET compiled_code = $3, compile_error = NULL
           WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id)
    .bind(user.id)
    .bind(&compiled)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({ "compiled": true, "compiled_code": compiled })))
}

// ── open-by-file ──────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct OpenByFileDto {
    pub file_id: Uuid,
}

pub async fn open_by_file(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<OpenByFileDto>,
) -> Result<Json<Value>> {
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id FROM office_script.scripts
           WHERE file_id = $1 AND owner_id = $2 AND is_trashed = FALSE"#,
    )
    .bind(dto.file_id).bind(user.id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Aucun script lié au fichier {}", dto.file_id)))?;

    let script = fetch_script(&state, id, user.id).await?;
    Ok(Json(json!({ "script": script })))
}

// ── Helper ────────────────────────────────────────────────────────────────────

pub async fn fetch_script(state: &AppState, id: Uuid, owner_id: Uuid) -> Result<Script> {
    let mut script = sqlx::query_as::<_, Script>(
        r#"SELECT id, owner_id, name, description, file_id, compiled_code, compile_error,
                  timeout_secs, memory_limit_mb, run_count, last_run_at, last_run_status,
                  is_starred, is_trashed, created_at, updated_at
           FROM office_script.scripts WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id)
    .bind(owner_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Script {id} introuvable")))?;

    // Source lue depuis le fichier .kbscr.
    if let Some(fid) = script.file_id {
        script.source_code = content_files::read_script_source(state, owner_id, fid).await
            .unwrap_or_default();
        // Nom = nom du fichier .kbscr (sans extension) ; self-heal si renommé ailleurs.
        if let Some(fname) = content_files::file_name(state, owner_id, fid).await {
            let stem = content_files::strip_ext(&fname);
            if !stem.is_empty() && stem != script.name {
                sqlx::query("UPDATE office_script.scripts SET name = $2 WHERE id = $1")
                    .bind(id).bind(&stem).execute(&state.db).await?;
                script.name = stem;
            }
        }
    }
    Ok(script)
}
