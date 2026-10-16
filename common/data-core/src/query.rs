//! An in-memory query engine: filter, group-by with aggregation, sort and limit over a dataset's cached
//! rows. A port of the office server's `execute_on_memory` path (`server/src/handlers/data_execute.rs`),
//! which runs whenever a dataset carries a `data_cache` instead of a live SQL datasource. The desktop app
//! uses it for the sample report and for offline rendering, so a widget resolves its data the same way the
//! server would (the web always calls `POST /office/data/execute`; the result shape is identical).
//!
//! Divergence from the server (intentional, documented): grouping keeps **first-seen order** (the server
//! uses a `HashMap`, so its unsorted order is undefined) and `COUNT_DISTINCT` is supported here (the server
//! only builds it in SQL). Everything else - operators, aggregations, numeric coercion - matches.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// A metric to aggregate (`MetricSpec`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSpec {
    pub column: String,
    pub function: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

/// A filter predicate (`FilterSpec`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterSpec {
    pub column: String,
    pub operator: String,
    #[serde(default)]
    pub value: Value,
}

/// A sort key (`SortSpec`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortSpec {
    pub column: String,
    pub direction: String,
}

/// A query against a dataset's cached rows.
#[derive(Debug, Clone, Default)]
pub struct Query {
    pub dimensions: Vec<String>,
    pub metrics: Vec<MetricSpec>,
    pub filters: Vec<FilterSpec>,
    pub sort: Vec<SortSpec>,
    pub limit: Option<i64>,
}

/// The result of [`execute`], the shape `executeApi.query` returns (`{ columns, rows, total }`).
#[derive(Debug, Clone, PartialEq)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Map<String, Value>>,
    pub total: usize,
}

fn metric_alias(m: &MetricSpec) -> String {
    m.alias.clone().unwrap_or_else(|| m.column.clone())
}

fn compare_values(a: Option<&Value>, b: &Value) -> i8 {
    match (a.and_then(Value::as_f64), b.as_f64()) {
        (Some(av), Some(bv)) => {
            if av > bv {
                1
            } else if av < bv {
                -1
            } else {
                0
            }
        }
        _ => 0,
    }
}

fn apply_filter(row: &Value, f: &FilterSpec) -> bool {
    let cell = row.get(&f.column);
    match f.operator.as_str() {
        "is_null" => cell.map(Value::is_null).unwrap_or(true),
        "is_not_null" => cell.map(|v| !v.is_null()).unwrap_or(false),
        "eq" | "=" => cell.map(|v| v == &f.value).unwrap_or(false),
        "neq" | "!=" => cell.map(|v| v != &f.value).unwrap_or(false),
        "gt" | ">" => compare_values(cell, &f.value) > 0,
        "gte" | ">=" => compare_values(cell, &f.value) >= 0,
        "lt" | "<" => compare_values(cell, &f.value) < 0,
        "lte" | "<=" => compare_values(cell, &f.value) <= 0,
        "in" => match &f.value {
            Value::Array(items) => cell.map(|c| items.contains(c)).unwrap_or(false),
            _ => false,
        },
        "like" => {
            let pattern = f.value.as_str().unwrap_or("").to_lowercase();
            let clean = pattern.trim_matches('%');
            cell.and_then(Value::as_str)
                .map(|s| s.to_lowercase().contains(clean))
                .unwrap_or(false)
        }
        _ => true,
    }
}

fn aggregate(function: &str, vals: &[f64], distinct_non_null: usize) -> f64 {
    match function.to_uppercase().as_str() {
        "SUM" => vals.iter().sum(),
        "COUNT" => vals.len() as f64,
        "COUNT_DISTINCT" => distinct_non_null as f64,
        "AVG" | "AVERAGE" => {
            if vals.is_empty() {
                0.0
            } else {
                vals.iter().sum::<f64>() / vals.len() as f64
            }
        }
        "MIN" => vals.iter().copied().fold(f64::INFINITY, f64::min),
        "MAX" => vals.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        _ => vals.iter().sum(),
    }
}

/// The canonical grouping key for a cell: the raw text of a string, the numeric text of a number, "" for
/// null/absent. (The server's memory path JSON-stringifies the cell, which double-quotes strings and makes
/// the visuals show `"Nord"`; we keep the typed value instead so a grouped label renders like the SQL path
/// and the web expect - a documented divergence from the server's memory-path quirk.)
fn cell_key(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn group_by(rows: &[&Value], dimensions: &[String], metrics: &[MetricSpec]) -> Vec<Map<String, Value>> {
    // First-seen order (deterministic, unlike the server's HashMap).
    let mut order: Vec<Vec<String>> = Vec::new();
    let mut groups: std::collections::HashMap<Vec<String>, Vec<&Value>> = std::collections::HashMap::new();
    for row in rows {
        let key: Vec<String> = dimensions.iter().map(|d| cell_key(row.get(d))).collect();
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(row);
    }

    order
        .into_iter()
        .map(|key_vals| {
            let group_rows = &groups[&key_vals];
            let mut obj = Map::new();
            for d in dimensions {
                // Keep the first row's typed value for this dimension (string stays string, number number).
                let v = group_rows.first().and_then(|r| r.get(d)).cloned().unwrap_or(Value::Null);
                obj.insert(d.clone(), v);
            }
            for m in metrics {
                let vals: Vec<f64> = group_rows
                    .iter()
                    .filter_map(|r| r.get(&m.column).and_then(Value::as_f64))
                    .collect();
                let distinct = {
                    let mut seen: Vec<String> = group_rows
                        .iter()
                        .filter_map(|r| r.get(&m.column))
                        .filter(|v| !v.is_null())
                        .map(|v| v.to_string())
                        .collect();
                    seen.sort();
                    seen.dedup();
                    seen.len()
                };
                obj.insert(metric_alias(m), json!(aggregate(&m.function, &vals, distinct)));
            }
            obj
        })
        .collect()
}

fn apply_sort(rows: &mut [Map<String, Value>], sort: &[SortSpec]) {
    if sort.is_empty() {
        return;
    }
    rows.sort_by(|a, b| {
        for s in sort {
            let av = a.get(&s.column).and_then(Value::as_f64).unwrap_or(0.0);
            let bv = b.get(&s.column).and_then(Value::as_f64).unwrap_or(0.0);
            let mut cmp = av.partial_cmp(&bv).unwrap_or(std::cmp::Ordering::Equal);
            if s.direction.eq_ignore_ascii_case("DESC") {
                cmp = cmp.reverse();
            }
            if cmp != std::cmp::Ordering::Equal {
                return cmp;
            }
        }
        std::cmp::Ordering::Equal
    });
}

/// Run a query over cached rows, returning the `{ columns, rows, total }` the visuals consume.
pub fn execute(rows: &[Value], query: &Query) -> QueryResult {
    let limit = query.limit.unwrap_or(500).clamp(0, 10_000) as usize;
    let filtered: Vec<&Value> = rows.iter().filter(|r| query.filters.iter().all(|f| apply_filter(r, f))).collect();

    if !query.dimensions.is_empty() && !query.metrics.is_empty() {
        let mut result = group_by(&filtered, &query.dimensions, &query.metrics);
        apply_sort(&mut result, &query.sort);
        result.truncate(limit);
        let mut columns: Vec<String> = query.dimensions.clone();
        columns.extend(query.metrics.iter().map(metric_alias));
        let total = result.len();
        return QueryResult { columns, rows: result, total };
    }

    let mut result: Vec<Map<String, Value>> = filtered
        .iter()
        .filter_map(|v| v.as_object().cloned())
        .collect();
    apply_sort(&mut result, &query.sort);
    result.truncate(limit);
    let total = result.len();
    QueryResult { columns: Vec::new(), rows: result, total }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<Value> {
        vec![
            json!({ "region": "Nord", "amount": 100, "orders": 3 }),
            json!({ "region": "Sud", "amount": 50, "orders": 2 }),
            json!({ "region": "Nord", "amount": 25, "orders": 1 }),
            json!({ "region": "Est", "amount": 10, "orders": 1 }),
        ]
    }

    #[test]
    fn group_and_sum_first_seen_order() {
        let q = Query {
            dimensions: vec!["region".into()],
            metrics: vec![MetricSpec { column: "amount".into(), function: "SUM".into(), alias: None }],
            ..Default::default()
        };
        let r = execute(&rows(), &q);
        assert_eq!(r.columns, vec!["region", "amount"]);
        assert_eq!(r.rows.len(), 3);
        assert_eq!(r.rows[0]["region"], json!("Nord"));
        assert_eq!(r.rows[0]["amount"], json!(125.0));
        assert_eq!(r.rows[1]["region"], json!("Sud"));
    }

    #[test]
    fn sort_desc_and_limit() {
        let q = Query {
            dimensions: vec!["region".into()],
            metrics: vec![MetricSpec { column: "amount".into(), function: "SUM".into(), alias: None }],
            sort: vec![SortSpec { column: "amount".into(), direction: "DESC".into() }],
            limit: Some(2),
            ..Default::default()
        };
        let r = execute(&rows(), &q);
        assert_eq!(r.rows.len(), 2);
        assert_eq!(r.rows[0]["region"], json!("Nord")); // 125
        assert_eq!(r.rows[1]["region"], json!("Sud")); // 50
    }

    #[test]
    fn filters_and_aggregations() {
        let q = Query {
            dimensions: vec!["region".into()],
            metrics: vec![
                MetricSpec { column: "orders".into(), function: "COUNT".into(), alias: Some("n".into()) },
                MetricSpec { column: "region".into(), function: "COUNT_DISTINCT".into(), alias: Some("d".into()) },
            ],
            filters: vec![FilterSpec { column: "amount".into(), operator: "gte".into(), value: json!(25) }],
            ..Default::default()
        };
        let r = execute(&rows(), &q);
        // Est (amount 10) filtered out -> Nord(2 rows), Sud(1 row)
        assert_eq!(r.rows.len(), 2);
        assert_eq!(r.rows[0]["n"], json!(2.0));
        assert_eq!(r.rows[0]["d"], json!(1.0));
    }

    #[test]
    fn no_grouping_returns_filtered_rows() {
        let q = Query { filters: vec![FilterSpec { column: "region".into(), operator: "eq".into(), value: json!("Sud") }], ..Default::default() };
        let r = execute(&rows(), &q);
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.rows[0]["amount"], json!(50));
    }
}
