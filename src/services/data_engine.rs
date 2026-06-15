use anyhow::{anyhow, Result};
use serde_json::Value;
use sqlx::PgPool;

use crate::models::data::{FilterSpec, MetricSpec, QueryStep, SortSpec};

// ── Query Step → SQL ──────────────────────────────────────────────────────────

/// Convertit un pipeline de steps JSON en SQL via des CTEs PostgreSQL.
pub fn steps_to_sql(steps: &[QueryStep]) -> Result<String> {
    if steps.is_empty() {
        return Ok("SELECT 1 WHERE FALSE".to_string());
    }

    let mut ctes: Vec<String> = Vec::new();
    let mut prev_cte = "cte_0".to_string();

    for (i, step) in steps.iter().enumerate() {
        let cte_name = format!("cte_{}", i + 1);
        let sql = match step {
            QueryStep::Source { sql, .. } => {
                let base = format!("cte_0 AS (\n  {sql}\n)");
                ctes.push(base);
                prev_cte = "cte_0".to_string();
                continue;
            }
            QueryStep::Filter { column, operator, value } => {
                let op = normalize_operator(operator);
                let val = value_to_sql_literal(value);
                format!("{cte_name} AS (\n  SELECT * FROM {prev_cte}\n  WHERE {column} {op} {val}\n)")
            }
            QueryStep::Sort { column, direction } => {
                let dir = if direction.to_uppercase() == "DESC" { "DESC" } else { "ASC" };
                format!("{cte_name} AS (\n  SELECT * FROM {prev_cte}\n  ORDER BY {column} {dir}\n)")
            }
            QueryStep::Group { by, aggregations } => {
                let by_cols = by.join(", ");
                let agg_exprs: Vec<String> = aggregations.iter().map(|a| {
                    let alias = a.alias.as_deref().unwrap_or(&a.column);
                    format!("{}({}) AS {}", agg_fn(&a.function), a.column, alias)
                }).collect();
                let select = if agg_exprs.is_empty() {
                    by_cols.clone()
                } else {
                    format!("{by_cols}, {}", agg_exprs.join(", "))
                };
                format!("{cte_name} AS (\n  SELECT {select}\n  FROM {prev_cte}\n  GROUP BY {by_cols}\n)")
            }
            QueryStep::AddColumn { name, expression } => {
                format!("{cte_name} AS (\n  SELECT *, ({expression}) AS {name}\n  FROM {prev_cte}\n)")
            }
            QueryStep::Rename { from, to } => {
                format!("{cte_name} AS (\n  SELECT *, {from} AS {to}\n  FROM {prev_cte}\n)")
            }
            QueryStep::RemoveColumns { columns } => {
                // PostgreSQL ne supporte pas SELECT * EXCEPT, on utilise une approche simple
                // En pratique, l'utilisateur devra lister les colonnes à conserver
                let excluded: Vec<String> = columns.iter().map(|c| format!("-- removed: {c}")).collect();
                format!("{cte_name} AS (\n  SELECT * FROM {prev_cte} -- {}\n)", excluded.join(", "))
            }
            QueryStep::ChangeType { column, data_type } => {
                let pg_type = m_type_to_pg(data_type);
                format!("{cte_name} AS (\n  SELECT *, {column}::{pg_type} AS {column}\n  FROM {prev_cte}\n)")
            }
            QueryStep::Limit { count } => {
                format!("{cte_name} AS (\n  SELECT * FROM {prev_cte}\n  LIMIT {count}\n)")
            }
            QueryStep::Join { left_column, right_column, join_type, .. } => {
                let jt = match join_type.to_lowercase().as_str() {
                    "left" | "left_outer" => "LEFT JOIN",
                    "right" | "right_outer" => "RIGHT JOIN",
                    "full" | "full_outer" => "FULL OUTER JOIN",
                    _ => "INNER JOIN",
                };
                // Note: join avec un autre dataset nécessite que les données soient
                // chargées. Ici on génère un placeholder car le dataset référencé
                // doit être matérialisé séparément.
                format!("{cte_name} AS (\n  SELECT {prev_cte}.* FROM {prev_cte}\n  {jt} (SELECT * FROM cte_join_right) joined_right\n  ON {prev_cte}.{left_column} = joined_right.{right_column}\n)")
            }
        };
        ctes.push(sql);
        prev_cte = cte_name;
    }

    if ctes.is_empty() {
        return Ok("SELECT 1 WHERE FALSE".to_string());
    }

    Ok(format!("WITH {}\nSELECT * FROM {prev_cte}", ctes.join(",\n")))
}

fn normalize_operator(op: &str) -> &str {
    match op {
        "eq" | "=" => "=",
        "neq" | "!=" => "!=",
        "gt" | ">" => ">",
        "gte" | ">=" => ">=",
        "lt" | "<" => "<",
        "lte" | "<=" => "<=",
        "like" => "ILIKE",
        "not_like" => "NOT ILIKE",
        "is_null" => "IS NULL",
        "is_not_null" => "IS NOT NULL",
        other => other,
    }
}

fn value_to_sql_literal(v: &Value) -> String {
    match v {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(value_to_sql_literal).collect();
            format!("({})", items.join(", "))
        }
        _ => "NULL".to_string(),
    }
}

fn agg_fn(f: &str) -> &str {
    match f.to_uppercase().as_str() {
        "SUM" => "SUM",
        "COUNT" => "COUNT",
        "AVG" | "AVERAGE" => "AVG",
        "MIN" => "MIN",
        "MAX" => "MAX",
        "COUNT_DISTINCT" => "COUNT(DISTINCT",
        _ => "SUM",
    }
}

fn m_type_to_pg(t: &str) -> &str {
    match t.to_lowercase().as_str() {
        "text" | "string" => "TEXT",
        "number" | "float" | "decimal" => "DOUBLE PRECISION",
        "integer" | "int" => "BIGINT",
        "date" => "DATE",
        "datetime" | "timestamp" => "TIMESTAMPTZ",
        "boolean" | "bool" => "BOOLEAN",
        _ => "TEXT",
    }
}

// ── Execute Query pour widgets ─────────────────────────────────────────────────

/// Construit une requête SQL d'agrégation pour un widget à partir de son config.
pub fn build_widget_query(
    base_sql: &str,
    dimensions: &[String],
    metrics: &[MetricSpec],
    filters: &[FilterSpec],
    sort: &[SortSpec],
    limit: i64,
) -> String {
    let mut select_parts: Vec<String> = dimensions.to_vec();

    for m in metrics {
        let fn_name = match m.function.to_uppercase().as_str() {
            "SUM" => "SUM",
            "COUNT" => "COUNT",
            "AVG" | "AVERAGE" => "AVG",
            "MIN" => "MIN",
            "MAX" => "MAX",
            _ => "SUM",
        };
        let alias = m.alias.as_deref().unwrap_or(&m.column);
        if m.function.to_uppercase() == "COUNT_DISTINCT" {
            select_parts.push(format!("COUNT(DISTINCT {}) AS {}", m.column, alias));
        } else {
            select_parts.push(format!("{fn_name}({}) AS {}", m.column, alias));
        }
    }

    if select_parts.is_empty() {
        select_parts.push("*".to_string());
    }

    let select = select_parts.join(", ");

    let mut where_clauses: Vec<String> = Vec::new();
    for f in filters {
        let op = normalize_operator(&f.operator);
        if op == "IS NULL" || op == "IS NOT NULL" {
            where_clauses.push(format!("{} {op}", f.column));
        } else {
            let val = value_to_sql_literal(&f.value);
            where_clauses.push(format!("{} {op} {val}", f.column));
        }
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    let group_sql = if dimensions.is_empty() || metrics.is_empty() {
        String::new()
    } else {
        format!("GROUP BY {}", dimensions.join(", "))
    };

    let sort_sql = if sort.is_empty() {
        String::new()
    } else {
        let parts: Vec<String> = sort.iter().map(|s| {
            let dir = if s.direction.to_uppercase() == "DESC" { "DESC" } else { "ASC" };
            format!("{} {dir}", s.column)
        }).collect();
        format!("ORDER BY {}", parts.join(", "))
    };

    format!(
        "SELECT {select} FROM ({base_sql}) base_data {where_sql} {group_sql} {sort_sql} LIMIT {limit}"
    )
}

// ── Évaluateur de mesures simples ─────────────────────────────────────────────

/// Évalue une expression de mesure simple sur les données du dataset.
/// Format supporté : SUM(col), COUNT(col), AVG(col), MIN(col), MAX(col),
///                   DIVIDE(a, b), DIVIDE(a, b, alt), constante numérique.
pub fn evaluate_measure_on_rows(expression: &str, rows: &[Value]) -> Result<Value> {
    let expr = expression.trim();

    // Fonctions d'agrégation simples
    if let Some(inner) = extract_fn_arg(expr, "SUM") {
        let vals = extract_numeric_col(rows, inner);
        return Ok(Value::Number(
            serde_json::Number::from_f64(vals.iter().sum::<f64>()).unwrap_or(serde_json::Number::from(0))
        ));
    }

    if let Some(inner) = extract_fn_arg(expr, "COUNT") {
        let col = inner.trim();
        let count = rows.iter().filter(|r| {
            r.get(col).map(|v| !v.is_null()).unwrap_or(false)
        }).count();
        return Ok(Value::Number(serde_json::Number::from(count as i64)));
    }

    if let Some(inner) = extract_fn_arg(expr, "COUNTROWS") {
        let _ = inner;
        return Ok(Value::Number(serde_json::Number::from(rows.len() as i64)));
    }

    if let Some(inner) = extract_fn_arg(expr, "AVG") {
        let vals = extract_numeric_col(rows, inner);
        if vals.is_empty() {
            return Ok(Value::Number(serde_json::Number::from(0)));
        }
        let avg = vals.iter().sum::<f64>() / vals.len() as f64;
        return Ok(Value::Number(serde_json::Number::from_f64(avg).unwrap_or(serde_json::Number::from(0))));
    }

    if let Some(inner) = extract_fn_arg(expr, "MIN") {
        let vals = extract_numeric_col(rows, inner);
        let min = vals.iter().copied().fold(f64::INFINITY, f64::min);
        return Ok(Value::Number(serde_json::Number::from_f64(min).unwrap_or(serde_json::Number::from(0))));
    }

    if let Some(inner) = extract_fn_arg(expr, "MAX") {
        let vals = extract_numeric_col(rows, inner);
        let max = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        return Ok(Value::Number(serde_json::Number::from_f64(max).unwrap_or(serde_json::Number::from(0))));
    }

    if let Some(inner) = extract_fn_arg(expr, "DIVIDE") {
        let args: Vec<&str> = inner.splitn(3, ',').map(str::trim).collect();
        if args.len() < 2 {
            return Err(anyhow!("DIVIDE nécessite au moins 2 arguments"));
        }
        let num = eval_simple_value(args[0], rows)?;
        let den = eval_simple_value(args[1], rows)?;
        let alt = if args.len() == 3 { eval_simple_value(args[2], rows)? } else { 0.0 };
        if den == 0.0 {
            return Ok(Value::Number(serde_json::Number::from_f64(alt).unwrap_or(serde_json::Number::from(0))));
        }
        return Ok(Value::Number(serde_json::Number::from_f64(num / den).unwrap_or(serde_json::Number::from(0))));
    }

    // Constante numérique
    if let Ok(n) = expr.parse::<f64>() {
        return Ok(Value::Number(serde_json::Number::from_f64(n).unwrap_or(serde_json::Number::from(0))));
    }

    Err(anyhow!("Expression non reconnue: {expr}"))
}

fn extract_fn_arg<'a>(expr: &'a str, fn_name: &str) -> Option<&'a str> {
    let upper = expr.to_uppercase();
    let prefix = fn_name.to_uppercase();
    if upper.starts_with(&prefix) && upper.as_bytes().get(prefix.len()) == Some(&b'(') && expr.ends_with(')') {
        Some(&expr[prefix.len() + 1..expr.len() - 1])
    } else {
        None
    }
}

fn extract_numeric_col(rows: &[Value], col: &str) -> Vec<f64> {
    let col = col.trim();
    rows.iter().filter_map(|r| {
        r.get(col).and_then(|v| match v {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.parse::<f64>().ok(),
            _ => None,
        })
    }).collect()
}

fn eval_simple_value(expr: &str, rows: &[Value]) -> Result<f64> {
    let expr = expr.trim();
    if let Ok(n) = expr.parse::<f64>() {
        return Ok(n);
    }
    let vals = extract_numeric_col(rows, expr);
    Ok(vals.iter().sum::<f64>())
}

// ── Validate measure expression ───────────────────────────────────────────────

pub fn validate_measure_expression(expression: &str) -> Result<(), String> {
    let expr = expression.trim();
    if expr.is_empty() {
        return Err("L'expression ne peut pas être vide".to_string());
    }

    // Check for known functions
    let known_fns = ["SUM", "COUNT", "COUNTROWS", "AVG", "AVERAGE", "MIN", "MAX", "DIVIDE"];
    let upper = expr.to_uppercase();

    for f in &known_fns {
        if upper.starts_with(f) {
            if upper.as_bytes().get(f.len()) == Some(&b'(') && expr.ends_with(')') {
                return Ok(());
            }
        }
    }

    // Maybe a number
    if expr.parse::<f64>().is_ok() {
        return Ok(());
    }

    Err(format!(
        "Expression non reconnue: '{expr}'. Fonctions disponibles: SUM, COUNT, COUNTROWS, AVG, MIN, MAX, DIVIDE"
    ))
}

// ── Execute on internal pool ──────────────────────────────────────────────────

/// Exécute une requête SQL sur le pool interne (office) et retourne les résultats JSON.
pub async fn execute_sql_on_pool(
    pool: &PgPool,
    sql: &str,
    limit: i64,
) -> Result<(Vec<String>, Vec<Value>)> {
    let limited = format!("SELECT * FROM ({sql}) __q LIMIT {limit}");

    let rows = sqlx::query(&limited).fetch_all(pool).await
        .map_err(|e| anyhow!("Erreur SQL: {e}"))?;

    if rows.is_empty() {
        return Ok((vec![], vec![]));
    }

    use sqlx::{Row as _, Column as _};
    let columns: Vec<String> = rows[0].columns().iter()
        .map(|c| c.name().to_string())
        .collect();

    let data: Vec<Value> = rows.iter().map(|row| {
        use sqlx::{Row as _, Column as _};
        let mut obj = serde_json::Map::new();
        for (i, col) in row.columns().iter().enumerate() {
            let val = extract_pg_value(row, i);
            obj.insert(col.name().to_string(), val);
        }
        Value::Object(obj)
    }).collect();

    Ok((columns, data))
}

fn extract_pg_value(row: &sqlx::postgres::PgRow, i: usize) -> Value {
    use sqlx::Row as _;

    // Try types in order
    if let Ok(v) = row.try_get::<Option<String>, _>(i) {
        return match v {
            Some(s) => Value::String(s),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.try_get::<Option<i64>, _>(i) {
        return match v {
            Some(n) => Value::Number(serde_json::Number::from(n)),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(i) {
        return match v {
            Some(n) => Value::Number(serde_json::Number::from_f64(n).unwrap_or(serde_json::Number::from(0))),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.try_get::<Option<bool>, _>(i) {
        return match v {
            Some(b) => Value::Bool(b),
            None => Value::Null,
        };
    }
    Value::Null
}
