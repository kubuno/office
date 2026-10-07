//! The port against the web: every case of `tools/golden/cases/*.json` evaluated by the Rust engine must
//! give what the web engine gave (`tests/golden/*.json`, produced by `tools/golden/harness.ts` running
//! `office/web/src/formula-engine.ts`). Numbers may differ in their last bits (libm), within 1e-11
//! relative; everything else must be identical.

use std::collections::HashMap;
use std::path::Path;

use kubuno_office_sheets_core::formula::eval::{evaluate, CellMap, Ctx, Data, FixedEnv};
use kubuno_office_sheets_core::formula::spill::compute_spills;
use kubuno_office_sheets_core::formula::value::{Scalar, Value};
use kubuno_office_sheets_core::jsdate::LocalNow;
use kubuno_office_sheets_core::model::cell::Cell;
use serde_json::Value as Json;

fn cells_of(v: Option<&Json>) -> CellMap {
    v.and_then(Json::as_object).map(|o| o.iter().map(|(k, c)| (k.clone(), Cell::from_json(c))).collect()).unwrap_or_default()
}

fn same_number(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= 1e-11 * a.abs().max(b.abs())
}

fn scalar_matches(s: &Scalar, e: &Json) -> bool {
    match (s, e.as_object()) {
        (Scalar::Num(x), Some(o)) if o.contains_key("n") => match &o["n"] {
            Json::Number(n) => n.as_f64().is_some_and(|y| same_number(*x, y)),
            Json::String(t) => kubuno_office_sheets_core::js::to_string(*x) == *t,
            _ => false,
        },
        (Scalar::Str(x), Some(o)) => o.get("s").and_then(Json::as_str) == Some(x.as_str()),
        (Scalar::Bool(x), Some(o)) => o.get("b").and_then(Json::as_bool) == Some(*x),
        (Scalar::Err(x), Some(o)) => o.get("e").and_then(Json::as_str) == Some(x.as_str()),
        _ => false,
    }
}

fn value_matches(v: &Value, e: &Json) -> bool {
    match v {
        Value::S(s) => scalar_matches(s, e),
        Value::M(m) => {
            let Some(rows) = e.get("m").and_then(Json::as_array) else { return false };
            rows.len() == m.len()
                && rows.iter().zip(m).all(|(er, r)| er.as_array().is_some_and(|er| er.len() == r.len() && er.iter().zip(r).all(|(ec, c)| scalar_matches(c, ec))))
        }
        Value::L(_) => e.get("lambda").is_some(),
    }
}

#[test]
fn the_engine_matches_the_web() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cases_dir = root.join("tools/golden/cases");
    let env = FixedEnv::new(LocalNow { year: 2026, month: 10, day: 7, hour: 14, minute: 30, second: 15, millis: 250 });
    let mut failures = Vec::new();
    let mut checked = 0;
    let mut entries: Vec<_> = std::fs::read_dir(&cases_dir).expect("cases").filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let expected_path = root.join("tests/golden").join(&name);
        let Ok(expected) = std::fs::read_to_string(&expected_path) else {
            failures.push(format!("{name}: no expected file (run the harness)"));
            continue;
        };
        let cases: Json = serde_json::from_str(&std::fs::read_to_string(entry.path()).expect("case file")).expect("case json");
        let expected: Json = serde_json::from_str(&expected).expect("expected json");
        for (case, exp) in cases.as_array().into_iter().flatten().zip(expected.as_array().into_iter().flatten()) {
            if case.get("calls").is_some() {
                continue; // module calls are checked by the tests of each module
            }
            let cells = cells_of(case.get("cells"));
            let names: HashMap<String, String> = case.get("names").and_then(Json::as_object).map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect()).unwrap_or_default();
            let other: Vec<(String, CellMap)> = case.get("sheets").and_then(Json::as_object).map(|o| o.iter().map(|(k, v)| (k.clone(), cells_of(Some(v)))).collect()).unwrap_or_default();
            let sheets: HashMap<String, &CellMap> = other.iter().map(|(k, v)| (k.clone(), v)).collect();
            let data = Data { cells: &cells, sheets: if other.is_empty() { None } else { Some(&sheets) }, names: &names };
            let ctx = Ctx::new(&env);
            let spill = if case.get("spill").and_then(Json::as_bool) == Some(true) { Some(compute_spills(data, &ctx)) } else { None };
            for res in exp.get("results").and_then(Json::as_array).into_iter().flatten() {
                let f = res.get("formula").and_then(Json::as_str).unwrap_or("");
                checked += 1;
                let Some(want) = res.get("value") else {
                    failures.push(format!("{name} / {}: the web threw {:?}", case["name"], res.get("threw")));
                    continue;
                };
                let got = evaluate(f, data, &ctx, spill.as_ref());
                if !value_matches(&got, want) {
                    failures.push(format!("{name} / {} / {f}: got {got:?}, web {want}", case["name"]));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} of {checked} differ:\n{}", failures.len(), failures.join("\n"));
}
