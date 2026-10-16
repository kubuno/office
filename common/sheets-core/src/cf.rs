//! Conditional formatting (`computeCondFormats`, `formula-engine.ts:966-1513`): the sheet's rule blocks
//! evaluated into per-cell overrides - fill, text colour, bold, italic, a data bar, an icon - rule for rule.
//!
//! Rules run in priority order (rules without a stored priority - created in the editor - first, in block
//! order; imported rules by their OOXML priority); the first rule to set a channel wins; `stop` freezes a
//! cell once a rule matched it. Ranges larger than 60 000 cells are skipped.

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::Value as Json;

use crate::addr;
use crate::formula::eval::{evaluate, Ctx, Data};
use crate::formula::value::{Scalar, Value};
use crate::js;
use crate::jsdate;
use crate::formula::translate::translate_refs;

/// The overrides a cell receives (`CondStyle`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CondStyle {
    pub bg: Option<String>,
    pub color: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    /// A data bar: its colour and the filled fraction of the cell's width.
    pub bar: Option<(String, f64)>,
    /// An icon: the set's name, the icon's index (0 = lowest) and the number of icons.
    pub icon: Option<(String, usize, usize)>,
    pub hide_value: bool,
}

impl CondStyle {
    fn is_empty(&self) -> bool {
        self.bg.is_none() && self.color.is_none() && self.bold.is_none() && self.italic.is_none() && self.bar.is_none() && self.icon.is_none() && !self.hide_value
    }
}

/// A threshold (`Cfvo`): min, max, num, percent, percentile, formula.
#[derive(Clone, Debug, PartialEq)]
struct Cfvo {
    t: String,
    v: Option<Json>,
}

fn cfvo(v: Option<&Json>) -> Option<Cfvo> {
    let o = v?.as_object()?;
    Some(Cfvo { t: o.get("t").and_then(Json::as_str).unwrap_or("").to_string(), v: o.get("v").cloned() })
}

/// A data bar: colour, `showValue`, min and max thresholds.
type BarSpec = (String, Option<bool>, Option<Cfvo>, Option<Cfvo>);
/// An icon set: name, `showValue`, `reverse`, thresholds.
type IconSpec = (String, Option<bool>, Option<bool>, Option<Vec<Cfvo>>);

/// One rule as stored (`CondRule`), read leniently from JSON.
#[derive(Clone, Debug)]
struct Rule {
    ty: String,
    op: String,
    formulas: Vec<String>,
    dxf_bg: Option<String>,
    dxf_color: Option<String>,
    dxf_bold: Option<bool>,
    dxf_italic: Option<bool>,
    stop: bool,
    priority: Option<f64>,
    cs: Option<(String, Option<String>, String)>,
    csv: Option<Vec<Cfvo>>,
    bar: Option<BarSpec>,
    icons: Option<IconSpec>,
    text: Option<String>,
    period: Option<String>,
    rank: Option<f64>,
    percent: bool,
    bottom: bool,
    above: Option<bool>,
    equal: Option<bool>,
    std_dev: Option<f64>,
}

fn s(o: &serde_json::Map<String, Json>, k: &str) -> Option<String> {
    o.get(k).and_then(Json::as_str).map(str::to_string)
}

impl Rule {
    fn from_json(v: &Json) -> Option<Rule> {
        let o = v.as_object()?;
        let dxf = o.get("dxf").and_then(Json::as_object);
        let cs = o.get("cs").and_then(Json::as_object).map(|c| (s(c, "lo").unwrap_or_default(), c.get("mid").and_then(Json::as_str).map(str::to_string), s(c, "hi").unwrap_or_default()));
        let bar = o.get("bar").and_then(Json::as_object).map(|b| (s(b, "color").unwrap_or_default(), b.get("showValue").and_then(Json::as_bool), cfvo(b.get("min")), cfvo(b.get("max"))));
        let icons = o.get("icons").and_then(Json::as_object).map(|i| {
            (
                s(i, "set").unwrap_or_default(),
                i.get("showValue").and_then(Json::as_bool),
                i.get("reverse").and_then(Json::as_bool),
                i.get("cfvo").and_then(Json::as_array).map(|a| a.iter().filter_map(|c| cfvo(Some(c))).collect()),
            )
        });
        Some(Rule {
            ty: s(o, "type").unwrap_or_default(),
            op: s(o, "op").unwrap_or_default(),
            formulas: o.get("formulas").and_then(Json::as_array).map(|a| a.iter().map(|f| f.as_str().map(str::to_string).unwrap_or_else(|| crate::model::cell::js_string_of(f))).collect()).unwrap_or_default(),
            dxf_bg: dxf.and_then(|d| s(d, "bg")),
            dxf_color: dxf.and_then(|d| s(d, "color")),
            dxf_bold: dxf.and_then(|d| d.get("bold").and_then(Json::as_bool)),
            dxf_italic: dxf.and_then(|d| d.get("italic").and_then(Json::as_bool)),
            stop: o.get("stop").and_then(Json::as_bool).unwrap_or(false),
            priority: o.get("priority").and_then(Json::as_f64),
            cs,
            csv: o.get("csv").and_then(Json::as_array).map(|a| a.iter().filter_map(|c| cfvo(Some(c))).collect()),
            bar,
            icons,
            text: s(o, "text"),
            period: s(o, "period"),
            rank: o.get("rank").and_then(Json::as_f64),
            percent: o.get("percent").and_then(Json::as_bool).unwrap_or(false),
            bottom: o.get("bottom").and_then(Json::as_bool).unwrap_or(false),
            above: o.get("above").and_then(Json::as_bool),
            equal: o.get("equal").and_then(Json::as_bool),
            std_dev: o.get("stdDev").and_then(Json::as_f64),
        })
    }

    fn dxf_apply(&self, acc: &mut CondStyle) {
        if self.dxf_bg.is_some() && acc.bg.is_none() {
            acc.bg = self.dxf_bg.clone();
        }
        if self.dxf_color.is_some() && acc.color.is_none() {
            acc.color = self.dxf_color.clone();
        }
        if self.dxf_bold.is_some() && acc.bold.is_none() {
            acc.bold = self.dxf_bold;
        }
        if self.dxf_italic.is_some() && acc.italic.is_none() {
            acc.italic = self.dxf_italic;
        }
    }
}

// ── Colour helpers (colour scales) ──────────────────────────────────────────

fn hex_to_rgb(h: &str) -> [f64; 3] {
    let s = h.replace('#', "");
    let v = if s.chars().count() == 3 { s.chars().flat_map(|c| [c, c]).collect::<String>() } else { s };
    let six: String = v.chars().take(6).collect();
    // parseInt(…, 16): leading hex digits, NaN → the channels read 0 (NaN >> 16 & 255 = 0).
    let n = js::parse_int(&six, 16);
    let n = if n.is_nan() { 0i64 } else { n as i64 };
    [((n >> 16) & 255) as f64, ((n >> 8) & 255) as f64, (n & 255) as f64]
}

fn rgb_to_hex(r: f64, g: f64, b: f64) -> String {
    let h = |x: f64| format!("{:02x}", js::round(x).clamp(0.0, 255.0) as i64);
    format!("#{}{}{}", h(r), h(g), h(b))
}

fn lerp_hex(a: &str, b: &str, t: f64) -> String {
    let (ca, cb) = (hex_to_rgb(a), hex_to_rgb(b));
    rgb_to_hex(ca[0] + (cb[0] - ca[0]) * t, ca[1] + (cb[1] - ca[1]) * t, ca[2] + (cb[2] - ca[2]) * t)
}

fn color_scale_color(lo: &str, mid: Option<&str>, hi: &str, v: f64, bounds: &[f64]) -> String {
    if let (Some(mid), true) = (mid, bounds.len() >= 3) {
        let (b0, b1, b2) = (bounds[0], bounds[1], bounds[2]);
        if v <= b0 {
            return lo.to_string();
        }
        if v >= b2 {
            return hi.to_string();
        }
        if v <= b1 {
            return if b1 > b0 { lerp_hex(lo, mid, (v - b0) / (b1 - b0)) } else { mid.to_string() };
        }
        return if b2 > b1 { lerp_hex(mid, hi, (v - b1) / (b2 - b1)) } else { mid.to_string() };
    }
    let b0 = bounds[0];
    let b1 = bounds[bounds.len() - 1];
    if b1 <= b0 {
        return lo.to_string();
    }
    let t = ((v - b0) / (b1 - b0)).clamp(0.0, 1.0);
    match mid {
        None => lerp_hex(lo, hi, t),
        Some("") => lerp_hex(lo, hi, t),
        Some(m) => {
            if t <= 0.5 {
                lerp_hex(lo, m, t * 2.0)
            } else {
                lerp_hex(m, hi, (t - 0.5) * 2.0)
            }
        }
    }
}

// ── Cell readers with a per-pass cache ───────────────────────────────────────

const CF_ERROR_LITERALS: [&str; 9] = ["#NULL!", "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A", "#SPILL!", "#CALC!"];

struct CfCtx<'d> {
    data: Data<'d>,
    ctx: &'d Ctx<'d>,
    nums: RefCell<HashMap<String, Option<f64>>>,
    strs: RefCell<HashMap<String, String>>,
    errs: RefCell<HashMap<String, bool>>,
}

fn top_left(v: &Value) -> Scalar {
    match v {
        Value::M(m) => m.first().and_then(|r| r.first()).cloned().unwrap_or_else(Scalar::empty),
        Value::S(s) => s.clone(),
        Value::L(_) => Scalar::str("[object Object]"),
    }
}

impl CfCtx<'_> {
    fn eval(&self, f: &str) -> Value {
        evaluate(f, self.data, self.ctx, None)
    }

    /// `cellNumber`.
    fn num(&self, key: &str) -> Option<f64> {
        if let Some(v) = self.nums.borrow().get(key) {
            return *v;
        }
        let v = (|| {
            let cell = self.data.cells.get(key)?;
            if let Some(f) = cell.formula() {
                return match top_left(&self.eval(f)) {
                    Scalar::Num(n) if n.is_finite() => Some(n),
                    _ => None,
                };
            }
            match &cell.v {
                Some(Json::Number(n)) => n.as_f64().filter(|x| x.is_finite()),
                Some(Json::String(s)) if !js::trim(s).is_empty() => js::number(s),
                _ => None,
            }
        })();
        self.nums.borrow_mut().insert(key.to_string(), v);
        v
    }

    /// `cellStringRaw`.
    fn str(&self, key: &str) -> String {
        if let Some(v) = self.strs.borrow().get(key) {
            return v.clone();
        }
        let v = match self.data.cells.get(key) {
            None => String::new(),
            Some(cell) => match cell.formula() {
                Some(f) => match top_left(&self.eval(f)) {
                    Scalar::Err(e) => e.as_str().to_string(),
                    Scalar::Bool(b) => if b { "TRUE" } else { "FALSE" }.to_string(),
                    Scalar::Num(n) => js::to_string(n),
                    Scalar::Str(s) => s,
                },
                None => match &cell.v {
                    None | Some(Json::Null) => String::new(),
                    Some(Json::Bool(b)) => if *b { "TRUE" } else { "FALSE" }.to_string(),
                    Some(other) => crate::model::cell::js_string_of(other),
                },
            },
        };
        self.strs.borrow_mut().insert(key.to_string(), v.clone());
        v
    }

    /// `cellErrorRaw`.
    fn err(&self, key: &str) -> bool {
        if let Some(v) = self.errs.borrow().get(key) {
            return *v;
        }
        let v = match self.data.cells.get(key) {
            None => false,
            Some(cell) => match cell.formula() {
                Some(f) => top_left(&self.eval(f)).is_err(),
                None => matches!(&cell.v, Some(Json::String(s)) if CF_ERROR_LITERALS.contains(&s.as_str())),
            },
        };
        self.errs.borrow_mut().insert(key.to_string(), v);
        v
    }

    /// `valueKey`.
    fn value_key(&self, key: &str) -> Option<String> {
        if let Some(n) = self.num(key) {
            return Some(format!("n:{}", js::to_string(n)));
        }
        let s = self.str(key);
        let t = js::trim(&s);
        if t.is_empty() {
            None
        } else {
            Some(format!("t:{}", t.to_lowercase()))
        }
    }
}

/// `truthyCF`: a boolean true or a non-zero number (text and errors are false).
fn truthy(v: &Value) -> bool {
    match top_left(v) {
        Scalar::Bool(b) => b,
        Scalar::Num(n) => n != 0.0,
        _ => false,
    }
}

// ── Range statistics, lazy, per block ───────────────────────────────────────

struct Stats {
    rects: Vec<addr::Rect>,
    nums: RefCell<Option<Vec<f64>>>,
    sorted: RefCell<Option<Vec<f64>>>,
    counts: RefCell<Option<HashMap<String, usize>>>,
}

impl Stats {
    fn for_each(&self, mut f: impl FnMut(&str)) {
        for b in &self.rects {
            if b.area() > MAX_CELLS as u64 {
                continue;
            }
            for r in b.r1..=b.r2 {
                for c in b.c1..=b.c2 {
                    f(&addr::key(c, r));
                }
            }
        }
    }
    fn nums(&self, cx: &CfCtx) -> Vec<f64> {
        if let Some(n) = self.nums.borrow().as_ref() {
            return n.clone();
        }
        let mut list = Vec::new();
        self.for_each(|k| {
            if let Some(n) = cx.num(k) {
                list.push(n);
            }
        });
        *self.nums.borrow_mut() = Some(list.clone());
        list
    }
    fn sorted(&self, cx: &CfCtx) -> Vec<f64> {
        if let Some(s) = self.sorted.borrow().as_ref() {
            return s.clone();
        }
        let mut s = self.nums(cx);
        js::sort_numbers(&mut s);
        *self.sorted.borrow_mut() = Some(s.clone());
        s
    }
    fn min(&self, cx: &CfCtx) -> Option<f64> {
        self.sorted(cx).first().copied()
    }
    fn max(&self, cx: &CfCtx) -> Option<f64> {
        self.sorted(cx).last().copied()
    }
    fn avg(&self, cx: &CfCtx) -> Option<f64> {
        let l = self.nums(cx);
        if l.is_empty() {
            None
        } else {
            Some(l.iter().sum::<f64>() / l.len() as f64)
        }
    }
    fn std(&self, cx: &CfCtx) -> f64 {
        let l = self.nums(cx);
        if l.len() < 2 {
            return 0.0;
        }
        let m = l.iter().sum::<f64>() / l.len() as f64;
        (l.iter().map(|b| (b - m) * (b - m)).sum::<f64>() / (l.len() - 1) as f64).sqrt()
    }
    fn percentile(&self, cx: &CfCtx, p: f64) -> Option<f64> {
        let s = self.sorted(cx);
        if s.is_empty() {
            return None;
        }
        let x = p.clamp(0.0, 100.0) / 100.0 * (s.len() - 1) as f64;
        let i = x.floor() as usize;
        Some(if i + 1 < s.len() { s[i] + (s[i + 1] - s[i]) * (x - i as f64) } else { s[i] })
    }
    fn counts(&self, cx: &CfCtx) -> HashMap<String, usize> {
        if let Some(c) = self.counts.borrow().as_ref() {
            return c.clone();
        }
        let mut m = HashMap::new();
        self.for_each(|k| {
            if let Some(vk) = cx.value_key(k) {
                *m.entry(vk).or_insert(0) += 1;
            }
        });
        *self.counts.borrow_mut() = Some(m.clone());
        m
    }
}

/// `resolveCfvo`.
fn resolve_cfvo(cx: &CfCtx, stats: &Stats, c: Option<&Cfvo>, role: &str) -> Option<f64> {
    let as_num = |v: &Option<Json>| -> Option<f64> {
        match v {
            Some(Json::Number(n)) => n.as_f64().filter(|x| x.is_finite()),
            Some(Json::String(t)) if !js::trim(t).is_empty() => {
                if let Some(n) = js::number(t) {
                    return Some(n);
                }
                match top_left(&cx.eval(t)) {
                    Scalar::Num(n) if n.is_finite() => Some(n),
                    _ => None,
                }
            }
            _ => None,
        }
    };
    match c.map(|c| c.t.as_str()) {
        Some("min") => stats.min(cx),
        Some("max") => stats.max(cx),
        Some("num") | Some("formula") => as_num(&c?.v),
        Some("percent") => {
            let n = as_num(&c?.v)?;
            let (lo, hi) = (stats.min(cx)?, stats.max(cx)?);
            Some(lo + (hi - lo) * n / 100.0)
        }
        Some("percentile") => {
            let n = as_num(&c?.v)?;
            stats.percentile(cx, n)
        }
        _ => match role {
            "min" => stats.min(cx),
            "max" => stats.max(cx),
            _ => None,
        },
    }
}

/// `matchesTimePeriod`.
fn matches_time_period(period: &str, serial: f64, now: &jsdate::LocalNow) -> bool {
    let d = serial.floor();
    let t = ((jsdate::ymd(now.year as f64, now.month as f64 - 1.0, now.day as f64) - jsdate::EXCEL_EPOCH_MS) / jsdate::DAY_MS).floor();
    let dow = jsdate::parts(now.time_value()).map(|p| p.weekday).unwrap_or(0) as f64;
    let week_start = t - dow;
    match period {
        "today" => d == t,
        "yesterday" => d == t - 1.0,
        "tomorrow" => d == t + 1.0,
        "last7Days" => d <= t && d > t - 7.0,
        "thisWeek" => d >= week_start && d < week_start + 7.0,
        "lastWeek" => d >= week_start - 7.0 && d < week_start,
        "nextWeek" => d >= week_start + 7.0 && d < week_start + 14.0,
        "thisMonth" | "lastMonth" | "nextMonth" => {
            let Some(dt) = jsdate::parts(jsdate::from_time(jsdate::EXCEL_EPOCH_MS + d * jsdate::DAY_MS)) else { return false };
            let shift = match period {
                "lastMonth" => -1.0,
                "nextMonth" => 1.0,
                _ => 0.0,
            };
            let Some(r) = jsdate::parts(jsdate::ymd(now.year as f64, now.month as f64 - 1.0 + shift, 1.0)) else { return false };
            dt.year == r.year && dt.month == r.month
        }
        _ => false,
    }
}

/// `MAX_CELLS`: a range larger than this is skipped.
pub const MAX_CELLS: usize = 60_000;

fn cell_is_op(op: &str) -> Option<&'static str> {
    Some(match op {
        "greaterThan" | "gt" => "gt",
        "lessThan" | "lt" => "lt",
        "greaterThanOrEqual" | "ge" => "ge",
        "lessThanOrEqual" | "le" => "le",
        "equal" | "eq" => "eq",
        "notEqual" | "ne" => "ne",
        "between" => "between",
        "notBetween" => "notBetween",
        _ => return None,
    })
}

/// Lazily computed per-rule state (thresholds), shared by the cells of the rule.
#[derive(Default)]
struct RuleCache {
    bounds: Option<Option<Vec<f64>>>,
    lohi: Option<Option<(f64, f64)>>,
    thr: Option<Option<Vec<f64>>>,
    top: Option<Option<f64>>,
}

/// Applies one rule to one cell; returns whether the rule matched (drives `stop`).
#[allow(clippy::too_many_arguments)]
fn apply(cx: &CfCtx, rule: &Rule, stats: &Stats, cache: &mut RuleCache, key: &str, c: u32, r: u32, ac: u32, ar: u32, acc: &mut CondStyle) -> bool {
    let (dc, dr) = (c as i64 - ac as i64, r as i64 - ar as i64);
    let expr = |f: &str, acc: &mut CondStyle| -> bool {
        if !truthy(&cx.eval(&translate_refs(f, dc, dr))) {
            return false;
        }
        rule.dxf_apply(acc);
        true
    };
    match rule.ty.as_str() {
        "expression" => match rule.formulas.first() {
            Some(f) if !f.is_empty() => expr(f, acc),
            _ => false,
        },
        "cellIs" => {
            let Some(op) = cell_is_op(&rule.op) else { return false };
            let Some(f0) = rule.formulas.first().filter(|f| !f.is_empty()) else { return false };
            let cell_n = cx.num(key);
            let cell_s = if cell_n.is_some() { None } else { Some(cx.str(key)) };
            if cell_n.is_none() && cell_s.as_deref() == Some("") {
                return false;
            }
            let cmp = |x: &Scalar| -> f64 {
                if let (Some(n), Scalar::Num(y)) = (cell_n, x) {
                    return n - y;
                }
                if let (Some(n), Scalar::Str(t)) = (cell_n, x) {
                    if !js::trim(t).is_empty() {
                        if let Some(y) = js::number(t) {
                            return n - y;
                        }
                    }
                }
                let xs = match x {
                    Scalar::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
                    Scalar::Num(n) => js::to_string(*n),
                    Scalar::Str(s) => s.clone(),
                    Scalar::Err(e) => e.as_str().to_string(),
                }
                .to_lowercase();
                let cs = match cell_n {
                    Some(n) => js::to_string(n),
                    None => cell_s.clone().unwrap_or_default(),
                }
                .to_lowercase();
                match js::cmp_str(&cs, &xs) {
                    std::cmp::Ordering::Equal => 0.0,
                    std::cmp::Ordering::Less => -1.0,
                    std::cmp::Ordering::Greater => 1.0,
                }
            };
            let a = top_left(&cx.eval(&translate_refs(f0, dc, dr)));
            if a.is_err() {
                return false;
            }
            let ok = if op == "between" || op == "notBetween" {
                let Some(f1) = rule.formulas.get(1).filter(|f| !f.is_empty()) else { return false };
                let b = top_left(&cx.eval(&translate_refs(f1, dc, dr)));
                if b.is_err() {
                    return false;
                }
                let inside = cmp(&a) >= 0.0 && cmp(&b) <= 0.0;
                if op == "between" {
                    inside
                } else {
                    !inside
                }
            } else {
                let d = cmp(&a);
                match op {
                    "gt" => d > 0.0,
                    "lt" => d < 0.0,
                    "ge" => d >= 0.0,
                    "le" => d <= 0.0,
                    "eq" => d == 0.0,
                    _ => d != 0.0,
                }
            };
            if !ok {
                return false;
            }
            rule.dxf_apply(acc);
            true
        }
        "containsText" | "notContainsText" | "beginsWith" | "endsWith" => match rule.text.as_deref() {
            None | Some("") => match rule.formulas.first() {
                Some(f) if !f.is_empty() => expr(f, acc),
                _ => false,
            },
            Some(t) => {
                let needle = t.to_lowercase();
                let s = cx.str(key).to_lowercase();
                let ok = match rule.ty.as_str() {
                    "containsText" => s.contains(&needle),
                    "notContainsText" => !s.contains(&needle),
                    "beginsWith" => s.starts_with(&needle),
                    _ => s.ends_with(&needle),
                };
                if !ok {
                    return false;
                }
                rule.dxf_apply(acc);
                true
            }
        },
        "containsBlanks" | "notContainsBlanks" => {
            let want = rule.ty == "containsBlanks";
            if (js::trim(&cx.str(key)).is_empty()) != want {
                return false;
            }
            rule.dxf_apply(acc);
            true
        }
        "containsErrors" | "notContainsErrors" => {
            if cx.err(key) != (rule.ty == "containsErrors") {
                return false;
            }
            rule.dxf_apply(acc);
            true
        }
        "timePeriod" => {
            let Some(p) = rule.period.as_deref().filter(|p| !p.is_empty()) else { return false };
            match cx.num(key) {
                Some(n) if matches_time_period(p, n, &cx.ctx.env.now()) => {
                    rule.dxf_apply(acc);
                    true
                }
                _ => false,
            }
        }
        "duplicateValues" | "uniqueValues" => {
            let Some(vk) = cx.value_key(key) else { return false };
            let n = stats.counts(cx).get(&vk).copied().unwrap_or(0);
            let want_dup = rule.ty == "duplicateValues";
            if if want_dup { n < 2 } else { n != 1 } {
                return false;
            }
            rule.dxf_apply(acc);
            true
        }
        "top10" => {
            let Some(v) = cx.num(key) else { return false };
            let thr = *cache.top.get_or_insert_with(|| {
                let rank = rule.rank.unwrap_or(10.0).max(1.0);
                let mut list = stats.nums(cx);
                list.sort_by(|a, b| {
                    let o = if rule.bottom { a.partial_cmp(b) } else { b.partial_cmp(a) };
                    o.unwrap_or(std::cmp::Ordering::Equal)
                });
                if list.is_empty() {
                    return None;
                }
                let k = if rule.percent { js::round(list.len() as f64 * rank / 100.0).max(1.0) } else { rank };
                let idx = (k.min(list.len() as f64) - 1.0).max(0.0);
                // `list[k - 1]` with a fractional k is undefined → no threshold.
                if idx.fract() != 0.0 {
                    return None;
                }
                list.get(idx as usize).copied()
            });
            let Some(t) = thr else { return false };
            if if rule.bottom { v > t } else { v < t } {
                return false;
            }
            rule.dxf_apply(acc);
            true
        }
        "aboveAverage" => {
            let above = rule.above != Some(false);
            let eq = rule.equal == Some(true);
            let (Some(v), Some(avg)) = (cx.num(key), stats.avg(cx)) else { return false };
            let lim = match rule.std_dev {
                Some(sd) if sd > 0.0 => {
                    if above {
                        avg + sd * stats.std(cx)
                    } else {
                        avg - sd * stats.std(cx)
                    }
                }
                _ => avg,
            };
            let ok = if above {
                if eq {
                    v >= lim
                } else {
                    v > lim
                }
            } else if eq {
                v <= lim
            } else {
                v < lim
            };
            if !ok {
                return false;
            }
            rule.dxf_apply(acc);
            true
        }
        "colorScale" => {
            let Some((lo, mid, hi)) = &rule.cs else { return false };
            let Some(v) = cx.num(key) else { return false };
            let bounds = cache
                .bounds
                .get_or_insert_with(|| {
                    let stops = if mid.is_some() { 3 } else { 2 };
                    let defaults = if stops == 3 {
                        vec![Cfvo { t: "min".into(), v: None }, Cfvo { t: "percentile".into(), v: Some(Json::from(50)) }, Cfvo { t: "max".into(), v: None }]
                    } else {
                        vec![Cfvo { t: "min".into(), v: None }, Cfvo { t: "max".into(), v: None }]
                    };
                    let csv = match &rule.csv {
                        Some(c) if c.len() == stops => c.clone(),
                        _ => defaults,
                    };
                    let vals: Vec<Option<f64>> = csv.iter().enumerate().map(|(i, c)| resolve_cfvo(cx, stats, Some(c), if i == 0 { "min" } else if i == stops - 1 { "max" } else { "mid" })).collect();
                    if vals.iter().all(Option::is_some) {
                        Some(vals.into_iter().flatten().collect())
                    } else {
                        None
                    }
                })
                .clone();
            let Some(b) = bounds else { return false };
            if acc.bg.is_none() {
                acc.bg = Some(color_scale_color(lo, mid.as_deref(), hi, v, &b));
            }
            false
        }
        "dataBar" => {
            let Some((color, show, min, max)) = &rule.bar else { return false };
            if color.is_empty() {
                return false;
            }
            let Some(v) = cx.num(key) else { return false };
            let lohi = *cache.lohi.get_or_insert_with(|| {
                let min_c = min.clone().unwrap_or(Cfvo { t: "min".into(), v: None });
                let max_c = max.clone().unwrap_or(Cfvo { t: "max".into(), v: None });
                let lo = resolve_cfvo(cx, stats, Some(&min_c), "min");
                let hi = resolve_cfvo(cx, stats, Some(&max_c), "max");
                match (lo, hi) {
                    (Some(lo), Some(hi)) => Some((lo, hi)),
                    _ => None,
                }
            });
            let Some((lo, hi)) = lohi else { return false };
            let frac = if hi > lo { ((v - lo) / (hi - lo)).clamp(0.0, 1.0) } else if v >= hi { 1.0 } else { 0.0 };
            if acc.bar.is_none() {
                acc.bar = Some((color.clone(), frac));
                if *show == Some(false) {
                    acc.hide_value = true;
                }
            }
            false
        }
        "iconSet" => {
            let Some((set, show, reverse, cfvos)) = &rule.icons else { return false };
            if set.is_empty() {
                return false;
            }
            let n = set.chars().next().and_then(|c| c.to_digit(10)).filter(|&d| d != 0).unwrap_or(3).clamp(3, 5) as usize;
            let Some(v) = cx.num(key) else { return false };
            let thr = cache
                .thr
                .get_or_insert_with(|| {
                    let list: Vec<Cfvo> = match cfvos {
                        Some(c) if c.len() == n => c.clone(),
                        _ => (0..n).map(|i| Cfvo { t: "percent".into(), v: serde_json::Number::from_f64(i as f64 * 100.0 / n as f64).map(Json::Number) }).collect(),
                    };
                    let mut vals = Vec::new();
                    for c in list.iter().skip(1) {
                        vals.push(resolve_cfvo(cx, stats, Some(c), "mid")?);
                    }
                    Some(vals)
                })
                .clone();
            let Some(t) = thr else { return false };
            let mut idx = 0;
            for (i, th) in t.iter().enumerate() {
                if v >= *th {
                    idx = i + 1;
                }
            }
            if *reverse == Some(true) {
                idx = n - 1 - idx;
            }
            if acc.icon.is_none() {
                acc.icon = Some((set.clone(), idx, n));
                if *show == Some(false) {
                    acc.hide_value = true;
                }
            }
            false
        }
        _ => false,
    }
}

/// Whether a rule type has an evaluator (`applierFor` returns non-null).
fn has_applier(rule: &Rule) -> bool {
    match rule.ty.as_str() {
        "expression" => rule.formulas.first().is_some_and(|f| !f.is_empty()),
        "cellIs" => cell_is_op(&rule.op).is_some() && rule.formulas.first().is_some_and(|f| !f.is_empty()),
        "containsText" | "notContainsText" | "beginsWith" | "endsWith" => rule.text.as_deref().is_some_and(|t| !t.is_empty()) || rule.formulas.first().is_some_and(|f| !f.is_empty()),
        "containsBlanks" | "notContainsBlanks" | "containsErrors" | "notContainsErrors" | "duplicateValues" | "uniqueValues" | "top10" | "aboveAverage" => true,
        "timePeriod" => rule.period.as_deref().is_some_and(|p| !p.is_empty()),
        "colorScale" => rule.cs.is_some(),
        "dataBar" => rule.bar.as_ref().is_some_and(|b| !b.0.is_empty()),
        "iconSet" => rule.icons.as_ref().is_some_and(|i| !i.0.is_empty()),
        _ => false,
    }
}

/// `computeCondFormats(data)`: the overrides of every cell the sheet's `cf` blocks touch.
pub fn compute_cond_formats(cf: Option<&Json>, data: Data<'_>, ctx: &Ctx<'_>) -> HashMap<String, CondStyle> {
    let Some(blocks) = cf.and_then(Json::as_array).filter(|b| !b.is_empty()) else { return HashMap::new() };
    let cx = CfCtx { data, ctx: shrink(ctx), nums: RefCell::default(), strs: RefCell::default(), errs: RefCell::default() };
    struct Entry {
        rule: Rule,
        block: usize,
        ar: u32,
        ac: u32,
    }
    let mut entries = Vec::new();
    let mut stats = Vec::new();
    for block in blocks {
        let mut rects = Vec::new();
        let (mut ar, mut ac) = (u32::MAX, u32::MAX);
        for r in block.get("ranges").and_then(Json::as_array).into_iter().flatten() {
            if let Some(b) = r.as_str().and_then(addr::Rect::parse) {
                ar = ar.min(b.r1);
                ac = ac.min(b.c1);
                rects.push(b);
            }
        }
        if rects.is_empty() {
            continue;
        }
        let bi = stats.len();
        stats.push(Stats { rects, nums: RefCell::default(), sorted: RefCell::default(), counts: RefCell::default() });
        for rule in block.get("rules").and_then(Json::as_array).into_iter().flatten() {
            if let Some(rule) = Rule::from_json(rule) {
                entries.push(Entry { rule, block: bi, ar, ac });
            }
        }
    }
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by(|&a, &b| {
        let (pa, pb) = (entries[a].rule.priority, entries[b].rule.priority);
        match (pa, pb) {
            (None, None) => a.cmp(&b),
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(&b)),
        }
    });
    let mut out: HashMap<String, CondStyle> = HashMap::new();
    let mut stopped = std::collections::HashSet::new();
    for oi in order {
        let e = &entries[oi];
        if !has_applier(&e.rule) {
            continue;
        }
        let st = &stats[e.block];
        let mut cache = RuleCache::default();
        for b in &st.rects {
            if b.area() > MAX_CELLS as u64 {
                continue;
            }
            for r in b.r1..=b.r2 {
                for c in b.c1..=b.c2 {
                    let key = addr::key(c, r);
                    if stopped.contains(&key) {
                        continue;
                    }
                    let acc = out.entry(key.clone()).or_default();
                    if apply(&cx, &e.rule, st, &mut cache, &key, c, r, e.ac, e.ar, acc) && e.rule.stop {
                        stopped.insert(key);
                    }
                }
            }
        }
    }
    out.retain(|_, v| !v.is_empty());
    out
}

fn shrink<'a, 'e: 'a>(ctx: &'a Ctx<'e>) -> &'a Ctx<'a> {
    ctx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::testing::{env, sheet};
    use serde_json::json;

    fn run(cells: &crate::formula::CellMap, cf: Json) -> HashMap<String, CondStyle> {
        let names = HashMap::new();
        let e = env();
        let ctx = Ctx::new(&e);
        compute_cond_formats(Some(&cf), Data { cells, sheets: None, names: &names }, &ctx)
    }

    #[test]
    fn expression_rules_are_anchored_to_each_cell() {
        let c = sheet(&[("A1", json!(5)), ("A2", json!(15)), ("A3", json!(25))]);
        let out = run(&c, json!([{"ranges": ["A1:A3"], "rules": [{"type": "expression", "op": "", "formulas": ["=A1>10"], "dxf": {"bg": "#ffc7ce"}, "stop": false}]}]));
        assert!(!out.contains_key("A1"));
        assert_eq!(out["A2"].bg.as_deref(), Some("#ffc7ce"));
        assert_eq!(out["A3"].bg.as_deref(), Some("#ffc7ce"));
    }

    #[test]
    fn scales_bars_and_icons() {
        let c = sheet(&[("A1", json!(0)), ("A2", json!(50)), ("A3", json!(100))]);
        let out = run(&c, json!([{"ranges": ["A1:A3"], "rules": [{"type": "colorScale", "op": "", "formulas": [], "dxf": {}, "stop": false, "cs": {"lo": "#ff0000", "hi": "#00ff00"}}]}]));
        assert_eq!(out["A1"].bg.as_deref(), Some("#ff0000"));
        assert_eq!(out["A2"].bg.as_deref(), Some("#808000"));
        let out = run(&c, json!([{"ranges": ["A1:A3"], "rules": [{"type": "dataBar", "op": "", "formulas": [], "dxf": {}, "stop": false, "bar": {"color": "#638ec6"}}]}]));
        assert_eq!(out["A2"].bar, Some(("#638ec6".to_string(), 0.5)));
        let out = run(&c, json!([{"ranges": ["A1:A3"], "rules": [{"type": "iconSet", "op": "", "formulas": [], "dxf": {}, "stop": false, "icons": {"set": "3Arrows"}}]}]));
        assert_eq!(out["A1"].icon, Some(("3Arrows".to_string(), 0, 3)));
        assert_eq!(out["A3"].icon, Some(("3Arrows".to_string(), 2, 3)));
    }

    #[test]
    fn priorities_and_stop() {
        let c = sheet(&[("A1", json!(5))]);
        let out = run(
            &c,
            json!([{"ranges": ["A1"], "rules": [
                {"type": "cellIs", "op": "gt", "formulas": ["1"], "dxf": {"bg": "#111111"}, "stop": true, "priority": 2},
                {"type": "cellIs", "op": "gt", "formulas": ["2"], "dxf": {"color": "#222222"}, "stop": false, "priority": 1}
            ]}]),
        );
        // priority 1 runs first and is not a stop; priority 2 then sets bg and stops.
        assert_eq!(out["A1"].color.as_deref(), Some("#222222"));
        assert_eq!(out["A1"].bg.as_deref(), Some("#111111"));
    }
}
