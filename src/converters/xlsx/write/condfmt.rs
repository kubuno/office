//! Conditional-formatting section builder: emits the `<conditionalFormatting>`
//! blocks of a worksheet part from the internal `cf` JSON
//! (`[{ ranges: [..], rules: [{ type, op, formulas, dxf, stop, … }] }]`) and
//! registers the differential formats (dxf) in the shared [`StyleRegistry`].
use std::collections::HashMap;

use serde_json::Value;

use super::super::util::esc_xml;
use super::styles::StyleRegistry;
use super::worksheet::prepare_formula;

/// Icon sets valid in the base (non-x14) OOXML namespace.
const ICON_SETS: &[&str] = &[
    "3Arrows", "3ArrowsGray", "3Flags", "3TrafficLights1", "3TrafficLights2",
    "3Signs", "3Symbols", "3Symbols2", "4Arrows", "4ArrowsGray", "4RedToBlack",
    "4Rating", "4TrafficLights", "5Arrows", "5ArrowsGray", "5Rating", "5Quarters",
];

// "$B$2:D9" → "B2" (top-left cell, `$` stripped) — the anchor the generated
// formulas of text/blank/error/timePeriod rules are written against.
fn anchor_of(range: &str) -> String {
    let first = range.split(':').next().unwrap_or(range);
    first.replace('$', "").trim().to_uppercase()
}

// Prepare an internal formula for a <formula> child: strip the leading '=',
// normalise separators, add the _xlfn. prefixes and escape for XML.
fn prep(f: &str) -> String {
    esc_xml(&prepare_formula(f.trim().trim_start_matches('=')))
}

// Quote a text operand as an in-formula string literal ("" doubling).
fn flit(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}

// Internal or OOXML cellIs operator name → canonical OOXML operator.
fn cellis_op(op: &str) -> Option<&'static str> {
    Some(match op {
        "greaterThan" | "gt" => "greaterThan",
        "lessThan" | "lt" => "lessThan",
        "greaterThanOrEqual" | "ge" => "greaterThanOrEqual",
        "lessThanOrEqual" | "le" => "lessThanOrEqual",
        "equal" | "eq" => "equal",
        "notEqual" | "ne" => "notEqual",
        "between" => "between",
        "notBetween" => "notBetween",
        _ => return None,
    })
}

// One `<cfvo/>` from the internal `{ t, v? }` threshold object; None when the
// type is unknown or a required value is missing.
fn cfvo_xml(o: &Value) -> Option<String> {
    let t = o.get("t").and_then(|v| v.as_str())?;
    if !matches!(t, "min" | "max" | "num" | "percent" | "percentile" | "formula") { return None }
    match o.get("v") {
        Some(Value::Number(n)) => Some(format!("<cfvo type=\"{t}\" val=\"{n}\"/>")),
        Some(Value::String(s)) if !s.is_empty() => Some(format!("<cfvo type=\"{t}\" val=\"{}\"/>", prep(s))),
        _ if matches!(t, "min" | "max") => Some(format!("<cfvo type=\"{t}\"/>")),
        _ => None,
    }
}

// The thresholds of a scale/bar/icon rule: the stored list when it is valid
// and has the expected count, the given defaults otherwise.
fn cfvos_or(stored: Option<&Vec<Value>>, defaults: Vec<String>) -> Vec<String> {
    if let Some(list) = stored {
        if list.len() == defaults.len() {
            let parsed: Vec<String> = list.iter().filter_map(cfvo_xml).collect();
            if parsed.len() == defaults.len() { return parsed; }
        }
    }
    defaults
}

// "#RRGGBB" → `<color rgb="FFRRGGBB"/>`.
fn color_xml(hex: &str) -> Option<String> {
    let h = hex.strip_prefix('#')?;
    if h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(format!("<color rgb=\"FF{}\"/>", h.to_uppercase()))
    } else { None }
}

// The generated comparison formula of a timePeriod rule (LibreOffice /
// Excel-compatible forms; `a` is the anchor cell).
fn time_period_formula(period: &str, a: &str) -> Option<String> {
    Some(match period {
        "today"     => format!("FLOOR({a},1)=TODAY()"),
        "yesterday" => format!("FLOOR({a},1)=TODAY()-1"),
        "tomorrow"  => format!("FLOOR({a},1)=TODAY()+1"),
        "last7Days" => format!("AND(TODAY()-FLOOR({a},1)<=6,FLOOR({a},1)<=TODAY())"),
        "thisWeek"  => format!("AND(TODAY()-ROUNDDOWN({a},0)<=WEEKDAY(TODAY())-1,ROUNDDOWN({a},0)-TODAY()<=7-WEEKDAY(TODAY()))"),
        "lastWeek"  => format!("AND(TODAY()-ROUNDDOWN({a},0)>=WEEKDAY(TODAY()),TODAY()-ROUNDDOWN({a},0)<WEEKDAY(TODAY())+7)"),
        "nextWeek"  => format!("AND(ROUNDDOWN({a},0)-TODAY()>7-WEEKDAY(TODAY()),ROUNDDOWN({a},0)-TODAY()<15-WEEKDAY(TODAY()))"),
        "thisMonth" => format!("AND(MONTH({a})=MONTH(TODAY()),YEAR({a})=YEAR(TODAY()))"),
        "lastMonth" => format!("AND(MONTH({a})=MONTH(EDATE(TODAY(),0-1)),YEAR({a})=YEAR(EDATE(TODAY(),0-1)))"),
        "nextMonth" => format!("AND(MONTH({a})=MONTH(EDATE(TODAY(),0+1)),YEAR({a})=YEAR(EDATE(TODAY(),0+1)))"),
        _ => return None,
    })
}

// One `<cfRule>` element, or None when the rule has no valid OOXML mapping.
fn cf_rule_xml(rule: &Value, priority: i64, anchor: &str, styles: &mut StyleRegistry) -> Option<String> {
    let ty = rule.get("type").and_then(|v| v.as_str())?;
    let stop = rule.get("stop").and_then(|v| v.as_bool()).unwrap_or(false);
    let get_bool = |k: &str| rule.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let formulas: Vec<&str> = rule.get("formulas").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|f| f.as_str()).filter(|f| !f.trim().is_empty()).collect())
        .unwrap_or_default();
    let dxf_id = rule.get("dxf").and_then(|d| styles.dxf_for(d));

    // (extra attributes, children) per type; dxf-less visual types clear dxf_id.
    let mut with_dxf = true;
    let (attrs, children): (String, String) = match ty {
        "expression" => {
            let f = formulas.first()?;
            (String::new(), format!("<formula>{}</formula>", prep(f)))
        }
        "cellIs" => {
            let op = cellis_op(rule.get("op").and_then(|v| v.as_str()).unwrap_or_default())?;
            let needs2 = matches!(op, "between" | "notBetween");
            let f1 = formulas.first()?;
            let mut ch = format!("<formula>{}</formula>", prep(f1));
            if needs2 { ch.push_str(&format!("<formula>{}</formula>", prep(formulas.get(1)?))); }
            (format!(" operator=\"{op}\""), ch)
        }
        "containsText" | "notContainsText" | "beginsWith" | "endsWith" => {
            let (op, tmpl): (&str, fn(&str, &str) -> String) = match ty {
                "containsText"    => ("containsText", |t, a| format!("NOT(ISERROR(SEARCH({t},{a})))")),
                "notContainsText" => ("notContains",  |t, a| format!("ISERROR(SEARCH({t},{a}))")),
                "beginsWith"      => ("beginsWith",   |t, a| format!("LEFT({a},LEN({t}))={t}")),
                _                 => ("endsWith",     |t, a| format!("RIGHT({a},LEN({t}))={t}")),
            };
            match rule.get("text").and_then(|v| v.as_str()) {
                Some(t) => (
                    format!(" operator=\"{op}\" text=\"{}\"", esc_xml(t)),
                    format!("<formula>{}</formula>", esc_xml(&tmpl(&flit(t), anchor))),
                ),
                // No operand stored: fall back to the stored comparison formula.
                None => {
                    let f = formulas.first()?;
                    (format!(" operator=\"{op}\""), format!("<formula>{}</formula>", prep(f)))
                }
            }
        }
        "containsBlanks"    => (String::new(), format!("<formula>LEN(TRIM({anchor}))=0</formula>")),
        "notContainsBlanks" => (String::new(), format!("<formula>LEN(TRIM({anchor}))&gt;0</formula>")),
        "containsErrors"    => (String::new(), format!("<formula>ISERROR({anchor})</formula>")),
        "notContainsErrors" => (String::new(), format!("<formula>NOT(ISERROR({anchor}))</formula>")),
        "timePeriod" => {
            let period = rule.get("period").and_then(|v| v.as_str())?;
            let f = time_period_formula(period, anchor)?;
            (format!(" timePeriod=\"{period}\""), format!("<formula>{}</formula>", esc_xml(&f)))
        }
        "duplicateValues" | "uniqueValues" => (String::new(), String::new()),
        "top10" => {
            let rank = rule.get("rank").and_then(|v| v.as_i64()).unwrap_or(10).max(1);
            let mut a = format!(" rank=\"{rank}\"");
            if get_bool("percent") { a.push_str(" percent=\"1\""); }
            if get_bool("bottom") { a.push_str(" bottom=\"1\""); }
            (a, String::new())
        }
        "aboveAverage" => {
            let mut a = String::new();
            if rule.get("above").and_then(|v| v.as_bool()) == Some(false) { a.push_str(" aboveAverage=\"0\""); }
            if get_bool("equal") { a.push_str(" equalAverage=\"1\""); }
            if let Some(sd) = rule.get("stdDev").and_then(|v| v.as_f64()) {
                if sd > 0.0 { a.push_str(&format!(" stdDev=\"{}\"", sd as i64)); }
            }
            (a, String::new())
        }
        "colorScale" => {
            with_dxf = false;
            let cs = rule.get("cs")?.as_object()?;
            let lo = cs.get("lo").and_then(|v| v.as_str()).and_then(color_xml)?;
            let hi = cs.get("hi").and_then(|v| v.as_str()).and_then(color_xml)?;
            let mid = cs.get("mid").and_then(|v| v.as_str()).and_then(color_xml);
            let defaults = if mid.is_some() {
                vec!["<cfvo type=\"min\"/>".into(), "<cfvo type=\"percentile\" val=\"50\"/>".into(), "<cfvo type=\"max\"/>".into()]
            } else {
                vec!["<cfvo type=\"min\"/>".into(), "<cfvo type=\"max\"/>".into()]
            };
            let cfvos = cfvos_or(rule.get("csv").and_then(|v| v.as_array()), defaults);
            let mut ch = String::from("<colorScale>");
            for v in &cfvos { ch.push_str(v); }
            ch.push_str(&lo);
            if let Some(m) = mid { ch.push_str(&m); }
            ch.push_str(&hi);
            ch.push_str("</colorScale>");
            (String::new(), ch)
        }
        "dataBar" => {
            with_dxf = false;
            let bar = rule.get("bar").and_then(|v| v.as_object());
            let color = bar.and_then(|b| b.get("color")).and_then(|v| v.as_str()).unwrap_or("#638EC6");
            let color = color_xml(color).unwrap_or_else(|| "<color rgb=\"FF638EC6\"/>".to_string());
            let show = bar.and_then(|b| b.get("showValue")).and_then(|v| v.as_bool()).unwrap_or(true);
            let min = bar.and_then(|b| b.get("min")).and_then(cfvo_xml).unwrap_or_else(|| "<cfvo type=\"min\"/>".into());
            let max = bar.and_then(|b| b.get("max")).and_then(cfvo_xml).unwrap_or_else(|| "<cfvo type=\"max\"/>".into());
            let attr = if show { "" } else { " showValue=\"0\"" };
            (String::new(), format!("<dataBar{attr}>{min}{max}{color}</dataBar>"))
        }
        "iconSet" => {
            with_dxf = false;
            let ic = rule.get("icons").and_then(|v| v.as_object());
            let set = ic.and_then(|i| i.get("set")).and_then(|v| v.as_str())
                .filter(|s| ICON_SETS.contains(s)).unwrap_or("3TrafficLights1");
            let n: usize = set.chars().next().and_then(|c| c.to_digit(10)).unwrap_or(3) as usize;
            let show = ic.and_then(|i| i.get("showValue")).and_then(|v| v.as_bool()).unwrap_or(true);
            let reverse = ic.and_then(|i| i.get("reverse")).and_then(|v| v.as_bool()).unwrap_or(false);
            // Default thresholds: evenly split percents (0, 100/n, 200/n, …).
            let defaults: Vec<String> = (0..n)
                .map(|i| format!("<cfvo type=\"percent\" val=\"{}\"/>", i * 100 / n))
                .collect();
            let cfvos = cfvos_or(ic.and_then(|i| i.get("cfvo")).and_then(|v| v.as_array()), defaults);
            let mut a = format!(" iconSet=\"{set}\"");
            if !show { a.push_str(" showValue=\"0\""); }
            if reverse { a.push_str(" reverse=\"1\""); }
            (String::new(), format!("<iconSet{a}>{}</iconSet>", cfvos.concat()))
        }
        _ => return None,
    };

    let mut out = format!("<cfRule type=\"{ty}\"");
    if with_dxf {
        if let Some(id) = dxf_id { out.push_str(&format!(" dxfId=\"{id}\"")); }
    }
    out.push_str(&format!(" priority=\"{priority}\""));
    if stop { out.push_str(" stopIfTrue=\"1\""); }
    out.push_str(&attrs);
    if children.is_empty() { out.push_str("/>"); } else { out.push_str(&format!(">{children}</cfRule>")); }
    Some(out)
}

/// Emit every `<conditionalFormatting>` block of a sheet (strict CT_Worksheet
/// position: after mergeCells, before dataValidations). Priorities are assigned
/// per sheet: rules without a stored priority (UI-created, newest first) outrank
/// imported ones, which keep their relative stored order.
pub fn write_conditional_formatting(out: &mut String, data: &Value, styles: &mut StyleRegistry) {
    let Some(blocks) = data.get("cf").and_then(|v| v.as_array()) else { return };

    let mut order: Vec<(usize, usize, Option<i64>)> = Vec::new();
    for (bi, block) in blocks.iter().enumerate() {
        if let Some(rules) = block.get("rules").and_then(|v| v.as_array()) {
            for (ri, r) in rules.iter().enumerate() {
                order.push((bi, ri, r.get("priority").and_then(|v| v.as_i64())));
            }
        }
    }
    let mut sorted: Vec<usize> = (0..order.len()).collect();
    sorted.sort_by_key(|&i| (order[i].2.is_some(), order[i].2.unwrap_or(0), i));
    let mut prio: HashMap<(usize, usize), i64> = HashMap::new();
    for (n, &i) in sorted.iter().enumerate() {
        prio.insert((order[i].0, order[i].1), n as i64 + 1);
    }

    for (bi, block) in blocks.iter().enumerate() {
        let ranges: Vec<&str> = block.get("ranges").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|r| r.as_str()).filter(|r| !r.trim().is_empty()).collect())
            .unwrap_or_default();
        if ranges.is_empty() { continue }
        let anchor = anchor_of(ranges[0]);
        if anchor.is_empty() { continue }
        let Some(rules) = block.get("rules").and_then(|v| v.as_array()) else { continue };
        let mut body = String::new();
        for (ri, rule) in rules.iter().enumerate() {
            let p = prio.get(&(bi, ri)).copied().unwrap_or(1);
            if let Some(x) = cf_rule_xml(rule, p, &anchor, styles) { body.push_str(&x); }
        }
        if !body.is_empty() {
            out.push_str(&format!("<conditionalFormatting sqref=\"{}\">", esc_xml(&ranges.join(" "))));
            out.push_str(&body);
            out.push_str("</conditionalFormatting>");
        }
    }
}
