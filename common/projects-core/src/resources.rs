//! The resources workspace of a project (`web/src/project/resources/`): the registry, the utilisation
//! heatmap, the availability (time off) and the cost per resource — everything they compute, so every client
//! shows the same figures:
//!
//! * the resource taxonomy (`KIND_META`, `CATEGORY_GROUPS`, `behaviorOf`) and the member colour;
//! * the registry's search, grouping and utilisation (`peakLoadByResource`, `taskCountByResource`);
//! * the heatmap's columns, daily ratios, cell values and semantic gradient (`ResourceHeatmap.tsx`);
//! * the cost model (`buildRows` of `ResourceCostView.tsx`) and the `Intl` number / currency formats it shows;
//! * the side data the editor keeps for the workspace ([`ResourceSide`]: time off, directory members, cost
//!   settings).

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::date::{self, Date, DateFormat, Lang};
use crate::model::{ProjectData, Resource};

// ── Taxonomy ────────────────────────────────────────────────────────────────

/// How a resource kind costs and schedules (`ResourceBehavior`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behavior {
    /// Consumes time, has a capacity: it appears in the heatmap.
    Work,
    /// Consumed by the unit (unit price), no capacity.
    Material,
    /// A fixed amount: no time, no unit.
    Cost,
}

/// The PMI resource category (`ResourceCategory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Human,
    Physical,
    Tech,
    Financial,
}

/// One kind of resource (`KIND_META` + `KIND_ICON`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KindMeta {
    pub kind: &'static str,
    pub label_fr: &'static str,
    pub label_en: &'static str,
    /// `#rrggbb`, or `primary` for the theme's primary colour.
    pub tint: &'static str,
    pub behavior: Behavior,
    pub category: Category,
    pub external: bool,
    /// The lucide glyph.
    pub icon: &'static str,
}

impl KindMeta {
    pub fn label(&self, lang: Lang) -> &'static str {
        match lang {
            Lang::Fr => self.label_fr,
            Lang::En => self.label_en,
        }
    }
}

pub const KINDS: [KindMeta; 10] = [
    KindMeta { kind: "person", label_fr: "Membre d'équipe", label_en: "Team member", tint: "primary", behavior: Behavior::Work, category: Category::Human, external: false, icon: "UserRound" },
    KindMeta { kind: "contractor", label_fr: "Consultant / sous-traitant", label_en: "Consultant / contractor", tint: "#5f6368", behavior: Behavior::Work, category: Category::Human, external: true, icon: "Briefcase" },
    KindMeta { kind: "equipment", label_fr: "Équipement", label_en: "Equipment", tint: "#00897b", behavior: Behavior::Work, category: Category::Physical, external: false, icon: "Wrench" },
    KindMeta { kind: "facility", label_fr: "Installation / local", label_en: "Facility / premises", tint: "#3949ab", behavior: Behavior::Work, category: Category::Physical, external: false, icon: "Building2" },
    KindMeta { kind: "material", label_fr: "Matériel / consommable", label_en: "Material / consumable", tint: "#e37400", behavior: Behavior::Material, category: Category::Physical, external: false, icon: "Package" },
    KindMeta { kind: "software", label_fr: "Licence logicielle", label_en: "Software licence", tint: "#8e24aa", behavior: Behavior::Material, category: Category::Tech, external: false, icon: "KeyRound" },
    KindMeta { kind: "infrastructure", label_fr: "Infrastructure cloud/réseau", label_en: "Cloud / network infrastructure", tint: "#0277bd", behavior: Behavior::Cost, category: Category::Tech, external: false, icon: "Server" },
    KindMeta { kind: "information", label_fr: "Données / doc / PI", label_en: "Data / docs / IP", tint: "#00838f", behavior: Behavior::Cost, category: Category::Tech, external: false, icon: "Database" },
    KindMeta { kind: "financial", label_fr: "Budget / réserve", label_en: "Budget / reserve", tint: "#2e7d32", behavior: Behavior::Cost, category: Category::Financial, external: false, icon: "Landmark" },
    KindMeta { kind: "cost", label_fr: "Coût forfaitaire", label_en: "Fixed cost", tint: "#9334e6", behavior: Behavior::Cost, category: Category::Financial, external: false, icon: "Banknote" },
];

/// The metadata of `kind` (`None` for a kind this client does not know).
pub fn meta(kind: &str) -> Option<&'static KindMeta> {
    KINDS.iter().find(|k| k.kind == kind)
}

/// `behaviorOf`: an unknown kind works like a person.
pub fn behavior_of(kind: &str) -> Behavior {
    meta(kind).map(|m| m.behavior).unwrap_or(Behavior::Work)
}

/// The label of `kind` (`t('proj_res_kind_<kind>', { defaultValue: meta.label })`), the raw kind when unknown.
pub fn kind_label(kind: &str, lang: Lang) -> String {
    meta(kind).map(|m| m.label(lang).to_string()).unwrap_or_else(|| kind.to_string())
}

/// One group of the « Ajouter » menu (`CATEGORY_GROUPS`).
pub struct CategoryGroup {
    pub category: Category,
    pub label_fr: &'static str,
    pub label_en: &'static str,
    pub kinds: &'static [&'static str],
}

pub const CATEGORY_GROUPS: [CategoryGroup; 4] = [
    CategoryGroup { category: Category::Human, label_fr: "Humaines", label_en: "Human", kinds: &["person", "contractor"] },
    CategoryGroup { category: Category::Physical, label_fr: "Physiques", label_en: "Physical", kinds: &["equipment", "facility", "material"] },
    CategoryGroup { category: Category::Tech, label_fr: "Technologiques", label_en: "Technological", kinds: &["software", "infrastructure", "information"] },
    CategoryGroup { category: Category::Financial, label_fr: "Financières", label_en: "Financial", kinds: &["financial", "cost"] },
];

/// The body of `addResource` for a kind of the menu: named after the kind, a material counted by the unit.
pub fn new_resource_body(kind: &str, lang: Lang, unit_word: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut b = serde_json::Map::new();
    b.insert("name".into(), kind_label(kind, lang).into());
    b.insert("kind".into(), kind.into());
    if behavior_of(kind) == Behavior::Material {
        b.insert("unit_label".into(), unit_word.into());
    }
    b
}

/// A stable avatar colour for a directory member added as a resource (`memberColor`: the JS hash over UTF-16
/// code units, `>>> 0`).
pub fn member_color(user_id: &str) -> &'static str {
    const PALETTE: [&str; 8] = ["#1a73e8", "#188038", "#e37400", "#9334e6", "#d93025", "#00897b", "#c2185b", "#5f6368"];
    let mut h: u32 = 0;
    for u in user_id.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(u as u32);
    }
    PALETTE[(h % PALETTE.len() as u32) as usize]
}

// ── Load ───────────────────────────────────────────────────────────────────

/// Absolute allocation (capacity units) per resource per day over `[0, span)` (summaries skipped).
fn allocation(data: &ProjectData, span: usize) -> HashMap<String, Vec<f64>> {
    let mut out: HashMap<String, Vec<f64>> = HashMap::new();
    for a in &data.assignments {
        let Some(t) = data.task(&a.task_id) else { continue };
        if t.task_type == "summary" {
            continue;
        }
        let es = t.early_start.unwrap_or(0);
        let end = (span as i64).min(es as i64 + (t.duration_days.max(1)) as i64);
        let arr = out.entry(a.resource_id.clone()).or_insert_with(|| vec![0.0; span]);
        let mut d = es.max(0) as i64;
        while d < end {
            arr[d as usize] += a.units;
            d += 1;
        }
    }
    out
}

/// `peakLoadByResource`: the peak daily allocation per resource over the project span.
pub fn peak_load(data: &ProjectData, total_days: i32) -> HashMap<String, f64> {
    allocation(data, total_days.max(1) as usize).into_iter().map(|(k, v)| (k, v.into_iter().fold(0.0, f64::max))).collect()
}

/// `taskCountByResource`.
pub fn task_counts(data: &ProjectData) -> HashMap<String, usize> {
    let mut m = HashMap::new();
    for a in &data.assignments {
        *m.entry(a.resource_id.clone()).or_insert(0) += 1;
    }
    m
}

/// The registry's utilisation: peak ÷ capacity in percent, `None` for what does not work by the hour.
pub fn utilisation(r: &Resource, peaks: &HashMap<String, f64>) -> Option<f64> {
    if behavior_of(&r.kind) != Behavior::Work {
        return None;
    }
    let cap = if r.capacity > 0.0 { r.capacity } else { 1.0 };
    Some(peaks.get(&r.id).copied().unwrap_or(0.0) / cap * 100.0)
}

/// `loadColor`: green under 80 %, amber to 100 %, red beyond.
pub fn load_color(pct: f64) -> &'static str {
    if pct > 100.0 {
        "#d93025"
    } else if pct >= 80.0 {
        "#e37400"
    } else {
        "#188038"
    }
}

// ── Registry ─────────────────────────────────────────────────────────────────

/// « Grouper par ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupMode {
    #[default]
    None,
    Type,
    Role,
    Skill,
}

impl GroupMode {
    pub fn parse(s: &str) -> Self {
        match s {
            "type" => Self::Type,
            "role" => Self::Role,
            "skill" => Self::Skill,
            _ => Self::None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Type => "type",
            Self::Role => "role",
            Self::Skill => "skill",
        }
    }
}

/// A group of the registry: its key, its title and its rows (indices into `data.resources`, by name).
#[derive(Debug, Clone, PartialEq)]
pub struct RegistryGroup {
    pub key: String,
    pub label: String,
    pub rows: Vec<usize>,
}

/// A key for `localeCompare` in the cases that matter here: case and accents fold, then the original breaks
/// ties.
pub fn collation_key(s: &str) -> (String, String) {
    let folded: String = s
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            c => c,
        })
        .collect();
    (folded, s.to_string())
}

/// The registry's rows: filtered by `query` (name, role, a skill), grouped by `mode`, sorted by name; the
/// groups sorted by title. `no_role` / `no_skill` title the rows without one.
pub fn registry(data: &ProjectData, query: &str, mode: GroupMode, lang: Lang, no_role: &str, no_skill: &str) -> Vec<RegistryGroup> {
    let q = query.trim().to_lowercase();
    let filtered: Vec<usize> = data
        .resources
        .iter()
        .enumerate()
        .filter(|(_, r)| q.is_empty() || r.name.to_lowercase().contains(&q) || r.role.to_lowercase().contains(&q) || r.skills.iter().any(|s| s.to_lowercase().contains(&q)))
        .map(|(i, _)| i)
        .collect();
    let by_name = |rows: &mut Vec<usize>| rows.sort_by_key(|&i| collation_key(&data.resources[i].name));
    if mode == GroupMode::None {
        let mut rows = filtered;
        by_name(&mut rows);
        return vec![RegistryGroup { key: String::new(), label: String::new(), rows }];
    }
    let mut groups: Vec<RegistryGroup> = Vec::new();
    let mut push = |key: String, label: String, i: usize| match groups.iter_mut().find(|g| g.key == key) {
        Some(g) => g.rows.push(i),
        None => groups.push(RegistryGroup { key, label, rows: vec![i] }),
    };
    for &i in &filtered {
        let r = &data.resources[i];
        match mode {
            GroupMode::Type => push(r.kind.clone(), kind_label(&r.kind, lang), i),
            GroupMode::Role => {
                let role = r.role.trim();
                if role.is_empty() {
                    push("∅role".into(), no_role.into(), i)
                } else {
                    push(role.into(), role.into(), i)
                }
            }
            GroupMode::Skill | GroupMode::None => {
                if r.skills.is_empty() {
                    push("∅skill".into(), no_skill.into(), i)
                } else {
                    for s in &r.skills {
                        push(s.clone(), s.clone(), i);
                    }
                }
            }
        }
    }
    for g in &mut groups {
        by_name(&mut g.rows);
    }
    groups.sort_by_key(|g| collation_key(&g.label));
    groups
}

// ── Heatmap ───────────────────────────────────────────────────────────────────

/// The heatmap's time scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scale {
    Day,
    #[default]
    Week,
    Month,
}

impl Scale {
    /// `COL_WIDTH`.
    pub fn col_width(self) -> f64 {
        match self {
            Scale::Day => 44.0,
            Scale::Week => 52.0,
            Scale::Month => 64.0,
        }
    }
}

/// The resource column's width (`LEFT_WIDTH`) and the hours of a working day (`HOURS_PER_DAY`).
pub const HEAT_LEFT_WIDTH: f64 = 224.0;
pub const HOURS_PER_DAY: f64 = 8.0;

/// One column of the heatmap (`Column`).
#[derive(Debug, Clone, PartialEq)]
pub struct HeatColumn {
    /// Day offsets from the project's start (may fall outside the span at the edges).
    pub start: i32,
    pub end: i32,
    /// The in-span (working) days it aggregates.
    pub days: Vec<i32>,
    pub group_key: String,
    pub group_label: String,
    pub sub_label: String,
    pub is_today: bool,
}

/// `bucketDays`: the working days of `[start, end)` inside the span, or all of them when none works.
fn bucket_days(start: i32, end: i32, total: i32, project_start: Date) -> Vec<i32> {
    let (from, to) = (start.max(0), end.min(total));
    let all: Vec<i32> = (from..to).collect();
    let work: Vec<i32> = all.iter().copied().filter(|&d| !project_start.add_days(d as i64).is_weekend()).collect();
    if work.is_empty() {
        all
    } else {
        work
    }
}

/// `{ month: 'short' }` — « oct. » / « Oct ».
fn month_short(d: Date, lang: Lang) -> String {
    let s = date::format(d, DateFormat::DayMonth, lang);
    match lang {
        Lang::Fr => s.split_once(' ').map(|(_, m)| m.to_string()).unwrap_or(s),
        Lang::En => s.split_once(' ').map(|(m, _)| m.to_string()).unwrap_or(s),
    }
}

/// `buildColumns`.
pub fn heat_columns(scale: Scale, project_start: Date, total: i32, today_offset: i32, lang: Lang) -> Vec<HeatColumn> {
    let in_today = |s: i32, e: i32| today_offset >= s && today_offset < e;
    let mut cols = Vec::new();
    match scale {
        Scale::Day => {
            for d in 0..total {
                let day = project_start.add_days(d as i64);
                cols.push(HeatColumn {
                    start: d,
                    end: d + 1,
                    days: vec![d],
                    group_key: format!("{}-{}", day.year, day.month),
                    group_label: date::format(day, DateFormat::MonthYear, lang),
                    sub_label: format!("{} {}", date::format(day, DateFormat::WeekdayShort, lang), day.day),
                    is_today: in_today(d, d + 1),
                });
            }
        }
        Scale::Week => {
            let mut d = 0;
            while d < total {
                let day = project_start.add_days(d as i64);
                let w_start = day.add_days(-(day.weekday_mon0() as i64));
                let start = w_start.days_since(project_start) as i32;
                let end = start + 7;
                cols.push(HeatColumn {
                    start,
                    end,
                    days: bucket_days(start, end, total, project_start),
                    group_key: format!("{}-{}", w_start.year, w_start.month),
                    group_label: date::format(w_start, DateFormat::MonthYear, lang),
                    sub_label: match lang {
                        Lang::Fr => format!("S{}", day.iso_week()),
                        Lang::En => format!("W{}", day.iso_week()),
                    },
                    is_today: in_today(start, end),
                });
                d = end;
            }
        }
        Scale::Month => {
            let mut d = 0;
            while d < total {
                let day = project_start.add_days(d as i64);
                let m_start = day.first_of_month();
                let n_start = m_start.next_month();
                let start = m_start.days_since(project_start) as i32;
                let end = n_start.days_since(project_start) as i32;
                cols.push(HeatColumn {
                    start,
                    end,
                    days: bucket_days(start, end, total, project_start),
                    group_key: m_start.year.to_string(),
                    group_label: m_start.year.to_string(),
                    sub_label: month_short(m_start, lang),
                    is_today: in_today(start, end),
                });
                d = end;
            }
        }
    }
    cols
}

/// `groupSpans`: consecutive columns of one month (year), with how many columns each spans.
pub fn group_spans(cols: &[HeatColumn]) -> Vec<(String, usize)> {
    let mut out: Vec<(String, String, usize)> = Vec::new();
    for c in cols {
        match out.last_mut() {
            Some(last) if last.0 == c.group_key => last.2 += 1,
            _ => out.push((c.group_key.clone(), c.group_label.clone(), 1)),
        }
    }
    out.into_iter().map(|(_, l, n)| (l, n)).collect()
}

/// The resources the heatmap and the availability list show (people and equipment: `behaviorOf == 'work'`).
pub fn work_resources(data: &ProjectData) -> Vec<usize> {
    data.resources.iter().enumerate().filter(|(_, r)| behavior_of(&r.kind) == Behavior::Work).map(|(i, _)| i).collect()
}

/// `dailyRatios`: allocation ÷ capacity per day, for each resource of `shown`.
pub fn daily_ratios(data: &ProjectData, shown: &[usize], total: i32) -> HashMap<String, Vec<f64>> {
    let span = total.max(1) as usize;
    let alloc = allocation(data, span);
    shown
        .iter()
        .map(|&i| {
            let r = &data.resources[i];
            let cap = if r.capacity > 0.0 { r.capacity } else { 1.0 };
            let v = alloc.get(&r.id).map(|a| a.iter().map(|x| x / cap).collect()).unwrap_or_else(|| vec![0.0; span]);
            (r.id.clone(), v)
        })
        .collect()
}

/// `cellValue`: the mean ratio over the column's days, and the hours per day it means.
pub fn cell_value(ratios: Option<&Vec<f64>>, col: &HeatColumn, capacity: f64) -> (f64, f64) {
    let Some(arr) = ratios else { return (0.0, 0.0) };
    if col.days.is_empty() {
        return (0.0, 0.0);
    }
    let sum: f64 = col.days.iter().map(|&d| arr.get(d as usize).copied().unwrap_or(0.0)).sum();
    let ratio = sum / col.days.len() as f64;
    (ratio, ratio * capacity * HOURS_PER_DAY)
}

/// How a cell is painted (`paintFor`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellPaint {
    /// Nothing booked: the neutral surface.
    pub zero: bool,
    pub rgb: (u8, u8, u8),
    /// White ink (a dark fill), else `#202124`.
    pub white_ink: bool,
    pub overload: bool,
}

fn rgb_of(hex: u32) -> (f64, f64, f64) {
    (((hex >> 16) & 255) as f64, ((hex >> 8) & 255) as f64, (hex & 255) as f64)
}

/// `mix(a, b, t)` with `Math.round`.
fn mix(a: u32, b: u32, t: f64) -> (u8, u8, u8) {
    let (ca, cb) = (rgb_of(a), rgb_of(b));
    let k = t.clamp(0.0, 1.0);
    let ch = |x: f64, y: f64| crate::rollup::js_round(x + (y - x) * k) as u8;
    (ch(ca.0, cb.0), ch(ca.1, cb.1), ch(ca.2, cb.2))
}

/// The semantic gradient: pale green → green → amber → red → dark red.
pub fn paint_for(ratio: f64) -> CellPaint {
    const PALE_GREEN: u32 = 0xe6f4ea;
    const GREEN: u32 = 0x34a853;
    const AMBER: u32 = 0xe37400;
    const RED: u32 = 0xd93025;
    const RED_DARK: u32 = 0xa50e0e;
    if ratio <= 0.0001 {
        return CellPaint { zero: true, rgb: (0, 0, 0), white_ink: false, overload: false };
    }
    let rgb = if ratio < 0.8 {
        mix(PALE_GREEN, GREEN, ratio / 0.8)
    } else if ratio <= 1.0 {
        mix(GREEN, AMBER, (ratio - 0.8) / 0.2)
    } else {
        mix(RED, RED_DARK, (ratio - 1.0).min(1.0))
    };
    let lum = (0.299 * rgb.0 as f64 + 0.587 * rgb.1 as f64 + 0.114 * rgb.2 as f64) / 255.0;
    CellPaint { zero: false, rgb, white_ink: lum < 0.6, overload: ratio > 1.0001 }
}

/// `tasksForResource`: the (non-summary) tasks a resource is booked on, with their units, by start.
pub fn tasks_for_resource(data: &ProjectData, resource: &str) -> Vec<(usize, f64)> {
    let mut out: Vec<(usize, f64)> = data
        .assignments
        .iter()
        .filter(|a| a.resource_id == resource)
        .filter_map(|a| {
            let i = data.task_index(&a.task_id)?;
            (data.tasks[i].task_type != "summary").then_some((i, a.units))
        })
        .collect();
    out.sort_by_key(|&(i, _)| data.tasks[i].early_start.unwrap_or(0));
    out
}

// ── Cost ───────────────────────────────────────────────────────────────────

/// One costed row (`CostRow`).
#[derive(Debug, Clone, PartialEq)]
pub struct CostRow {
    /// Index into `data.resources`.
    pub resource: usize,
    /// Planned worked hours (labour-like kinds).
    pub planned_hours: Option<f64>,
    /// Planned quantity (material).
    pub planned_qty: Option<f64>,
    pub rate: f64,
    /// `/h`, `/<unit>` or empty (a flat fee).
    pub rate_suffix: String,
    pub planned_cost: f64,
    /// Distributed from logged hours; `None` when nothing is logged.
    pub actual_cost: Option<f64>,
}

/// `buildRows`: planned cost = effort × rate + a per-use fee per booking; actuals split by allocation.
pub fn cost_rows(data: &ProjectData, default_rate: f64) -> (Vec<CostRow>, bool) {
    let counts = task_counts(data);
    let mut unit_total: HashMap<&str, f64> = HashMap::new();
    for a in &data.assignments {
        if data.task(&a.task_id).is_some_and(|t| t.task_type != "summary") {
            *unit_total.entry(a.task_id.as_str()).or_insert(0.0) += a.units;
        }
    }
    let has_actuals = data.tasks.iter().any(|t| t.spent_hours.unwrap_or(0.0) > 0.0);
    let mut planned_h: HashMap<&str, f64> = HashMap::new();
    let mut actual_h: HashMap<&str, f64> = HashMap::new();
    for a in &data.assignments {
        let Some(t) = data.task(&a.task_id) else { continue };
        if t.task_type == "summary" {
            continue;
        }
        *planned_h.entry(a.resource_id.as_str()).or_insert(0.0) += a.units * (t.duration_days.max(0) as f64) * HOURS_PER_DAY;
        let total = unit_total.get(a.task_id.as_str()).copied().unwrap_or(a.units);
        let spent = t.spent_hours.unwrap_or(0.0);
        if spent > 0.0 && total > 0.0 {
            *actual_h.entry(a.resource_id.as_str()).or_insert(0.0) += spent * (a.units / total);
        }
    }
    let rows = data
        .resources
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let per_use = r.cost_per_use.unwrap_or(0.0) * counts.get(&r.id).copied().unwrap_or(0) as f64;
            match behavior_of(&r.kind) {
                Behavior::Cost => CostRow { resource: i, planned_hours: None, planned_qty: None, rate: r.cost_per_use.unwrap_or(0.0), rate_suffix: String::new(), planned_cost: per_use, actual_cost: None },
                Behavior::Material => {
                    let rate = r.hourly_rate.unwrap_or(0.0);
                    let suffix = match r.unit_label.as_deref().filter(|u| !u.is_empty()) {
                        Some(u) => format!("/{u}"),
                        None => "/u".into(),
                    };
                    CostRow { resource: i, planned_hours: None, planned_qty: Some(0.0), rate, rate_suffix: suffix, planned_cost: per_use, actual_cost: None }
                }
                Behavior::Work => {
                    let rate = r.hourly_rate.unwrap_or(default_rate);
                    let hours = planned_h.get(r.id.as_str()).copied().unwrap_or(0.0);
                    let actual = actual_h.get(r.id.as_str()).copied().filter(|_| has_actuals).map(|h| h * rate);
                    CostRow { resource: i, planned_hours: Some(hours), planned_qty: None, rate, rate_suffix: "/h".into(), planned_cost: hours * rate + per_use, actual_cost: actual }
                }
            }
        })
        .collect();
    (rows, has_actuals)
}

/// The mix recap: planned cost per kind, `person`, `equipment`, `material`, `cost` first (the web's object
/// literal), then the other kinds in order of appearance; zero amounts kept (the view skips them).
pub fn cost_by_kind(data: &ProjectData, rows: &[CostRow]) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = ["person", "equipment", "material", "cost"].iter().map(|k| (k.to_string(), 0.0)).collect();
    for r in rows {
        let kind = &data.resources[r.resource].kind;
        match out.iter_mut().find(|(k, _)| k == kind) {
            Some(e) => e.1 += r.planned_cost,
            None => out.push((kind.clone(), r.planned_cost)),
        }
    }
    out
}

/// `normaliseCurrency`.
pub fn normalise_currency(raw: Option<&str>) -> String {
    match raw.map(str::trim) {
        None | Some("") | Some("€") => "EUR".into(),
        Some("$") => "USD".into(),
        Some("£") => "GBP".into(),
        Some(c) => c.to_string(),
    }
}

/// `Math.round`-like rounding to `digits` decimals (`Intl` rounds half away from zero).
fn round_to(v: f64, digits: u32) -> f64 {
    let p = 10f64.powi(digits as i32);
    let x = (v.abs() * p + 0.5).floor() / p;
    if v < 0.0 {
        -x
    } else {
        x
    }
}

/// `Intl.NumberFormat(lang, { maximumFractionDigits: digits })`: fr groups with U+202F and a decimal comma,
/// en with commas and a point; trailing zeros dropped.
pub fn number(v: f64, lang: Lang, digits: u32) -> String {
    let r = round_to(v, digits);
    let neg = r < 0.0;
    let s = format!("{:.*}", digits as usize, r.abs());
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), f.trim_end_matches('0').to_string()),
        None => (s.clone(), String::new()),
    };
    let sep = match lang {
        Lang::Fr => '\u{202f}',
        Lang::En => ',',
    };
    let mut grouped = String::new();
    for (n, c) in int.chars().enumerate() {
        if n > 0 && (int.len() - n) % 3 == 0 {
            grouped.push(sep);
        }
        grouped.push(c);
    }
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    out.push_str(&grouped);
    if !frac.is_empty() {
        out.push(if lang == Lang::Fr { ',' } else { '.' });
        out.push_str(&frac);
    }
    out
}

/// A three-letter code `Intl` accepts as a currency.
fn is_currency_code(c: &str) -> bool {
    c.len() == 3 && c.chars().all(|x| x.is_ascii_alphabetic())
}

/// `makeMoney`: `{ style: 'currency', currency, maximumFractionDigits: 0 }`, a plain number for a code `Intl`
/// would refuse.
pub fn money(v: f64, currency: &str, lang: Lang) -> String {
    let n = number(v, lang, 0);
    if !is_currency_code(currency) {
        return n;
    }
    let code = currency.to_ascii_uppercase();
    let symbol = match (code.as_str(), lang) {
        ("EUR", _) => "€".to_string(),
        ("USD", Lang::Fr) => "$US".to_string(),
        ("USD", Lang::En) => "$".to_string(),
        ("GBP", Lang::Fr) => "£GB".to_string(),
        ("GBP", Lang::En) => "£".to_string(),
        (c, _) => c.to_string(),
    };
    match lang {
        Lang::Fr => format!("{n}\u{a0}{symbol}"),
        Lang::En if symbol.chars().count() == 1 => match n.strip_prefix('-') {
            Some(abs) => format!("-{symbol}{abs}"),
            None => format!("{symbol}{n}"),
        },
        Lang::En => format!("{symbol}\u{a0}{n}"),
    }
}

// ── Side data ──────────────────────────────────────────────────────────────

/// A time-off entry (`ResourceTimeOff`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TimeOff {
    pub id: String,
    #[serde(default)]
    pub resource_id: String,
    pub from_date: String,
    pub to_date: String,
    #[serde(default)]
    pub reason: String,
}

/// A directory member of the user's unit (`OrgMember`), a candidate « person ».
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OrgMember {
    pub id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

/// What the workspace reads next to the project.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResourceSide {
    /// Time off per resource id, once read (`listTimeOff`).
    pub time_off: BTreeMap<String, Vec<TimeOff>>,
    /// The directory members (`orgMembers`), once read.
    pub members: Option<Vec<OrgMember>>,
    /// `costs.config.currency` (free-form) and `default_hourly_rate`, once read.
    pub currency: Option<String>,
    pub default_hourly_rate: Option<f64>,
}

impl ResourceSide {
    pub fn add_time_off(&mut self, t: TimeOff) {
        self.time_off.entry(t.resource_id.clone()).or_default().push(t);
    }

    pub fn remove_time_off(&mut self, resource: &str, id: &str) {
        if let Some(v) = self.time_off.get_mut(resource) {
            v.retain(|t| t.id != id);
        }
    }

    /// The server's list for `resource` replaces the local one.
    pub fn set_time_off(&mut self, resource: &str, entries: Vec<TimeOff>) {
        self.time_off.insert(resource.to_string(), entries);
    }

    /// The currency code the cost view formats with.
    pub fn currency_code(&self) -> String {
        normalise_currency(self.currency.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Assignment, Task};
    use serde_json::Map;

    fn res(id: &str, name: &str, kind: &str) -> Resource {
        serde_json::from_value(serde_json::json!({ "id": id, "name": name, "kind": kind })).expect("resource")
    }

    fn task(id: &str, es: i32, dur: i32) -> Task {
        let mut t = Task::new(id, "p");
        t.early_start = Some(es);
        t.duration_days = dur;
        t
    }

    fn assign(task: &str, resource: &str, units: f64) -> Assignment {
        Assignment { id: format!("{task}-{resource}"), task_id: task.into(), resource_id: resource.into(), units, rest: Map::new() }
    }

    /// The web demo: Alice on three tasks (two overlapping), Bruno at 80 %, a truck, boxes and an insurance fee.
    fn demo() -> ProjectData {
        let mut d = ProjectData::default();
        d.project.id = "p".into();
        d.project.start_date = Some("2026-10-05".into());
        d.tasks = vec![task("t1", 0, 3), task("t2", 0, 4), task("t6", 10, 3), task("t5", 5, 2)];
        let mut alice = res("a", "Alice Martin", "person");
        alice.hourly_rate = Some(65.0);
        alice.skills = vec!["Planification".into(), "Budget".into()];
        alice.role = "Cheffe de projet".into();
        let mut bruno = res("b", "Bruno Diallo", "person");
        bruno.capacity = 0.8;
        bruno.hourly_rate = Some(48.0);
        let mut truck = res("c", "Camion", "equipment");
        truck.hourly_rate = Some(35.0);
        let mut boxes = res("m", "Cartons", "material");
        boxes.hourly_rate = Some(2.0);
        boxes.unit_label = Some("carton".into());
        let mut fee = res("f", "Assurance", "cost");
        fee.cost_per_use = Some(400.0);
        d.resources = vec![alice, bruno, truck, boxes, fee];
        d.assignments = vec![assign("t1", "a", 1.0), assign("t2", "a", 1.0), assign("t6", "a", 1.0), assign("t6", "b", 1.0), assign("t5", "c", 1.0), assign("t5", "m", 1.0), assign("t5", "f", 1.0)];
        d
    }

    #[test]
    fn taxonomy_and_member_colour() {
        assert_eq!(behavior_of("material"), Behavior::Material);
        assert_eq!(behavior_of("infrastructure"), Behavior::Cost);
        assert_eq!(behavior_of("robot"), Behavior::Work);
        assert_eq!(kind_label("contractor", Lang::Fr), "Consultant / sous-traitant");
        // JS: [...'u1'] → h = 117*31 + 49 = 3676; 3676 % 8 = 4.
        assert_eq!(member_color("u1"), "#d93025");
        let b = new_resource_body("material", Lang::Fr, "unité");
        assert_eq!(b.get("unit_label").and_then(|v| v.as_str()), Some("unité"));
        assert!(new_resource_body("person", Lang::Fr, "unité").get("unit_label").is_none());
    }

    #[test]
    fn utilisation_is_the_peak_over_capacity() {
        let d = demo();
        let peaks = peak_load(&d, 20);
        assert_eq!(peaks.get("a"), Some(&2.0));
        assert_eq!(utilisation(&d.resources[0], &peaks), Some(200.0));
        assert_eq!(utilisation(&d.resources[1], &peaks), Some(125.0));
        assert_eq!(utilisation(&d.resources[3], &peaks), None);
        assert_eq!(load_color(125.0), "#d93025");
        assert_eq!(load_color(100.0), "#e37400");
        assert_eq!(load_color(40.0), "#188038");
        assert_eq!(task_counts(&d).get("a"), Some(&3));
    }

    #[test]
    fn registry_filters_groups_and_sorts() {
        let d = demo();
        let all = registry(&d, "", GroupMode::None, Lang::Fr, "Sans rôle", "Sans compétence");
        let names: Vec<&str> = all[0].rows.iter().map(|&i| d.resources[i].name.as_str()).collect();
        assert_eq!(names, ["Alice Martin", "Assurance", "Bruno Diallo", "Camion", "Cartons"]);
        let q = registry(&d, "budg", GroupMode::None, Lang::Fr, "", "");
        assert_eq!(q[0].rows, vec![0]);
        let by_skill = registry(&d, "", GroupMode::Skill, Lang::Fr, "Sans rôle", "Sans compétence");
        let labels: Vec<&str> = by_skill.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, ["Budget", "Planification", "Sans compétence"]);
        let by_role = registry(&d, "", GroupMode::Role, Lang::Fr, "Sans rôle", "Sans compétence");
        assert_eq!(by_role.iter().map(|g| g.label.as_str()).collect::<Vec<_>>(), ["Cheffe de projet", "Sans rôle"]);
        let by_type = registry(&d, "", GroupMode::Type, Lang::Fr, "", "");
        assert_eq!(by_type[0].label, "Coût forfaitaire");
    }

    #[test]
    fn heatmap_columns_and_cells() {
        let start = Date::new(2026, 10, 5).expect("d");
        // 47 days from Monday 5 October: weeks S41…S47 (S48 starts on 30 November, past the span).
        let weeks = heat_columns(Scale::Week, start, 47, 2, Lang::Fr);
        assert_eq!(weeks.len(), 7);
        assert_eq!(weeks[0].sub_label, "S41");
        assert!(weeks[0].is_today);
        assert_eq!(weeks[0].days, vec![0, 1, 2, 3, 4]);
        assert_eq!(group_spans(&weeks), vec![("octobre 2026".to_string(), 4), ("novembre 2026".to_string(), 3)]);
        let days = heat_columns(Scale::Day, start, 3, 0, Lang::Fr);
        assert_eq!(days[0].sub_label, "lun. 5");
        let months = heat_columns(Scale::Month, start, 47, 0, Lang::Fr);
        assert_eq!(months.iter().map(|c| c.sub_label.as_str()).collect::<Vec<_>>(), ["oct.", "nov."]);
        assert_eq!(months[0].start, -4);
        let d = demo();
        let shown = work_resources(&d);
        assert_eq!(shown, vec![0, 1, 2]);
        let ratios = daily_ratios(&d, &shown, 47);
        let (r, h) = cell_value(ratios.get("a"), &weeks[0], 1.0);
        // Days 0–2 at 2, day 3 at 1, day 4 at 0: 7 / 5.
        assert!((r - 1.4).abs() < 1e-9);
        assert!((h - 11.2).abs() < 1e-9);
    }

    #[test]
    fn gradient_follows_the_web() {
        assert!(paint_for(0.0).zero);
        let p = paint_for(0.4);
        // mix(#e6f4ea, #34a853, 0.5) = (141, 206, 159).
        assert_eq!(p.rgb, (141, 206, 159));
        assert!(!p.white_ink);
        let full = paint_for(1.0);
        assert_eq!(full.rgb, (0xe3, 0x74, 0x00));
        let over = paint_for(1.4);
        assert!(over.overload && over.white_ink);
    }

    #[test]
    fn costs_like_the_web() {
        let d = demo();
        let (rows, actuals) = cost_rows(&d, 0.0);
        assert!(!actuals);
        assert_eq!(rows[0].planned_hours, Some(80.0));
        assert_eq!(rows[0].planned_cost, 5200.0);
        assert_eq!(rows[3].rate_suffix, "/carton");
        assert_eq!(rows[3].planned_cost, 0.0);
        assert_eq!(rows[4].planned_cost, 400.0);
        assert_eq!(rows[4].rate_suffix, "");
        let mix = cost_by_kind(&d, &rows);
        assert_eq!(mix[0].0, "person");
        assert_eq!(money(12208.0, "EUR", Lang::Fr), "12\u{202f}208\u{a0}€");
        assert_eq!(money(2034.67, "EUR", Lang::Fr), "2\u{202f}035\u{a0}€");
        assert_eq!(money(65.0, "EUR", Lang::En), "€65");
        assert_eq!(money(5.0, "points", Lang::Fr), "5");
        assert_eq!(number(12.5, Lang::Fr, 1), "12,5");
        assert_eq!(number(80.0, Lang::Fr, 1), "80");
        assert_eq!(normalise_currency(Some("€")), "EUR");
    }

    #[test]
    fn side_data() {
        let mut s = ResourceSide::default();
        s.add_time_off(TimeOff { id: "x".into(), resource_id: "a".into(), from_date: "2026-10-19".into(), to_date: "2026-10-21".into(), reason: "Formation".into() });
        assert_eq!(s.time_off.get("a").map(Vec::len), Some(1));
        s.remove_time_off("a", "x");
        assert_eq!(s.time_off.get("a").map(Vec::len), Some(0));
        assert_eq!(s.currency_code(), "EUR");
        let t: TimeOff = serde_json::from_value(serde_json::json!({"id":"1","resource_id":"a","from_date":"2026-10-19","to_date":"2026-10-21","reason":""})).expect("t");
        assert_eq!(t.from_date, "2026-10-19");
    }
}
