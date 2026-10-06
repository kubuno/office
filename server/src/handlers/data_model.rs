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
    state::AppState,
};

/// Retourne le modèle sémantique complet : datasets + relations + mesures.
pub async fn get_model(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
) -> Result<Json<Value>> {
    let mut datasets: Vec<Dataset> = sqlx::query_as::<_, Dataset>(
        r#"SELECT id, owner_id, datasource_id, name, description, file_id, row_count,
                  last_refresh_at, refresh_error, refresh_schedule, status,
                  created_at, updated_at
           FROM office_data.datasets WHERE owner_id = $1 ORDER BY name"#,
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;

    // Schéma/définition peuplés depuis les fichiers .kbdst (pour le modèle sémantique).
    for ds in &mut datasets {
        if let Some(fid) = ds.file_id {
            if let Ok(content) = crate::services::content_files::read_kb(&state, user.id, fid).await {
                ds.raw_sql = content.get("raw_sql").and_then(|v| v.as_str()).map(|s| s.to_string());
                ds.query_steps = content.get("query_steps").cloned().unwrap_or_else(|| json!([]));
                ds.schema_cache = content.get("schema").cloned().unwrap_or_else(|| json!([]));
            }
        }
    }

    let relations: Vec<Relation> = sqlx::query_as::<_, Relation>(
        r#"SELECT id, owner_id, from_dataset_id, from_column,
                  to_dataset_id, to_column, cardinality, cross_filter, is_active, created_at
           FROM office_data.relations WHERE owner_id = $1"#,
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;

    let measures: Vec<Measure> = sqlx::query_as::<_, Measure>(
        r#"SELECT id, owner_id, dataset_id, name, description, expression,
                  result_type, format_string, display_folder, is_valid, compile_error,
                  created_at, updated_at
           FROM office_data.measures WHERE owner_id = $1"#,
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({
        "datasets": datasets,
        "relations": relations,
        "measures": measures,
    })))
}

pub async fn create_relation(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<CreateRelationDto>,
) -> Result<Json<Value>> {
    // Vérifier que les datasets appartiennent à l'utilisateur
    for ds_id in [dto.from_dataset_id, dto.to_dataset_id] {
        let exists: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM office_data.datasets WHERE id = $1 AND owner_id = $2",
        )
        .bind(ds_id)
        .bind(user.id)
        .fetch_optional(&state.db)
        .await?;

        if exists.is_none() {
            return Err(OfficeError::NotFound(format!("Dataset {ds_id} introuvable")));
        }
    }

    let row: Relation = sqlx::query_as::<_, Relation>(
        r#"INSERT INTO office_data.relations
               (owner_id, from_dataset_id, from_column, to_dataset_id, to_column, cardinality, cross_filter)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           RETURNING id, owner_id, from_dataset_id, from_column,
                     to_dataset_id, to_column, cardinality, cross_filter, is_active, created_at"#,
    )
    .bind(user.id)
    .bind(dto.from_dataset_id)
    .bind(&dto.from_column)
    .bind(dto.to_dataset_id)
    .bind(&dto.to_column)
    .bind(dto.cardinality.as_deref().unwrap_or("many_to_one"))
    .bind(dto.cross_filter.as_deref().unwrap_or("single"))
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "relation": row })))
}

pub async fn update_relation(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateRelationDto>,
) -> Result<Json<Value>> {
    let existing: Relation = sqlx::query_as::<_, Relation>(
        r#"SELECT id, owner_id, from_dataset_id, from_column,
                  to_dataset_id, to_column, cardinality, cross_filter, is_active, created_at
           FROM office_data.relations WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Relation {id} introuvable")))?;

    let cardinality  = dto.cardinality.as_deref().unwrap_or(&existing.cardinality).to_string();
    let cross_filter = dto.cross_filter.as_deref().unwrap_or(&existing.cross_filter).to_string();
    let is_active    = dto.is_active.unwrap_or(existing.is_active);

    let row: Relation = sqlx::query_as::<_, Relation>(
        r#"UPDATE office_data.relations
           SET cardinality = $3, cross_filter = $4, is_active = $5
           WHERE id = $1 AND owner_id = $2
           RETURNING id, owner_id, from_dataset_id, from_column,
                     to_dataset_id, to_column, cardinality, cross_filter, is_active, created_at"#,
    )
    .bind(id)
    .bind(user.id)
    .bind(&cardinality)
    .bind(&cross_filter)
    .bind(is_active)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "relation": row })))
}

pub async fn delete_relation(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let affected = sqlx::query(
        "DELETE FROM office_data.relations WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(OfficeError::NotFound(format!("Relation {id} introuvable")));
    }
    Ok(Json(json!({ "deleted": true })))
}
