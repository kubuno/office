use axum::{
    extract::State,
    Extension, Json,
};
use serde_json::{json, Value};

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    models::data::*,
    services::{content_files, data_engine},
    state::AppState,
};

/// Lit (data_cache, raw_sql) d'un dataset depuis son fichier .kbdst.
async fn dataset_content(state: &AppState, dataset_id: uuid::Uuid, owner_id: uuid::Uuid)
    -> Result<(Option<Value>, Option<String>)>
{
    let file_id: Option<Option<uuid::Uuid>> = sqlx::query_scalar(
        "SELECT file_id FROM office_data.datasets WHERE id = $1 AND owner_id = $2",
    )
    .bind(dataset_id)
    .bind(owner_id)
    .fetch_optional(&state.db)
    .await?;

    let file_id = file_id.ok_or_else(|| OfficeError::NotFound(format!("Dataset {dataset_id} introuvable")))?;
    match file_id {
        Some(fid) => {
            let content = content_files::read_kb(state, owner_id, fid).await.unwrap_or(Value::Null);
            let data_cache = content.get("data_cache").cloned().filter(|v| !v.is_null());
            let raw_sql    = content.get("raw_sql").and_then(|v| v.as_str()).map(|s| s.to_string());
            Ok((data_cache, raw_sql))
        }
        None => Ok((None, None)),
    }
}

/// Execute une requête d'agrégation pour alimenter un widget.
pub async fn execute(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<ExecuteQueryDto>,
) -> Result<Json<Value>> {
    // Définition + cache lus depuis le fichier .kbdst.
    let (data_cache, raw_sql) = dataset_content(&state, dto.dataset_id, user.id).await?;
    let dimensions = dto.dimensions.as_deref().unwrap_or(&[]);
    let metrics    = dto.metrics.as_deref().unwrap_or(&[]);
    let filters    = dto.filters.as_deref().unwrap_or(&[]);
    let sort       = dto.sort.as_deref().unwrap_or(&[]);
    let limit      = dto.limit.unwrap_or(500).min(10_000);

    // Si données en cache, les utiliser
    if let Some(Value::Array(rows)) = &data_cache {
        let result = execute_on_memory(rows, dimensions, metrics, filters, sort, limit);
        return Ok(Json(result));
    }

    // Sinon, construire et exécuter la requête SQL
    let base_sql = raw_sql.as_deref().unwrap_or("SELECT 1 WHERE FALSE");
    let widget_sql = data_engine::build_widget_query(
        base_sql,
        dimensions,
        metrics,
        filters,
        sort,
        limit,
    );

    let (columns, rows) = data_engine::execute_sql_on_pool(&state.db, &widget_sql, limit)
        .await
        .map_err(|e| OfficeError::Validation(format!("Erreur d'exécution: {e}")))?;

    Ok(Json(json!({
        "columns": columns,
        "rows": rows,
        "total": rows.len(),
    })))
}

/// Évalue une mesure sur le dataset en cache.
pub async fn evaluate_measure(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(body): Json<Value>,
) -> Result<Json<Value>> {
    let dataset_id = body.get("dataset_id")
        .and_then(|v| v.as_str())
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .ok_or_else(|| OfficeError::Validation("dataset_id manquant".to_string()))?;

    let measure_id = body.get("measure_id")
        .and_then(|v| v.as_str())
        .and_then(|s| uuid::Uuid::parse_str(s).ok());

    let expression = if let Some(mid) = measure_id {
        let m: Option<(String,)> = sqlx::query_as(
            "SELECT expression FROM office_data.measures WHERE id = $1 AND owner_id = $2",
        )
        .bind(mid)
        .bind(user.id)
        .fetch_optional(&state.db)
        .await?;
        m.map(|(e,)| e)
            .ok_or_else(|| OfficeError::NotFound(format!("Mesure {mid} introuvable")))?
    } else {
        body.get("expression")
            .and_then(|v| v.as_str())
            .ok_or_else(|| OfficeError::Validation("expression ou measure_id requis".to_string()))?
            .to_string()
    };

    let (data_cache, _raw_sql) = dataset_content(&state, dataset_id, user.id).await?;

    let rows = match data_cache {
        Some(Value::Array(r)) => r,
        _ => return Err(OfficeError::Validation("Dataset non chargé. Rafraîchissez d'abord le dataset.".to_string())),
    };

    match data_engine::evaluate_measure_on_rows(&expression, &rows) {
        Ok(value)  => Ok(Json(json!({ "value": value }))),
        Err(e)     => Err(OfficeError::Validation(e.to_string())),
    }
}

// ── In-memory execution ───────────────────────────────────────────────────────

fn execute_on_memory(
    rows: &[Value],
    dimensions: &[String],
    metrics: &[MetricSpec],
    filters: &[FilterSpec],
    sort: &[SortSpec],
    limit: i64,
) -> Value {
    // Filtrer
    let mut filtered: Vec<&Value> = rows.iter().filter(|row| {
        filters.iter().all(|f| apply_filter(row, f))
    }).collect();

    // Si dimensions + metrics : grouper
    if !dimensions.is_empty() && !metrics.is_empty() {
        let grouped = group_by_memory(&filtered, dimensions, metrics);
        let mut result_rows: Vec<Value> = grouped;

        // Trier
        apply_sort_memory(&mut result_rows, sort);
        result_rows.truncate(limit as usize);

        let metric_cols: Vec<String> = metrics.iter()
            .map(|m| m.alias.as_deref().unwrap_or(&m.column).to_string())
            .collect();
        let columns: Vec<Value> = dimensions.iter().map(|s| s.as_str())
            .chain(metric_cols.iter().map(|s| s.as_str()))
            .map(|c| json!(c))
            .collect();
        let total = result_rows.len();
        return json!({ "columns": columns, "rows": result_rows, "total": total });
    }

    // Pas de groupement : retourner les lignes filtrées
    let mut result_rows: Vec<Value> = filtered.iter().map(|v| (*v).clone()).collect();
    apply_sort_memory(&mut result_rows, sort);
    result_rows.truncate(limit as usize);

    let total = result_rows.len();
    json!({ "columns": [], "rows": result_rows, "total": total })
}

fn apply_filter(row: &Value, f: &FilterSpec) -> bool {
    let cell = row.get(&f.column);
    match f.operator.as_str() {
        "is_null"     => cell.map(|v| v.is_null()).unwrap_or(true),
        "is_not_null" => cell.map(|v| !v.is_null()).unwrap_or(false),
        "eq" | "="    => cell.map(|v| v == &f.value).unwrap_or(false),
        "neq" | "!="  => cell.map(|v| v != &f.value).unwrap_or(false),
        "gt" | ">"    => compare_values(cell, &f.value) > 0,
        "gte" | ">="  => compare_values(cell, &f.value) >= 0,
        "lt" | "<"    => compare_values(cell, &f.value) < 0,
        "lte" | "<="  => compare_values(cell, &f.value) <= 0,
        "like"        => {
            let pattern = f.value.as_str().unwrap_or("").to_lowercase();
            let clean = pattern.trim_matches('%');
            cell.and_then(|v| v.as_str())
                .map(|s| s.to_lowercase().contains(clean))
                .unwrap_or(false)
        }
        _ => true,
    }
}

fn compare_values(a: Option<&Value>, b: &Value) -> i8 {
    match (a.and_then(|v| v.as_f64()), b.as_f64()) {
        (Some(av), Some(bv)) => {
            if av > bv { 1 } else if av < bv { -1 } else { 0 }
        }
        _ => 0,
    }
}

fn group_by_memory(
    rows: &[&Value],
    dimensions: &[String],
    metrics: &[MetricSpec],
) -> Vec<Value> {
    use std::collections::HashMap;

    let mut groups: HashMap<Vec<String>, Vec<&Value>> = HashMap::new();

    for row in rows {
        let key: Vec<String> = dimensions.iter()
            .map(|d| row.get(d).map(|v| v.to_string()).unwrap_or_default())
            .collect();
        groups.entry(key).or_default().push(row);
    }

    groups.into_iter().map(|(key_vals, group_rows)| {
        let mut obj = serde_json::Map::new();

        for (d, v) in dimensions.iter().zip(key_vals.iter()) {
            // Try to parse as number
            if let Ok(n) = v.parse::<f64>() {
                obj.insert(d.clone(), json!(n));
            } else {
                obj.insert(d.clone(), json!(v));
            }
        }

        for m in metrics {
            let alias = m.alias.as_deref().unwrap_or(&m.column).to_string();
            let col = &m.column;
            let vals: Vec<f64> = group_rows.iter()
                .filter_map(|r| r.get(col).and_then(|v| v.as_f64()))
                .collect();

            let agg_val = match m.function.to_uppercase().as_str() {
                "SUM" => vals.iter().sum(),
                "COUNT" => vals.len() as f64,
                "AVG" | "AVERAGE" => if vals.is_empty() { 0.0 } else { vals.iter().sum::<f64>() / vals.len() as f64 },
                "MIN" => vals.iter().copied().fold(f64::INFINITY, f64::min),
                "MAX" => vals.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                _ => vals.iter().sum(),
            };

            obj.insert(alias, json!(agg_val));
        }

        Value::Object(obj)
    }).collect()
}

fn apply_sort_memory(rows: &mut Vec<Value>, sort: &[SortSpec]) {
    if sort.is_empty() { return; }

    rows.sort_by(|a, b| {
        for s in sort {
            let av = a.get(&s.column).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let bv = b.get(&s.column).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let cmp = av.partial_cmp(&bv).unwrap_or(std::cmp::Ordering::Equal);
            let cmp = if s.direction.to_uppercase() == "DESC" { cmp.reverse() } else { cmp };
            if cmp != std::cmp::Ordering::Equal {
                return cmp;
            }
        }
        std::cmp::Ordering::Equal
    });
}
