//! Chart part builder (xl/charts/chartN.xml).
//!
//! Builds a DrawingML chart from the internal chart JSON. Input shapes:
//!   - imported charts: explicit bare cell refs (`vals`/`cats`, and optionally a
//!     per-series `series` array with names/colors/kinds) — exported as absolute
//!     references qualified with the sheet name;
//!   - v2 series: `series[].valsRange`/`xRange`/`nameRef` and chart-level
//!     `catsRange` (rectangular A1 ranges, expanded here);
//!   - UI-created charts: a rectangular `range` "A1:B6". With any of the v2
//!     layout keys (`seriesIn`/`firstRowHeader`/`firstColHeader`) the range is
//!     expanded into one series per column/row like the frontend does; without
//!     them the legacy shape applies (1st column = labels, 2nd = values,
//!     header row auto-detected).
//!
//! Cached values (`c:numCache`/`c:strCache`) are emitted from the sheet's cells
//! so readers show the chart before any recalculation.
use serde_json::Value;

use super::super::read::chart::normalize_refs;
use super::super::util::{col_to_idx, esc_xml, idx_to_col, split_ref};
use super::shape;
use super::workbook::quote_sheet_name;

// Mirrors the frontend's defensive cap on range-driven charts.
const MAX_RANGE_POINTS: i32 = 4096;
// Cap on the number of series expanded from a rectangular range.
const MAX_RANGE_SERIES: usize = 256;

// Per-series plot family: dictates which optional <c:ser> children apply
// (marker/smooth on line-drawn series, explosion on pie slices, …).
#[derive(Clone, Copy, PartialEq)]
enum Family { Bar, Line, Pie, Area, Radar, Scatter }

// One series to emit: display name (literal or cell ref) + bare cell refs
// ("B2") on each side, plus per-series presentation.
struct Ser {
    name:          Option<String>,
    name_ref:      Option<String>, // bare cell ref holding the series name
    cats:          Vec<String>,
    vals:          Vec<String>,
    color:         Option<String>,
    line_in_combo: bool,           // combo charts: emit inside the lineChart plot
}

// ── Data-side helpers ────────────────────────────────────────────────────────

fn str_list(v: Option<&Value>) -> Vec<String> {
    v.and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|r| r.as_str()).map(|r| r.to_uppercase()).collect())
        .unwrap_or_default()
}

// Raw cell value of a bare ref, from the sheet's cells map.
fn cell_value<'a>(data: &'a Value, bare_ref: &str) -> Option<&'a Value> {
    data.get("cells")?.get(bare_ref)?.get("v")
}

fn cell_text(data: &Value, bare_ref: &str) -> Option<String> {
    cell_value(data, bare_ref).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(str::to_string)
}

fn is_text_cell(data: &Value, bare_ref: &str) -> bool {
    matches!(cell_value(data, bare_ref), Some(Value::String(s)) if !s.is_empty())
}

// Expand an A1 range string (possibly sheet-qualified) into bare refs, capped.
fn range_refs(range: &str) -> Vec<String> {
    let mut refs = normalize_refs(&range.to_uppercase());
    refs.truncate(MAX_RANGE_POINTS as usize);
    refs
}

// Rectangle bounds (c1, c2, r1, r2) of an "A1:C9" range string.
fn range_rect(range: &str) -> Option<(usize, usize, i32, i32)> {
    let (a, b) = range.split_once(':').unwrap_or((range, range));
    let (ca, ra) = split_ref(a.trim())?;
    let (cb, rb) = split_ref(b.trim())?;
    let (c1, c2) = (col_to_idx(&ca).min(col_to_idx(&cb)), col_to_idx(&ca).max(col_to_idx(&cb)));
    Some((c1, c2, ra.min(rb), ra.max(rb)))
}

// Collect the series list from the chart JSON (see module docs for the shapes).
fn collect_series(chart: &Value, data: &Value) -> Vec<Ser> {
    let cats_range: Vec<String> = chart.get("catsRange").and_then(|v| v.as_str())
        .map(range_refs).unwrap_or_default();
    if let Some(list) = chart.get("series").and_then(|v| v.as_array()) {
        let sers: Vec<Ser> = list.iter().map(|s| {
            let gets = |k: &str| s.get(k).and_then(|v| v.as_str());
            let mut vals = str_list(s.get("vals"));
            if vals.is_empty() { if let Some(r) = gets("valsRange") { vals = range_refs(r); } }
            let mut cats = str_list(s.get("cats"));
            if cats.is_empty() { if let Some(r) = gets("xRange") { cats = range_refs(r); } }
            if cats.is_empty() { cats = cats_range.clone(); }
            Ser {
                name:          gets("name").map(str::to_string),
                name_ref:      gets("nameRef").and_then(|r| range_refs(r).into_iter().next()),
                cats, vals,
                color:         gets("color").map(str::to_string),
                line_in_combo: gets("kind") == Some("line"),
            }
        }).filter(|s| !s.vals.is_empty()).collect();
        if !sers.is_empty() { return mark_combo_lines(chart, sers); }
    }
    let vals = str_list(chart.get("vals"));
    if !vals.is_empty() {
        let cats = { let c = str_list(chart.get("cats")); if c.is_empty() { cats_range } else { c } };
        let ser = Ser { name: None, name_ref: None, cats, vals, color: None, line_in_combo: false };
        return mark_combo_lines(chart, vec![ser]);
    }
    if let Some(range) = chart.get("range").and_then(|v| v.as_str()) {
        // Always expand with the v2 resolver: explicit layout keys win, missing
        // ones fall back to the Calc auto-detection — exactly what the frontend
        // renders. (The old behaviour only did this when a layout key was
        // present, silently exporting wizard charts as a single series.)
        let sers = series_from_range_v2(chart, range, data);
        if !sers.is_empty() { return mark_combo_lines(chart, sers); }
        if let Some(s) = series_from_range(range, data) { return mark_combo_lines(chart, vec![s]); }
    }
    Vec::new()
}

// Combo charts: when no series carries an explicit kind, the trailing
// `numLines` series (default 1) are drawn as lines.
fn mark_combo_lines(chart: &Value, mut sers: Vec<Ser>) -> Vec<Ser> {
    if chart.get("type").and_then(|v| v.as_str()) != Some("combo") { return sers; }
    if sers.iter().any(|s| s.line_in_combo) { return sers; }
    let n = sers.len();
    if n < 2 { return sers; }
    let nl = chart.get("numLines").and_then(|v| v.as_u64()).unwrap_or(1).clamp(1, (n - 1) as u64) as usize;
    for s in sers.iter_mut().skip(n - nl) { s.line_in_combo = true; }
    sers
}

// Derive a single series from a legacy UI chart range: 1st column = labels,
// 2nd = values; a text cell atop the value column is a header (series name).
fn series_from_range(range: &str, data: &Value) -> Option<Ser> {
    let (c1, c2, ra, rb) = range_rect(range)?;
    let r1 = ra;
    let r2 = rb.min(r1 + MAX_RANGE_POINTS);
    let val_col = (c1 + 1).min(c2);
    let header = cell_text(data, &format!("{}{}", idx_to_col(val_col), r1));
    let start = if header.is_some() { r1 + 1 } else { r1 };
    if start > r2 { return None; }
    let vals: Vec<String> = (start..=r2).map(|r| format!("{}{}", idx_to_col(val_col), r)).collect();
    let cats: Vec<String> = if c2 > c1 {
        (start..=r2).map(|r| format!("{}{}", idx_to_col(c1), r)).collect()
    } else { Vec::new() };
    Some(Ser { name: header, name_ref: None, cats, vals, color: None, line_in_combo: false })
}

// Expand a v2 range chart (`seriesIn`/`firstRowHeader`/`firstColHeader`) into
// one series per data column (or row), mirroring the frontend's resolver.
// Missing flags fall back to the Calc auto-detection heuristics.
fn series_from_range_v2(chart: &Value, range: &str, data: &Value) -> Vec<Ser> {
    let Some((c1, c2, r1, rb)) = range_rect(range) else { return Vec::new() };
    let r2 = rb.min(r1 + MAX_RANGE_POINTS);
    let refc = |c: usize, r: i32| format!("{}{}", idx_to_col(c), r);
    let rows_mode = match chart.get("seriesIn").and_then(|v| v.as_str()) {
        Some("rows") => true,
        Some(_) => false,
        None => r1 == r2 && c1 < c2, // a single data row → series in rows (Calc)
    };
    // Calc header detection: whole first row/col is text and the second is not.
    let auto_col_header = || r1 < r2
        && (c1..=c2).all(|c| is_text_cell(data, &refc(c, r1)))
        && (c1..=c2).any(|c| !is_text_cell(data, &refc(c, r1 + 1)));
    let auto_row_header = || c1 < c2
        && (r1..=r2).all(|r| is_text_cell(data, &refc(c1, r)))
        && (r1..=r2).any(|r| !is_text_cell(data, &refc(c1 + 1, r)));
    let frh = chart.get("firstRowHeader").and_then(|v| v.as_bool()).unwrap_or_else(auto_col_header);
    let fch = chart.get("firstColHeader").and_then(|v| v.as_bool()).unwrap_or_else(auto_row_header);

    let mut out = Vec::new();
    if rows_mode {
        // Series in rows: first col (if header) = series names, first row = categories.
        let dc1 = if fch { c1 + 1 } else { c1 };
        let dr1 = if frh { r1 + 1 } else { r1 };
        if dc1 > c2 || dr1 > r2 { return out; }
        let dc2 = c2.min(dc1 + MAX_RANGE_POINTS as usize - 1);
        let cats: Vec<String> = if frh { (dc1..=dc2).map(|c| refc(c, r1)).collect() } else { Vec::new() };
        for r in dr1..=r2.min(dr1 + MAX_RANGE_SERIES as i32 - 1) {
            out.push(Ser {
                name: None,
                name_ref: if fch { Some(refc(c1, r)) } else { None },
                cats: cats.clone(),
                vals: (dc1..=dc2).map(|c| refc(c, r)).collect(),
                color: None, line_in_combo: false,
            });
        }
    } else {
        // Series in columns: first row (if header) = series names, first col = categories.
        let dr1 = if frh { r1 + 1 } else { r1 };
        let dc1 = if fch { c1 + 1 } else { c1 };
        if dr1 > r2 || dc1 > c2 { return out; }
        let cats: Vec<String> = if fch { (dr1..=r2).map(|r| refc(c1, r)).collect() } else { Vec::new() };
        for c in dc1..=c2.min(dc1 + MAX_RANGE_SERIES - 1) {
            out.push(Ser {
                name: None,
                name_ref: if frh { Some(refc(c, r1)) } else { None },
                cats: cats.clone(),
                vals: (dr1..=r2).map(|r| refc(c, r)).collect(),
                color: None, line_in_combo: false,
            });
        }
    }
    out
}

// ── Reference formatting ─────────────────────────────────────────────────────

// Bare refs → a chart reference formula. Contiguous single-column/row runs
// become a plain absolute range; anything else becomes a (…) multi-area union.
fn refs_to_f(refs: &[String], q: &str) -> Option<String> {
    if refs.is_empty() { return None; }
    let parsed: Vec<(String, i32)> = refs.iter().filter_map(|r| split_ref(r)).collect();
    if parsed.len() != refs.len() { return None; }
    let single = |&(ref c, r): &(String, i32)| format!("{q}!${c}${r}");
    if parsed.len() == 1 { return Some(single(&parsed[0])); }
    // Vertical run: same column, consecutive rows.
    let same_col = parsed.iter().all(|(c, _)| c == &parsed[0].0);
    if same_col && parsed.windows(2).all(|w| w[1].1 == w[0].1 + 1) {
        let (c, r1) = (&parsed[0].0, parsed[0].1);
        let r2 = parsed[parsed.len() - 1].1;
        return Some(format!("{q}!${c}${r1}:${c}${r2}"));
    }
    // Horizontal run: same row, consecutive columns.
    let same_row = parsed.iter().all(|(_, r)| *r == parsed[0].1);
    if same_row && parsed.windows(2).all(|w| col_to_idx(&w[1].0) == col_to_idx(&w[0].0) + 1) {
        let r = parsed[0].1;
        return Some(format!("{q}!${}${r}:${}${r}", parsed[0].0, parsed[parsed.len() - 1].0));
    }
    Some(format!("({})", parsed.iter().map(single).collect::<Vec<_>>().join(",")))
}

// Number → canonical chart cache string.
fn num_str(n: f64) -> String {
    if (n - n.round()).abs() < 1e-9 && n.abs() < 1e15 { format!("{}", n.round() as i64) } else { format!("{n}") }
}

// <c:numCache> from the current cell values (missing/non-numeric points are skipped).
fn num_cache(refs: &[String], data: &Value) -> String {
    let mut pts = String::new();
    for (i, r) in refs.iter().enumerate() {
        if let Some(n) = cell_value(data, r).and_then(|v| v.as_f64()) {
            pts.push_str(&format!("<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", num_str(n)));
        }
    }
    format!("<c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>{pts}", refs.len())
}

// <c:strCache> from the current cell values (any scalar rendered as text).
fn str_cache(refs: &[String], data: &Value) -> String {
    let mut pts = String::new();
    for (i, r) in refs.iter().enumerate() {
        let txt = match cell_value(data, r) {
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Number(n)) => n.as_f64().map(num_str),
            Some(Value::Bool(b)) => Some(if *b { "TRUE".into() } else { "FALSE".into() }),
            _ => None,
        };
        if let Some(t) = txt {
            pts.push_str(&format!("<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", esc_xml(&t)));
        }
    }
    format!("<c:ptCount val=\"{}\"/>{pts}", refs.len())
}

// ── Series & chart assembly ──────────────────────────────────────────────────

// "#RRGGBB" (or "RRGGBB") → validated uppercase hex, else None.
fn hex6(c: &str) -> Option<String> {
    let h = c.trim_start_matches('#').to_uppercase();
    (h.len() == 6 && h.chars().all(|ch| ch.is_ascii_hexdigit())).then_some(h)
}

// Chart-level styling forwarded to each series.
struct SerOpts<'a> {
    family:     Family,
    fill_color: bool,           // series colour = fill (vs line stroke)
    symbols:    Option<bool>,   // explicit marker choice (line-drawn families)
    smooth:     Option<bool>,   // explicit smoothing choice (line/scatter)
    explosion:  Option<i64>,    // pie slice offset, percent of the radius
    dpt_colors: &'a [String],   // per-point colours (first series only)
}

// One <c:ser>. Children follow the schema order of every CT_*Ser variant:
// idx, order, tx, spPr, marker, explosion, dPt, cat/xVal, val/yVal, smooth.
fn ser_xml(i: usize, ser: &Ser, q: &str, data: &Value, o: &SerOpts) -> String {
    let scatter = o.family == Family::Scatter;
    let mut out = format!("<c:ser><c:idx val=\"{i}\"/><c:order val=\"{i}\"/>");
    // Series name: cell reference (stays live in the workbook) or literal text.
    if let Some(nref) = &ser.name_ref {
        if let Some(f) = refs_to_f(std::slice::from_ref(nref), q) {
            out.push_str(&format!(
                "<c:tx><c:strRef><c:f>{}</c:f><c:strCache>{}</c:strCache></c:strRef></c:tx>",
                esc_xml(&f), str_cache(std::slice::from_ref(nref), data)
            ));
        }
    } else if let Some(n) = &ser.name {
        out.push_str(&format!("<c:tx><c:v>{}</c:v></c:tx>", esc_xml(n)));
    }
    // Series colour: fill for area-like marks, line stroke for line-drawn ones.
    if let Some(hex) = ser.color.as_deref().and_then(hex6) {
        out.push_str(&if o.fill_color {
            format!("<c:spPr><a:solidFill><a:srgbClr val=\"{hex}\"/></a:solidFill></c:spPr>")
        } else {
            format!("<c:spPr><a:ln><a:solidFill><a:srgbClr val=\"{hex}\"/></a:solidFill></a:ln></c:spPr>")
        });
    }
    if matches!(o.family, Family::Line | Family::Scatter | Family::Radar) {
        match o.symbols {
            Some(true) => out.push_str("<c:marker><c:symbol val=\"circle\"/><c:size val=\"5\"/></c:marker>"),
            Some(false) => out.push_str("<c:marker><c:symbol val=\"none\"/></c:marker>"),
            None => {}
        }
    }
    if o.family == Family::Pie {
        if let Some(x) = o.explosion { out.push_str(&format!("<c:explosion val=\"{x}\"/>")); }
    }
    for (k, c) in o.dpt_colors.iter().enumerate() {
        if let Some(hex) = hex6(c) {
            out.push_str(&format!(
                "<c:dPt><c:idx val=\"{k}\"/><c:spPr><a:solidFill><a:srgbClr val=\"{hex}\"/></a:solidFill></c:spPr></c:dPt>"
            ));
        }
    }
    if let Some(f) = refs_to_f(&ser.cats, q) {
        let (open, close) = if scatter { ("<c:xVal>", "</c:xVal>") } else { ("<c:cat>", "</c:cat>") };
        out.push_str(&format!(
            "{open}<c:strRef><c:f>{}</c:f><c:strCache>{}</c:strCache></c:strRef>{close}",
            esc_xml(&f), str_cache(&ser.cats, data)
        ));
    }
    if let Some(f) = refs_to_f(&ser.vals, q) {
        let (open, close) = if scatter { ("<c:yVal>", "</c:yVal>") } else { ("<c:val>", "</c:val>") };
        out.push_str(&format!(
            "{open}<c:numRef><c:f>{}</c:f><c:numCache>{}</c:numCache></c:numRef>{close}",
            esc_xml(&f), num_cache(&ser.vals, data)
        ));
    }
    if matches!(o.family, Family::Line | Family::Scatter) {
        if let Some(sm) = o.smooth { out.push_str(&format!("<c:smooth val=\"{}\"/>", sm as u8)); }
    }
    out.push_str("</c:ser>");
    out
}

// Chart-level <c:dLbls> from the internal dataLabels option.
fn dlbls_xml(data_labels: Option<&str>) -> String {
    let Some(dl) = data_labels else { return String::new() };
    let (v, p, c) = match dl {
        "percent" => (0, 1, 0),
        "value" => (1, 0, 0),
        "category" => (0, 0, 1),
        _ => return String::new(),
    };
    format!(concat!(
        "<c:dLbls><c:showLegendKey val=\"0\"/><c:showVal val=\"{}\"/><c:showCatName val=\"{}\"/>",
        "<c:showSerName val=\"0\"/><c:showPercent val=\"{}\"/><c:showBubbleSize val=\"0\"/></c:dLbls>"
    ), v, c, p)
}

// Scatter plot style from the lines/smooth/symbols flags (defaults mirror the
// legacy export: connected lines with markers).
fn scatter_style(lines: Option<bool>, smooth: Option<bool>, symbols: Option<bool>) -> &'static str {
    match (lines.unwrap_or(true), smooth.unwrap_or(false), symbols.unwrap_or(true)) {
        (true, true, true) => "smoothMarker",
        (true, true, false) => "smooth",
        (true, false, true) => "lineMarker",
        (true, false, false) => "line",
        (false, _, true) => "marker",
        (false, _, false) => "none",
    }
}

// <c:title> block used for the chart and axis titles.
fn title_xml(t: &str) -> String {
    format!(concat!(
        "<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>{}</a:t></a:r></a:p></c:rich></c:tx>",
        "<c:overlay val=\"0\"/></c:title>"
    ), esc_xml(t))
}

// Category/value axis pair (or two value axes for scatter), with the optional
// v2 decorations: axis titles, manual value bounds, major gridlines.
fn axes_xml(chart: &Value, ctype: &str) -> String {
    const X: u64 = 100_000_001;
    const Y: u64 = 100_000_002;
    let grid_on = |k: &str| chart.get("grid").and_then(|g| g.get(k)).and_then(|v| v.as_bool()).unwrap_or(false);
    let axis = |k: &str| chart.get(k);
    // One axis; `key` is "axisX"/"axisY", bounds only apply to value axes.
    let ax = |tag: &str, id: u64, pos: &str, cross: u64, key: &str, with_bounds: bool| {
        let mut scaling = String::from("<c:orientation val=\"minMax\"/>");
        if with_bounds {
            if let Some(v) = axis(key).and_then(|a| a.get("max")).and_then(|v| v.as_f64()) {
                scaling.push_str(&format!("<c:max val=\"{}\"/>", num_str(v)));
            }
            if let Some(v) = axis(key).and_then(|a| a.get("min")).and_then(|v| v.as_f64()) {
                scaling.push_str(&format!("<c:min val=\"{}\"/>", num_str(v)));
            }
        }
        let grid = if grid_on(if key == "axisX" { "x" } else { "y" }) { "<c:majorGridlines/>" } else { "" };
        let title = axis(key).and_then(|a| a.get("title")).and_then(|v| v.as_str())
            .filter(|t| !t.trim().is_empty()).map(|t| title_xml(t.trim())).unwrap_or_default();
        format!(concat!(
            "<c:{tag}><c:axId val=\"{id}\"/><c:scaling>{scaling}</c:scaling>",
            "<c:delete val=\"0\"/><c:axPos val=\"{pos}\"/>{grid}{title}<c:crossAx val=\"{cross}\"/></c:{tag}>"
        ), tag = tag, id = id, scaling = scaling, pos = pos, grid = grid, title = title, cross = cross)
    };
    // hbar swaps the axis positions (categories on the left).
    let (cat_pos, val_pos) = if ctype == "hbar" { ("l", "b") } else { ("b", "l") };
    if ctype == "scatter" {
        format!("{}{}", ax("valAx", X, "b", Y, "axisX", true), ax("valAx", Y, "l", X, "axisY", true))
    } else {
        format!("{}{}", ax("catAx", X, cat_pos, Y, "axisX", false), ax("valAx", Y, val_pos, X, "axisY", true))
    }
}

/// Build a chart part from the internal chart JSON. Returns None when no data
/// series can be derived (such a chart is skipped by the drawing writer).
pub fn build_chart_xml(chart: &Value, sheet_name: &str, data: &Value) -> Option<String> {
    let sers = collect_series(chart, data);
    if sers.is_empty() { return None; }
    let ctype = chart.get("type").and_then(|v| v.as_str()).unwrap_or("bar");
    let q = quote_sheet_name(sheet_name);
    let colors = str_list(chart.get("colors"));
    let gb = |k: &str| chart.get(k).and_then(|v| v.as_bool());
    let (symbols, smooth, lines) = (gb("symbols"), gb("smooth"), gb("lines"));
    let filled = gb("filled").unwrap_or(false);
    let explosion = chart.get("explode").and_then(|v| v.as_f64())
        .map(|e| (e * 100.0).round().clamp(0.0, 400.0) as i64).filter(|e| *e > 0);
    let dlbls = dlbls_xml(chart.get("dataLabels").and_then(|v| v.as_str()));

    // Grouping: preserved stacked/percentStacked, else the type's default.
    let grouping = chart.get("grouping").and_then(|v| v.as_str())
        .filter(|g| *g == "stacked" || *g == "percentStacked");
    let stacked = grouping.is_some();
    let axes = axes_xml(chart, ctype);
    let ax_ids = "<c:axId val=\"100000001\"/><c:axId val=\"100000002\"/>";

    // Render the <c:ser> run of one plot family, keeping global series indices.
    let ser_block = |family: Family, pick: &dyn Fn(&Ser) -> bool| -> String {
        let fill_color = matches!(family, Family::Bar | Family::Pie | Family::Area)
            || (family == Family::Radar && filled);
        let mut out = String::new();
        for (i, s) in sers.iter().enumerate() {
            if !pick(s) { continue; }
            let o = SerOpts {
                family, fill_color, symbols, smooth, explosion,
                dpt_colors: if i == 0 { &colors } else { &[] },
            };
            out.push_str(&ser_xml(i, s, &q, data, &o));
        }
        out
    };
    let all = |_: &Ser| true;

    let plot = match ctype {
        "pie" => format!("<c:pieChart><c:varyColors val=\"1\"/>{}{dlbls}</c:pieChart>", ser_block(Family::Pie, &all)),
        "donut" => {
            let hole = chart.get("holeSize").and_then(|v| v.as_f64())
                .map(|h| (h * 100.0).round().clamp(1.0, 90.0) as i64).unwrap_or(40);
            format!(concat!(
                "<c:doughnutChart><c:varyColors val=\"1\"/>{}{}",
                "<c:firstSliceAng val=\"0\"/><c:holeSize val=\"{}\"/></c:doughnutChart>"
            ), ser_block(Family::Pie, &all), dlbls, hole)
        }
        "line" | "area" => {
            let (tag, fam) = if ctype == "line" { ("lineChart", Family::Line) } else { ("areaChart", Family::Area) };
            let g = grouping.unwrap_or("standard");
            let marker = if ctype == "line" && symbols == Some(true) { "<c:marker val=\"1\"/>" } else { "" };
            format!(
                "<c:{tag}><c:grouping val=\"{g}\"/><c:varyColors val=\"0\"/>{}{dlbls}{marker}{ax_ids}</c:{tag}>{axes}",
                ser_block(fam, &all)
            )
        }
        "scatter" => format!(concat!(
            "<c:scatterChart><c:scatterStyle val=\"{}\"/><c:varyColors val=\"0\"/>",
            "{}{}{}</c:scatterChart>{}"
        ), scatter_style(lines, smooth, symbols), ser_block(Family::Scatter, &all), dlbls, ax_ids, axes),
        "radar" => {
            let style = if filled { "filled" } else if symbols == Some(true) { "marker" } else { "standard" };
            format!(
                "<c:radarChart><c:radarStyle val=\"{style}\"/><c:varyColors val=\"0\"/>{}{dlbls}{ax_ids}</c:radarChart>{axes}",
                ser_block(Family::Radar, &all)
            )
        }
        "combo" => {
            // Column + line: two plot elements sharing the same axis pair.
            let bars = ser_block(Family::Bar, &|s| !s.line_in_combo);
            let line_sers = ser_block(Family::Line, &|s| s.line_in_combo);
            let mut out = String::new();
            if !bars.is_empty() {
                let g = grouping.unwrap_or("clustered");
                let overlap = if stacked { "<c:overlap val=\"100\"/>" } else { "" };
                out.push_str(&format!(concat!(
                    "<c:barChart><c:barDir val=\"col\"/><c:grouping val=\"{}\"/><c:varyColors val=\"0\"/>",
                    "{}{}<c:gapWidth val=\"150\"/>{}{}</c:barChart>"
                ), g, bars, dlbls, overlap, ax_ids));
            }
            if !line_sers.is_empty() {
                let marker = if symbols == Some(true) { "<c:marker val=\"1\"/>" } else { "" };
                out.push_str(&format!(
                    "<c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{line_sers}{marker}{ax_ids}</c:lineChart>"
                ));
            }
            out.push_str(&axes);
            out
        }
        // "bar" | "hbar"
        _ => {
            let dir = if ctype == "hbar" { "bar" } else { "col" };
            let g = grouping.unwrap_or("clustered");
            let overlap = if stacked { "<c:overlap val=\"100\"/>" } else { "" };
            format!(concat!(
                "<c:barChart><c:barDir val=\"{}\"/><c:grouping val=\"{}\"/><c:varyColors val=\"0\"/>",
                "{}{}<c:gapWidth val=\"150\"/>{}{}</c:barChart>{}"
            ), dir, g, ser_block(Family::Bar, &all), dlbls, overlap, ax_ids, axes)
        }
    };

    let title = match chart.get("title").and_then(|v| v.as_str()).filter(|t| !t.trim().is_empty()) {
        Some(t) => format!("{}<c:autoTitleDeleted val=\"0\"/>", title_xml(t.trim())),
        None => String::new(),
    };
    // Legend: explicit flag wins; in auto mode mirror the frontend
    // (legendVisible): pies/donuts and multi-series charts show it.
    let legend_on = match chart.get("legend") {
        Some(Value::Bool(b)) => *b,
        _ => ctype == "pie" || ctype == "donut" || sers.len() > 1,
    };
    let legend = if legend_on {
        let pos = match chart.get("legendPos").and_then(|v| v.as_str()) {
            Some("left") => "l",
            Some("top") => "t",
            Some("bottom") => "b",
            _ => "r",
        };
        format!("<c:legend><c:legendPos val=\"{pos}\"/><c:overlay val=\"0\"/></c:legend>")
    } else { String::new() };

    // Chart-area formatting. CT_ChartSpace orders its children
    // c:chart, c:spPr, c:txPr, c:externalData — the shape properties therefore
    // come after </c:chart>, never inside it.
    let sp_pr = {
        let fill = shape::fill_xml(chart.get("fill").and_then(|v| v.as_str()));
        let ln = shape::line_xml(
            chart.get("border").and_then(|v| v.as_str()),
            chart.get("borderWidth").and_then(|v| v.as_f64()),
        );
        if fill.is_empty() && ln.is_empty() { String::new() } else { format!("<c:spPr>{fill}{ln}</c:spPr>") }
    };
    let tx_pr = match chart.get("font").and_then(|v| v.as_str()).map(str::trim).filter(|f| !f.is_empty()) {
        Some(f) => format!(concat!(
            "<c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr><a:latin typeface=\"{}\"/></a:defRPr></a:pPr>",
            "<a:endParaRPr lang=\"fr-FR\"/></a:p></c:txPr>"
        ), esc_xml(f)),
        None => String::new(),
    };

    Some(format!(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" ",
        "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
        "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">",
        "<c:chart>{}<c:plotArea><c:layout/>{}</c:plotArea>{}<c:plotVisOnly val=\"1\"/></c:chart>{}{}",
        "</c:chartSpace>"
    ), title, plot, legend, sp_pr, tx_pr))
}
