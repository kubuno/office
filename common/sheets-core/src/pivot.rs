//! Pivot-table aggregation engine: a port of the web editor's `pivot-engine.ts`.
//!
//! Aggregates source rows by nested row fields, an optional column field, several value fields (each with
//! its function) and report filters, and returns a 2-D grid of strings (headers + grand totals) ready to be
//! written into a sheet. Source cells are [`serde_json::Value`]s (`null`, string, number, bool), which is
//! what the web's `unknown[][]` holds; the configuration deserialises from the stored JSON
//! (`rowFields`, `colField`, `values`, `filters`, `headers`).

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::js;

/// `PivotAgg`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PivotAgg {
    Sum,
    Count,
    CountNum,
    Avg,
    Min,
    Max,
}

/// `AGG_LABEL`: the French label of each aggregation, in the web's key order.
pub const AGG_LABEL: [(PivotAgg, &str); 6] = [
    (PivotAgg::Sum, "Somme"),
    (PivotAgg::Count, "Nombre"),
    (PivotAgg::CountNum, "Nombre (num.)"),
    (PivotAgg::Avg, "Moyenne"),
    (PivotAgg::Min, "Min"),
    (PivotAgg::Max, "Max"),
];

/// `AGG_LABEL[agg]`.
pub fn agg_label(agg: PivotAgg) -> &'static str {
    AGG_LABEL.iter().find(|(a, _)| *a == agg).map(|(_, l)| *l).unwrap_or("")
}

/// `PivotValueSpec`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PivotValueSpec {
    pub field: usize,
    pub agg: PivotAgg,
}

/// `PivotFilterSpec`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PivotFilterSpec {
    pub field: usize,
    pub selected: Vec<String>,
}

/// `PivotConfig` (the pivot definition stored with the sheet).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PivotConfig {
    /// Column indexes (relative to the source matrix), nested.
    #[serde(default)]
    pub row_fields: Vec<usize>,
    /// A breakdown column, or `None`.
    #[serde(default)]
    pub col_field: Option<usize>,
    /// Aggregated fields (≥ 1; an empty list means "sum of column 0").
    #[serde(default)]
    pub values: Vec<PivotValueSpec>,
    /// Report filters: a row must match every filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<PivotFilterSpec>>,
    /// The label of each source column.
    #[serde(default)]
    pub headers: Vec<String>,
}

/// `String(v)` of a cell, `''` for `null`/`undefined` (`String(r[f] ?? '')`).
fn cell_str(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(v) => js_string(v),
    }
}

/// JavaScript's `String(x)` of a JSON value.
fn js_string(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.as_f64().map(js::to_string).unwrap_or_else(|| n.to_string()),
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(|x| if x.is_null() { String::new() } else { js_string(x) }).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

fn is_blank(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.is_empty(),
        _ => false,
    }
}

/// `num(v)`: a finite number, or a non-blank numeric string (`+v` — `Infinity` and hex pass); else `None`.
fn num(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Number(n)) => n.as_f64().filter(|x| x.is_finite()),
        Some(Value::String(s)) => num_str(s),
        _ => None,
    }
}

fn num_str(s: &str) -> Option<f64> {
    if js::trim(s).is_empty() {
        return None;
    }
    js::number(s)
}

fn aggregate(values: &[Option<&Value>], agg: PivotAgg) -> f64 {
    let nums: Vec<f64> = values.iter().filter_map(|v| num(*v)).collect();
    match agg {
        PivotAgg::Count => values.iter().filter(|v| !is_blank(**v)).count() as f64,
        PivotAgg::CountNum => nums.len() as f64,
        PivotAgg::Sum => nums.iter().fold(0.0, |a, b| a + b),
        PivotAgg::Avg => {
            if nums.is_empty() {
                0.0
            } else {
                nums.iter().fold(0.0, |a, b| a + b) / nums.len() as f64
            }
        }
        PivotAgg::Min => {
            if nums.is_empty() {
                0.0
            } else {
                js::min(&nums)
            }
        }
        PivotAgg::Max => {
            if nums.is_empty() {
                0.0
            } else {
                js::max(&nums)
            }
        }
    }
}

fn fmt(n: f64) -> String {
    js::to_string(js::round(n * 1e10) / 1e10)
}

/// "Natural" order: numbers before text, else alphabetical (UTF-16 code unit order). The web's comparator
/// returns a number (`na - nb` can be `NaN` for two infinities: treated as equal, like `Array.sort`).
fn cmp_vals(a: &str, b: &str) -> Ordering {
    let (na, nb) = (num_str(a), num_str(b));
    match (na, nb) {
        (Some(x), Some(y)) => {
            let d = x - y;
            if d < 0.0 {
                Ordering::Less
            } else if d > 0.0 {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => js::cmp_str(a, b),
    }
}

/// A stable merge sort with a comparator that need not be a strict weak order (Rust's `sort_by` may panic
/// on such comparators; `Array.prototype.sort` never does).
fn stable_sort<T: Clone>(v: &mut Vec<T>, cmp: &dyn Fn(&T, &T) -> Ordering) {
    if v.len() <= 1 {
        return;
    }
    let mut right = v.split_off(v.len() / 2);
    stable_sort(v, cmp);
    stable_sort(&mut right, cmp);
    let left = std::mem::take(v);
    let (mut i, mut j) = (0, 0);
    while i < left.len() && j < right.len() {
        if cmp(&right[j], &left[i]) == Ordering::Less {
            v.push(right[j].clone());
            j += 1;
        } else {
            v.push(left[i].clone());
            i += 1;
        }
    }
    v.extend_from_slice(&left[i..]);
    v.extend_from_slice(&right[j..]);
}

/// Composite key separator: a control character that cannot appear in a cell value.
const SEP: char = '\u{1}';

type Row = Vec<Value>;

/// `buildPivot`: the 2-D string grid of the pivot (header row, one line per row-field combination, then the
/// "Total général" line).
pub fn build_pivot(rows: &[Row], cfg: &PivotConfig) -> Vec<Vec<String>> {
    let row_fields = &cfg.row_fields;
    let col_field = cfg.col_field;
    let headers = &cfg.headers;
    let default_values = [PivotValueSpec { field: 0, agg: PivotAgg::Sum }];
    let values: &[PivotValueSpec] = if cfg.values.is_empty() { &default_values } else { &cfg.values };
    let filters: &[PivotFilterSpec] = cfg.filters.as_deref().unwrap_or(&[]);

    let kept: Vec<&Row> = rows
        .iter()
        .filter(|r| filters.is_empty() || filters.iter().all(|f| f.selected.is_empty() || f.selected.contains(&cell_str(r.get(f.field)))))
        .collect();

    let sep = SEP.to_string();
    let row_key = |r: &Row| row_fields.iter().map(|&f| cell_str(r.get(f))).collect::<Vec<_>>().join(&sep);
    let col_key = |r: &Row| match col_field {
        Some(c) => cell_str(r.get(c)),
        None => "__total__".to_string(),
    };

    let mut row_keys: Vec<String> = Vec::new();
    let mut row_seen: HashSet<String> = HashSet::new();
    let mut col_keys: Vec<String> = Vec::new();
    let mut col_seen: HashSet<String> = HashSet::new();
    let mut bucket: HashMap<String, HashMap<String, Vec<&Row>>> = HashMap::new();

    for r in &kept {
        let rk = row_key(r);
        let ck = col_key(r);
        if row_seen.insert(rk.clone()) {
            row_keys.push(rk.clone());
        }
        if col_seen.insert(ck.clone()) {
            col_keys.push(ck.clone());
        }
        bucket.entry(rk).or_default().entry(ck).or_default().push(r);
    }

    stable_sort(&mut row_keys, &|a: &String, b: &String| {
        let aa: Vec<&str> = a.split(SEP).collect();
        let bb: Vec<&str> = b.split(SEP).collect();
        for (i, x) in aa.iter().enumerate() {
            let d = cmp_vals(x, bb.get(i).copied().unwrap_or(""));
            if d != Ordering::Equal {
                return d;
            }
        }
        Ordering::Equal
    });
    let mut cols: Vec<String> = col_keys.into_iter().filter(|c| c != "__total__").collect();
    stable_sort(&mut cols, &|a: &String, b: &String| cmp_vals(a, b));
    let has_col_field = col_field.is_some();

    let header_of = |f: usize| -> String {
        match headers.get(f) {
            Some(h) if !h.is_empty() => h.clone(),
            _ => format!("Col {}", f + 1),
        }
    };
    let v_label = |v: &PivotValueSpec| js::trim(&format!("{} de {}", agg_label(v.agg), header_of(v.field))).to_string();
    let cell_of = |src: &[&Row], v: &PivotValueSpec| -> String {
        if src.is_empty() {
            String::new()
        } else {
            let col: Vec<Option<&Value>> = src.iter().map(|r| r.get(v.field)).collect();
            fmt(aggregate(&col, v.agg))
        }
    };

    let mut grid: Vec<Vec<String>> = Vec::new();
    // Header: row fields, then (columns × values), then totals.
    let mut header: Vec<String> = row_fields.iter().map(|&f| header_of(f)).collect();
    if has_col_field {
        for ck in &cols {
            if values.len() == 1 {
                header.push(ck.clone());
            } else {
                for v in values {
                    header.push(format!("{} — {}", ck, v_label(v)));
                }
            }
        }
        for v in values {
            header.push(if values.len() == 1 { "Total général".to_string() } else { format!("Total — {}", v_label(v)) });
        }
    } else {
        for v in values {
            header.push(v_label(v));
        }
    }
    grid.push(header);

    let mut all_rows: Vec<&Row> = Vec::new();
    let mut col_rows: Vec<Vec<&Row>> = cols.iter().map(|_| Vec::new()).collect();

    for rk in &row_keys {
        let empty = HashMap::new();
        let m = bucket.get(rk).unwrap_or(&empty);
        let mut line: Vec<String> = rk.split(SEP).map(str::to_string).collect();
        if has_col_field {
            let mut row_src: Vec<&Row> = Vec::new();
            for (ci, ck) in cols.iter().enumerate() {
                let src: &[&Row] = m.get(ck).map(Vec::as_slice).unwrap_or(&[]);
                col_rows[ci].extend_from_slice(src);
                row_src.extend_from_slice(src);
                for v in values {
                    line.push(cell_of(src, v));
                }
            }
            all_rows.extend_from_slice(&row_src);
            for v in values {
                line.push(cell_of(&row_src, v));
            }
        } else {
            let src: &[&Row] = m.get("__total__").map(Vec::as_slice).unwrap_or(&[]);
            all_rows.extend_from_slice(src);
            for v in values {
                line.push(cell_of(src, v));
            }
        }
        grid.push(line);
    }

    // "Total général" line.
    let mut total_row: Vec<String> = vec!["Total général".to_string()];
    total_row.extend(row_fields.iter().skip(1).map(|_| String::new()));
    if has_col_field {
        for cr in &col_rows {
            for v in values {
                total_row.push(cell_of(cr, v));
            }
        }
        for v in values {
            total_row.push(cell_of(&all_rows, v));
        }
    } else {
        for v in values {
            total_row.push(cell_of(&all_rows, v));
        }
    }
    grid.push(total_row);
    grid
}

/// `distinctValues`: the sorted distinct values of a source column (for the filters UI).
pub fn distinct_values(rows: &[Row], field: usize) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for r in rows {
        let s = cell_str(r.get(field));
        if seen.insert(s.clone()) {
            out.push(s);
        }
    }
    stable_sort(&mut out, &|a: &String, b: &String| cmp_vals(a, b));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rows() -> Vec<Row> {
        vec![
            vec![json!("N"), json!("a"), json!(1)],
            vec![json!("S"), json!("a"), json!(2)],
            vec![json!("N"), json!("b"), json!("3")],
            vec![json!("N"), json!("a"), json!(null)],
        ]
    }

    #[test]
    fn simple_pivot() {
        let cfg: PivotConfig = serde_json::from_value(json!({"rowFields":[0],"colField":null,"values":[{"field":2,"agg":"sum"}],"headers":["Zone","Prod","Qty"]})).unwrap();
        let g = build_pivot(&rows(), &cfg);
        assert_eq!(g[0], vec!["Zone", "Somme de Qty"]);
        assert_eq!(g[1], vec!["N", "4"]);
        assert_eq!(g[2], vec!["S", "2"]);
        assert_eq!(g[3], vec!["Total général", "6"]);
    }

    #[test]
    fn column_field_and_distinct() {
        let cfg: PivotConfig = serde_json::from_value(json!({"rowFields":[0],"colField":1,"values":[{"field":2,"agg":"count"}],"headers":["Zone","Prod","Qty"]})).unwrap();
        let g = build_pivot(&rows(), &cfg);
        assert_eq!(g[0], vec!["Zone", "a", "b", "Total général"]);
        assert_eq!(g[1], vec!["N", "1", "1", "2"]);
        assert_eq!(distinct_values(&rows(), 2), vec!["1", "2", "3", ""]);
    }
}
