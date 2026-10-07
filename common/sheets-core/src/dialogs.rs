//! What the editor's dialogs compute, so every client builds the same rules: the conditional-formatting
//! dialog (`ConditionalFormatDialog.tsx`), the data-validation dialog (`DataValidationDialog.tsx`), the
//! custom sort (`SortDialog.tsx`, `openSortDialog` SA:6929) and the column filter (`ColumnFilterPopup`
//! SA:556, its value list SA:7997). The dialogs only collect input; the stored JSON is built here, exactly
//! as the web writes it.
//!
//! The rule lists are kept as the stored JSON (`sheet.data.cf`, `sheet.data.validations`): removing or
//! adding a rule never rewrites the others.

use std::cmp::Ordering;
use std::collections::HashSet;

use serde_json::{json, Map, Value as Json};

use crate::addr::{self, Rect};
use crate::editor::Editor;
use crate::js;
use crate::validation::{criterion_label, DVCriterion, DataValidation, NumOp};

// ── Conditional formatting ──────────────────────────────────────────────────────

/// The « valeur de la cellule » operators (`CellOp`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellOp {
    Gt,
    Lt,
    Ge,
    Le,
    Eq,
    Ne,
    Between,
}

/// The « texte » operators (`TextOp`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOp {
    Contains,
    NotContains,
    Starts,
    Ends,
}

/// A colour scale (`ColorScale`): `mid` only for three colours.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ColorScale {
    pub lo: String,
    pub mid: Option<String>,
    pub hi: String,
}

/// The dialog's scale presets (`SCALES`).
pub fn cf_scale_presets() -> Vec<ColorScale> {
    let s = |lo: &str, mid: Option<&str>, hi: &str| ColorScale { lo: lo.into(), mid: mid.map(Into::into), hi: hi.into() };
    vec![s("#57bb8a", Some("#ffd666"), "#e67c73"), s("#e67c73", Some("#ffd666"), "#57bb8a"), s("#ffffff", None, "#5a9bdc"), s("#ffffff", None, "#57bb8a")]
}

/// The data-bar colours offered (`BAR_COLORS`).
pub const CF_BAR_COLORS: [&str; 5] = ["#638ec6", "#63c384", "#ff555a", "#ffb628", "#8957e5"];

/// The icon sets offered, in the dialog's order (`3Arrows` first).
pub const CF_ICON_SETS: [&str; 9] = ["3Arrows", "3TrafficLights1", "3Symbols2", "3Flags", "4Arrows", "4Rating", "5Arrows", "5Rating", "5Quarters"];

/// A « format if true » style (`CondStyle` of the dialog): fill, text colour, bold, italic — only the
/// keys the user touched are written (`bold: false` once toggled off, as on the web).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dxf {
    pub bg: Option<String>,
    pub color: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
}

impl Dxf {
    fn to_json(&self) -> Json {
        let mut m = Map::new();
        if let Some(v) = &self.bg {
            m.insert("bg".into(), json!(v));
        }
        if let Some(v) = &self.color {
            m.insert("color".into(), json!(v));
        }
        if let Some(v) = self.bold {
            m.insert("bold".into(), json!(v));
        }
        if let Some(v) = self.italic {
            m.insert("italic".into(), json!(v));
        }
        Json::Object(m)
    }
}

/// The dialog's style presets (`PRESETS`), with their French labels.
pub fn cf_dxf_presets() -> Vec<(&'static str, Dxf)> {
    let d = |bg: Option<&str>, color: Option<&str>, bold: Option<bool>| Dxf { bg: bg.map(Into::into), color: color.map(Into::into), bold, italic: None };
    vec![
        ("Rouge clair / texte rouge", d(Some("#ffc7ce"), Some("#9c0006"), None)),
        ("Jaune clair / texte jaune", d(Some("#ffeb9c"), Some("#9c6500"), None)),
        ("Vert clair / texte vert", d(Some("#c6efce"), Some("#006100"), None)),
        ("Remplissage rouge clair", d(Some("#ffc7ce"), None, None)),
        ("Texte rouge gras", d(None, Some("#9c0006"), Some(true))),
    ]
}

/// What the new rule is (`Kind` and the inputs that go with it).
#[derive(Clone, Debug, PartialEq)]
pub enum CfKind {
    Cell { op: CellOp, v1: String, v2: String },
    Text { op: TextOp, v: String },
    Empty { empty: bool },
    Formula(String),
    Top { bottom: bool, rank: String, percent: bool },
    Average { above: bool },
    Duplicates { duplicate: bool },
    Scale(ColorScale),
    Bar { color: String },
    Icons { set: String },
}

impl CfKind {
    /// Whether the « format if true » block applies (`showDxf`).
    pub fn uses_dxf(&self) -> bool {
        !matches!(self, CfKind::Scale(_) | CfKind::Bar { .. } | CfKind::Icons { .. })
    }
}

/// The top-left cell of an A1 range, `$` removed, upper-cased (`anchorOf`).
fn anchor_of(range: &str) -> String {
    let first = range.split(':').next().filter(|s| !s.is_empty()).unwrap_or(range);
    first.replace('$', "").to_uppercase()
}

/// A value as a formula literal: numeric stays bare (trimmed), text is quoted with `"` doubled (`lit`).
fn lit(v: &str) -> String {
    let t = v.trim();
    if !t.is_empty() && !js::number_or_nan(v).is_nan() {
        t.to_string()
    } else {
        format!("\"{}\"", v.replace('"', "\"\""))
    }
}

/// `buildRule`: the rule the dialog adds for `range`, or `None` when an input is missing.
pub fn build_cf_rule(kind: &CfKind, range: &str, dxf: &Dxf) -> Option<Json> {
    let base = |ty: &str, op: &str| -> Map<String, Json> {
        let mut m = Map::new();
        m.insert("op".into(), json!(op));
        m.insert("formulas".into(), json!([]));
        m.insert("dxf".into(), json!({}));
        m.insert("stop".into(), json!(false));
        m.insert("type".into(), json!(ty));
        m
    };
    match kind {
        CfKind::Scale(cs) => {
            let mut m = base("colorScale", if cs.mid.is_some() { "3" } else { "2" });
            let mut c = Map::new();
            c.insert("lo".into(), json!(cs.lo));
            if let Some(mid) = &cs.mid {
                c.insert("mid".into(), json!(mid));
            }
            c.insert("hi".into(), json!(cs.hi));
            m.insert("cs".into(), Json::Object(c));
            return Some(Json::Object(m));
        }
        CfKind::Bar { color } => {
            let mut m = base("dataBar", "");
            m.insert("bar".into(), json!({ "color": color }));
            return Some(Json::Object(m));
        }
        CfKind::Icons { set } => {
            let mut m = base("iconSet", "");
            m.insert("icons".into(), json!({ "set": set }));
            return Some(Json::Object(m));
        }
        CfKind::Top { bottom, rank, percent } => {
            // `parseInt(topRank, 10)`: leading digits; 0, NaN or negative refuse.
            let r = js::parse_int(rank, 10);
            if r.is_nan() || r < 1.0 {
                return None;
            }
            let mut m = base("top10", "");
            m.insert("dxf".into(), dxf.to_json());
            m.insert("stop".into(), json!(true));
            m.insert("rank".into(), json!(r as i64));
            if *percent {
                m.insert("percent".into(), json!(true));
            }
            if *bottom {
                m.insert("bottom".into(), json!(true));
            }
            return Some(Json::Object(m));
        }
        CfKind::Average { above } => {
            let mut m = base("aboveAverage", "");
            m.insert("dxf".into(), dxf.to_json());
            m.insert("stop".into(), json!(true));
            if !above {
                m.insert("above".into(), json!(false));
            }
            return Some(Json::Object(m));
        }
        CfKind::Duplicates { duplicate } => {
            let mut m = base(if *duplicate { "duplicateValues" } else { "uniqueValues" }, "");
            m.insert("dxf".into(), dxf.to_json());
            m.insert("stop".into(), json!(true));
            return Some(Json::Object(m));
        }
        _ => {}
    }
    let a = anchor_of(range);
    let f = match kind {
        CfKind::Cell { op: CellOp::Between, v1, v2 } => {
            if v1.is_empty() || v2.is_empty() {
                return None;
            }
            format!("=AND({a}>={},{a}<={})", lit(v1), lit(v2))
        }
        CfKind::Cell { op, v1, .. } => {
            if v1.trim().is_empty() && *op != CellOp::Eq {
                return None;
            }
            let o = match op {
                CellOp::Gt => ">",
                CellOp::Lt => "<",
                CellOp::Ge => ">=",
                CellOp::Le => "<=",
                CellOp::Eq => "=",
                CellOp::Ne => "<>",
                CellOp::Between => unreachable!("handled above"),
            };
            format!("={a}{o}{}", lit(v1))
        }
        CfKind::Text { op, v } => {
            if v.is_empty() {
                return None;
            }
            let l = lit(v);
            match op {
                TextOp::Contains => format!("=ISNUMBER(SEARCH({l},{a}))"),
                TextOp::NotContains => format!("=NOT(ISNUMBER(SEARCH({l},{a})))"),
                TextOp::Starts => format!("=LEFT({a},LEN({l}))={l}"),
                TextOp::Ends => format!("=RIGHT({a},LEN({l}))={l}"),
            }
        }
        CfKind::Empty { empty } => {
            if *empty {
                format!("={a}=\"\"")
            } else {
                format!("={a}<>\"\"")
            }
        }
        CfKind::Formula(f) => {
            let t = f.trim();
            if t.is_empty() {
                return None;
            }
            if t.starts_with('=') {
                t.to_string()
            } else {
                format!("={t}")
            }
        }
        _ => return None,
    };
    Some(json!({ "type": "expression", "op": "", "formulas": [f], "dxf": dxf.to_json(), "stop": true }))
}

/// `addRule`: the block `{ranges: [RANGE], rules: [rule]}` put first (new rules take priority).
pub fn cf_add(blocks: &[Json], range: &str, rule: Json) -> Option<Vec<Json>> {
    let range = range.trim();
    if range.is_empty() {
        return None;
    }
    let mut out = vec![json!({ "ranges": [range.to_uppercase()], "rules": [rule] })];
    out.extend(blocks.iter().cloned());
    Some(out)
}

/// The ranges of a stored block (`b.ranges.join(', ')`).
pub fn block_ranges(block: &Json) -> String {
    block.get("ranges").and_then(Json::as_array).map(|a| a.iter().map(|r| r.as_str().map(str::to_string).unwrap_or_else(|| r.to_string())).collect::<Vec<_>>().join(", ")).unwrap_or_default()
}

/// What a CF block's first rule is, for the rule list (`ruleSummary`); the window words it.
#[derive(Clone, Debug, PartialEq)]
pub enum CfSummary {
    None,
    Scale,
    Bar,
    Icons,
    Top { bottom: bool, rank: String, percent: bool },
    AboveAverage,
    BelowAverage,
    Duplicates,
    Unique,
    /// `timePeriod`: the period's name (`r.period ?? 'timePeriod'`).
    Period(String),
    /// Anything else: a fixed text (a formula, a text rule « type « text » », the rule's type, or `—`).
    Text(String),
}

/// `ruleSummary(b)`.
pub fn cf_summary(block: &Json) -> CfSummary {
    let Some(r) = block.get("rules").and_then(Json::as_array).and_then(|a| a.first()) else { return CfSummary::None };
    let ty = r.get("type").and_then(Json::as_str).unwrap_or("");
    let first_formula = r.get("formulas").and_then(Json::as_array).and_then(|a| a.first()).and_then(Json::as_str).filter(|s| !s.is_empty());
    match ty {
        "colorScale" => CfSummary::Scale,
        "dataBar" => CfSummary::Bar,
        "iconSet" => CfSummary::Icons,
        "top10" => CfSummary::Top {
            bottom: r.get("bottom").and_then(Json::as_bool).unwrap_or(false),
            rank: r.get("rank").filter(|v| !v.is_null()).map(js_string).unwrap_or_else(|| "10".into()),
            percent: r.get("percent").and_then(Json::as_bool).unwrap_or(false),
        },
        "aboveAverage" => {
            if r.get("above") == Some(&Json::Bool(false)) {
                CfSummary::BelowAverage
            } else {
                CfSummary::AboveAverage
            }
        }
        "duplicateValues" => CfSummary::Duplicates,
        "uniqueValues" => CfSummary::Unique,
        "timePeriod" => CfSummary::Period(r.get("period").and_then(Json::as_str).unwrap_or("timePeriod").to_string()),
        "containsText" | "notContainsText" | "beginsWith" | "endsWith" => match r.get("text").and_then(Json::as_str).filter(|s| !s.is_empty()) {
            Some(t) => CfSummary::Text(format!("{ty} « {t} »")),
            None => CfSummary::Text(first_formula.unwrap_or(ty).to_string()),
        },
        _ => CfSummary::Text(first_formula.map(str::to_string).unwrap_or_else(|| if ty.is_empty() { "—".into() } else { ty.into() })),
    }
}

fn js_string(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        Json::Number(n) => n.as_f64().map(js::to_string).unwrap_or_else(|| n.to_string()),
        other => other.to_string(),
    }
}

/// The colour swatch of a CF block in the rule list (`swatch(b)`).
#[derive(Clone, Debug, PartialEq)]
pub enum Swatch {
    /// No fill (`transparent`).
    None,
    Solid(String),
    /// A left-to-right gradient through these stops (`lo`, `mid ?? hi`, `hi`).
    Gradient(Vec<String>),
    /// A bar of this colour over 60 % of the width.
    Bar(String),
    /// The red / amber / green thirds of an icon set.
    Icons,
}

/// `swatch(b)`.
pub fn cf_swatch(block: &Json) -> Swatch {
    let Some(r) = block.get("rules").and_then(Json::as_array).and_then(|a| a.first()) else { return Swatch::None };
    let s = |k: &str, o: &Json| o.get(k).and_then(Json::as_str).map(str::to_string);
    if let Some(cs) = r.get("cs").filter(|v| v.is_object()) {
        let lo = s("lo", cs).unwrap_or_default();
        let hi = s("hi", cs).unwrap_or_default();
        let mid = s("mid", cs).unwrap_or_else(|| hi.clone());
        return Swatch::Gradient(vec![lo, mid, hi]);
    }
    match r.get("type").and_then(Json::as_str) {
        Some("dataBar") => Swatch::Bar(r.get("bar").and_then(|b| s("color", b)).unwrap_or_else(|| "#638ec6".into())),
        Some("iconSet") => Swatch::Icons,
        _ => r.get("dxf").and_then(|d| s("bg", d)).map(Swatch::Solid).unwrap_or(Swatch::None),
    }
}

// ── Data validation ─────────────────────────────────────────────────────────────

/// The criteria of the dialog (`Kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DvKind {
    List,
    ListRange,
    Checkbox,
    Number,
    TextLength,
}

/// What the data-validation dialog collected.
#[derive(Clone, Debug, PartialEq)]
pub struct DvInput {
    pub range: String,
    pub kind: DvKind,
    /// `Oui, Non, Peut-être`.
    pub list_text: String,
    /// `A1:A10`.
    pub source: String,
    pub op: NumOp,
    pub v1: String,
    pub v2: String,
    pub strict: bool,
    pub dropdown: bool,
    pub reject: bool,
    pub help: String,
}

impl Default for DvInput {
    fn default() -> Self {
        Self {
            range: "A1".into(),
            kind: DvKind::List,
            list_text: String::new(),
            source: String::new(),
            op: NumOp::Between,
            v1: String::new(),
            v2: String::new(),
            strict: true,
            dropdown: true,
            reject: true,
            help: String::new(),
        }
    }
}

/// `buildCrit`.
pub fn build_dv_criterion(i: &DvInput) -> Option<DVCriterion> {
    match i.kind {
        DvKind::Checkbox => Some(DVCriterion::Checkbox { on: None, off: None }),
        DvKind::List => {
            let values: Vec<String> = i.list_text.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            if values.is_empty() {
                return None;
            }
            Some(DVCriterion::List { values, strict: Some(i.strict), dropdown: Some(i.dropdown) })
        }
        DvKind::ListRange => {
            let s = i.source.trim();
            if s.is_empty() {
                return None;
            }
            Some(DVCriterion::ListRange { source: s.to_uppercase(), strict: Some(i.strict), dropdown: Some(i.dropdown) })
        }
        DvKind::Number | DvKind::TextLength => {
            let a = js::number_or_nan(&i.v1);
            if i.v1.trim().is_empty() || a.is_nan() {
                return None;
            }
            let b = if i.v2.trim().is_empty() { None } else { Some(js::number_or_nan(&i.v2)) };
            if matches!(i.op, NumOp::Between | NumOp::NotBetween) && b.is_none_or(f64::is_nan) {
                return None;
            }
            let (op, v1, v2) = (i.op, Some(a), b);
            Some(if i.kind == DvKind::Number { DVCriterion::Number { op, v1, v2 } } else { DVCriterion::TextLen { op, v1, v2 } })
        }
    }
}

/// `add`: the new block `{ranges: [RANGE], rule: {crit, reject, help?}}` put first.
pub fn dv_add(blocks: &[Json], input: &DvInput) -> Option<Vec<Json>> {
    let crit = build_dv_criterion(input)?;
    let range = input.range.trim();
    if range.is_empty() {
        return None;
    }
    let help = input.help.trim();
    let rule = DataValidation { crit, reject: Some(if input.kind == DvKind::Checkbox { false } else { input.reject }), help: (!help.is_empty()).then(|| help.to_string()) };
    let rule = serde_json::to_value(&rule).ok()?;
    let mut out = vec![json!({ "ranges": [range.to_uppercase()], "rule": rule })];
    out.extend(blocks.iter().cloned());
    Some(out)
}

/// The label of a stored DV block's criterion (`criterionLabel(b.rule.crit)`), `—` when unreadable.
pub fn dv_summary(block: &Json) -> String {
    block.get("rule").and_then(|r| r.get("crit")).and_then(|c| serde_json::from_value::<DVCriterion>(c.clone()).ok()).map(|c| criterion_label(&c)).unwrap_or_else(|| "—".into())
}

// ── Custom sort ─────────────────────────────────────────────────────────────────

/// A column of the range to sort (`SortColumn`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortColumn {
    pub idx: u32,
    pub letter: String,
    pub header: String,
}

/// What `openSortDialog` prepares.
#[derive(Clone, Debug, PartialEq)]
pub struct SortSetup {
    pub range: Rect,
    pub columns: Vec<SortColumn>,
    /// The header heuristic: a text cell on the first row and a number on the second.
    pub headers: bool,
    pub label: String,
}

impl Editor {
    /// `openSortDialog`: the selection (the used area when one cell is selected), its columns with their
    /// first-row texts, the header guess; `None` when there is nothing below the first row.
    pub fn sort_setup(&self) -> Option<SortSetup> {
        let (uc, ur) = self.sheet().used_extent();
        let single = self.sel.end.is_none_or(|e| e == self.sel.active);
        let sb = self.sel.rect();
        let b = if single {
            Rect::new(0, 1, uc, ur.max(1))
        } else {
            Rect::new(sb.c1, sb.r1, sb.c2.min(uc.max(sb.c1)), sb.r2.min(ur.max(sb.r1)))
        };
        if b.r2 <= b.r1 {
            return None;
        }
        let cells = self.cells();
        let numeric = |c: u32, r: u32| self.shown(c, r).num.is_some() && cells.contains_key(&addr::key(c, r));
        let mut text_hdr = false;
        let mut num_below = false;
        for c in b.c1..=b.c2 {
            if let Some(h) = cells.get(&addr::key(c, b.r1)) {
                let has_v = h.v.as_ref().is_some_and(|v| !v.is_null() && v.as_str() != Some(""));
                if has_v && !numeric(c, b.r1) {
                    text_hdr = true;
                }
            }
            if numeric(c, b.r1 + 1) {
                num_below = true;
            }
        }
        let columns = (b.c1..=b.c2)
            .map(|c| SortColumn {
                idx: c,
                letter: addr::index_to_col(c as i64),
                header: if cells.contains_key(&addr::key(c, b.r1)) { self.shown(c, b.r1).raw_text } else { String::new() },
            })
            .collect();
        Some(SortSetup { range: b, columns, headers: text_hdr && num_below, label: format!("{}:{}", addr::key(b.c1, b.r1), addr::key(b.c2, b.r2)) })
    }

    /// The conditional-formatting blocks as stored (`sheet.data.cf`).
    pub fn cf_blocks(&self) -> Vec<Json> {
        self.sheet().cf().and_then(Json::as_array).cloned().unwrap_or_default()
    }

    /// `persistCf`: replaces the blocks (not an undoable edit on the web: saved at once).
    pub fn set_cf_blocks(&mut self, blocks: Vec<Json>) {
        self.sheet_mut().set("cf", Json::Array(blocks));
        self.touch();
    }

    /// The validation blocks as stored (`sheet.data.validations`).
    pub fn dv_blocks(&self) -> Vec<Json> {
        self.sheet().validations().and_then(Json::as_array).cloned().unwrap_or_default()
    }

    /// `persistValidations`: replaces the blocks (saved at once, not undoable, as on the web).
    pub fn set_dv_blocks(&mut self, blocks: Vec<Json>) {
        self.sheet_mut().set("validations", Json::Array(blocks));
        self.touch();
    }

    /// The column filter's values (SA:7997): each distinct shown text of column `col` down to the last used
    /// row (`''` for the blanks), in `localeCompare(…, { numeric: true })` order.
    pub fn filter_values(&self, col: u32) -> Vec<String> {
        let (_, max_row) = self.sheet().used_extent();
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for r in 1..=max_row {
            let v = self.cell_text(col, r);
            if seen.insert(v.clone()) {
                out.push(v);
            }
        }
        out.sort_by(|a, b| locale_compare_numeric(a, b));
        out
    }

    /// Applies a column filter: `None` clears it (every value allowed), else only these values show.
    pub fn set_col_filter(&mut self, col: u32, allowed: Option<HashSet<String>>) {
        match allowed {
            Some(a) => {
                self.col_filters.insert(col, a);
            }
            None => {
                self.col_filters.remove(&col);
            }
        }
        self.touch_view();
    }
}

/// `localeCompare(a, b, lang, { numeric: true })` as browsers order these lists: digit runs compare as
/// numbers; otherwise letters compare without case or accents first (`a` < `B` < `é` < `f`), then lower
/// before upper case, then by code unit.
pub fn locale_compare_numeric(a: &str, b: &str) -> Ordering {
    fn base(c: char) -> char {
        let l = c.to_lowercase().next().unwrap_or(c);
        match l {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            other => other,
        }
    }
    // Primary pass: base letters, numbers by value; punctuation and spaces before digits before letters.
    fn class(c: char) -> u8 {
        if c.is_ascii_digit() {
            1
        } else if c.is_alphabetic() {
            2
        } else {
            0
        }
    }
    let (ac, bc): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (mut i, mut j) = (0, 0);
    while i < ac.len() && j < bc.len() {
        let (x, y) = (ac[i], bc[j]);
        if x.is_ascii_digit() && y.is_ascii_digit() {
            let si = i;
            while i < ac.len() && ac[i].is_ascii_digit() {
                i += 1;
            }
            let sj = j;
            while j < bc.len() && bc[j].is_ascii_digit() {
                j += 1;
            }
            let na: String = ac[si..i].iter().collect::<String>().trim_start_matches('0').to_string();
            let nb: String = bc[sj..j].iter().collect::<String>().trim_start_matches('0').to_string();
            let o = na.len().cmp(&nb.len()).then_with(|| na.cmp(&nb));
            if o != Ordering::Equal {
                return o;
            }
            continue;
        }
        let o = class(x).cmp(&class(y)).then_with(|| base(x).cmp(&base(y)));
        if o != Ordering::Equal {
            return o;
        }
        i += 1;
        j += 1;
    }
    let o = (ac.len() - i).cmp(&(bc.len() - j));
    if o != Ordering::Equal {
        return o;
    }
    // Tertiary: accents, then lower case first.
    for (x, y) in a.chars().zip(b.chars()) {
        if x == y {
            continue;
        }
        let ax = x.to_lowercase().next().unwrap_or(x) != base(x);
        let ay = y.to_lowercase().next().unwrap_or(y) != base(y);
        if ax != ay {
            return ax.cmp(&ay);
        }
        if x.is_lowercase() != y.is_lowercase() {
            return if x.is_lowercase() { Ordering::Less } else { Ordering::Greater };
        }
    }
    a.cmp(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::tests::editor;
    use crate::validation::DVBlock;

    fn red() -> Dxf {
        Dxf { bg: Some("#ffc7ce".into()), color: Some("#9c0006".into()), ..Dxf::default() }
    }

    #[test]
    fn cell_value_rules_are_anchored_expressions_like_the_web() {
        let r = build_cf_rule(&CfKind::Cell { op: CellOp::Gt, v1: " 10 ".into(), v2: String::new() }, "$b2:c9", &red()).unwrap();
        assert_eq!(r, json!({"type": "expression", "op": "", "formulas": ["=B2>10"], "dxf": {"bg": "#ffc7ce", "color": "#9c0006"}, "stop": true}));
        let r = build_cf_rule(&CfKind::Cell { op: CellOp::Between, v1: "1".into(), v2: "x\"y".into() }, "A1", &red()).unwrap();
        assert_eq!(r["formulas"][0], "=AND(A1>=1,A1<=\"x\"\"y\")");
        // Equal to nothing is allowed (the cell is empty), any other operator needs a value.
        assert_eq!(build_cf_rule(&CfKind::Cell { op: CellOp::Eq, v1: String::new(), v2: String::new() }, "A1", &red()).unwrap()["formulas"][0], "=A1=\"\"");
        assert!(build_cf_rule(&CfKind::Cell { op: CellOp::Lt, v1: "  ".into(), v2: String::new() }, "A1", &red()).is_none());
        assert!(build_cf_rule(&CfKind::Cell { op: CellOp::Between, v1: "1".into(), v2: String::new() }, "A1", &red()).is_none());
    }

    #[test]
    fn text_empty_and_formula_rules() {
        let t = |op| build_cf_rule(&CfKind::Text { op, v: "ab".into() }, "C3:C9", &red()).unwrap()["formulas"][0].as_str().unwrap().to_string();
        assert_eq!(t(TextOp::Contains), "=ISNUMBER(SEARCH(\"ab\",C3))");
        assert_eq!(t(TextOp::NotContains), "=NOT(ISNUMBER(SEARCH(\"ab\",C3)))");
        assert_eq!(t(TextOp::Starts), "=LEFT(C3,LEN(\"ab\"))=\"ab\"");
        assert_eq!(t(TextOp::Ends), "=RIGHT(C3,LEN(\"ab\"))=\"ab\"");
        assert_eq!(build_cf_rule(&CfKind::Empty { empty: false }, "D1", &red()).unwrap()["formulas"][0], "=D1<>\"\"");
        assert_eq!(build_cf_rule(&CfKind::Formula(" $A1>2 ".into()), "D1", &red()).unwrap()["formulas"][0], "=$A1>2");
        assert!(build_cf_rule(&CfKind::Formula("  ".into()), "D1", &red()).is_none());
    }

    #[test]
    fn graphic_and_statistic_rules_carry_the_web_base() {
        let s = build_cf_rule(&CfKind::Scale(cf_scale_presets()[2].clone()), "A1:A9", &red()).unwrap();
        assert_eq!(s, json!({"op": "2", "formulas": [], "dxf": {}, "stop": false, "type": "colorScale", "cs": {"lo": "#ffffff", "hi": "#5a9bdc"}}));
        assert_eq!(build_cf_rule(&CfKind::Scale(cf_scale_presets()[0].clone()), "A1", &red()).unwrap()["op"], "3");
        assert_eq!(build_cf_rule(&CfKind::Bar { color: "#63c384".into() }, "A1", &red()).unwrap()["bar"], json!({"color": "#63c384"}));
        assert_eq!(build_cf_rule(&CfKind::Icons { set: "5Quarters".into() }, "A1", &red()).unwrap()["icons"], json!({"set": "5Quarters"}));
        let top = build_cf_rule(&CfKind::Top { bottom: true, rank: "5x".into(), percent: true }, "A1", &red()).unwrap();
        assert_eq!(top, json!({"op": "", "formulas": [], "dxf": {"bg": "#ffc7ce", "color": "#9c0006"}, "stop": true, "type": "top10", "rank": 5, "percent": true, "bottom": true}));
        assert!(build_cf_rule(&CfKind::Top { bottom: false, rank: "0".into(), percent: false }, "A1", &red()).is_none());
        assert_eq!(build_cf_rule(&CfKind::Average { above: false }, "A1", &red()).unwrap()["above"], false);
        assert!(build_cf_rule(&CfKind::Average { above: true }, "A1", &red()).unwrap().get("above").is_none());
        assert_eq!(build_cf_rule(&CfKind::Duplicates { duplicate: false }, "A1", &red()).unwrap()["type"], "uniqueValues");
    }

    #[test]
    fn new_blocks_go_first_and_summaries_follow_the_web() {
        let old = json!({"ranges": ["B1"], "rules": [{"type": "dataBar"}]});
        let rule = build_cf_rule(&CfKind::Top { bottom: false, rank: "3".into(), percent: true }, "a1:a5", &red()).unwrap();
        let blocks = cf_add(std::slice::from_ref(&old), " a1:a5 ", rule).unwrap();
        assert_eq!(blocks[0]["ranges"], json!(["A1:A5"]));
        assert_eq!(blocks[1], old);
        assert_eq!(cf_summary(&blocks[0]), CfSummary::Top { bottom: false, rank: "3".into(), percent: true });
        assert_eq!(cf_summary(&blocks[1]), CfSummary::Bar);
        assert_eq!(cf_summary(&json!({"rules": [{"type": "expression", "formulas": ["=A1>1"]}]})), CfSummary::Text("=A1>1".into()));
        assert_eq!(cf_summary(&json!({"rules": [{"type": "containsText", "text": "ok"}]})), CfSummary::Text("containsText « ok »".into()));
        assert_eq!(cf_summary(&json!({"rules": []})), CfSummary::None);
        assert_eq!(cf_swatch(&json!({"rules": [{"cs": {"lo": "#a", "hi": "#c"}}]})), Swatch::Gradient(vec!["#a".into(), "#c".into(), "#c".into()]));
        assert_eq!(cf_swatch(&blocks[0]), Swatch::Solid("#ffc7ce".into()));
        assert_eq!(block_ranges(&json!({"ranges": ["A1", "C3:D4"]})), "A1, C3:D4");
    }

    #[test]
    fn validation_blocks_are_built_like_the_web() {
        let mut i = DvInput { range: "b2:b9".into(), list_text: "Oui, Non,, Peut-être ".into(), help: "  ".into(), ..DvInput::default() };
        let b = dv_add(&[], &i).unwrap();
        assert_eq!(b[0], json!({"ranges": ["B2:B9"], "rule": {"crit": {"kind": "list", "values": ["Oui", "Non", "Peut-être"], "strict": true, "dropdown": true}, "reject": true}}));
        i.kind = DvKind::Number;
        i.op = NumOp::Between;
        i.v1 = "1".into();
        assert!(dv_add(&[], &i).is_none(), "between needs two values");
        i.v2 = "5".into();
        i.reject = false;
        i.help = "Entre 1 et 5".into();
        let b = dv_add(&b, &i).unwrap();
        let block: DVBlock = serde_json::from_value(b[0].clone()).unwrap();
        assert_eq!(block.rule.crit, DVCriterion::Number { op: NumOp::Between, v1: Some(1.0), v2: Some(5.0) });
        assert_eq!(block.rule.reject, Some(false));
        assert_eq!(block.rule.help.as_deref(), Some("Entre 1 et 5"));
        assert_eq!(b.len(), 2);
        i.kind = DvKind::Checkbox;
        i.reject = true;
        assert_eq!(dv_add(&[], &i).unwrap()[0]["rule"]["reject"], false, "a checkbox never rejects");
        assert_eq!(dv_summary(&b[1]), "Liste : Oui, Non, Peut-être");
    }

    #[test]
    fn sort_setup_guesses_the_header_like_the_web() {
        let ed = editor(json!({"A1": {"v": "Nom"}, "B1": {"v": "Âge"}, "A2": {"v": "Zoé"}, "B2": {"v": 30}, "A3": {"v": "Al"}, "B3": {"v": 4}}));
        let s = ed.sort_setup().unwrap();
        assert_eq!(s.range, Rect::new(0, 1, 1, 3));
        assert!(s.headers);
        assert_eq!(s.label, "A1:B3");
        assert_eq!(s.columns[1], SortColumn { idx: 1, letter: "B".into(), header: "Âge".into() });
        let ed = editor(json!({"A1": {"v": 3}, "A2": {"v": 1}}));
        assert!(!ed.sort_setup().unwrap().headers, "no text on the first row: no header");
        assert!(editor(json!({"A1": {"v": 1}})).sort_setup().is_none());
    }

    #[test]
    fn filter_values_are_distinct_and_naturally_ordered() {
        let mut ed = editor(json!({"A1": {"v": "item10"}, "A2": {"v": "item2"}, "A3": {"v": "b"}, "A4": {"v": "B"}, "A6": {"v": "é"}, "A7": {"v": "item2"}}));
        assert_eq!(ed.filter_values(0), vec!["", "b", "B", "é", "item2", "item10"]);
        ed.set_col_filter(0, Some(["b".to_string()].into_iter().collect()));
        assert!(ed.hidden_rows().contains(&1));
        assert!(!ed.hidden_rows().contains(&3));
        ed.set_col_filter(0, None);
        assert!(ed.hidden_rows().is_empty());
    }

    #[test]
    fn an_imported_auto_filter_shows_the_funnels() {
        let mut wb = crate::model::workbook::Workbook::new_local("t", "s1");
        wb.sheets[0] = crate::model::sheet::Sheet::from_data(crate::model::sheet::SheetMeta { id: "s1".into(), name: "F".into(), position: 0, raw: Default::default() }, &json!({ "cells": {}, "auto_filter": "A1:C9" }));
        let ed = Editor::new(wb, std::rc::Rc::new(crate::formula::testing::env()));
        assert!(ed.filter_mode);
        assert!(!editor(json!({})).filter_mode);
    }

    #[test]
    fn rule_lists_are_replaced_as_stored() {
        let mut ed = editor(json!({}));
        assert!(ed.cf_blocks().is_empty());
        ed.set_cf_blocks(vec![json!({"ranges": ["A1"], "rules": [], "custom": 1})]);
        assert_eq!(ed.cf_blocks()[0]["custom"], 1, "unknown keys survive");
        ed.set_dv_blocks(vec![json!({"ranges": ["A1"], "rule": {"crit": {"kind": "checkbox"}}})]);
        assert!(crate::validation::is_checkbox(ed.validation_at(crate::addr::Cell::new(0, 1)).as_ref()));
    }
}

// ── Name Manager (NameManagerDialog.tsx) ────────────────────────────────────────

/// Why the Name Manager refused a name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameError {
    /// `names_err_name`: letters, digits, `_` and `.`, not a cell reference, not TRUE/FALSE.
    Name,
    /// `names_err_def`: the definition does not parse (the parser's message).
    Definition(String),
}

impl Editor {
    /// The workbook's defined names, sorted (`Object.entries(names).sort`).
    pub fn defined_names(&self) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = self.wb.names.iter().map(|(k, d)| (k.clone(), d.as_str().map(str::to_string).unwrap_or_else(|| d.to_string()))).collect();
        v.sort_by(|a, b| locale_compare_numeric(&a.0, &b.0));
        v
    }

    /// `submit`: defines `name` (upper-cased) as `def` (a leading `=` added), replacing `editing` when it is
    /// renamed. Names travel through the collaboration room (`setDefinedName`, `SA:885`).
    pub fn set_defined_name(&mut self, editing: Option<&str>, name: &str, def: &str) -> Result<(), NameError> {
        let name = name.trim();
        if !crate::formula::is_valid_defined_name(name) {
            return Err(NameError::Name);
        }
        let mut def = def.trim().to_string();
        if !def.starts_with('=') {
            def.insert(0, '=');
        }
        if let Some(e) = crate::formula::validate_name_formula(&def) {
            return Err(NameError::Definition(e));
        }
        if let Some(old) = editing.filter(|o| !o.eq_ignore_ascii_case(name)) {
            self.wb.names.remove(&old.to_uppercase());
        }
        self.wb.names.insert(name.to_uppercase(), Json::String(def));
        self.touch();
        Ok(())
    }

    /// `deleteDefinedName`.
    pub fn delete_defined_name(&mut self, name: &str) {
        if self.wb.names.remove(&name.to_uppercase()).is_some() {
            self.touch();
        }
    }

    // ── Sheet protection (SA:3936-3949) ─────────────────────────────────────────

    /// The active sheet's protection record.
    pub fn protection(&self) -> Option<crate::crypto::SheetProtection> {
        self.sheet().get("protection").and_then(|p| serde_json::from_value(p.clone()).ok())
    }

    /// `saveProtection`: sets the record, or removes it (saved at once, as on the web).
    pub fn set_protection(&mut self, p: Option<crate::crypto::SheetProtection>) {
        // Removed as `false`: the API reads `null` as « unchanged » (the web sends `null`, which the server ignores), and
        // both clients read a non-object as « not protected ».
        let v = p.and_then(|p| serde_json::to_value(p).ok()).unwrap_or(Json::Bool(false));
        self.sheet_mut().set("protection", v);
        self.touch();
    }

    /// `applyUnlock`: removes the protection when `password` matches (true), else leaves it (false).
    pub fn unprotect(&mut self, password: &str) -> bool {
        match self.protection() {
            None => true,
            Some(p) if crate::crypto::verify_protection(password, &p) => {
                self.set_protection(None);
                true
            }
            Some(_) => false,
        }
    }
}

#[cfg(test)]
mod names_tests {
    use super::*;
    use crate::editor::tests::editor;

    #[test]
    fn names_are_validated_upper_cased_and_renamed() {
        let mut ed = editor(json!({}));
        assert_eq!(ed.set_defined_name(None, "A1", "=1"), Err(NameError::Name));
        assert!(matches!(ed.set_defined_name(None, "Taux", "=(1"), Err(NameError::Definition(_))));
        ed.set_defined_name(None, "taux", "0.2").expect("valid");
        assert_eq!(ed.defined_names(), vec![("TAUX".to_string(), "=0.2".to_string())]);
        ed.set_defined_name(Some("TAUX"), "Rate", "=0.3").expect("renamed");
        assert_eq!(ed.defined_names(), vec![("RATE".to_string(), "=0.3".to_string())]);
        ed.delete_defined_name("rate");
        assert!(ed.defined_names().is_empty());
    }

    #[test]
    fn protection_is_set_checked_and_removed() {
        let mut ed = editor(json!({"A1": {"v": 1}}));
        ed.set_protection(Some(crate::crypto::create_protection("pw", 5).expect("random")));
        assert!(ed.sheet().is_protected());
        assert!(ed.update_cell(crate::addr::Cell::new(0, 1), "2").is_err(), "a protected sheet refuses edits");
        assert!(!ed.unprotect("PW"));
        assert!(ed.unprotect("pw"));
        assert!(!ed.sheet().is_protected());
        assert_eq!(ed.sheet().get("protection"), Some(&Json::Bool(false)), "saved as false: null would not cross the API");
    }
}
