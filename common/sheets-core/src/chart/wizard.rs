//! The chart wizard's model (`web/src/sheet-chart/wizard/{catalog,state}.ts`): the type/subtype catalogue
//! (LibreOffice Calc's chart-type dialog), and the draft operations of the four steps. A draft is the
//! persisted chart JSON itself; every operation edits a copy and keeps unknown fields (the web spreads).

use serde_json::{json, Map, Value as Json};

use super::data::{cell_ref, detect_range_layout, CellGetter, CellVal};
use super::model::RangeSource;
use crate::refs::ref_bounds;

/// One subtype: the fields it writes (`None` = remove the field).
#[derive(Clone, Debug)]
pub struct Subtype {
    pub id: &'static str,
    pub label_key: &'static str,
    pub label: &'static str,
    pub apply: &'static [(&'static str, Option<ApplyVal>)],
}

#[derive(Clone, Copy, Debug)]
pub enum ApplyVal {
    Str(&'static str),
    Bool(bool),
    Num(f64),
}

#[derive(Clone, Debug)]
pub struct ChartType {
    pub id: &'static str,
    pub label_key: &'static str,
    pub label: &'static str,
    pub smooth_option: bool,
    pub subtypes: Vec<Subtype>,
}

/// The cross-type flags a pick clears (`CLEAR`); `smooth` survives.
const CLEAR: [&str; 7] = ["grouping", "lines", "symbols", "holeSize", "explode", "filled", "numLines"];

use ApplyVal::*;

fn stack_triplet(t: &'static str) -> Vec<Subtype> {
    let (n, s, p): (&'static [_], &'static [_], &'static [_]) = match t {
        "bar" => (&[("type", Some(Str("bar")))], &[("type", Some(Str("bar"))), ("grouping", Some(Str("stacked")))], &[("type", Some(Str("bar"))), ("grouping", Some(Str("percentStacked")))]),
        "hbar" => (&[("type", Some(Str("hbar")))], &[("type", Some(Str("hbar"))), ("grouping", Some(Str("stacked")))], &[("type", Some(Str("hbar"))), ("grouping", Some(Str("percentStacked")))]),
        _ => (&[("type", Some(Str("area")))], &[("type", Some(Str("area"))), ("grouping", Some(Str("stacked")))], &[("type", Some(Str("area"))), ("grouping", Some(Str("percentStacked")))]),
    };
    vec![
        Subtype { id: "normal", label_key: "sheet_chart_st_normal", label: "Normal", apply: n },
        Subtype { id: "stacked", label_key: "sheet_chart_st_stacked", label: "Empilé", apply: s },
        Subtype { id: "percent", label_key: "sheet_chart_st_percent", label: "Pourcentage", apply: p },
    ]
}

fn symbol_triplet(t: &'static str) -> Vec<Subtype> {
    let (a, b, c): (&'static [_], &'static [_], &'static [_]) = if t == "line" {
        (
            &[("type", Some(Str("line"))), ("lines", Some(Bool(false))), ("symbols", Some(Bool(true)))],
            &[("type", Some(Str("line"))), ("lines", Some(Bool(true))), ("symbols", Some(Bool(true)))],
            &[("type", Some(Str("line"))), ("lines", Some(Bool(true))), ("symbols", Some(Bool(false)))],
        )
    } else {
        (
            &[("type", Some(Str("scatter"))), ("lines", Some(Bool(false))), ("symbols", Some(Bool(true)))],
            &[("type", Some(Str("scatter"))), ("lines", Some(Bool(true))), ("symbols", Some(Bool(true)))],
            &[("type", Some(Str("scatter"))), ("lines", Some(Bool(true))), ("symbols", Some(Bool(false)))],
        )
    };
    vec![
        Subtype { id: "points", label_key: "sheet_chart_st_points", label: "Points seuls", apply: a },
        Subtype { id: "points_lines", label_key: "sheet_chart_st_points_lines", label: "Points et lignes", apply: b },
        Subtype { id: "lines", label_key: "sheet_chart_st_lines", label: "Lignes seules", apply: c },
    ]
}

/// `CHART_CATALOG`.
pub fn catalog() -> Vec<ChartType> {
    vec![
        ChartType { id: "column", label_key: "sheet_chart_ty_column", label: "Colonnes", smooth_option: false, subtypes: stack_triplet("bar") },
        ChartType { id: "bar", label_key: "sheet_chart_ty_bar", label: "Barres", smooth_option: false, subtypes: stack_triplet("hbar") },
        ChartType {
            id: "pie",
            label_key: "sheet_chart_ty_pie",
            label: "Secteurs",
            smooth_option: false,
            subtypes: vec![
                Subtype { id: "normal", label_key: "sheet_chart_st_normal", label: "Normal", apply: &[("type", Some(Str("pie")))] },
                Subtype { id: "exploded", label_key: "sheet_chart_st_exploded", label: "Éclaté", apply: &[("type", Some(Str("pie"))), ("explode", Some(Num(0.5)))] },
                Subtype { id: "donut", label_key: "sheet_chart_st_donut", label: "Tore", apply: &[("type", Some(Str("donut")))] },
                Subtype { id: "donut_exploded", label_key: "sheet_chart_st_donut_exploded", label: "Tore éclaté", apply: &[("type", Some(Str("donut"))), ("explode", Some(Num(0.35)))] },
            ],
        },
        ChartType { id: "area", label_key: "sheet_chart_ty_area", label: "Zones", smooth_option: false, subtypes: stack_triplet("area") },
        ChartType { id: "line", label_key: "sheet_chart_ty_line", label: "Lignes", smooth_option: true, subtypes: symbol_triplet("line") },
        ChartType { id: "scatter", label_key: "sheet_chart_ty_scatter", label: "XY (dispersion)", smooth_option: true, subtypes: symbol_triplet("scatter") },
        ChartType {
            id: "radar",
            label_key: "sheet_chart_ty_radar",
            label: "Toile",
            smooth_option: false,
            subtypes: vec![
                Subtype { id: "lines", label_key: "sheet_chart_st_lines", label: "Lignes seules", apply: &[("type", Some(Str("radar")))] },
                Subtype { id: "filled", label_key: "sheet_chart_st_filled", label: "Rempli", apply: &[("type", Some(Str("radar"))), ("filled", Some(Bool(true)))] },
            ],
        },
        ChartType {
            id: "combo",
            label_key: "sheet_chart_ty_combo",
            label: "Colonnes et lignes",
            smooth_option: false,
            subtypes: vec![
                Subtype { id: "col_line", label_key: "sheet_chart_st_col_line", label: "Colonnes et lignes", apply: &[("type", Some(Str("combo")))] },
                Subtype { id: "col_stacked_line", label_key: "sheet_chart_st_col_stacked_line", label: "Colonnes empilées et lignes", apply: &[("type", Some(Str("combo"))), ("grouping", Some(Str("stacked")))] },
            ],
        },
    ]
}

fn s<'a>(c: &'a Json, k: &str) -> Option<&'a str> {
    c.get(k).and_then(Json::as_str)
}

fn b(c: &Json, k: &str) -> Option<bool> {
    c.get(k).and_then(Json::as_bool)
}

fn n(c: &Json, k: &str) -> Option<f64> {
    c.get(k).and_then(Json::as_f64)
}

fn grouping_of(c: &Json) -> &'static str {
    match s(c, "grouping") {
        Some("stacked") => "stacked",
        Some("percentStacked") => "percent",
        _ => "normal",
    }
}

/// `catalogTypeOf`: the catalogue type id of a chart.
pub fn catalog_type_of(c: &Json) -> &'static str {
    match s(c, "type") {
        Some("hbar") => "bar",
        Some("pie") | Some("donut") => "pie",
        Some("area") => "area",
        Some("line") => "line",
        Some("scatter") => "scatter",
        Some("radar") => "radar",
        Some("combo") => "combo",
        _ => "column",
    }
}

/// `subtypeOf`: the active subtype id within its type (first match wins).
pub fn subtype_of(c: &Json) -> &'static str {
    let explode = n(c, "explode").unwrap_or(0.0) > 0.0;
    match catalog_type_of(c) {
        "column" | "bar" | "area" => match grouping_of(c) {
            "stacked" => "stacked",
            "percent" => "percent",
            _ => "normal",
        },
        "pie" => {
            let donut = s(c, "type") == Some("donut") || n(c, "holeSize").unwrap_or(0.0) > 0.0;
            if s(c, "type") == Some("pie") && !explode {
                "normal"
            } else if s(c, "type") == Some("pie") && explode {
                "exploded"
            } else if donut && !explode {
                "donut"
            } else if donut && explode {
                "donut_exploded"
            } else {
                "normal"
            }
        }
        t @ ("line" | "scatter") => {
            let (dl, ds) = if t == "line" { (true, false) } else { (false, true) };
            let lines = b(c, "lines").unwrap_or(dl);
            let symbols = b(c, "symbols").unwrap_or(ds);
            if symbols && !lines {
                "points"
            } else if symbols && lines {
                "points_lines"
            } else if !symbols {
                "lines"
            } else {
                "points"
            }
        }
        "radar" => {
            if b(c, "filled").unwrap_or(false) {
                "filled"
            } else {
                "lines"
            }
        }
        _ => {
            if grouping_of(c) == "normal" {
                "col_line"
            } else {
                "col_stacked_line"
            }
        }
    }
}

/// `applySubtype`: `{ ...c, ...CLEAR, ...sub.apply }` (undefined fields dropped, as JSON does).
pub fn apply_subtype(c: &Json, sub: &Subtype) -> Json {
    let mut m = c.as_object().cloned().unwrap_or_default();
    for k in CLEAR {
        m.remove(k);
    }
    for (k, v) in sub.apply {
        match v {
            Some(Str(x)) => {
                m.insert((*k).into(), json!(x));
            }
            Some(Bool(x)) => {
                m.insert((*k).into(), json!(x));
            }
            Some(Num(x)) => {
                m.insert((*k).into(), crate::model::cell::json_number(*x));
            }
            None => {
                m.remove(*k);
            }
        }
    }
    Json::Object(m)
}

/// Sets a field (`None` removes it, like `undefined` dropped by JSON).
pub fn set(c: &Json, k: &str, v: Option<Json>) -> Json {
    let mut m = c.as_object().cloned().unwrap_or_default();
    match v {
        Some(v) => {
            m.insert(k.into(), v);
        }
        None => {
            m.remove(k);
        }
    }
    Json::Object(m)
}

/// Step 2: a new range binds the chart to a simple-range source (explicit series and imports dropped).
pub fn set_range(c: &Json, range: &str) -> Json {
    let mut m = c.as_object().cloned().unwrap_or_default();
    for k in ["series", "vals", "cats", "catsRange"] {
        m.remove(k);
    }
    if range.is_empty() {
        m.remove("range");
    } else {
        m.insert("range".into(), json!(range));
    }
    Json::Object(m)
}

/// The step-2 layout shown: the draft's choices over the auto-detection.
pub fn range_layout(c: &Json, get: &CellGetter) -> (bool, bool, bool) {
    let detected = s(c, "range").and_then(|r| detect_range_layout(&RangeSource { range: r.into(), series_in_rows: None, first_row_header: None, first_col_header: None }, get));
    let rows = s(c, "seriesIn").map(|v| v == "rows").or(detected.as_ref().map(|d| d.series_in_rows)).unwrap_or(false);
    let first_row = b(c, "firstRowHeader").or(detected.as_ref().map(|d| d.first_row_header)).unwrap_or(false);
    let first_col = b(c, "firstColHeader").or(detected.as_ref().map(|d| d.first_col_header)).unwrap_or(false);
    (rows, first_row, first_col)
}

/// Step 3's explicit view of a draft (`ExplicitData`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Explicit {
    pub series: Vec<Json>,
    pub cats_range: Option<String>,
    pub derived: bool,
}

/// `seriesFromRange`.
pub fn series_from_range(c: &Json, get: &CellGetter) -> Explicit {
    let empty = Explicit { derived: true, ..Default::default() };
    let Some(range) = s(c, "range") else { return empty };
    let src = RangeSource { range: range.into(), series_in_rows: s(c, "seriesIn").map(|v| v == "rows"), first_row_header: b(c, "firstRowHeader"), first_col_header: b(c, "firstColHeader") };
    let (Some(layout), Some(bd)) = (detect_range_layout(&src, get), ref_bounds(range)) else { return empty };
    let mut series = Vec::new();
    let mut cats = None;
    if !layout.series_in_rows {
        let r1 = bd.r1 + layout.first_row_header as i64;
        if r1 > bd.r2 {
            return empty;
        }
        let c1 = bd.c1 + layout.first_col_header as i64;
        if layout.first_col_header {
            cats = Some(format!("{}:{}", cell_ref(bd.c1, r1), cell_ref(bd.c1, bd.r2)));
        }
        for col in c1..=bd.c2 {
            let mut m = Map::new();
            if layout.first_row_header {
                m.insert("nameRef".into(), json!(cell_ref(col, bd.r1)));
            }
            m.insert("valsRange".into(), json!(format!("{}:{}", cell_ref(col, r1), cell_ref(col, bd.r2))));
            series.push(Json::Object(m));
        }
    } else {
        let c1 = bd.c1 + layout.first_col_header as i64;
        if c1 > bd.c2 {
            return empty;
        }
        let r1 = bd.r1 + layout.first_row_header as i64;
        if layout.first_row_header {
            cats = Some(format!("{}:{}", cell_ref(c1, bd.r1), cell_ref(bd.c2, bd.r1)));
        }
        for row in r1..=bd.r2 {
            let mut m = Map::new();
            if layout.first_col_header {
                m.insert("nameRef".into(), json!(cell_ref(bd.c1, row)));
            }
            m.insert("valsRange".into(), json!(format!("{}:{}", cell_ref(c1, row), cell_ref(bd.c2, row))));
            series.push(Json::Object(m));
        }
    }
    Explicit { series, cats_range: cats, derived: true }
}

/// `explicitData`.
pub fn explicit_data(c: &Json, get: &CellGetter) -> Explicit {
    if let Some(a) = c.get("series").and_then(Json::as_array).filter(|a| !a.is_empty()) {
        return Explicit { series: a.clone(), cats_range: s(c, "catsRange").map(str::to_string), derived: false };
    }
    if let Some(v) = c.get("vals").and_then(Json::as_array).filter(|a| !a.is_empty()) {
        let mut m = Map::new();
        m.insert("vals".into(), Json::Array(v.clone()));
        if let Some(cats) = c.get("cats") {
            m.insert("cats".into(), cats.clone());
        }
        return Explicit { series: vec![Json::Object(m)], cats_range: s(c, "catsRange").map(str::to_string), derived: false };
    }
    series_from_range(c, get)
}

/// `withExplicitSeries`.
pub fn with_explicit_series(c: &Json, series: Vec<Json>, cats_range: Option<&str>) -> Json {
    let mut m = c.as_object().cloned().unwrap_or_default();
    m.insert("series".into(), Json::Array(series));
    match cats_range {
        Some(r) => {
            m.insert("catsRange".into(), json!(r));
        }
        None => {
            m.remove("catsRange");
        }
    }
    for k in ["range", "seriesIn", "firstRowHeader", "firstColHeader", "vals", "cats"] {
        m.remove(k);
    }
    Json::Object(m)
}

/// `seriesDisplayName`.
pub fn series_display_name(sr: &Json, i: usize, get: &CellGetter, series_n: &str) -> String {
    if let Some(n) = s(sr, "name").filter(|n| !n.is_empty()) {
        return n.to_string();
    }
    if let Some(r) = s(sr, "nameRef") {
        let v = super::data::to_label(&get(r));
        if !v.is_empty() && get(r) != CellVal::Empty {
            return v;
        }
    }
    series_n.replace("{{n}}", &(i + 1).to_string())
}

/// `moveItem`.
pub fn move_item<T: Clone>(arr: &[T], i: usize, delta: i64) -> Option<Vec<T>> {
    let j = i as i64 + delta;
    if j < 0 || j as usize >= arr.len() || i >= arr.len() {
        return None;
    }
    let mut next = arr.to_vec();
    let it = next.remove(i);
    next.insert(j as usize, it);
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtypes_apply_and_match() {
        let cat = catalog();
        let c = json!({"type": "bar", "range": "A1:B4", "grouping": "stacked", "smooth": true, "x": 1});
        let line = &cat[4];
        let pl = apply_subtype(&c, &line.subtypes[1]);
        assert_eq!(pl, json!({"type": "line", "range": "A1:B4", "smooth": true, "x": 1, "lines": true, "symbols": true}));
        assert_eq!(catalog_type_of(&pl), "line");
        assert_eq!(subtype_of(&pl), "points_lines");
        assert_eq!(subtype_of(&json!({"type": "donut", "explode": 0.35})), "donut_exploded");
        assert_eq!(subtype_of(&c), "stacked");
    }

    #[test]
    fn ranges_become_explicit_series() {
        let cells = [("A1", ""), ("B1", "s1"), ("C1", "s2"), ("A2", "a"), ("A3", "b")];
        let get = |r: &str| match cells.iter().find(|(k, _)| *k == r) {
            Some((_, v)) => CellVal::Str(v.to_string()),
            None => CellVal::Num(1.0),
        };
        let e = explicit_data(&json!({"type": "bar", "range": "A1:C3"}), &get);
        assert_eq!(e.cats_range.as_deref(), Some("A2:A3"));
        assert_eq!(e.series, vec![json!({"nameRef": "B1", "valsRange": "B2:B3"}), json!({"nameRef": "C1", "valsRange": "C2:C3"})]);
        let c = with_explicit_series(&json!({"type": "bar", "range": "A1:C3"}), e.series.clone(), e.cats_range.as_deref());
        assert!(c.get("range").is_none());
        assert_eq!(series_display_name(&e.series[0], 0, &get, "Série {{n}}"), "s1");
    }
}
