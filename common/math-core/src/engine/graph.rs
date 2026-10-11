//! Port of the model and tick maths of `office/web/src/mathGraph.ts` — a function-graph block.
//! The actual painting (`drawGraph`, a Canvas-2D routine) stays per-platform; this module holds the
//! platform-neutral parts the desktop painter and the stored-document round-trip need: the
//! [`GraphSpec`] model, [`normalize_graph_spec`], [`nice_step`] and [`fmt_tick`].

use super::num_fmt::{js_number_to_string, json_number, json_string};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One plotted function of `x` (`mathGraph.ts` `GraphFn`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphFn {
    pub expr: String,
    pub color: String,
}

/// A function-graph block's data (`mathGraph.ts` `GraphSpec`). Field order matches the web's
/// `JSON.stringify` so the stored document round-trips byte-identically.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphSpec {
    pub fns: Vec<GraphFn>,
    pub xmin: f64,
    pub xmax: f64,
    /// `null` → auto-scale Y from the sampled values.
    pub ymin: Option<f64>,
    pub ymax: Option<f64>,
    pub grid: bool,
    #[serde(rename = "axisNumbers")]
    pub axis_numbers: bool,
    /// Canvas height in CSS px.
    pub height: f64,
}

/// A pleasant, distinguishable default palette for successive curves (`mathGraph.ts`
/// `GRAPH_COLORS`).
pub const GRAPH_COLORS: &[&str] =
    &["#1a73e8", "#d93025", "#188038", "#e8710a", "#9334e6", "#00897b", "#c2185b", "#5d4037"];

/// `mathGraph.ts` `defaultGraphSpec`.
pub fn default_graph_spec(n: usize) -> GraphSpec {
    GraphSpec {
        fns: vec![GraphFn {
            expr: "sin(x)".to_string(),
            color: GRAPH_COLORS[n % GRAPH_COLORS.len()].to_string(),
        }],
        xmin: -10.0,
        xmax: 10.0,
        ymin: None,
        ymax: None,
        grid: true,
        axis_numbers: true,
        height: 280.0,
    }
}

fn num(v: Option<&Value>, fb: f64) -> f64 {
    match v.and_then(Value::as_f64) {
        Some(x) if x.is_finite() => x,
        _ => fb,
    }
}
fn num_or_null(v: Option<&Value>) -> Option<f64> {
    match v.and_then(Value::as_f64) {
        Some(x) if x.is_finite() => Some(x),
        _ => None,
    }
}

/// Validate + fill a possibly-partial spec coming from storage (`mathGraph.ts`
/// `normalizeGraphSpec`).
pub fn normalize_graph_spec(s: &Value) -> GraphSpec {
    let d = default_graph_spec(0);
    let o = s.as_object();
    let get = |k: &str| o.and_then(|m| m.get(k));

    let fns = match get("fns").and_then(Value::as_array) {
        Some(arr) if !arr.is_empty() => arr
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let fo = f.as_object();
                let expr = fo
                    .and_then(|m| m.get("expr"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let color = fo
                    .and_then(|m| m.get("color"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| GRAPH_COLORS[i % GRAPH_COLORS.len()].to_string());
                GraphFn { expr, color }
            })
            .collect(),
        _ => d.fns,
    };

    GraphSpec {
        fns,
        xmin: num(get("xmin"), d.xmin),
        xmax: num(get("xmax"), d.xmax),
        ymin: num_or_null(get("ymin")),
        ymax: num_or_null(get("ymax")),
        grid: get("grid") != Some(&Value::Bool(false)),
        axis_numbers: get("axisNumbers") != Some(&Value::Bool(false)),
        height: num(get("height"), d.height),
    }
}

/// Serialise a spec exactly as the web's `JSON.stringify` writes it inside the stored document
/// (field order `fns, xmin, xmax, ymin, ymax, grid, axisNumbers, height`; `null` for an auto axis;
/// numbers via `Number.toString`).
pub fn spec_to_stored_json(spec: &GraphSpec) -> String {
    let fns = spec
        .fns
        .iter()
        .map(|f| format!("{{\"expr\":{},\"color\":{}}}", json_string(&f.expr), json_string(&f.color)))
        .collect::<Vec<_>>()
        .join(",");
    let opt = |o: Option<f64>| o.map(json_number).unwrap_or_else(|| "null".to_string());
    format!(
        "{{\"fns\":[{fns}],\"xmin\":{},\"xmax\":{},\"ymin\":{},\"ymax\":{},\"grid\":{},\"axisNumbers\":{},\"height\":{}}}",
        json_number(spec.xmin),
        json_number(spec.xmax),
        opt(spec.ymin),
        opt(spec.ymax),
        spec.grid,
        spec.axis_numbers,
        json_number(spec.height)
    )
}

/// "Nice" tick step (1, 2, 5 × 10^k) covering `[lo, hi]` with about `target` divisions
/// (`mathGraph.ts` `niceStep`).
pub fn nice_step(lo: f64, hi: f64, target: f64) -> f64 {
    let raw = (hi - lo) / target.max(1.0);
    if !raw.is_finite() || raw <= 0.0 {
        return 1.0;
    }
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = if norm >= 5.0 {
        5.0
    } else if norm >= 2.0 {
        2.0
    } else {
        1.0
    };
    step * mag
}

/// Format a tick label without trailing float noise (`mathGraph.ts` `fmtTick`).
pub fn fmt_tick(v: f64, step: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    let decimals = (-step.log10().floor()).max(0.0) as usize;
    let d = decimals.min(6);
    let rounded: f64 = format!("{v:.d$}").parse().unwrap_or(v);
    js_number_to_string(rounded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks() {
        assert_eq!(nice_step(0.0, 1.0, 10.0), 0.1);
        assert_eq!(nice_step(0.0, 100.0, 10.0), 10.0);
        assert_eq!(fmt_tick(0.0, 1.0), "0");
        assert_eq!(fmt_tick(0.5, 0.1), "0.5");
        assert_eq!(fmt_tick(2.0, 1.0), "2");
    }

    #[test]
    fn round_trips_json() {
        let spec = default_graph_spec(1);
        let json = serde_json::to_string(&spec).unwrap();
        // field order + null ymin/ymax + camelCase axisNumbers, like the web's JSON.stringify.
        assert!(json.starts_with("{\"fns\":[{\"expr\":\"sin(x)\",\"color\":\"#d93025\"}],\"xmin\":-10"));
        assert!(json.contains("\"ymin\":null"));
        assert!(json.contains("\"axisNumbers\":true"));
        let back: GraphSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back, spec);
    }
}
