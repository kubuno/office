//! The chart renderer against the web: every case of `tools/golden/cases/charts.json` is rendered by the
//! Rust port and must produce the elements the web's `ChartView` produced (`tests/golden/charts.json`, from
//! `tools/golden/charts-harness.tsx`): same tags in the same order, same attributes, numbers within 1e-9,
//! same text. Labels use the harness's locale (English).

use std::path::Path;

use kubuno_office_sheets_core::chart::{render_chart, CellVal, ChartWords};
use serde_json::Value as Json;

fn load(p: &str) -> Json {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(p);
    serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json")
}

fn same_value(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0),
        _ => false,
    }
}

#[test]
fn charts_render_like_the_web() {
    let cases = load("tools/golden/cases/charts.json");
    let gold = load("tests/golden/charts.json");
    let words = ChartWords { series: "Series".into(), no_data: "No data".into() };
    let mut failures = Vec::new();
    for (case, want) in cases.as_array().expect("cases").iter().zip(gold.as_array().expect("gold")) {
        let name = case["name"].as_str().unwrap_or("");
        let cells = case["cells"].as_object().cloned().unwrap_or_default();
        let get = |r: &str| match cells.get(r) {
            Some(Json::Number(n)) => CellVal::Num(n.as_f64().unwrap_or(0.0)),
            Some(Json::String(s)) => CellVal::Str(s.clone()),
            Some(Json::Bool(b)) => CellVal::Bool(*b),
            _ => CellVal::Str(String::new()),
        };
        let prims = render_chart(&case["chart"], case["width"].as_f64().unwrap_or(0.0), case["height"].as_f64().unwrap_or(0.0), &get, &words, "en");
        let got: Vec<_> = prims.iter().filter_map(|p| p.svg()).collect();
        let want_els = want["els"].as_array().expect("els");
        let mut first_diff = None;
        for (i, w) in want_els.iter().enumerate() {
            let Some((tag, attrs, text)) = got.get(i) else {
                first_diff = Some(format!("missing element {i}: {w}"));
                break;
            };
            let wa = w["attrs"].as_object().expect("attrs");
            let ok_tag = w["tag"].as_str() == Some(tag);
            let ok_keys = wa.len() == attrs.len() && attrs.iter().all(|(k, v)| wa.get(*k).and_then(Json::as_str).is_some_and(|wv| same_value(wv, v)));
            let ok_text = w.get("text").and_then(Json::as_str).unwrap_or("") == text.as_deref().unwrap_or("");
            if !(ok_tag && ok_keys && ok_text) {
                first_diff = Some(format!("element {i}\n  web : {w}\n  rust: {tag} {attrs:?} {text:?}"));
                break;
            }
        }
        if first_diff.is_none() && got.len() != want_els.len() {
            first_diff = Some(format!("{} elements, the web has {}", got.len(), want_els.len()));
        }
        if let Some(d) = first_diff {
            failures.push(format!("{name}: {d}"));
        }
    }
    assert!(failures.is_empty(), "{} of {} charts differ:\n{}", failures.len(), cases.as_array().map(|a| a.len()).unwrap_or(0), failures.join("\n"));
}
