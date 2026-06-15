use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    models::data::*,
    services::{content_files, data_engine},
    state::AppState,
};

const DATASET_COLS: &str = "id, owner_id, datasource_id, name, description, file_id, row_count,
                            last_refresh_at, refresh_error, refresh_schedule, status, created_at, updated_at";

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
) -> Result<Json<Value>> {
    // Liste = métadonnée seule (la définition vit dans le fichier .kbdst).
    let rows: Vec<Dataset> = sqlx::query_as::<_, Dataset>(
        &format!(r#"SELECT {DATASET_COLS}
           FROM office_data.datasets
           WHERE owner_id = $1
           ORDER BY updated_at DESC"#),
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "datasets": rows })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<CreateDatasetDto>,
) -> Result<Json<Value>> {
    let query_steps = dto.query_steps.unwrap_or(json!([]));
    let raw_sql     = dto.raw_sql.clone().map(Value::String).unwrap_or(Value::Null);

    // Définition → fichier .kbdst.
    let content = content_files::dataset_content_from(&raw_sql, &query_steps, &json!([]), &Value::Null);
    let file_id = content_files::create_dataset_file(&state, user.id, &dto.name, &content).await?;

    let mut row: Dataset = sqlx::query_as::<_, Dataset>(
        &format!(r#"INSERT INTO office_data.datasets
               (owner_id, datasource_id, name, description, file_id)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING {DATASET_COLS}"#),
    )
    .bind(user.id)
    .bind(dto.datasource_id)
    .bind(&dto.name)
    .bind(&dto.description)
    .bind(file_id)
    .fetch_one(&state.db)
    .await?;

    row.raw_sql     = dto.raw_sql;
    row.query_steps = query_steps;
    Ok(Json(json!({ "dataset": row })))
}

pub async fn get(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let row = fetch_dataset(&state, id, user.id).await?;
    Ok(Json(json!({ "dataset": row })))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateDatasetDto>,
) -> Result<Json<Value>> {
    let existing = fetch_dataset(&state, id, user.id).await?;

    let name        = dto.name.as_deref().unwrap_or(&existing.name).to_string();
    let raw_sql     = dto.raw_sql.or(existing.raw_sql.clone());
    let query_steps = dto.query_steps.unwrap_or_else(|| existing.query_steps.clone());
    let ds_id       = dto.datasource_id.or(existing.datasource_id);

    // Réécrit la définition dans le fichier (en conservant le cache/schéma existants).
    let file_id = match existing.file_id {
        Some(fid) => {
            let mut content = content_files::read_kb(&state, user.id, fid).await
                .unwrap_or_else(|_| content_files::dataset_content_from(&Value::Null, &json!([]), &json!([]), &Value::Null));
            content["raw_sql"]     = raw_sql.clone().map(Value::String).unwrap_or(Value::Null);
            content["query_steps"] = query_steps.clone();
            content_files::write_kb_gzip(&state, user.id, fid, &content).await?;
            fid
        }
        None => {
            let content = content_files::dataset_content_from(
                &raw_sql.clone().map(Value::String).unwrap_or(Value::Null), &query_steps, &json!([]), &Value::Null);
            content_files::create_dataset_file(&state, user.id, &name, &content).await?
        }
    };

    let mut row: Dataset = sqlx::query_as::<_, Dataset>(
        &format!(r#"UPDATE office_data.datasets
           SET name = $3, description = $4, datasource_id = $5, file_id = $6
           WHERE id = $1 AND owner_id = $2
           RETURNING {DATASET_COLS}"#),
    )
    .bind(id)
    .bind(user.id)
    .bind(&name)
    .bind(dto.description.as_deref().or(existing.description.as_deref()))
    .bind(ds_id)
    .bind(file_id)
    .fetch_one(&state.db)
    .await?;

    // Nom modifié → renommer le fichier .kbdst (nom = nom du fichier). Best-effort.
    if let (true, Some(fid)) = (dto.name.is_some(), row.file_id) {
        if !name.trim().is_empty() {
            if let Err(e) = content_files::rename_content_file(&state, user.id, fid, &name, "kbdst").await {
                tracing::warn!(error = %e, dataset = %id, "rename .kbdst (nom) échoué");
            }
        }
    }

    row.raw_sql     = raw_sql;
    row.query_steps = query_steps;
    Ok(Json(json!({ "dataset": row })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let file_id: Option<Option<Uuid>> = sqlx::query_scalar(
        "DELETE FROM office_data.datasets WHERE id = $1 AND owner_id = $2 RETURNING file_id",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?;

    match file_id {
        Some(fid) => {
            if let Some(fid) = fid {
                let _ = state.files_client.delete_file(user.id, fid).await;
            }
            Ok(Json(json!({ "deleted": true })))
        }
        None => Err(OfficeError::NotFound(format!("Dataset {id} introuvable"))),
    }
}

/// Rafraîchit le dataset : exécute le SQL, met à jour le cache et le schéma (fichier).
pub async fn refresh(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let ds = fetch_dataset(&state, id, user.id).await?;

    sqlx::query("UPDATE office_data.datasets SET status = 'refreshing' WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    let sql = build_sql_for_dataset(&ds)?;
    const MAX_CACHE_ROWS: i64 = 50_000;

    match data_engine::execute_sql_on_pool(&state.db, &sql, MAX_CACHE_ROWS).await {
        Ok((columns, rows)) => {
            let schema: Vec<Value> = columns.iter().map(|c| json!({ "name": c, "type": "text" })).collect();
            let row_count = rows.len() as i64;
            let schema_val = Value::Array(schema.clone());
            let data_cache = Value::Array(rows);

            // Schéma + cache → fichier .kbdst.
            if let Some(fid) = ds.file_id {
                let mut content = content_files::read_kb(&state, user.id, fid).await
                    .unwrap_or_else(|_| content_files::dataset_content_from(&Value::Null, &json!([]), &json!([]), &Value::Null));
                content["schema"]     = schema_val.clone();
                content["data_cache"] = data_cache;
                content_files::write_kb_gzip(&state, user.id, fid, &content).await?;
            }

            sqlx::query(
                r#"UPDATE office_data.datasets
                   SET status = 'ready', last_refresh_at = NOW(), refresh_error = NULL, row_count = $2
                   WHERE id = $1"#,
            )
            .bind(id)
            .bind(row_count)
            .execute(&state.db)
            .await?;

            Ok(Json(json!({ "ok": true, "row_count": row_count, "schema": schema })))
        }
        Err(e) => {
            let err_msg = e.to_string();
            sqlx::query(
                "UPDATE office_data.datasets SET status = 'error', refresh_error = $2 WHERE id = $1",
            )
            .bind(id)
            .bind(&err_msg)
            .execute(&state.db)
            .await?;

            Err(OfficeError::Validation(format!("Erreur d'exécution: {err_msg}")))
        }
    }
}

/// Retourne un aperçu des données (cache du fichier ou exécution live).
pub async fn preview(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Query(q): Query<DatasetPreviewQuery>,
) -> Result<Json<Value>> {
    let ds = fetch_dataset(&state, id, user.id).await?;
    let limit = q.limit.unwrap_or(50).min(500);

    // Cache disponible dans le fichier ?
    if let Some(fid) = ds.file_id {
        if let Ok(content) = content_files::read_kb(&state, user.id, fid).await {
            if let Some(Value::Array(rows)) = content.get("data_cache").cloned() {
                if !rows.is_empty() {
                    let preview_rows: Vec<Value> = rows.into_iter().take(limit as usize).collect();
                    return Ok(Json(json!({
                        "columns": &ds.schema_cache,
                        "rows": preview_rows,
                        "from_cache": true,
                    })));
                }
            }
        }
    }

    // Sinon, exécution live.
    let sql = build_sql_for_dataset(&ds)?;
    let (columns, rows) = data_engine::execute_sql_on_pool(&state.db, &sql, limit).await
        .map_err(|e| OfficeError::Validation(e.to_string()))?;

    Ok(Json(json!({ "columns": columns, "rows": rows, "from_cache": false })))
}

pub async fn validate_m(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Value>> {
    let _ds = fetch_dataset(&state, id, user.id).await?;
    let sql = body.get("sql").and_then(|v| v.as_str()).unwrap_or("");

    if sql.is_empty() {
        return Ok(Json(json!({ "valid": false, "error": "SQL vide" })));
    }

    let explain = format!("EXPLAIN {sql}");
    match sqlx::query(&explain).execute(&state.db).await {
        Ok(_)  => Ok(Json(json!({ "valid": true }))),
        Err(e) => Ok(Json(json!({ "valid": false, "error": e.to_string() }))),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Charge un dataset et peuple raw_sql/query_steps/schema_cache depuis le fichier.
pub async fn fetch_dataset(state: &AppState, id: Uuid, owner_id: Uuid) -> Result<Dataset> {
    let mut ds = sqlx::query_as::<_, Dataset>(
        &format!(r#"SELECT {DATASET_COLS}
           FROM office_data.datasets
           WHERE id = $1 AND owner_id = $2"#),
    )
    .bind(id)
    .bind(owner_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Dataset {id} introuvable")))?;

    if let Some(fid) = ds.file_id {
        if let Ok(content) = content_files::read_kb(state, owner_id, fid).await {
            ds.raw_sql = content.get("raw_sql").and_then(|v| v.as_str()).map(|s| s.to_string());
            ds.query_steps = content.get("query_steps").cloned().unwrap_or_else(|| json!([]));
            ds.schema_cache = content.get("schema").cloned().unwrap_or_else(|| json!([]));
        }
        // Nom = nom du fichier .kbdst (sans extension) ; self-heal si renommé ailleurs.
        if let Some(fname) = content_files::file_name(state, owner_id, fid).await {
            let stem = content_files::strip_ext(&fname);
            if !stem.is_empty() && stem != ds.name {
                sqlx::query("UPDATE office_data.datasets SET name = $2 WHERE id = $1")
                    .bind(id).bind(&stem).execute(&state.db).await?;
                ds.name = stem;
            }
        }
    }
    Ok(ds)
}

fn build_sql_for_dataset(ds: &Dataset) -> Result<String> {
    if let Some(sql) = &ds.raw_sql {
        if !sql.trim().is_empty() {
            return Ok(sql.clone());
        }
    }

    let steps: Vec<QueryStep> = serde_json::from_value(ds.query_steps.clone())
        .map_err(|e| OfficeError::Validation(format!("Steps invalides: {e}")))?;

    if steps.is_empty() {
        return Ok("SELECT 1 WHERE FALSE".to_string());
    }

    data_engine::steps_to_sql(&steps)
        .map_err(|e| OfficeError::Validation(e.to_string()))
}
