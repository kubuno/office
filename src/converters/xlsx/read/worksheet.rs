//! Worksheet part (xl/worksheets/sheetN.xml) parsing: cells (values, formulas,
//! styles), merged ranges, column widths / row heights, sheet view options,
//! frozen panes, tab colour, outline groups, data validations, hyperlinks,
//! sheet protection, autofilter and conditional formatting.
use std::collections::{HashMap, HashSet};

use quick_xml::events::Event;
use quick_xml::Reader;
use serde_json::{json, Map, Value};

use super::super::util::{attr, col_to_idx, idx_to_col, resolve_color, split_ref, strip_xlfn, translate_formula};
use super::super::XlsxSheet;
use super::styles::{style_for, Styles};

// Pending <cfRule> being parsed (attributes + typed children). Container marks
// which child element the cfvo/color children belong to.
#[derive(Default)]
struct CfPending {
    ty:        String,
    op:        String,
    dxf_id:    Option<usize>,
    stop:      bool,
    priority:  Option<i64>,
    text:      Option<String>, // containsText / beginsWith / endsWith operand
    period:    Option<String>, // timePeriod attribute
    rank:      Option<i64>,    // top10
    percent:   bool,           // top10
    bottom:    bool,           // top10
    above:     bool,           // aboveAverage (default true)
    equal:     bool,           // aboveAverage equalAverage
    std_dev:   Option<f64>,    // aboveAverage stdDev
    formulas:  Vec<String>,
    container: u8,             // 0 none, 1 colorScale, 2 dataBar, 3 iconSet
    cfvos:     Vec<Value>,     // { t, v? } thresholds of the active container
    colors:    Vec<String>,    // colorScale stop colours
    bar_color: Option<String>, // dataBar colour
    icon_set:  String,         // iconSet name
    show_value: bool,          // dataBar / iconSet showValue (default true)
    reverse:   bool,           // iconSet reverse
}

// Pending <dataValidation> being parsed (attributes + formula texts).
#[derive(Default)]
struct DvPending {
    ty:          String,
    op:          String,
    error_style: String,
    hide_dropdown: bool, // showDropDown="1" HIDES the in-cell dropdown (legacy semantics)
    prompt:      String,
    sqref:       String,
    f1:          String,
    f2:          String,
}

/// `hlinks` maps the sheet's hyperlink relationship ids to their external URLs.
pub fn parse_worksheet(xml: &str, shared: &[String], styles: &Styles, hlinks: &HashMap<String, String>) -> XlsxSheet {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut sheet = XlsxSheet { show_gridlines: true, ..Default::default() }; // overridden by <sheetView showGridLines="0">

    let mut cur_ref = String::new();
    let mut cur_type = String::new();
    let mut cur_xf: Option<usize> = None;
    let mut cur_formula: Option<String> = None;
    let mut cur_value = String::new();
    let mut in_v = false;
    let mut in_f = false;
    let mut in_is_t = false;
    // Shared formulas: the master cell (with text + si) defines the formula; the
    // followers carry only si and reuse the master, translated to their position.
    let mut shared_masters: HashMap<u32, (String, usize, i32)> = HashMap::new();
    let mut cur_f_si: Option<u32> = None;
    // Conditional formatting state.
    let mut cf_ranges: Vec<String> = Vec::new();
    let mut cf_rules: Vec<Value> = Vec::new();
    let mut cur_rule: Option<CfPending> = None;
    let mut in_cf_formula = false;
    let mut cf_formula_buf = String::new();
    let mut in_cf_ext = 0u32; // depth inside a cfRule <extLst> (x14 children are skipped)
    // Outline (grouping) state: per-row / per-column outline levels + the
    // summary-side flags of <outlinePr> (defaults per the spec: below/right).
    let mut summary_below = true;
    let mut summary_right = true;
    let mut row_levels: HashMap<i64, u8> = HashMap::new();
    let mut row_marks: HashSet<i64> = HashSet::new(); // rows carrying collapsed="1"
    let mut col_levels: HashMap<i64, u8> = HashMap::new();
    let mut col_marks: HashSet<i64> = HashSet::new();
    // Data-validation state.
    let mut cur_dv: Option<DvPending> = None;
    let mut in_dv_f: u8 = 0; // 1 = inside <formula1>, 2 = inside <formula2>
    let mut dv_f_buf = String::new();
    // x14 (extLst) conditional-formatting state: Excel 2010+ stores the "new"
    // icon sets (3Triangles, 3Stars, 5Boxes…) exclusively as
    // <x14:conditionalFormatting> blocks with <xm:sqref>/<xm:f> children.
    let mut x14_ranges: Vec<String> = Vec::new();
    let mut x14_rules: Vec<Value> = Vec::new();
    let mut x14_rule: Option<CfPending> = None;
    let mut x14_in_cfvo = false;
    let mut x14_in_sqref = false;
    let mut x14_in_f = false;
    let mut x14_buf = String::new();

    loop {
        match reader.read_event() {
            // A self-closing styled cell `<c r=".." s=".."/>` carries no value/formula
            // and gets no End event — emit its style now (e.g. an empty blue header cell).
            Ok(Event::Empty(e)) if e.local_name().as_ref() == b"c" => {
                let r = attr(&e, b"r").unwrap_or_default();
                let xf = attr(&e, b"s").and_then(|v| v.parse::<usize>().ok());
                if let (true, Some(xf)) = (split_ref(&r).is_some(), xf) {
                    if let Some(s) = style_for(styles, xf) {
                        let mut obj = Map::new();
                        obj.insert("s".into(), s);
                        sheet.cells.insert(r, Value::Object(obj));
                    }
                }
            }
            // <extLst> inside an open cfRule (x14 dataBar extensions…): skip its
            // children so x14 cfvo/color elements don't pollute the rule. Depth is
            // tracked on Start only (a self-closing <extLst/> has no children).
            Ok(Event::Start(e)) if e.local_name().as_ref() == b"extLst" && cur_rule.is_some() => in_cf_ext += 1,
            // A self-closing <cfRule …/> (top10, aboveAverage, duplicateValues…)
            // gets no End event — finalize it immediately.
            Ok(Event::Empty(e)) if e.local_name().as_ref() == b"cfRule" && e.name().as_ref() == b"cfRule" => {
                cf_rules.push(cf_rule_json(cf_pending_from(&e), styles));
            }
            // x14 / xm prefixed elements (worksheet <extLst> conditional formatting):
            // handled apart so their local names (cfRule, conditionalFormatting, f…)
            // never leak into the classic state machine.
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.name().as_ref().starts_with(b"x14:") || e.name().as_ref().starts_with(b"xm:") => {
                match e.name().as_ref() {
                    b"x14:conditionalFormatting" => { x14_ranges.clear(); x14_rules.clear(); }
                    b"x14:cfRule" => { x14_rule = Some(cf_pending_from(&e)); }
                    b"x14:iconSet" => {
                        if let Some(p) = x14_rule.as_mut() {
                            p.container = 3;
                            if let Some(s) = attr(&e, b"iconSet") { p.icon_set = s; }
                            p.show_value = attr(&e, b"showValue").as_deref() != Some("0");
                            p.reverse = bool_attr(&attr(&e, b"reverse"));
                        }
                    }
                    b"x14:cfvo" => {
                        if let Some(p) = x14_rule.as_mut() {
                            // x14 threshold types mirror the classic ones plus autoMin/autoMax.
                            let t = match attr(&e, b"type").unwrap_or_default().as_str() {
                                "autoMin" => "min".to_string(),
                                "autoMax" => "max".to_string(),
                                other => other.to_string(),
                            };
                            p.cfvos.push(json!({ "t": t }));
                            x14_in_cfvo = true;
                        }
                    }
                    b"xm:sqref" => { x14_in_sqref = true; x14_buf.clear(); }
                    // Threshold value of the current x14 cfvo (other xm:f uses are ignored).
                    b"xm:f" if x14_in_cfvo => { x14_in_f = true; x14_buf.clear(); }
                    _ => {}
                }
            }
            Ok(Event::End(e)) if e.name().as_ref().starts_with(b"x14:") || e.name().as_ref().starts_with(b"xm:") => {
                match e.name().as_ref() {
                    b"xm:f" if x14_in_f => {
                        if let Some(o) = x14_rule.as_mut().and_then(|p| p.cfvos.last_mut()) {
                            let v = x14_buf.trim().to_string();
                            if !v.is_empty() {
                                o["v"] = v.parse::<f64>().map(|n| json!(n)).unwrap_or_else(|_| json!(v));
                            }
                        }
                        x14_in_f = false;
                    }
                    b"xm:sqref" => {
                        x14_ranges = x14_buf.split_whitespace().map(|s| s.to_string()).collect();
                        x14_in_sqref = false;
                    }
                    b"x14:cfvo" => x14_in_cfvo = false,
                    b"x14:cfRule" => {
                        // Only standalone x14 rule types are kept; dataBar/colorScale x14
                        // entries merely extend a classic rule and would duplicate it.
                        if let Some(p) = x14_rule.take().filter(|p| p.ty == "iconSet") {
                            x14_rules.push(cf_rule_json(p, styles));
                        }
                    }
                    b"x14:conditionalFormatting" if !x14_ranges.is_empty() && !x14_rules.is_empty() => {
                        sheet.cond_formats.push(json!({ "ranges": x14_ranges.clone(), "rules": x14_rules.clone() }));
                    }
                    _ => {}
                }
            }
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let ln = e.local_name();
                match ln.as_ref() {
                    b"col" => {
                        let min = attr(&e, b"min").and_then(|v| v.parse::<usize>().ok());
                        let max = attr(&e, b"max").and_then(|v| v.parse::<usize>().ok());
                        let w = attr(&e, b"width").and_then(|v| v.parse::<f64>().ok());
                        let hidden = bool_attr(&attr(&e, b"hidden"));
                        let level = attr(&e, b"outlineLevel").and_then(|v| v.parse::<u8>().ok()).unwrap_or(0);
                        let collapsed = bool_attr(&attr(&e, b"collapsed"));
                        if let (Some(min), Some(max)) = (min, max) {
                            // A hidden column collapses to width 0; otherwise use its width.
                            // Cap the range so a trailing "min..16384" default block doesn't
                            // write 16k entries.
                            let hi = if max > 256 { min } else { max };
                            let px = if hidden { Some(0.0) } else { w.map(|w| (w * 7.0 + 5.0).round()) };
                            if let Some(px) = px {
                                for c in min..=hi { sheet.col_widths.insert(idx_to_col(c - 1), px); }
                            }
                            for c in min..=hi {
                                // 0-based column indices, matching the internal col_groups model.
                                if level > 0 { col_levels.insert(c as i64 - 1, level); }
                                if collapsed { col_marks.insert(c as i64 - 1); }
                            }
                        }
                    }
                    b"row" => {
                        let r = attr(&e, b"r").and_then(|v| v.parse::<i32>().ok());
                        let ht = attr(&e, b"ht").and_then(|v| v.parse::<f64>().ok());
                        let hidden = bool_attr(&attr(&e, b"hidden"));
                        let level = attr(&e, b"outlineLevel").and_then(|v| v.parse::<u8>().ok()).unwrap_or(0);
                        if let Some(r) = r {
                            if level > 0 { row_levels.insert(r as i64, level); }
                            if bool_attr(&attr(&e, b"collapsed")) { row_marks.insert(r as i64); }
                            // Insert immediately (self-closing <row/> rows get no End event).
                            if hidden { sheet.row_heights.insert(r, 0.0); }             // hidden row → height 0
                            else if let Some(h) = ht { sheet.row_heights.insert(r, (h * 4.0 / 3.0).round()); }
                        }
                    }
                    b"outlinePr" => {
                        if let Some(v) = attr(&e, b"summaryBelow") { summary_below = bool_attr(&Some(v)); }
                        if let Some(v) = attr(&e, b"summaryRight") { summary_right = bool_attr(&Some(v)); }
                    }
                    b"tabColor" => {
                        let rgb = attr(&e, b"rgb");
                        let theme = attr(&e, b"theme").and_then(|v| v.parse::<usize>().ok());
                        let tint = attr(&e, b"tint").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                        sheet.tab_color = resolve_color(rgb.as_deref(), theme, tint);
                    }
                    b"pane" => {
                        // Frozen panes: xSplit = frozen column count, ySplit = frozen
                        // row count (integers when state is frozen/frozenSplit).
                        if matches!(attr(&e, b"state").as_deref(), Some("frozen") | Some("frozenSplit")) {
                            sheet.frozen_cols = attr(&e, b"xSplit").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) as i64;
                            sheet.frozen_rows = attr(&e, b"ySplit").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) as i64;
                        }
                    }
                    // Modern ECMA-376 hash only — the model stores it verbatim.
                    // Legacy 16-bit `password` hashes are not representable and
                    // are dropped (the sheet imports unprotected).
                    b"sheetProtection" if bool_attr(&attr(&e, b"sheet")) => {
                        if let (Some(alg), Some(hash), Some(salt), Some(spin)) = (
                            attr(&e, b"algorithmName"), attr(&e, b"hashValue"),
                            attr(&e, b"saltValue"), attr(&e, b"spinCount").and_then(|v| v.parse::<u32>().ok()),
                        ) {
                            sheet.protection = Some(json!({
                                "algorithmName": alg, "hashValue": hash,
                                "saltValue": salt, "spinCount": spin,
                            }));
                        }
                    }
                    // Keep the filtered range; per-column filter state has no
                    // internal model (frontend filters are transient).
                    b"autoFilter" if sheet.auto_filter.is_none() => {
                        sheet.auto_filter = attr(&e, b"ref").filter(|r| !r.is_empty());
                    }
                    b"dataValidation" => {
                        cur_dv = Some(DvPending {
                            ty:            attr(&e, b"type").unwrap_or_default(),
                            op:            attr(&e, b"operator").unwrap_or_default(),
                            error_style:   attr(&e, b"errorStyle").unwrap_or_default(),
                            hide_dropdown: bool_attr(&attr(&e, b"showDropDown")),
                            prompt:        attr_unescaped(&e, b"prompt").unwrap_or_default(),
                            sqref:         attr(&e, b"sqref").unwrap_or_default(),
                            f1: String::new(), f2: String::new(),
                        });
                    }
                    b"formula1" if cur_dv.is_some() => { in_dv_f = 1; dv_f_buf.clear(); }
                    b"formula2" if cur_dv.is_some() => { in_dv_f = 2; dv_f_buf.clear(); }
                    b"hyperlink" => {
                        // External links resolve through the sheet rels (r:id);
                        // internal ones carry a `location` and become "#<location>".
                        let rid = e.attributes().flatten()
                            .find(|a| a.key.as_ref().ends_with(b":id") || a.key.local_name().as_ref() == b"id")
                            .map(|a| String::from_utf8_lossy(&a.value).into_owned());
                        let location = attr_unescaped(&e, b"location").filter(|l| !l.is_empty());
                        let link = match rid.and_then(|id| hlinks.get(&id).cloned()) {
                            Some(url) => Some(url),
                            None => location.map(|l| format!("#{l}")),
                        };
                        if let (Some(link), Some(r)) = (link, attr(&e, b"ref")) {
                            // A range ref anchors the link on its top-left cell.
                            let anchor = r.split(':').next().unwrap_or(&r).to_string();
                            if split_ref(&anchor).is_some() {
                                let entry = sheet.cells.entry(anchor)
                                    .or_insert_with(|| Value::Object(Map::new()));
                                if let Some(obj) = entry.as_object_mut() {
                                    obj.insert("link".into(), json!(link));
                                }
                            }
                        }
                    }
                    b"c" => {
                        cur_ref = attr(&e, b"r").unwrap_or_default();
                        cur_type = attr(&e, b"t").unwrap_or_default();
                        cur_xf = attr(&e, b"s").and_then(|v| v.parse().ok());
                        cur_formula = None;
                        cur_value.clear();
                        in_f = false; in_v = false; cur_f_si = None;
                    }
                    b"f" => { in_f = true; cur_formula = Some(String::new()); cur_f_si = attr(&e, b"si").and_then(|v| v.parse().ok()); }
                    b"v" => { in_v = true; in_f = false; }
                    b"t" if cur_type == "inlineStr" => in_is_t = true,
                    b"mergeCell" => { if let Some(r) = attr(&e, b"ref") { sheet.merges.push(r); } }
                    b"sheetView" if attr(&e, b"showGridLines").as_deref() == Some("0") => sheet.show_gridlines = false,
                    b"sheetFormatPr" => {
                        sheet.default_row_height = attr(&e, b"defaultRowHeight").and_then(|v| v.parse::<f64>().ok()).map(|h| (h * 4.0 / 3.0).round());
                        // defaultColWidth is in "characters" → px = width × 7 (max digit width) + 5 (cell padding).
                        sheet.default_col_width = attr(&e, b"defaultColWidth").and_then(|v| v.parse::<f64>().ok()).map(|w| (w * 7.0 + 5.0).round());
                    }
                    b"conditionalFormatting" => {
                        cf_ranges = attr(&e, b"sqref").unwrap_or_default().split_whitespace().map(|s| s.to_string()).collect();
                        cf_rules = Vec::new();
                    }
                    b"cfRule" => { cur_rule = Some(cf_pending_from(&e)); }
                    b"colorScale" if in_cf_ext == 0 => { if let Some(p) = cur_rule.as_mut() { p.container = 1; } }
                    b"dataBar" if in_cf_ext == 0 => {
                        if let Some(p) = cur_rule.as_mut() {
                            p.container = 2;
                            p.show_value = attr(&e, b"showValue").as_deref() != Some("0");
                        }
                    }
                    b"iconSet" if in_cf_ext == 0 => {
                        if let Some(p) = cur_rule.as_mut() {
                            p.container = 3;
                            if let Some(s) = attr(&e, b"iconSet") { p.icon_set = s; }
                            p.show_value = attr(&e, b"showValue").as_deref() != Some("0");
                            p.reverse = bool_attr(&attr(&e, b"reverse"));
                        }
                    }
                    // Threshold of the active colorScale/dataBar/iconSet: numeric
                    // values are stored as numbers, formulas as strings.
                    b"cfvo" if in_cf_ext == 0 => {
                        if let Some(p) = cur_rule.as_mut().filter(|p| p.container != 0) {
                            let t = attr(&e, b"type").unwrap_or_default();
                            let mut o = json!({ "t": t });
                            if let Some(v) = attr_unescaped(&e, b"val") {
                                o["v"] = v.parse::<f64>().map(|n| json!(n)).unwrap_or_else(|_| json!(v));
                            }
                            p.cfvos.push(o);
                        }
                    }
                    b"color" if in_cf_ext == 0 && cur_rule.is_some() => {
                        // colorScale stop colour or dataBar colour, resolved through
                        // the real theme/indexed palettes of the styles part.
                        let tint = attr(&e, b"tint").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                        let theme = attr(&e, b"theme").and_then(|v| v.parse().ok());
                        let indexed = attr(&e, b"indexed").and_then(|v| v.parse().ok());
                        let c = styles.ctx.resolve(attr(&e, b"rgb").as_deref(), theme, indexed, tint);
                        if let (Some(c), Some(p)) = (c, cur_rule.as_mut()) {
                            match p.container { 1 => p.colors.push(c), 2 => p.bar_color = Some(c), _ => {} }
                        }
                    }
                    b"formula" if cur_rule.is_some() && in_cf_ext == 0 => { in_cf_formula = true; cf_formula_buf.clear(); }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                let txt = e.unescape().unwrap_or_default();
                if x14_in_sqref || x14_in_f { x14_buf.push_str(&txt); }
                else if in_cf_formula { cf_formula_buf.push_str(&txt); }
                else if in_dv_f > 0 { dv_f_buf.push_str(&txt); }
                else if in_v { cur_value.push_str(&txt); }
                else if in_f { if let Some(f) = cur_formula.as_mut() { f.push_str(&txt); } }
                else if in_is_t { cur_value.push_str(&txt); }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"v" => in_v = false,
                b"f" => in_f = false,
                b"t" if in_is_t => in_is_t = false,
                b"extLst" if in_cf_ext > 0 => in_cf_ext -= 1,
                b"formula" if in_cf_formula => {
                    in_cf_formula = false;
                    if let Some(p) = cur_rule.as_mut() { p.formulas.push(cf_formula_buf.clone()); }
                }
                b"formula1" if in_dv_f == 1 => {
                    if let Some(dv) = cur_dv.as_mut() { dv.f1 = dv_f_buf.trim().to_string(); }
                    in_dv_f = 0;
                }
                b"formula2" if in_dv_f == 2 => {
                    if let Some(dv) = cur_dv.as_mut() { dv.f2 = dv_f_buf.trim().to_string(); }
                    in_dv_f = 0;
                }
                b"dataValidation" => {
                    if let Some(block) = cur_dv.take().and_then(dv_to_block) {
                        sheet.validations.push(block);
                    }
                }
                b"cfRule" => {
                    if let Some(p) = cur_rule.take() {
                        cf_rules.push(cf_rule_json(p, styles));
                    }
                }
                b"conditionalFormatting" if !cf_ranges.is_empty() && !cf_rules.is_empty() => {
                    sheet.cond_formats.push(json!({ "ranges": cf_ranges.clone(), "rules": cf_rules.clone() }));
                }
                b"c" => {
                    let mut obj = Map::new();
                    // Resolve the formula text, expanding shared formulas.
                    let (cur_col, cur_row) = split_ref(&cur_ref).map(|(c, r)| (col_to_idx(&c), r)).unwrap_or((0, 0));
                    let formula: Option<String> = match (&cur_formula, cur_f_si) {
                        (Some(f), Some(si)) if !f.is_empty() => { // shared master (or plain with si)
                            shared_masters.insert(si, (f.clone(), cur_col, cur_row));
                            Some(f.clone())
                        }
                        (Some(f), Some(si)) => { // shared follower → translate the master
                            shared_masters.get(&si).map(|(mf, mc, mr)| translate_formula(mf, cur_col as i64 - *mc as i64, cur_row as i64 - *mr as i64))
                                .filter(|f| !f.is_empty()).or_else(|| if f.is_empty() { None } else { Some(f.clone()) })
                        }
                        (Some(f), None) if !f.is_empty() => Some(f.clone()),
                        _ => None,
                    };
                    if let Some(f) = formula {
                        // Modern functions arrive prefixed (`_xlfn.XLOOKUP`) — strip
                        // the prefix so the internal engine sees the bare name.
                        obj.insert("f".into(), json!(format!("={}", strip_xlfn(&f))));
                    }
                    let has_val = !cur_value.is_empty();
                    if has_val {
                        match cur_type.as_str() {
                            "s" => { // shared string index
                                if let Ok(i) = cur_value.parse::<usize>() {
                                    obj.insert("v".into(), json!(shared.get(i).cloned().unwrap_or_default()));
                                }
                            }
                            "b" => { obj.insert("v".into(), json!(cur_value == "1")); }
                            "str" | "inlineStr" | "e" => { obj.insert("v".into(), json!(cur_value.clone())); }
                            _ => {
                                if let Ok(n) = cur_value.parse::<f64>() { obj.insert("v".into(), json!(n)); }
                                else { obj.insert("v".into(), json!(cur_value.clone())); }
                            }
                        }
                    }
                    // A cell with content but no explicit style → default format (xf 0)
                    // so it inherits the workbook default font (Calibri).
                    let xf = cur_xf.or(if obj.contains_key("v") { Some(0) } else { None });
                    if let Some(xf) = xf { if let Some(s) = style_for(styles, xf) { obj.insert("s".into(), s); } }
                    if !obj.is_empty() && split_ref(&cur_ref).is_some() {
                        sheet.cells.insert(cur_ref.clone(), Value::Object(obj));
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    // Outline levels → internal groups { start, end, collapsed } (details are
    // start+1..=end, so `start` is one before the first detail row/column).
    sheet.row_groups = {
        let hidden = |r: i64| sheet.row_heights.get(&(r as i32)) == Some(&0.0);
        levels_to_groups(&row_levels, &row_marks, &hidden, summary_below)
    };
    sheet.col_groups = {
        let hidden = |c: i64| c >= 0 && sheet.col_widths.get(&idx_to_col(c as usize)) == Some(&0.0);
        levels_to_groups(&col_levels, &col_marks, &hidden, summary_right)
    };
    // A collapsed group already hides its details in the internal model — drop
    // the redundant 0-sizes so expanding the group actually reveals the rows.
    for g in &sheet.row_groups {
        if g["collapsed"] == json!(true) {
            let (s, e) = (g["start"].as_i64().unwrap_or(0), g["end"].as_i64().unwrap_or(0));
            for r in (s + 1)..=e {
                if sheet.row_heights.get(&(r as i32)) == Some(&0.0) { sheet.row_heights.remove(&(r as i32)); }
            }
        }
    }
    for g in &sheet.col_groups {
        if g["collapsed"] == json!(true) {
            let (s, e) = (g["start"].as_i64().unwrap_or(0), g["end"].as_i64().unwrap_or(0));
            for c in (s + 1)..=e {
                if c < 0 { continue }
                let letter = idx_to_col(c as usize);
                if sheet.col_widths.get(&letter) == Some(&0.0) { sheet.col_widths.remove(&letter); }
            }
        }
    }
    sheet
}

// "1"/"true" → true (OOXML boolean attribute values).
fn bool_attr(v: &Option<String>) -> bool {
    matches!(v.as_deref(), Some("1") | Some("true"))
}

// Attribute with XML entities unescaped (for free-text values: prompts,
// hyperlink locations…). `attr()` returns the raw bytes as-is.
fn attr_unescaped(e: &quick_xml::events::BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().flatten()
        .find(|a| a.key.local_name().as_ref() == name)
        .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

// ── Conditional formatting ───────────────────────────────────────────────────

// Parse the attributes of a <cfRule> element into a pending rule.
fn cf_pending_from(e: &quick_xml::events::BytesStart) -> CfPending {
    CfPending {
        ty:         attr(e, b"type").unwrap_or_default(),
        op:         attr(e, b"operator").unwrap_or_default(),
        dxf_id:     attr(e, b"dxfId").and_then(|v| v.parse().ok()),
        stop:       attr(e, b"stopIfTrue").as_deref() == Some("1"),
        priority:   attr(e, b"priority").and_then(|v| v.parse().ok()),
        text:       attr_unescaped(e, b"text"),
        period:     attr(e, b"timePeriod"),
        rank:       attr(e, b"rank").and_then(|v| v.parse().ok()),
        percent:    bool_attr(&attr(e, b"percent")),
        bottom:     bool_attr(&attr(e, b"bottom")),
        above:      attr(e, b"aboveAverage").as_deref() != Some("0"),
        equal:      bool_attr(&attr(e, b"equalAverage")),
        std_dev:    attr(e, b"stdDev").and_then(|v| v.parse().ok()),
        icon_set:   "3TrafficLights1".into(),
        show_value: true,
        ..Default::default()
    }
}

// Build the internal rule JSON from a parsed <cfRule>. The base shape
// { type, op, formulas, dxf, stop } is kept unchanged for backward
// compatibility; the typed extensions (cs/csv, bar, icons, rank/percent/bottom,
// above/equal/stdDev, text, period, priority) are added per rule type.
fn cf_rule_json(p: CfPending, styles: &Styles) -> Value {
    let dxf = p.dxf_id.and_then(|i| styles.dxfs.get(i).cloned()).unwrap_or_else(|| json!({}));
    let mut rule = json!({ "type": p.ty, "op": p.op, "formulas": p.formulas, "dxf": dxf, "stop": p.stop });
    if let Some(pr) = p.priority { rule["priority"] = json!(pr); }
    match p.ty.as_str() {
        "colorScale" if p.colors.len() >= 2 => {
            let last = p.colors.len() - 1;
            let mut cs = json!({ "lo": p.colors[0], "hi": p.colors[last] });
            if p.colors.len() >= 3 { cs["mid"] = json!(p.colors[1]); }
            rule["cs"] = cs;
            // Keep the thresholds only when they pair up with the colours.
            if p.cfvos.len() == p.colors.len() { rule["csv"] = json!(p.cfvos); }
        }
        "dataBar" => {
            let mut bar = json!({ "color": p.bar_color.unwrap_or_else(|| "#638EC6".to_string()) });
            if !p.show_value { bar["showValue"] = json!(false); }
            if p.cfvos.len() == 2 {
                bar["min"] = p.cfvos[0].clone();
                bar["max"] = p.cfvos[1].clone();
            }
            rule["bar"] = bar;
        }
        "iconSet" => {
            let mut ic = json!({ "set": p.icon_set, "cfvo": p.cfvos });
            if !p.show_value { ic["showValue"] = json!(false); }
            if p.reverse { ic["reverse"] = json!(true); }
            rule["icons"] = ic;
        }
        "top10" => {
            rule["rank"] = json!(p.rank.unwrap_or(10));
            if p.percent { rule["percent"] = json!(true); }
            if p.bottom { rule["bottom"] = json!(true); }
        }
        "aboveAverage" => {
            if !p.above { rule["above"] = json!(false); }
            if p.equal { rule["equal"] = json!(true); }
            if let Some(sd) = p.std_dev { rule["stdDev"] = json!(sd); }
        }
        "containsText" | "notContainsText" | "beginsWith" | "endsWith" => {
            if let Some(t) = p.text { rule["text"] = json!(t); }
        }
        "timePeriod" => {
            if let Some(per) = p.period { rule["period"] = json!(per); }
        }
        _ => {}
    }
    rule
}

// ── Outline groups ───────────────────────────────────────────────────────────

/// Convert per-index outline levels into group objects. For every level L,
/// each maximal run of consecutive indices with level ≥ L becomes one group
/// { start: run_start - 1, end: run_end, collapsed }. The collapsed flag comes
/// from the summary index (after the run when the summary is below/right,
/// before it otherwise) or from every detail index being hidden.
fn levels_to_groups(
    levels: &HashMap<i64, u8>,
    marks: &HashSet<i64>,
    hidden: &dyn Fn(i64) -> bool,
    summary_after: bool,
) -> Vec<Value> {
    if levels.is_empty() { return Vec::new(); }
    let max_level = levels.values().copied().max().unwrap_or(0);
    let mut idx: Vec<i64> = levels.keys().copied().collect();
    idx.sort_unstable();
    let mut groups: Vec<(i64, i64, bool)> = Vec::new();
    for level in 1..=max_level {
        let mut run: Option<(i64, i64)> = None;
        for &i in &idx {
            if levels.get(&i).copied().unwrap_or(0) < level { continue }
            run = match run {
                Some((a, b)) if i == b + 1 => Some((a, i)),
                Some((a, b)) => { push_group(&mut groups, a, b, marks, hidden, summary_after); Some((i, i)) }
                None => Some((i, i)),
            };
        }
        if let Some((a, b)) = run { push_group(&mut groups, a, b, marks, hidden, summary_after); }
    }
    groups.sort_unstable();
    groups.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    groups.into_iter().map(|(s, e, c)| json!({ "start": s, "end": e, "collapsed": c })).collect()
}

fn push_group(
    groups: &mut Vec<(i64, i64, bool)>,
    a: i64, b: i64,
    marks: &HashSet<i64>,
    hidden: &dyn Fn(i64) -> bool,
    summary_after: bool,
) {
    let mark = if summary_after { b + 1 } else { a - 1 };
    let collapsed = marks.contains(&mark) || (a..=b).all(hidden);
    groups.push((a - 1, b, collapsed));
}

// ── Data validation ──────────────────────────────────────────────────────────

// OOXML operator → internal NumOp (missing operator defaults to "between").
fn dv_op(op: &str) -> &'static str {
    match op {
        "notBetween" => "notBetween",
        "greaterThan" => "gt",
        "lessThan" => "lt",
        "greaterThanOrEqual" => "ge",
        "lessThanOrEqual" => "le",
        "equal" => "eq",
        "notEqual" => "ne",
        _ => "between",
    }
}

// Map a parsed <dataValidation> to the internal DVBlock JSON, or None when the
// rule has no internal equivalent (`custom` formulas, unparseable bounds).
fn dv_to_block(dv: DvPending) -> Option<Value> {
    let ranges: Vec<String> = dv.sqref.split_whitespace().map(|s| s.to_string()).collect();
    if ranges.is_empty() { return None }

    let op = dv_op(&dv.op);
    let needs_v2 = matches!(op, "between" | "notBetween");
    let crit = match dv.ty.as_str() {
        "list" => {
            let f1 = dv.f1.trim_start_matches('=').trim();
            if f1.is_empty() { return None }
            if f1.len() >= 2 && f1.starts_with('"') && f1.ends_with('"') {
                // Explicit value list: "a,b,c".
                let values: Vec<String> = f1[1..f1.len() - 1].split(',')
                    .map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect();
                let mut c = json!({ "kind": "list", "values": values });
                if dv.hide_dropdown { c["dropdown"] = json!(false); }
                c
            } else {
                // Range source (possibly sheet-qualified — kept as-is).
                let mut c = json!({ "kind": "listRange", "source": f1.to_uppercase() });
                if dv.hide_dropdown { c["dropdown"] = json!(false); }
                c
            }
        }
        // Whole/decimal/date/time all compare numbers (dates/times are serials).
        "whole" | "decimal" | "date" | "time" => {
            let v1 = dv.f1.trim().parse::<f64>().ok()?;
            let mut c = json!({ "kind": "number", "op": op, "v1": v1 });
            if needs_v2 { c["v2"] = json!(dv.f2.trim().parse::<f64>().ok()?); }
            c
        }
        "textLength" => {
            let v1 = dv.f1.trim().parse::<f64>().ok()?;
            let mut c = json!({ "kind": "textLen", "op": op, "v1": v1 });
            if needs_v2 { c["v2"] = json!(dv.f2.trim().parse::<f64>().ok()?); }
            c
        }
        // `custom` (free formula) and unknown types have no internal model.
        _ => return None,
    };

    // errorStyle stop (default) blocks the entry; warning/information only warn.
    let reject = !matches!(dv.error_style.as_str(), "warning" | "information");
    let mut rule = json!({ "crit": crit, "reject": reject });
    if crit_is_list(&rule) && reject { rule["crit"]["strict"] = json!(true); }
    if !dv.prompt.trim().is_empty() { rule["help"] = json!(dv.prompt.trim()); }
    Some(json!({ "ranges": ranges, "rule": rule }))
}

fn crit_is_list(rule: &Value) -> bool {
    matches!(rule["crit"]["kind"].as_str(), Some("list") | Some("listRange"))
}
