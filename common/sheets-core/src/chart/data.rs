//! Spec + cell accessor → resolved, render-ready data (`web/src/sheet-chart/data.ts`).

use super::model::*;
use crate::js;
use crate::refs::ref_bounds;

/// A raw cell value as the host surfaces it (formula results included).
#[derive(Clone, Debug, PartialEq)]
pub enum CellVal {
    Num(f64),
    Str(String),
    Bool(bool),
    Empty,
}

pub type CellGetter<'a> = dyn Fn(&str) -> CellVal + 'a;

/// The drawing kind of one resolved series.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    Column,
    Bar,
    Line,
    Area,
    Pie,
    Radar,
    Scatter,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedSeries {
    pub name: String,
    pub color: String,
    pub kind: MarkKind,
    pub secondary: bool,
    pub values: Vec<Option<f64>>,
    pub x: Option<Vec<Option<f64>>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChartData {
    pub series: Vec<ResolvedSeries>,
    pub categories: Vec<String>,
    pub point_colors: Option<Vec<String>>,
}

/// 0-indexed column → letters.
pub fn col_letter(c: i64) -> String {
    crate::addr::index_to_col(c)
}

pub fn cell_ref(c: i64, r: i64) -> String {
    format!("{}{r}", col_letter(c))
}

/// `expandRange`: the refs of a rectangle, `grid[row][col]`, capped at `MAX_CHART_POINTS` cells.
pub fn expand_range(range: &str) -> Option<Vec<Vec<String>>> {
    let b = ref_bounds(range)?;
    let rows = ((b.r2 - b.r1 + 1).max(0) as usize).min(MAX_CHART_POINTS);
    let cols = ((b.c2 - b.c1 + 1).max(0) as usize).min(256);
    if rows * cols > MAX_CHART_POINTS {
        return None;
    }
    Some((0..rows).map(|r| (0..cols).map(|c| cell_ref(b.c1 + c as i64, b.r1 + r as i64)).collect()).collect())
}

/// `toNumber`.
pub fn to_number(v: &CellVal) -> Option<f64> {
    match v {
        CellVal::Num(n) => n.is_finite().then_some(*n),
        CellVal::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        CellVal::Str(s) if !js::trim(s).is_empty() => js::number(&s.replacen(',', ".", 1)).filter(|n| n.is_finite()),
        _ => None,
    }
}

/// `toLabel` (`String(v)`).
pub fn to_label(v: &CellVal) -> String {
    match v {
        CellVal::Num(n) => js::to_string(*n),
        CellVal::Str(s) => s.clone(),
        CellVal::Bool(b) => if *b { "true" } else { "false" }.into(),
        CellVal::Empty => String::new(),
    }
}

pub struct RangeLayout {
    pub series_in_rows: bool,
    pub first_row_header: bool,
    pub first_col_header: bool,
}

/// `detectRangeLayout` (Calc's chart-insertion heuristics).
pub fn detect_range_layout(source: &RangeSource, get: &CellGetter) -> Option<RangeLayout> {
    let grid = expand_range(&source.range)?;
    if grid.is_empty() || grid[0].is_empty() {
        return None;
    }
    let (rows, cols) = (grid.len(), grid[0].len());
    let has_col_header = || rows >= 2 && (0..cols).all(|c| to_number(&get(&grid[0][c])).is_none());
    let has_row_header = || cols >= 2 && (0..rows).all(|r| to_number(&get(&grid[r][0])).is_none());
    Some(RangeLayout {
        series_in_rows: source.series_in_rows.unwrap_or(rows == 1),
        first_row_header: source.first_row_header.unwrap_or_else(has_col_header),
        first_col_header: source.first_col_header.unwrap_or_else(has_row_header),
    })
}

fn default_name(i: usize, series_word: &str) -> String {
    format!("{series_word} {}", i + 1)
}

fn series_kind(spec: &ChartSpec, s: Option<&SeriesSpec>, i: usize, n: usize) -> MarkKind {
    if spec.kind == ChartKind::Combo {
        if let Some(k) = s.and_then(|s| s.kind) {
            return if k == SeriesKind::Line { MarkKind::Line } else { MarkKind::Column };
        }
        let lines = spec.num_lines.min(n.saturating_sub(1) as f64);
        return if (i as f64) >= n as f64 - lines { MarkKind::Line } else { MarkKind::Column };
    }
    match spec.kind {
        ChartKind::Column | ChartKind::Combo => MarkKind::Column,
        ChartKind::Bar => MarkKind::Bar,
        ChartKind::Line => MarkKind::Line,
        ChartKind::Area => MarkKind::Area,
        ChartKind::Pie => MarkKind::Pie,
        ChartKind::Scatter => MarkKind::Scatter,
        ChartKind::Radar => MarkKind::Radar,
    }
}

fn read_refs(refs: &[String], get: &CellGetter) -> Vec<Option<f64>> {
    refs.iter().take(MAX_CHART_POINTS).map(|r| to_number(&get(r))).collect()
}

fn flat_column(grid: &[Vec<String>]) -> Vec<String> {
    if grid.len() == 1 {
        grid[0].clone()
    } else if grid.first().is_some_and(|r| r.len() == 1) {
        grid.iter().map(|r| r[0].clone()).collect()
    } else {
        grid.iter().flatten().cloned().collect()
    }
}

fn read_range_column(range: &str, get: &CellGetter) -> Vec<Option<f64>> {
    match expand_range(range) {
        Some(g) => read_refs(&flat_column(&g), get),
        None => Vec::new(),
    }
}

fn read_label_range(range: &str, get: &CellGetter) -> Vec<String> {
    let Some(grid) = expand_range(range) else { return Vec::new() };
    let flat: Vec<String> = if grid.len() == 1 { grid[0].clone() } else { grid.iter().map(|r| r[0].clone()).collect() };
    flat.iter().take(MAX_CHART_POINTS).map(|r| to_label(&get(r))).collect()
}

fn resolve_range(spec: &ChartSpec, source: &RangeSource, get: &CellGetter, word: &str) -> ChartData {
    let (Some(layout), Some(grid0)) = (detect_range_layout(source, get), expand_range(&source.range)) else {
        return ChartData::default();
    };
    let grid: Vec<Vec<String>> = if layout.series_in_rows { (0..grid0[0].len()).map(|c| grid0.iter().map(|row| row[c].clone()).collect()).collect() } else { grid0 };
    let name_header = if layout.series_in_rows { layout.first_col_header } else { layout.first_row_header };
    let cat_header = if layout.series_in_rows { layout.first_row_header } else { layout.first_col_header };
    let data_rows: &[Vec<String>] = if name_header { &grid[1..] } else { &grid[..] };
    if data_rows.is_empty() {
        return ChartData::default();
    }
    let n_cols = grid[0].len();
    let categories = if cat_header { data_rows.iter().map(|row| to_label(&get(&row[0]))).collect() } else { Vec::new() };
    let first = if cat_header { 1 } else { 0 };
    let is_scatter = spec.kind == ChartKind::Scatter;
    let x_col: Option<usize> = (is_scatter && n_cols >= first + 2).then_some(first);
    let x: Option<Vec<Option<f64>>> = x_col.map(|xc| data_rows.iter().map(|row| to_number(&get(&row[xc]))).collect());
    let n_series = (n_cols as i64 - first as i64 - if x_col.is_some() { 1 } else { 0 }).max(1) as usize;
    let mut series = Vec::new();
    for c in first..n_cols {
        if Some(c) == x_col {
            continue;
        }
        let i = series.len();
        let name = if name_header { to_label(&get(&grid[0][c])) } else { String::new() };
        series.push(ResolvedSeries {
            name: if name.is_empty() { default_name(i, word) } else { name },
            color: series_color(i, None),
            kind: series_kind(spec, None, i, n_series),
            secondary: false,
            values: data_rows.iter().map(|row| to_number(&get(&row[c]))).collect(),
            x: x.clone(),
        });
    }
    ChartData { series, categories, point_colors: spec.point_colors.clone() }
}

/// `resolveChartData`. `series_word` is the i18n fallback name stem (« Series » / « Série »).
pub fn resolve_chart_data(spec: &ChartSpec, get: &CellGetter, series_word: &str) -> ChartData {
    if let Some(src) = &spec.source {
        return resolve_range(spec, src, get, series_word);
    }
    let n = spec.series.len();
    let series = spec
        .series
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let values = if let Some(r) = &s.vals_range {
                read_range_column(r, get)
            } else if let Some(v) = &s.vals {
                read_refs(v, get)
            } else {
                Vec::new()
            };
            let x = if spec.kind == ChartKind::Scatter {
                if let Some(r) = &s.x_range {
                    Some(read_range_column(r, get))
                } else {
                    s.cat_refs.as_ref().map(|c| read_refs(c, get))
                }
            } else {
                None
            };
            let live = s.name_ref.as_ref().map(|r| to_label(&get(r))).unwrap_or_default();
            let name = if !live.is_empty() {
                live
            } else {
                s.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| default_name(i, series_word))
            };
            ResolvedSeries { name, color: series_color(i, s.color.as_deref()), kind: series_kind(spec, Some(s), i, n), secondary: s.secondary, values, x }
        })
        .collect();
    let categories = if let Some(r) = &spec.cats_range {
        read_label_range(r, get)
    } else {
        let refs = spec.cat_refs.as_ref().or_else(|| spec.series.iter().find_map(|s| s.cat_refs.as_ref()));
        match refs {
            Some(c) if spec.kind != ChartKind::Scatter => c.iter().take(MAX_CHART_POINTS).map(|r| to_label(&get(r))).collect(),
            _ => Vec::new(),
        }
    };
    ChartData { series, categories, point_colors: spec.point_colors.clone() }
}
