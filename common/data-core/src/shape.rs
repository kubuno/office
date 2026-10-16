//! Data-shaping, a port of `web/src/data/shape.ts`: turn flat query rows into the series structures the
//! visuals consume, plus the statistical helpers (histogram, quartiles, linear regression, moving average,
//! squarified treemap). Pure functions. Rows are JSON objects, exactly what `executeApi.query` returns.

use crate::format::to_num;
use serde_json::{Map, Value};

/// A named series of numbers, optionally coloured (`Series` in `shape.ts`).
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub name: String,
    pub values: Vec<f64>,
    pub color: Option<String>,
}

/// Categories (X) plus one or more series (`ShapedData`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShapedData {
    pub categories: Vec<String>,
    pub series: Vec<Series>,
}

type Row = Map<String, Value>;

fn cell(row: &Row, key: &str) -> Value {
    row.get(key).cloned().unwrap_or(Value::Null)
}

fn as_cat(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// `shapeSeries` - shape rows for category charts. With a `legend`, one series per distinct legend value
/// (only `metrics[0]` is used); without, one series per metric.
pub fn shape_series(rows: &[Row], axis: &str, metrics: &[String], legend: Option<&str>) -> ShapedData {
    if rows.is_empty() {
        return ShapedData::default();
    }

    if let Some(legend) = legend {
        let Some(value_col) = metrics.first() else {
            return ShapedData::default();
        };
        let mut cats: Vec<String> = Vec::new();
        let mut seen_cat: std::collections::HashSet<String> = std::collections::HashSet::new();
        // Insertion-ordered series, each a map category -> value.
        let mut series_order: Vec<String> = Vec::new();
        let mut series_map: std::collections::HashMap<String, std::collections::HashMap<String, f64>> =
            std::collections::HashMap::new();
        for r in rows {
            let c = as_cat(&cell(r, axis));
            let s = as_cat(&cell(r, legend));
            if seen_cat.insert(c.clone()) {
                cats.push(c.clone());
            }
            let entry = series_map.entry(s.clone()).or_insert_with(|| {
                series_order.push(s.clone());
                std::collections::HashMap::new()
            });
            entry.insert(c, to_num(&cell(r, value_col)));
        }
        let series = series_order
            .into_iter()
            .map(|name| {
                let m = &series_map[&name];
                let values = cats.iter().map(|c| *m.get(c).unwrap_or(&0.0)).collect();
                Series { name, values, color: None }
            })
            .collect();
        return ShapedData { categories: cats, series };
    }

    let categories = rows.iter().map(|r| as_cat(&cell(r, axis))).collect();
    let series = metrics
        .iter()
        .map(|m| Series {
            name: m.clone(),
            values: rows.iter().map(|r| to_num(&cell(r, m))).collect(),
            color: None,
        })
        .collect();
    ShapedData { categories, series }
}

/// `sumMetric` - sum of a single metric across rows.
pub fn sum_metric(rows: &[Row], metric: &str) -> f64 {
    rows.iter().map(|r| to_num(&cell(r, metric))).sum()
}

/// One histogram bin.
#[derive(Debug, Clone, PartialEq)]
pub struct Bin {
    pub label: String,
    pub count: usize,
    pub x0: f64,
    pub x1: f64,
}

/// `histogram` - equal-width bins.
pub fn histogram(values: &[f64], bin_count: usize) -> Vec<Bin> {
    if values.is_empty() {
        return Vec::new();
    }
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if min == max {
        return vec![Bin { label: format!("{}", min as i64), count: values.len(), x0: min, x1: max }];
    }
    let n = bin_count.max(1);
    let width = (max - min) / n as f64;
    let mut counts = vec![0usize; n];
    for &v in values {
        let mut idx = ((v - min) / width).floor() as isize;
        if idx >= n as isize {
            idx = n as isize - 1;
        }
        if idx < 0 {
            idx = 0;
        }
        counts[idx as usize] += 1;
    }
    (0..n)
        .map(|i| {
            let x0 = min + i as f64 * width;
            let x1 = min + (i + 1) as f64 * width;
            Bin { label: format!("{}", x0.round() as i64), count: counts[i], x0, x1 }
        })
        .collect()
}

/// Five-number summary (`quartiles`), linear interpolation like the web.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quartiles {
    pub min: f64,
    pub q1: f64,
    pub med: f64,
    pub q3: f64,
    pub max: f64,
}

/// `quartiles` - min, Q1, median, Q3, max with linear interpolation between order statistics.
pub fn quartiles(values: &[f64]) -> Quartiles {
    let mut s = values.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let q = |p: f64| -> f64 {
        if s.is_empty() {
            return 0.0;
        }
        let idx = (s.len() - 1) as f64 * p;
        let lo = idx.floor() as usize;
        let hi = idx.ceil() as usize;
        s[lo] + (s[hi] - s[lo]) * (idx - lo as f64)
    };
    Quartiles {
        min: s.first().copied().unwrap_or(0.0),
        q1: q(0.25),
        med: q(0.5),
        q3: q(0.75),
        max: s.last().copied().unwrap_or(0.0),
    }
}

/// `linregress` - ordinary least squares slope/intercept for a trend line.
pub fn linregress(xs: &[f64], ys: &[f64]) -> (f64, f64) {
    let n = xs.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    let nf = n as f64;
    let sx: f64 = xs.iter().sum();
    let sy: f64 = ys.iter().sum();
    let sxx: f64 = xs.iter().map(|x| x * x).sum();
    let sxy: f64 = xs.iter().zip(ys).map(|(x, y)| x * y).sum();
    let denom = nf * sxx - sx * sx;
    if denom == 0.0 {
        return (0.0, sy / nf);
    }
    let slope = (nf * sxy - sx * sy) / denom;
    (slope, (sy - slope * sx) / nf)
}

/// `movingAverage` - trailing moving average of `window` points.
pub fn moving_average(values: &[f64], window: usize) -> Vec<f64> {
    if window <= 1 {
        return values.to_vec();
    }
    (0..values.len())
        .map(|i| {
            let start = i + 1 - (i + 1).min(window);
            let slice = &values[start..=i];
            slice.iter().sum::<f64>() / slice.len() as f64
        })
        .collect()
}

/// A rectangle produced by [`treemap_layout`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// `treemapLayout` - gap-free squarified treemap in a `w × h` rectangle, one rect per input item,
/// returned in the input order.
pub fn treemap_layout(values: &[f64], w: f64, h: f64) -> Vec<Rect> {
    let mut out = vec![Rect::default(); values.len()];
    if values.is_empty() {
        return out;
    }
    let total: f64 = values.iter().map(|v| v.max(0.0)).sum::<f64>().max(1.0);
    let mut idx: Vec<usize> = (0..values.len()).collect();
    idx.sort_by(|&a, &b| values[b].partial_cmp(&values[a]).unwrap_or(std::cmp::Ordering::Equal));
    let areas: Vec<f64> = idx.iter().map(|&i| (values[i].max(0.0) / total) * w * h).collect();

    let worst = |row: &[f64], len: f64| -> f64 {
        let s: f64 = row.iter().sum();
        let max = row.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let min = row.iter().cloned().fold(f64::INFINITY, f64::min);
        let len2 = len * len;
        let s2 = s * s;
        f64::max((len2 * max) / s2, s2 / (len2 * min))
    };

    let (mut x, mut y, mut avail_w, mut avail_h) = (0.0, 0.0, w, h);
    let mut i = 0usize;
    while i < areas.len() {
        let horizontal = avail_w >= avail_h;
        let len = if horizontal { avail_h } else { avail_w };
        let mut row: Vec<f64> = vec![areas[i]];
        let mut j = i + 1;
        while j < areas.len() {
            let mut next = row.clone();
            next.push(areas[j]);
            if worst(&next, len) > worst(&row, len) {
                break;
            }
            row.push(areas[j]);
            j += 1;
        }
        let row_sum: f64 = row.iter().sum();
        let thickness = row_sum / len;
        let mut along = 0.0;
        for (k, &area) in row.iter().enumerate() {
            let cell = area / thickness;
            out[idx[i + k]] = if horizontal {
                Rect { x, y: y + along, w: thickness, h: cell }
            } else {
                Rect { x: x + along, y, w: cell, h: thickness }
            };
            along += cell;
        }
        if horizontal {
            x += thickness;
            avail_w -= thickness;
        } else {
            y += thickness;
            avail_h -= thickness;
        }
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(pairs: &[(&str, Value)]) -> Row {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn shape_without_legend_one_series_per_metric() {
        let rows = vec![
            row(&[("m", json!("Jan")), ("a", json!(1)), ("b", json!(2))]),
            row(&[("m", json!("Feb")), ("a", json!(3)), ("b", json!(4))]),
        ];
        let out = shape_series(&rows, "m", &["a".into(), "b".into()], None);
        assert_eq!(out.categories, vec!["Jan", "Feb"]);
        assert_eq!(out.series.len(), 2);
        assert_eq!(out.series[0].values, vec![1.0, 3.0]);
        assert_eq!(out.series[1].values, vec![2.0, 4.0]);
    }

    #[test]
    fn shape_with_legend_pivots() {
        let rows = vec![
            row(&[("m", json!("Jan")), ("g", json!("X")), ("v", json!(1))]),
            row(&[("m", json!("Jan")), ("g", json!("Y")), ("v", json!(2))]),
            row(&[("m", json!("Feb")), ("g", json!("X")), ("v", json!(3))]),
        ];
        let out = shape_series(&rows, "m", &["v".into()], Some("g"));
        assert_eq!(out.categories, vec!["Jan", "Feb"]);
        assert_eq!(out.series.len(), 2);
        assert_eq!(out.series[0].name, "X");
        assert_eq!(out.series[0].values, vec![1.0, 3.0]);
        assert_eq!(out.series[1].name, "Y");
        assert_eq!(out.series[1].values, vec![2.0, 0.0]); // missing Feb/Y -> 0
    }

    #[test]
    fn stats_helpers() {
        assert_eq!(sum_metric(&[row(&[("v", json!(2))]), row(&[("v", json!(3))])], "v"), 5.0);
        let q = quartiles(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(q.med, 3.0);
        assert_eq!(q.q1, 2.0);
        assert_eq!(q.q3, 4.0);
        let (slope, intercept) = linregress(&[0.0, 1.0, 2.0], &[1.0, 3.0, 5.0]);
        assert!((slope - 2.0).abs() < 1e-9);
        assert!((intercept - 1.0).abs() < 1e-9);
        assert_eq!(moving_average(&[1.0, 2.0, 3.0], 2), vec![1.0, 1.5, 2.5]);
        assert_eq!(histogram(&[1.0, 1.0, 1.0], 4).len(), 1);
    }

    #[test]
    fn treemap_is_gap_free() {
        let rects = treemap_layout(&[4.0, 2.0, 1.0, 1.0], 100.0, 100.0);
        let covered: f64 = rects.iter().map(|r| r.w * r.h).sum();
        assert!((covered - 10000.0).abs() < 1.0, "covered {covered}");
        assert_eq!(rects.len(), 4);
    }
}
