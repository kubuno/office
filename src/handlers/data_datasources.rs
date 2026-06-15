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

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
) -> Result<Json<Value>> {
    let rows: Vec<Datasource> = sqlx::query_as::<_, Datasource>(
        r#"SELECT id, owner_id, name, description, source_type, config,
                  connection_status, last_tested_at, connection_error, created_at, updated_at
           FROM office_data.datasources
           WHERE owner_id = $1
           ORDER BY created_at DESC"#,
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "datasources": rows })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<CreateDatasourceDto>,
) -> Result<Json<Value>> {
    let config = dto.config.unwrap_or(json!({}));

    let row: Datasource = sqlx::query_as::<_, Datasource>(
        r#"INSERT INTO office_data.datasources
               (owner_id, name, description, source_type, config)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, owner_id, name, description, source_type, config,
                     connection_status, last_tested_at, connection_error, created_at, updated_at"#,
    )
    .bind(user.id)
    .bind(&dto.name)
    .bind(&dto.description)
    .bind(&dto.source_type)
    .bind(&config)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "datasource": row })))
}

pub async fn get(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let row: Option<Datasource> = sqlx::query_as::<_, Datasource>(
        r#"SELECT id, owner_id, name, description, source_type, config,
                  connection_status, last_tested_at, connection_error, created_at, updated_at
           FROM office_data.datasources
           WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?;

    match row {
        Some(ds) => Ok(Json(json!({ "datasource": ds }))),
        None => Err(OfficeError::NotFound(format!("Datasource {id} introuvable"))),
    }
}

pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateDatasourceDto>,
) -> Result<Json<Value>> {
    let existing: Datasource = sqlx::query_as::<_, Datasource>(
        r#"SELECT id, owner_id, name, description, source_type, config,
                  connection_status, last_tested_at, connection_error, created_at, updated_at
           FROM office_data.datasources WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Datasource {id} introuvable")))?;

    let name   = dto.name.as_deref().unwrap_or(&existing.name).to_string();
    let config = dto.config.unwrap_or_else(|| existing.config.clone());

    let row: Datasource = sqlx::query_as::<_, Datasource>(
        r#"UPDATE office_data.datasources
           SET name = $3, description = $4, config = $5
           WHERE id = $1 AND owner_id = $2
           RETURNING id, owner_id, name, description, source_type, config,
                     connection_status, last_tested_at, connection_error, created_at, updated_at"#,
    )
    .bind(id)
    .bind(user.id)
    .bind(&name)
    .bind(dto.description.as_deref().or(existing.description.as_deref()))
    .bind(&config)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "datasource": row })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let affected = sqlx::query(
        "DELETE FROM office_data.datasources WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(OfficeError::NotFound(format!("Datasource {id} introuvable")));
    }

    Ok(Json(json!({ "deleted": true })))
}

pub async fn test_connection(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let ds: Datasource = sqlx::query_as::<_, Datasource>(
        r#"SELECT id, owner_id, name, description, source_type, config,
                  connection_status, last_tested_at, connection_error, created_at, updated_at
           FROM office_data.datasources WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Datasource {id} introuvable")))?;

    let (ok, error) = match ds.source_type.as_str() {
        "internal" => {
            // Test la connexion au pool interne
            match sqlx::query("SELECT 1").execute(&state.db).await {
                Ok(_)  => (true, None),
                Err(e) => (false, Some(e.to_string())),
            }
        }
        "postgres" => {
            let url = ds.config.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if url.is_empty() {
                (false, Some("URL de connexion manquante".to_string()))
            } else {
                match sqlx::postgres::PgPoolOptions::new()
                    .max_connections(1)
                    .acquire_timeout(std::time::Duration::from_secs(5))
                    .connect(url)
                    .await
                {
                    Ok(pool) => {
                        let ok = sqlx::query("SELECT 1").execute(&pool).await.is_ok();
                        pool.close().await;
                        if ok { (true, None) } else { (false, Some("Connexion établie mais requête échouée".to_string())) }
                    }
                    Err(e) => (false, Some(e.to_string())),
                }
            }
        }
        _ => (true, Some("Type de source non testé automatiquement".to_string())),
    };

    let status = if ok { "ok" } else { "error" };
    sqlx::query(
        "UPDATE office_data.datasources SET connection_status = $3, last_tested_at = NOW(), connection_error = $4 WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .bind(status)
    .bind(&error)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({ "ok": ok, "error": error })))
}
