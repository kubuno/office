//! Worksheet part builder (xl/worksheets/sheetN.xml).
//!
//! The CT_Worksheet schema imposes a strict child order; sections are emitted
//! by dedicated functions IN THAT ORDER so later agents can slot new parts in
//! without breaking validity:
//!   sheetPr → dimension → sheetViews → sheetFormatPr → cols → sheetData →
//!   sheetProtection → autoFilter → mergeCells → conditionalFormatting →
//!   dataValidations → hyperlinks → pageMargins/pageSetup → drawing →
//!   tableParts → extLst
//! (Only a violation-free subset is produced today; add a new `write_*`
//! function and call it from `build_worksheet_xml` at the right position.)
use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{Map, Value};

use super::super::util::{col_to_idx, esc_xml, idx_to_col, split_ref, XLFN_FUNCTIONS, XLWS_FUNCTIONS, ERROR_LITERALS};
use super::strings::SharedStrings;
use super::styles::StyleRegistry;

/// One built worksheet part: the XML plus the sheet-level relationships it
/// references (today: external hyperlink targets, as (rId, URL) pairs).
pub struct WorksheetPart {
    pub xml:  String,
    pub rels: Vec<(String, String)>,
}

// ── Formula preparation ──────────────────────────────────────────────────────

/// Convert an internal formula (leading `=` already stripped) to the canonical
/// OOXML grammar: the engine accepts `;` as an argument separator — normalise
/// to `,` (outside strings and array literals, where `;` separates rows) — and
/// prefix modern functions with `_xlfn.` (and `_xlfn._xlws.` for SORT/FILTER).
pub fn prepare_formula(f: &str) -> String {
    let b = f.as_bytes();
    let mut out = String::with_capacity(f.len() + 8);
    let mut i = 0usize;
    let mut in_str = false;
    let mut brace_depth = 0u32;
    while i < b.len() {
        let c = b[i] as char;
        if in_str { out.push(c); if c == '"' { in_str = false } i += 1; continue; }
        match c {
            '"' => { in_str = true; out.push(c); i += 1; }
            '{' => { brace_depth += 1; out.push(c); i += 1; }
            '}' => { brace_depth = brace_depth.saturating_sub(1); out.push(c); i += 1; }
            ';' if brace_depth == 0 => { out.push(','); i += 1; }
            c if c.is_ascii_alphabetic() || c == '_' => {
                // Candidate identifier (function name, cell ref or defined name).
                let prev = out.chars().last().unwrap_or(' ');
                let boundary = !(prev.is_ascii_alphanumeric() || prev == '_' || prev == '.' || prev == '!' || prev == '\'' || prev == '$');
                let start = i;
                let mut j = i;
                while j < b.len() && ((b[j] as char).is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'.') { j += 1; }
                let ident = &f[start..j];
                let is_call = j < b.len() && b[j] == b'(';
                if boundary && is_call {
                    let up = ident.to_uppercase();
                    if XLWS_FUNCTIONS.contains(&up.as_str()) {
                        out.push_str("_xlfn._xlws.");
                        out.push_str(&up);
                    } else if XLFN_FUNCTIONS.contains(&up.as_str()) {
                        out.push_str("_xlfn.");
                        out.push_str(&up);
                    } else {
                        out.push_str(ident);
                    }
                } else {
                    out.push_str(ident);
                }
                i = j;
            }
            c => { out.push(c); i += 1; }
        }
    }
    out
}

// ── Section writers (in schema order) ────────────────────────────────────────

// Sorted list of (row, col_idx, key) for every cell of the sheet.
fn sorted_cells(cells: &Map<String, Value>) -> Vec<(i32, usize, &String)> {
    let mut list: Vec<(i32, usize, &String)> = cells.keys()
        .filter_map(|k| split_ref(k).map(|(c, r)| (r, col_to_idx(&c), k)))
        .collect();
    list.sort_unstable();
    list
}

// ── Outline (grouping) helpers ───────────────────────────────────────────────

// Per-axis outline state computed from the internal groups { start, end,
// collapsed }: details are start+1..=end, `start` is the summary index (the
// internal model keeps the summary BEFORE the details, hence
// <outlinePr summaryBelow="0" summaryRight="0"> is emitted when groups exist).
#[derive(Default)]
struct AxisOutline {
    levels:    HashMap<i64, u8>,
    hidden:    HashSet<i64>, // details of collapsed groups
    collapsed: HashSet<i64>, // summary indices of collapsed groups
    max:       u8,
}

fn axis_outline(groups: Option<&Vec<Value>>) -> AxisOutline {
    let mut out = AxisOutline::default();
    let Some(groups) = groups else { return out };
    for g in groups {
        let (Some(start), Some(end)) = (g.get("start").and_then(|v| v.as_i64()), g.get("end").and_then(|v| v.as_i64())) else { continue };
        if end < start + 1 || end - start > 1_048_576 { continue }
        let collapsed = g.get("collapsed").and_then(|v| v.as_bool()).unwrap_or(false);
        for i in (start + 1)..=end {
            let lv = out.levels.entry(i).or_insert(0);
            *lv = (*lv + 1).min(7); // Excel caps outline depth at 7
            out.max = out.max.max(*lv);
            if collapsed { out.hidden.insert(i); }
        }
        if collapsed { out.collapsed.insert(start); }
    }
    out
}

// ── Section writers (in schema order) ────────────────────────────────────────

fn write_sheet_pr(out: &mut String, data: &Value, has_outline: bool) {
    let tab = data.get("tab_color").and_then(|v| v.as_str())
        .map(|c| c.trim_start_matches('#').to_uppercase())
        .filter(|c| c.len() == 6 && c.chars().all(|ch| ch.is_ascii_hexdigit()));
    if tab.is_none() && !has_outline { return }
    out.push_str("<sheetPr>");
    if let Some(c) = tab { out.push_str(&format!("<tabColor rgb=\"FF{c}\"/>")); }
    if has_outline {
        // Internal groups keep the summary before the details.
        out.push_str("<outlinePr summaryBelow=\"0\" summaryRight=\"0\"/>");
    }
    out.push_str("</sheetPr>");
}

fn write_dimension(out: &mut String, sorted: &[(i32, usize, &String)]) {
    let dim = if sorted.is_empty() { "A1".to_string() } else {
        let min_r = sorted.iter().map(|(r, _, _)| *r).min().unwrap_or(1);
        let max_r = sorted.iter().map(|(r, _, _)| *r).max().unwrap_or(1);
        let min_c = sorted.iter().map(|(_, c, _)| *c).min().unwrap_or(0);
        let max_c = sorted.iter().map(|(_, c, _)| *c).max().unwrap_or(0);
        format!("{}{}:{}{}", idx_to_col(min_c), min_r, idx_to_col(max_c), max_r)
    };
    out.push_str(&format!("<dimension ref=\"{dim}\"/>"));
}

fn write_sheet_views(out: &mut String, data: &Value, is_first: bool) {
    let gridlines = data.get("gridlines").and_then(|v| v.as_bool()).unwrap_or(true);
    let frozen_rows = data.get("frozen_rows").and_then(|v| v.as_i64()).unwrap_or(0).max(0);
    let frozen_cols = data.get("frozen_cols").and_then(|v| v.as_i64()).unwrap_or(0).max(0);

    let mut view = String::from("<sheetView");
    if !gridlines { view.push_str(" showGridLines=\"0\""); }
    if is_first { view.push_str(" tabSelected=\"1\""); }
    view.push_str(" workbookViewId=\"0\"");
    if frozen_rows > 0 || frozen_cols > 0 {
        view.push('>');
        // Frozen panes: xSplit = frozen column count, ySplit = frozen row count;
        // topLeftCell = first cell of the unfrozen area.
        let top_left = format!("{}{}", idx_to_col(frozen_cols as usize), frozen_rows + 1);
        let pane = match (frozen_cols > 0, frozen_rows > 0) {
            (true, true) => "bottomRight",
            (false, true) => "bottomLeft",
            _ => "topRight",
        };
        let mut p = String::from("<pane");
        if frozen_cols > 0 { p.push_str(&format!(" xSplit=\"{frozen_cols}\"")); }
        if frozen_rows > 0 { p.push_str(&format!(" ySplit=\"{frozen_rows}\"")); }
        p.push_str(&format!(" topLeftCell=\"{top_left}\" activePane=\"{pane}\" state=\"frozen\"/>"));
        view.push_str(&p);
        view.push_str(&format!("<selection pane=\"{pane}\" activeCell=\"{top_left}\" sqref=\"{top_left}\"/>"));
        view.push_str("</sheetView>");
    } else {
        view.push_str("/>");
    }
    out.push_str(&format!("<sheetViews>{view}</sheetViews>"));
}

fn write_sheet_format(out: &mut String, data: &Value, row_out: &AxisOutline, col_out: &AxisOutline) {
    // Internal default row height is px; xlsx wants points (pt = px × 3/4).
    let drh_pt = data.get("default_row_height").and_then(|v| v.as_f64())
        .map(|px| px * 3.0 / 4.0).unwrap_or(15.0);
    let mut s = format!("<sheetFormatPr defaultRowHeight=\"{}\"", fmt_num(drh_pt));
    if let Some(px) = data.get("default_col_width").and_then(|v| v.as_f64()) {
        s.push_str(&format!(" defaultColWidth=\"{}\"", fmt_num(px_to_chars(px))));
    }
    if row_out.max > 0 { s.push_str(&format!(" outlineLevelRow=\"{}\"", row_out.max)); }
    if col_out.max > 0 { s.push_str(&format!(" outlineLevelCol=\"{}\"", col_out.max)); }
    s.push_str("/>");
    out.push_str(&s);
}

fn write_cols(out: &mut String, data: &Value, col_out: &AxisOutline) {
    let empty = Map::new();
    let widths = data.get("col_widths").and_then(|v| v.as_object()).unwrap_or(&empty);
    // Union of columns carrying a width and columns carrying outline info,
    // keyed by 0-based index: (width px, outline level, hidden, collapsed).
    let mut cols: BTreeMap<i64, (Option<f64>, u8, bool, bool)> = BTreeMap::new();
    for (k, v) in widths {
        if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphabetic()) { continue }
        if let Some(px) = v.as_f64() {
            cols.entry(col_to_idx(&k.to_uppercase()) as i64).or_insert((None, 0, false, false)).0 = Some(px);
        }
    }
    for (&i, &lv) in &col_out.levels {
        if i < 0 { continue }
        let e = cols.entry(i).or_insert((None, 0, false, false));
        e.1 = lv;
        if col_out.hidden.contains(&i) { e.2 = true; }
    }
    for &i in &col_out.collapsed {
        if i < 0 { continue }
        cols.entry(i).or_insert((None, 0, false, false)).3 = true;
    }
    if cols.is_empty() { return }
    out.push_str("<cols>");
    for (idx, (px, level, out_hidden, collapsed)) in cols {
        let n = idx + 1; // 1-based
        let mut c = format!("<col min=\"{n}\" max=\"{n}\"");
        match px {
            Some(px) if px <= 0.0 => c.push_str(" width=\"0\" hidden=\"1\" customWidth=\"1\""),
            Some(px) => {
                c.push_str(&format!(" width=\"{}\" customWidth=\"1\"", fmt_num(px_to_chars(px))));
                if out_hidden { c.push_str(" hidden=\"1\""); }
            }
            None if out_hidden => c.push_str(" width=\"8.43\" hidden=\"1\""),
            None => c.push_str(" width=\"8.43\""),
        }
        if level > 0 { c.push_str(&format!(" outlineLevel=\"{level}\"")); }
        if collapsed { c.push_str(" collapsed=\"1\""); }
        c.push_str("/>");
        out.push_str(&c);
    }
    out.push_str("</cols>");
}

#[allow(clippy::too_many_arguments)]
fn write_sheet_data(
    out: &mut String,
    data: &Value,
    sorted: &[(i32, usize, &String)],
    cells: &Map<String, Value>,
    styles: &mut StyleRegistry,
    sst: &mut SharedStrings,
    row_out: &AxisOutline,
) {
    let empty = Map::new();
    let row_heights = data.get("row_heights").and_then(|v| v.as_object()).unwrap_or(&empty);
    let col_styles = data.get("col_styles").and_then(|v| v.as_object()).unwrap_or(&empty);
    let row_styles = data.get("row_styles").and_then(|v| v.as_object()).unwrap_or(&empty);

    out.push_str("<sheetData>");
    let mut i = 0usize;
    // Rows that only carry a height/hidden flag or outline info (no cells)
    // still need a <row> element.
    let mut extra_rows: std::collections::BTreeSet<i32> = row_heights.keys()
        .filter_map(|k| k.parse::<i32>().ok()).collect();
    extra_rows.extend(row_out.levels.keys().filter_map(|&r| i32::try_from(r).ok()));
    extra_rows.extend(row_out.collapsed.iter().filter_map(|&r| i32::try_from(r).ok()).filter(|&r| r >= 1));
    let mut extra_iter = extra_rows.into_iter().peekable();

    while i < sorted.len() || extra_iter.peek().is_some() {
        // Next row number: the smaller of the next cell row and next extra row.
        let cell_row = sorted.get(i).map(|(r, _, _)| *r);
        let extra_row = extra_iter.peek().copied();
        let row = match (cell_row, extra_row) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => break,
        };
        while extra_iter.peek() == Some(&row) { extra_iter.next(); }

        let mut row_attrs = format!(" r=\"{row}\"");
        let px = row_heights.get(&row.to_string()).and_then(|v| v.as_f64());
        if let Some(px) = px {
            if px > 0.0 { row_attrs.push_str(&format!(" ht=\"{}\" customHeight=\"1\"", fmt_num(px * 3.0 / 4.0))); }
        }
        if px.is_some_and(|px| px <= 0.0) || row_out.hidden.contains(&(row as i64)) {
            row_attrs.push_str(" hidden=\"1\"");
        }
        if let Some(&lv) = row_out.levels.get(&(row as i64)) {
            row_attrs.push_str(&format!(" outlineLevel=\"{lv}\""));
        }
        if row_out.collapsed.contains(&(row as i64)) { row_attrs.push_str(" collapsed=\"1\""); }
        out.push_str(&format!("<row{row_attrs}>"));
        while i < sorted.len() && sorted[i].0 == row {
            let (_, col_idx, key) = sorted[i];
            if let Some(cell) = cells.get(key.as_str()) {
                write_cell(out, key, col_idx, row, cell, col_styles, row_styles, styles, sst);
            }
            i += 1;
        }
        out.push_str("</row>");
    }
    out.push_str("</sheetData>");
}

// Effective style of a cell: column style < row style < cell style (same
// precedence as the frontend's `styleAt`).
fn effective_style(
    col_idx: usize,
    row: i32,
    cell: &Value,
    col_styles: &Map<String, Value>,
    row_styles: &Map<String, Value>,
) -> Value {
    let cell_s = cell.get("s").and_then(|v| v.as_object());
    let col_s = col_styles.get(&idx_to_col(col_idx)).and_then(|v| v.as_object());
    let row_s = row_styles.get(&row.to_string()).and_then(|v| v.as_object());
    if col_s.is_none() && row_s.is_none() {
        return cell.get("s").cloned().unwrap_or(Value::Null);
    }
    let mut merged = Map::new();
    for src in [col_s, row_s, cell_s].into_iter().flatten() {
        for (k, v) in src { merged.insert(k.clone(), v.clone()); }
    }
    Value::Object(merged)
}

#[allow(clippy::too_many_arguments)]
fn write_cell(
    out: &mut String,
    key: &str,
    col_idx: usize,
    row: i32,
    cell: &Value,
    col_styles: &Map<String, Value>,
    row_styles: &Map<String, Value>,
    styles: &mut StyleRegistry,
    sst: &mut SharedStrings,
) {
    let style = effective_style(col_idx, row, cell, col_styles, row_styles);
    let xf = styles.xf_for(&style);
    let s_attr = if xf != 0 { format!(" s=\"{xf}\"") } else { String::new() };

    let v = cell.get("v");
    let formula = cell.get("f").and_then(|f| f.as_str()).map(|f| f.strip_prefix('=').unwrap_or(f));

    if let Some(f) = formula {
        let fx = esc_xml(&prepare_formula(f));
        // Cached result: numbers plainly, strings as t="str", booleans as t="b".
        match v {
            Some(Value::Number(n)) => out.push_str(&format!("<c r=\"{key}\"{s_attr}><f>{fx}</f><v>{n}</v></c>")),
            Some(Value::Bool(b)) => out.push_str(&format!("<c r=\"{key}\"{s_attr} t=\"b\"><f>{fx}</f><v>{}</v></c>", if *b { 1 } else { 0 })),
            Some(Value::String(txt)) if !txt.is_empty() => {
                if ERROR_LITERALS.contains(&txt.as_str()) {
                    out.push_str(&format!("<c r=\"{key}\"{s_attr} t=\"e\"><f>{fx}</f><v>{}</v></c>", esc_xml(txt)));
                } else {
                    out.push_str(&format!("<c r=\"{key}\"{s_attr} t=\"str\"><f>{fx}</f><v>{}</v></c>", esc_xml(txt)));
                }
            }
            _ => out.push_str(&format!("<c r=\"{key}\"{s_attr}><f>{fx}</f></c>")),
        }
        return;
    }

    match v {
        Some(Value::Number(n)) => out.push_str(&format!("<c r=\"{key}\"{s_attr}><v>{n}</v></c>")),
        Some(Value::Bool(b)) => out.push_str(&format!("<c r=\"{key}\"{s_attr} t=\"b\"><v>{}</v></c>", if *b { 1 } else { 0 })),
        Some(Value::String(txt)) if !txt.is_empty() => {
            if ERROR_LITERALS.contains(&txt.as_str()) {
                out.push_str(&format!("<c r=\"{key}\"{s_attr} t=\"e\"><v>{}</v></c>", esc_xml(txt)));
            } else {
                let idx = sst.index(txt);
                out.push_str(&format!("<c r=\"{key}\"{s_attr} t=\"s\"><v>{idx}</v></c>"));
            }
        }
        // Style-only (or empty) cell. A cell whose style happens to equal the
        // defaults (xf 0) must still be emitted, or it would vanish on a
        // round trip — write the explicit s="0" so the re-import keeps it.
        _ => {
            let has_style = cell.get("s").and_then(|s| s.as_object()).is_some_and(|o| !o.is_empty());
            if xf != 0 || has_style {
                out.push_str(&format!("<c r=\"{key}\" s=\"{xf}\"/>"));
            }
        }
    }
}

fn write_sheet_protection(out: &mut String, data: &Value) {
    let Some(p) = data.get("protection").and_then(|v| v.as_object()) else { return };
    let (Some(alg), Some(hash), Some(salt)) = (
        p.get("algorithmName").and_then(|v| v.as_str()),
        p.get("hashValue").and_then(|v| v.as_str()),
        p.get("saltValue").and_then(|v| v.as_str()),
    ) else { return };
    let spin = p.get("spinCount").and_then(|v| v.as_u64()).unwrap_or(100_000);
    out.push_str(&format!(
        "<sheetProtection algorithmName=\"{}\" hashValue=\"{}\" saltValue=\"{}\" spinCount=\"{spin}\" sheet=\"1\" objects=\"1\" scenarios=\"1\"/>",
        esc_xml(alg), esc_xml(hash), esc_xml(salt)
    ));
}

fn write_auto_filter(out: &mut String, data: &Value) {
    if let Some(r) = data.get("auto_filter").and_then(|v| v.as_str()).filter(|r| !r.is_empty()) {
        out.push_str(&format!("<autoFilter ref=\"{}\"/>", esc_xml(r)));
    }
}

fn write_merges(out: &mut String, data: &Value) {
    let Some(merges) = data.get("merges").and_then(|v| v.as_array()) else { return };
    let refs: Vec<&str> = merges.iter().filter_map(|m| m.as_str()).filter(|m| m.contains(':')).collect();
    if refs.is_empty() { return }
    out.push_str(&format!("<mergeCells count=\"{}\">", refs.len()));
    for r in refs { out.push_str(&format!("<mergeCell ref=\"{}\"/>", esc_xml(r))); }
    out.push_str("</mergeCells>");
}

// ── Data validation ──────────────────────────────────────────────────────────

// Internal NumOp → OOXML operator ("between" is the schema default and omitted).
fn dv_operator(op: &str) -> Option<&'static str> {
    match op {
        "notBetween" => Some("notBetween"),
        "gt" => Some("greaterThan"),
        "lt" => Some("lessThan"),
        "ge" => Some("greaterThanOrEqual"),
        "le" => Some("lessThanOrEqual"),
        "eq" => Some("equal"),
        "ne" => Some("notEqual"),
        _ => None, // "between"
    }
}

// One <dataValidation> element from an internal DVBlock, or None when the
// block is malformed.
fn dv_entry(block: &Value) -> Option<String> {
    let ranges: Vec<&str> = block.get("ranges")?.as_array()?
        .iter().filter_map(|r| r.as_str()).filter(|r| !r.is_empty()).collect();
    if ranges.is_empty() { return None }
    let rule = block.get("rule")?;
    let crit = rule.get("crit")?;
    let num_v = |k: &str| crit.get(k).and_then(|v| v.as_f64()).map(fmt_num);

    // (type, operator, formula1, formula2, hide_dropdown)
    let (ty, op, f1, f2, hide_dropdown) = match crit.get("kind").and_then(|v| v.as_str())? {
        "list" => {
            // Explicit values → quoted comma-separated literal; a comma inside a
            // value would break the format, so it is replaced by a space.
            let values: Vec<String> = crit.get("values")?.as_array()?
                .iter().filter_map(|v| v.as_str())
                .map(|v| v.replace([',', '"'], " ").trim().to_string())
                .collect();
            let hide = crit.get("dropdown").and_then(|v| v.as_bool()) == Some(false);
            ("list", None, format!("\"{}\"", values.join(",")), None, hide)
        }
        "listRange" => {
            let src = crit.get("source")?.as_str()?.trim_start_matches('=').to_string();
            if src.is_empty() { return None }
            let hide = crit.get("dropdown").and_then(|v| v.as_bool()) == Some(false);
            ("list", None, src, None, hide)
        }
        // Excel has no native checkbox validation → export the two states as a list.
        "checkbox" => {
            let on  = crit.get("on").and_then(|v| v.as_str()).unwrap_or("TRUE");
            let off = crit.get("off").and_then(|v| v.as_str()).unwrap_or("FALSE");
            ("list", None, format!("\"{},{}\"", on.replace([',', '"'], " "), off.replace([',', '"'], " ")), None, false)
        }
        "number" => {
            let op = crit.get("op").and_then(|v| v.as_str()).unwrap_or("between");
            let needs2 = matches!(op, "between" | "notBetween");
            ("decimal", dv_operator(op), num_v("v1")?, if needs2 { Some(num_v("v2").unwrap_or_else(|| num_v("v1").unwrap_or_default())) } else { None }, false)
        }
        "textLen" => {
            let op = crit.get("op").and_then(|v| v.as_str()).unwrap_or("between");
            let needs2 = matches!(op, "between" | "notBetween");
            ("textLength", dv_operator(op), num_v("v1")?, if needs2 { Some(num_v("v2").unwrap_or_else(|| num_v("v1").unwrap_or_default())) } else { None }, false)
        }
        _ => return None,
    };

    let reject = rule.get("reject").and_then(|v| v.as_bool()).unwrap_or(true);
    let help = rule.get("help").and_then(|v| v.as_str()).filter(|h| !h.trim().is_empty());

    let mut e = format!("<dataValidation type=\"{ty}\"");
    if let Some(op) = op { e.push_str(&format!(" operator=\"{op}\"")); }
    if !reject { e.push_str(" errorStyle=\"warning\""); }
    e.push_str(" allowBlank=\"1\"");
    if hide_dropdown { e.push_str(" showDropDown=\"1\""); } // legacy semantics: 1 HIDES the dropdown
    if let Some(h) = help { e.push_str(&format!(" showInputMessage=\"1\" prompt=\"{}\"", esc_xml(h))); }
    e.push_str(" showErrorMessage=\"1\"");
    e.push_str(&format!(" sqref=\"{}\">", esc_xml(&ranges.join(" "))));
    e.push_str(&format!("<formula1>{}</formula1>", esc_xml(&f1)));
    if let Some(f2) = f2 { e.push_str(&format!("<formula2>{f2}</formula2>")); }
    e.push_str("</dataValidation>");
    Some(e)
}

fn write_data_validations(out: &mut String, data: &Value) {
    let Some(blocks) = data.get("validations").and_then(|v| v.as_array()) else { return };
    let entries: Vec<String> = blocks.iter().filter_map(dv_entry).collect();
    if entries.is_empty() { return }
    out.push_str(&format!("<dataValidations count=\"{}\">", entries.len()));
    for e in &entries { out.push_str(e); }
    out.push_str("</dataValidations>");
}

// ── Hyperlinks ───────────────────────────────────────────────────────────────

// Emit <hyperlinks> from cells carrying a "link" key. External links get a
// relationship id (returned for the sheet .rels part); internal "#target"
// links use the `location` attribute.
fn write_hyperlinks(
    out: &mut String,
    sorted: &[(i32, usize, &String)],
    cells: &Map<String, Value>,
) -> Vec<(String, String)> {
    let mut rels: Vec<(String, String)> = Vec::new();
    let mut body = String::new();
    for (_, _, key) in sorted {
        let Some(link) = cells.get(key.as_str()).and_then(|c| c.get("link")).and_then(|l| l.as_str()) else { continue };
        if link.is_empty() { continue }
        if let Some(loc) = link.strip_prefix('#') {
            if loc.is_empty() { continue }
            body.push_str(&format!("<hyperlink ref=\"{}\" location=\"{}\"/>", esc_xml(key), esc_xml(loc)));
        } else {
            let rid = format!("rIdHl{}", rels.len() + 1);
            body.push_str(&format!("<hyperlink ref=\"{}\" r:id=\"{rid}\"/>", esc_xml(key)));
            rels.push((rid, link.to_string()));
        }
    }
    if !body.is_empty() {
        out.push_str("<hyperlinks>");
        out.push_str(&body);
        out.push_str("</hyperlinks>");
    }
    rels
}

// ── Unit helpers ─────────────────────────────────────────────────────────────

// px → xlsx column width in "characters" (inverse of the importer's px = w×7+5).
fn px_to_chars(px: f64) -> f64 {
    (((px - 5.0) / 7.0) * 100.0).round() / 100.0
}

fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 { format!("{}", v.round() as i64) } else { format!("{v}") }
}

// ── Entry point ──────────────────────────────────────────────────────────────

/// Build one worksheet part from the internal (snake_case) sheet_data JSON.
/// `is_active` marks the selected tab. Sections are written in the strict
/// CT_Worksheet order.
pub fn build_worksheet_xml(
    data: &Value,
    styles: &mut StyleRegistry,
    sst: &mut SharedStrings,
    is_active: bool,
) -> WorksheetPart {
    let empty = Map::new();
    let cells = data.get("cells").and_then(|v| v.as_object()).unwrap_or(&empty);
    let sorted = sorted_cells(cells);
    let row_out = axis_outline(data.get("row_groups").and_then(|v| v.as_array()));
    let col_out = axis_outline(data.get("col_groups").and_then(|v| v.as_array()));

    let mut out = String::with_capacity(1024 + cells.len() * 48);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    out.push_str("<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">");
    write_sheet_pr(&mut out, data, row_out.max > 0 || col_out.max > 0);
    write_dimension(&mut out, &sorted);
    write_sheet_views(&mut out, data, is_active);
    write_sheet_format(&mut out, data, &row_out, &col_out);
    write_cols(&mut out, data, &col_out);
    write_sheet_data(&mut out, data, &sorted, cells, styles, sst, &row_out);
    write_sheet_protection(&mut out, data);
    write_auto_filter(&mut out, data);
    write_merges(&mut out, data);
    super::condfmt::write_conditional_formatting(&mut out, data, styles);
    write_data_validations(&mut out, data);
    let rels = write_hyperlinks(&mut out, &sorted, cells);
    // Hook point: pageMargins / pageSetup / drawing / tableParts go here.
    out.push_str("</worksheet>");
    WorksheetPart { xml: out, rels }
}
