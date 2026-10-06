use axum::{
    extract::{Path, State},
    Extension, Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    models::data::*,
    services::data_engine,
    state::AppState,
};

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
) -> Result<Json<Value>> {
    let rows: Vec<Measure> = sqlx::query_as::<_, Measure>(
        r#"SELECT id, owner_id, dataset_id, name, description, expression,
                  result_type, format_string, display_folder, is_valid, compile_error,
                  created_at, updated_at
           FROM office_data.measures
           WHERE owner_id = $1
           ORDER BY dataset_id, name"#,
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "measures": rows })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<CreateMeasureDto>,
) -> Result<Json<Value>> {
    // Valider l'expression
    let (is_valid, compile_error) = match data_engine::validate_measure_expression(&dto.expression) {
        Ok(()) => (true, None),
        Err(e) => (false, Some(e)),
    };

    let row: Measure = sqlx::query_as::<_, Measure>(
        r#"INSERT INTO office_data.measures
               (owner_id, dataset_id, name, description, expression,
                result_type, format_string, display_folder, is_valid, compile_error)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
           RETURNING id, owner_id, dataset_id, name, description, expression,
                     result_type, format_string, display_folder, is_valid, compile_error,
                     created_at, updated_at"#,
    )
    .bind(user.id)
    .bind(dto.dataset_id)
    .bind(&dto.name)
    .bind(&dto.description)
    .bind(&dto.expression)
    .bind(dto.result_type.as_deref().unwrap_or("number"))
    .bind(&dto.format_string)
    .bind(&dto.display_folder)
    .bind(is_valid)
    .bind(&compile_error)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "measure": row })))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateMeasureDto>,
) -> Result<Json<Value>> {
    let existing: Measure = sqlx::query_as::<_, Measure>(
        r#"SELECT id, owner_id, dataset_id, name, description, expression,
                  result_type, format_string, display_folder, is_valid, compile_error,
                  created_at, updated_at
           FROM office_data.measures WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Mesure {id} introuvable")))?;

    let name       = dto.name.as_deref().unwrap_or(&existing.name).to_string();
    let expression = dto.expression.as_deref().unwrap_or(&existing.expression).to_string();
    let result_type = dto.result_type.as_deref().unwrap_or(&existing.result_type).to_string();
    let format_string = dto.format_string.or(existing.format_string);
    let display_folder = dto.display_folder.or(existing.display_folder);
    let description = dto.description.or(existing.description);

    let (is_valid, compile_error) = match data_engine::validate_measure_expression(&expression) {
        Ok(()) => (true, None),
        Err(e) => (false, Some(e)),
    };

    let row: Measure = sqlx::query_as::<_, Measure>(
        r#"UPDATE office_data.measures
           SET name = $3, description = $4, expression = $5, result_type = $6,
               format_string = $7, display_folder = $8, is_valid = $9, compile_error = $10
           WHERE id = $1 AND owner_id = $2
           RETURNING id, owner_id, dataset_id, name, description, expression,
                     result_type, format_string, display_folder, is_valid, compile_error,
                     created_at, updated_at"#,
    )
    .bind(id)
    .bind(user.id)
    .bind(&name)
    .bind(&description)
    .bind(&expression)
    .bind(&result_type)
    .bind(&format_string)
    .bind(&display_folder)
    .bind(is_valid)
    .bind(&compile_error)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "measure": row })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let affected = sqlx::query(
        "DELETE FROM office_data.measures WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(OfficeError::NotFound(format!("Mesure {id} introuvable")));
    }
    Ok(Json(json!({ "deleted": true })))
}

pub async fn validate(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<ValidateMeasureDto>,
) -> Result<Json<Value>> {
    // Vérifier que le dataset appartient à l'utilisateur
    let exists: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM office_data.datasets WHERE id = $1 AND owner_id = $2",
    )
    .bind(dto.dataset_id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?;

    if exists.is_none() {
        return Err(OfficeError::NotFound(format!("Dataset {} introuvable", dto.dataset_id)));
    }

    match data_engine::validate_measure_expression(&dto.expression) {
        Ok(()) => {
            // Tenter d'évaluer sur un échantillon (cache lu depuis le fichier .kbdst).
            let file_id: Option<Option<Uuid>> = sqlx::query_scalar(
                "SELECT file_id FROM office_data.datasets WHERE id = $1",
            )
            .bind(dto.dataset_id)
            .fetch_optional(&state.db)
            .await?;

            let preview: Option<Value> = match file_id.flatten() {
                Some(fid) => {
                    let content = crate::services::content_files::read_kb(&state, user.id, fid).await.unwrap_or(Value::Null);
                    match content.get("data_cache").cloned() {
                        Some(Value::Array(rows)) => data_engine::evaluate_measure_on_rows(&dto.expression, &rows).ok(),
                        _ => None,
                    }
                }
                None => None,
            };

            Ok(Json(json!({ "valid": true, "preview": preview })))
        }
        Err(e) => Ok(Json(json!({ "valid": false, "error": e }))),
    }
}
