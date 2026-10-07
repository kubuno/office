//! The auxiliary (non-formula) modules against the web: every call of `tools/golden/cases/aux-*.json` is
//! made on the Rust port and must return what the web module returned (`tests/golden/aux-*.json`, produced
//! by `tools/golden/harness.ts` running `fill-series.ts`, `formula-refs.ts`, `formula-edit.ts`,
//! `data-validation.ts` and `pivot-engine.ts`). Numbers are compared within 1e-11 relative; everything
//! else must be identical. The function catalogue is checked against `tests/golden/catalog-web.json`, a
//! dump of the web's `FUNCTION_CATALOG`.

use std::collections::HashSet;
use std::path::Path;

use kubuno_office_sheets_core::catalog::{catalog_entry, function_catalog, locale_compare_names};
use kubuno_office_sheets_core::fill::{fill_series, Fill};
use kubuno_office_sheets_core::formula::functions::known_functions;
use kubuno_office_sheets_core::js;
use kubuno_office_sheets_core::pivot::{build_pivot, distinct_values, PivotConfig};
use kubuno_office_sheets_core::refs;
use kubuno_office_sheets_core::validation::{criterion_label, has_dropdown, is_checkbox, validate_value, DVCriterion, DataValidation};
use serde_json::{json, Map, Value as Json};

fn num(x: f64) -> Json {
    if x.is_finite() {
        json!(x)
    } else {
        Json::Null // JSON.stringify(NaN | ±Infinity)
    }
}

fn same(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Number(x), Json::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => x == y || (x - y).abs() <= 1e-11 * x.abs().max(y.abs()),
            _ => false,
        },
        (Json::Array(x), Json::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q)),
        (Json::Object(x), Json::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w))),
        _ => a == b,
    }
}

fn s(v: &Json) -> &str {
    v.as_str().unwrap_or("")
}

fn u(v: &Json) -> usize {
    v.as_u64().unwrap_or(0) as usize
}

fn opt<T>(o: Option<T>, f: impl FnOnce(T) -> Json) -> Json {
    o.map(f).unwrap_or(Json::Null)
}

fn string_set(v: Option<&Json>) -> Option<HashSet<String>> {
    v.and_then(Json::as_array).map(|a| a.iter().map(|x| s(x).to_string()).collect())
}

fn obj(pairs: Vec<(&str, Json)>) -> Json {
    Json::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect::<Map<_, _>>())
}

/// A JavaScript `String(x)` of a JSON scalar (list entries).
fn js_str(v: &Json) -> String {
    match v {
        Json::String(x) => x.clone(),
        Json::Number(n) => n.as_f64().map(js::to_string).unwrap_or_default(),
        other => other.to_string(),
    }
}

/// Runs one call on the Rust port; `None` when the function is not covered.
fn call(module: &str, f: &str, a: &[Json]) -> Option<Json> {
    let arg = |i: usize| a.get(i).unwrap_or(&Json::Null);
    Some(match (module, f) {
        ("fill-series", "fillSeries") => {
            let src: Vec<Fill> = arg(0)
                .as_array()?
                .iter()
                .map(|v| match v {
                    Json::Number(n) => Fill::Num(n.as_f64().unwrap_or(f64::NAN)),
                    other => Fill::Str(other.as_str().unwrap_or("").to_string()),
                })
                .collect();
            let r = fill_series(&src, arg(1).as_i64()?, arg(2).as_bool()?, s(arg(3)));
            opt(r, |v| Json::Array(v.into_iter().map(|x| match x {
                Fill::Num(n) => num(n),
                Fill::Str(t) => Json::String(t),
            }).collect()))
        }
        ("formula-refs", "parseRefs") => Json::Array(
            refs::parse_refs(s(arg(0)))
                .into_iter()
                .map(|r| obj(vec![("start", json!(r.start)), ("end", json!(r.end)), ("text", json!(r.text)), ("normalized", json!(r.normalized)), ("color", json!(r.color))]))
                .collect(),
        ),
        ("formula-refs", "refBounds") => opt(refs::ref_bounds(s(arg(0))), |b| obj(vec![("c1", json!(b.c1)), ("r1", json!(b.r1)), ("c2", json!(b.c2)), ("r2", json!(b.r2))])),
        ("formula-refs", "nameTokenAt") => json!(refs::name_token_at(s(arg(0)), u(arg(1)))),
        ("formula-refs", "argContextAt") => opt(refs::arg_context_at(s(arg(0)), u(arg(1))), |c| obj(vec![("name", json!(c.name)), ("argIndex", json!(c.arg_index))])),
        ("formula-refs", "parseSyntaxArgs") => Json::Array(
            refs::parse_syntax_args(s(arg(0))).into_iter().map(|x| obj(vec![("name", json!(x.name)), ("optional", json!(x.optional)), ("repeat", json!(x.repeat))])).collect(),
        ),
        ("formula-refs", "colorSegments") => {
            let known = string_set(a.get(1));
            let names = string_set(a.get(2));
            Json::Array(
                refs::color_segments(s(arg(0)), known.as_ref(), names.as_ref())
                    .into_iter()
                    .map(|g| {
                        let mut o = Map::new();
                        o.insert("text".into(), json!(g.text));
                        if let Some(c) = g.color {
                            o.insert("color".into(), json!(c));
                        }
                        if g.wavy {
                            o.insert("wavy".into(), json!(true));
                        }
                        Json::Object(o)
                    })
                    .collect(),
            )
        }
        ("formula-edit", "cycleAbsolute") => json!(refs::cycle_absolute(s(arg(0)))),
        ("formula-edit", "refAtCaret") => opt(refs::ref_at_caret(s(arg(0)), u(arg(1))), |r| obj(vec![("start", json!(r.start)), ("end", json!(r.end)), ("text", json!(r.text))])),
        ("formula-edit", "applyF4") => opt(refs::apply_f4(s(arg(0)), u(arg(1))), |(t, c)| obj(vec![("text", json!(t)), ("caret", json!(c))])),
        ("formula-edit", "autoPair") => opt(refs::auto_pair(s(arg(0)), u(arg(1)), s(arg(2))), |(t, c)| obj(vec![("text", json!(t)), ("caret", json!(c))])),
        ("formula-edit", "matchParen") => opt(refs::match_paren(s(arg(0)), u(arg(1))), |(x, y)| obj(vec![("a", json!(x)), ("b", json!(y))])),
        ("formula-edit", "parenBalance") => {
            let (c, o) = refs::paren_balance(s(arg(0)));
            obj(vec![("missingClose", json!(c)), ("missingOpen", json!(o))])
        }
        ("formula-edit", "autoBalance") => json!(refs::auto_balance(s(arg(0)))),
        ("formula-edit", "normalizeFormula") => json!(refs::normalize_formula(s(arg(0)))),
        ("formula-edit", "errorHelp") => opt(refs::error_help(s(arg(0))), |h| json!(h)),
        ("formula-edit", "levenshtein") => json!(refs::levenshtein(s(arg(0)), s(arg(1)))),
        ("formula-edit", "didYouMean") => {
            let known: Vec<String> = arg(1).as_array()?.iter().map(|x| s(x).to_string()).collect();
            opt(refs::did_you_mean(s(arg(0)), &known), |n| json!(n))
        }
        ("formula-edit", "fuzzyScore") => num(refs::fuzzy_score(s(arg(0)), s(arg(1)))),
        ("formula-edit", "rankFunctions") => {
            let list = arg(1).as_array()?;
            let limit = a.get(2).and_then(Json::as_u64).map(|x| x as usize).unwrap_or(12);
            let r = refs::rank_functions(s(arg(0)), list, limit, |v| s(&v["name"]));
            Json::Array(r.into_iter().cloned().collect())
        }
        ("formula-edit", "splitSignature") => {
            let (fname, parts) = refs::split_signature(s(arg(0)), u(arg(1)));
            obj(vec![("fn", json!(fname)), ("parts", Json::Array(parts.into_iter().map(|p| obj(vec![("text", json!(p.text)), ("active", json!(p.active))])).collect()))])
        }
        ("data-validation", "validateValue") => {
            let crit: DVCriterion = serde_json::from_value(arg(0).clone()).ok()?;
            let allowed: Option<Vec<String>> = a.get(2).and_then(Json::as_array).map(|v| v.iter().map(js_str).collect());
            json!(validate_value(&crit, s(arg(1)), allowed.as_deref()))
        }
        ("data-validation", "criterionLabel") => {
            let crit: DVCriterion = serde_json::from_value(arg(0).clone()).ok()?;
            json!(criterion_label(&crit))
        }
        ("data-validation", "isCheckbox") | ("data-validation", "hasDropdown") => {
            let rule: Option<DataValidation> = if arg(0).is_null() { None } else { Some(serde_json::from_value(arg(0).clone()).ok()?) };
            json!(if f == "isCheckbox" { is_checkbox(rule.as_ref()) } else { has_dropdown(rule.as_ref()) })
        }
        ("pivot-engine", "buildPivot") => {
            let rows: Vec<Vec<Json>> = serde_json::from_value(arg(0).clone()).ok()?;
            let cfg: PivotConfig = serde_json::from_value(arg(1).clone()).ok()?;
            json!(build_pivot(&rows, &cfg))
        }
        ("pivot-engine", "distinctValues") => {
            let rows: Vec<Vec<Json>> = serde_json::from_value(arg(0).clone()).ok()?;
            json!(distinct_values(&rows, u(arg(1))))
        }
        _ => return None,
    })
}

#[test]
fn the_aux_modules_match_the_web() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut entries: Vec<_> = std::fs::read_dir(root.join("tools/golden/cases")).expect("cases dir").filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    let mut failures = Vec::new();
    let mut checked = 0usize;
    let mut files = 0usize;
    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("aux-") {
            continue;
        }
        files += 1;
        let Ok(expected) = std::fs::read_to_string(root.join("tests/golden").join(&name)) else {
            failures.push(format!("{name}: no expected file (run the harness)"));
            continue;
        };
        let cases: Json = serde_json::from_str(&std::fs::read_to_string(entry.path()).expect("case file")).expect("case json");
        let expected: Json = serde_json::from_str(&expected).expect("expected json");
        for (case, exp) in cases.as_array().into_iter().flatten().zip(expected.as_array().into_iter().flatten()) {
            for res in exp.get("results").and_then(Json::as_array).into_iter().flatten() {
                let c = &res["call"];
                let module = s(&c["module"]);
                let fname = s(&c["fn"]);
                let args: Vec<Json> = c["args"].as_array().cloned().unwrap_or_default();
                checked += 1;
                let Some(want) = res.get("json") else {
                    failures.push(format!("{name} / {} / {c}: the web threw {:?}", case["name"], res.get("threw")));
                    continue;
                };
                match call(module, fname, &args) {
                    None => failures.push(format!("{name} / {} / {c}: not covered or unparsable", case["name"])),
                    Some(got) if !same(&got, want) => failures.push(format!("{name} / {} / {c}:\n   got {got}\n   web {want}", case["name"])),
                    Some(_) => {}
                }
            }
        }
    }
    println!("aux golden: {checked} calls in {files} files");
    assert!(files >= 4, "expected the aux case files");
    let shown: Vec<&String> = failures.iter().take(40).collect();
    assert!(failures.is_empty(), "{} of {checked} differ (first {}):\n{}", failures.len(), shown.len(), shown.iter().map(|x| x.as_str()).collect::<Vec<_>>().join("\n"));
}

#[test]
fn the_catalogue_matches_the_web() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dump: Json = serde_json::from_str(&std::fs::read_to_string(root.join("tests/golden/catalog-web.json")).expect("catalog dump")).expect("json");
    let web: Vec<(String, String, String)> = dump["catalog"]
        .as_array()
        .expect("catalog")
        .iter()
        .map(|f| (s(&f["name"]).to_string(), s(&f["cat"]).to_string(), s(&f["syntax"]).to_string()))
        .collect();
    // Category and syntax of every function name of the web.
    for (name, cat, syntax) in &web {
        let e = catalog_entry(name);
        assert_eq!((e.cat.key(), e.syntax.as_str()), (cat.as_str(), syntax.as_str()), "{name}");
    }
    // The web's order is `localeCompare` order, which the emulation reproduces.
    let mut names: Vec<&str> = web.iter().map(|w| w.0.as_str()).collect();
    names.sort_by(|a, b| locale_compare_names(a, b));
    assert_eq!(names, web.iter().map(|w| w.0.as_str()).collect::<Vec<_>>());
    // The Rust catalogue (built from the registered functions) is a sub-list of the web's, in the same order.
    let rust = function_catalog();
    let mut at = 0usize;
    for f in rust {
        let Some(p) = web[at..].iter().position(|w| w.0 == f.name) else { panic!("{} is not in the web catalogue (or out of order)", f.name) };
        at += p + 1;
    }
    // Once every function is ported the two lists are the same.
    let known: HashSet<&str> = known_functions().into_iter().collect();
    let web_known: HashSet<&str> = dump["known"].as_array().expect("known").iter().map(s).collect();
    if known == web_known {
        assert_eq!(rust.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), web.iter().map(|w| w.0.as_str()).collect::<Vec<_>>());
    }
}

/// `colorSegments` with real `Set` arguments (the harness cannot pass a `Set`): `tests/golden/segments-web.json`
/// is a dump of the web module's results for the same inputs.
#[test]
fn color_segments_with_sets_match_the_web() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dump: Json = serde_json::from_str(&std::fs::read_to_string(root.join("tests/golden/segments-web.json")).expect("segments dump")).expect("json");
    let mut bad = Vec::new();
    for c in dump.as_array().expect("array") {
        let known = string_set(Some(&c["known"]));
        let names = string_set(Some(&c["names"]));
        let got = call("formula-refs", "colorSegments", &[c["formula"].clone(), json!(known.as_ref().map(|k| k.iter().cloned().collect::<Vec<_>>())), json!(names.as_ref().map(|k| k.iter().cloned().collect::<Vec<_>>()))]);
        let got = got.expect("covered");
        if !same(&got, &c["segments"]) {
            bad.push(format!("{} / {} / {}: got {got}, web {}", c["formula"], c["known"], c["names"], c["segments"]));
        }
    }
    assert!(bad.is_empty(), "{} differ:\n{}", bad.len(), bad.join("\n"));
}
