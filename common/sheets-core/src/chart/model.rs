//! The normalized chart model (`web/src/sheet-chart/model.ts`) and the normalization of a persisted
//! `SheetChart` of any vintage into it (`normalize.ts`). The persisted JSON is never modified here: the
//! object layer keeps it as stored (byte-identical round trip) and only this read-only view is derived.

use serde_json::Value as Json;

/// Normalized chart kind (LibreOffice chart2 naming: column = vertical bars).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartKind {
    Column,
    Bar,
    Line,
    Area,
    Pie,
    Scatter,
    Radar,
    Combo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stacking {
    None,
    Stacked,
    Percent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegendPos {
    Right,
    Left,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataLabels {
    Value,
    Percent,
}

/// `'auto' | boolean` of the legend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegendShow {
    Auto,
    Yes,
    No,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AxisSpec {
    pub title: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub format: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeriesKind {
    Column,
    Line,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SeriesSpec {
    pub name: Option<String>,
    pub name_ref: Option<String>,
    pub vals: Option<Vec<String>>,
    pub vals_range: Option<String>,
    pub cat_refs: Option<Vec<String>>,
    pub x_range: Option<String>,
    pub color: Option<String>,
    pub kind: Option<SeriesKind>,
    pub secondary: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RangeSource {
    pub range: String,
    /// `Some(true)` = rows, `Some(false)` = cols, `None` = auto.
    pub series_in_rows: Option<bool>,
    pub first_row_header: Option<bool>,
    pub first_col_header: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartSpec {
    pub kind: ChartKind,
    pub stacking: Stacking,
    pub title: Option<String>,
    pub series: Vec<SeriesSpec>,
    pub cat_refs: Option<Vec<String>>,
    pub cats_range: Option<String>,
    pub source: Option<RangeSource>,
    pub legend_show: LegendShow,
    pub legend_pos: LegendPos,
    pub grid_x: bool,
    pub grid_y: bool,
    pub axis_x: AxisSpec,
    pub axis_y: AxisSpec,
    pub data_labels: Option<DataLabels>,
    pub point_colors: Option<Vec<String>>,
    pub lines: bool,
    pub symbols: bool,
    pub smooth: bool,
    pub hole_size: f64,
    pub explode: f64,
    pub filled: bool,
    pub num_lines: f64,
}

/// LibreOffice chart2 default series palette (officecfg Chart.xcs), cycled modulo 12.
pub const CHART2_PALETTE: [&str; 12] = [
    "#004586", "#ff420e", "#ffd320", "#579d1c", "#7e0021", "#83caff", "#314004", "#aecf00", "#4b1f6f", "#ff950e", "#c5000b", "#0084d1",
];
/// chart2 "gray30": axis lines, gridlines, legend border, plot outline.
pub const CHART_NEUTRAL: &str = "#b3b3b3";
/// Marker shapes cycled by series index.
pub const SYMBOL_CYCLE: [&str; 8] = ["square", "diamond", "arrow-down", "arrow-up", "arrow-right", "arrow-left", "bowtie", "hourglass"];

pub const FONT_TITLE: f64 = 13.0;
pub const FONT_AXIS_TITLE: f64 = 11.0;
pub const FONT_LEGEND: f64 = 11.0;
pub const FONT_AXIS_LABEL: f64 = 10.0;
pub const FONT_DATA_LABEL: f64 = 10.0;
pub const BAR_GAP_RATIO: f64 = 1.0;
pub const DONUT_HOLE_DEFAULT: f64 = 0.4;
pub const PAGE_MARGIN_RATIO: f64 = 0.02;
pub const LEGEND_MAX_WIDTH_RATIO: f64 = 0.3;
pub const MAX_CHART_POINTS: usize = 4096;

/// Color for series `i`, honouring an explicit override.
pub fn series_color(i: usize, explicit: Option<&str>) -> String {
    explicit.map(str::to_string).unwrap_or_else(|| CHART2_PALETTE[i % CHART2_PALETTE.len()].to_string())
}

/// Persisted `type` → normalized kind (legacy 'bar' = vertical → column) (`normalizeKind`).
pub fn normalize_kind(t: Option<&str>) -> ChartKind {
    match t {
        Some("hbar") => ChartKind::Bar,
        Some("line") => ChartKind::Line,
        Some("area") => ChartKind::Area,
        Some("pie") | Some("donut") => ChartKind::Pie,
        Some("scatter") => ChartKind::Scatter,
        Some("radar") => ChartKind::Radar,
        Some("combo") => ChartKind::Combo,
        _ => ChartKind::Column,
    }
}

pub fn is_cartesian(k: ChartKind) -> bool {
    matches!(k, ChartKind::Column | ChartKind::Bar | ChartKind::Line | ChartKind::Area | ChartKind::Scatter | ChartKind::Combo)
}

fn s(v: Option<&Json>) -> Option<String> {
    v.and_then(Json::as_str).map(str::to_string)
}

fn strs(v: Option<&Json>) -> Option<Vec<String>> {
    v.and_then(Json::as_array).map(|a| a.iter().map(|x| x.as_str().map(str::to_string).unwrap_or_else(|| crate::model::cell::js_string_of(x))).collect())
}

fn b(v: Option<&Json>) -> Option<bool> {
    v.and_then(Json::as_bool)
}

fn n(v: Option<&Json>) -> Option<f64> {
    v.and_then(Json::as_f64)
}

fn axis(v: Option<&Json>) -> AxisSpec {
    match v.and_then(Json::as_object) {
        Some(o) => AxisSpec { title: s(o.get("title")), min: n(o.get("min")), max: n(o.get("max")), format: s(o.get("format")) },
        None => AxisSpec::default(),
    }
}

/// `normalizeChart`: a persisted chart of any vintage → the spec.
pub fn normalize_chart(c: &Json) -> ChartSpec {
    let kind = normalize_kind(c.get("type").and_then(Json::as_str));
    let series_json = c.get("series").and_then(Json::as_array).filter(|a| !a.is_empty());
    let vals = strs(c.get("vals")).filter(|v| !v.is_empty());
    let cats = strs(c.get("cats"));
    let mut series = Vec::new();
    let mut source = None;
    if let Some(arr) = series_json {
        series = arr
            .iter()
            .map(|x| SeriesSpec {
                name: s(x.get("name")),
                name_ref: s(x.get("nameRef")),
                vals: strs(x.get("vals")),
                vals_range: s(x.get("valsRange")),
                cat_refs: strs(x.get("cats")),
                x_range: s(x.get("xRange")),
                color: s(x.get("color")),
                kind: match x.get("kind").and_then(Json::as_str) {
                    Some("column") => Some(SeriesKind::Column),
                    Some("line") => Some(SeriesKind::Line),
                    _ => None,
                },
                secondary: x.get("axis").and_then(Json::as_str) == Some("secondary"),
            })
            .collect();
    } else if let Some(v) = vals {
        series = vec![SeriesSpec { vals: Some(v), cat_refs: cats.clone(), ..Default::default() }];
    } else if let Some(range) = s(c.get("range")).filter(|r| !r.is_empty()) {
        source = Some(RangeSource {
            range,
            series_in_rows: c.get("seriesIn").and_then(Json::as_str).map(|v| v == "rows"),
            first_row_header: b(c.get("firstRowHeader")),
            first_col_header: b(c.get("firstColHeader")),
        });
    }
    let stacking = if matches!(kind, ChartKind::Pie | ChartKind::Scatter) {
        Stacking::None
    } else {
        match c.get("grouping").and_then(Json::as_str) {
            Some("stacked") => Stacking::Stacked,
            Some("percentStacked") => Stacking::Percent,
            _ => Stacking::None,
        }
    };
    let cat_refs = match &cats {
        Some(cs) if !series.is_empty() && series[0].cat_refs.is_none() => Some(cs.clone()),
        _ => None,
    };
    let grid = c.get("grid");
    let ty = c.get("type").and_then(Json::as_str);
    ChartSpec {
        kind,
        stacking,
        title: s(c.get("title")).filter(|t| !t.is_empty()),
        series,
        cat_refs,
        cats_range: s(c.get("catsRange")).filter(|t| !t.is_empty()),
        source,
        legend_show: match c.get("legend").and_then(Json::as_bool) {
            Some(true) => LegendShow::Yes,
            Some(false) => LegendShow::No,
            None => LegendShow::Auto,
        },
        legend_pos: match c.get("legendPos").and_then(Json::as_str) {
            Some("left") => LegendPos::Left,
            Some("top") => LegendPos::Top,
            Some("bottom") => LegendPos::Bottom,
            _ => LegendPos::Right,
        },
        grid_x: b(grid.and_then(|g| g.get("x"))).unwrap_or(false),
        grid_y: b(grid.and_then(|g| g.get("y"))).unwrap_or(is_cartesian(kind)),
        axis_x: axis(c.get("axisX")),
        axis_y: axis(c.get("axisY")),
        data_labels: match c.get("dataLabels").and_then(Json::as_str) {
            Some("value") => Some(DataLabels::Value),
            Some("percent") => Some(DataLabels::Percent),
            _ => None,
        },
        point_colors: strs(c.get("colors")),
        lines: b(c.get("lines")).unwrap_or(kind != ChartKind::Scatter),
        symbols: b(c.get("symbols")).unwrap_or(kind == ChartKind::Scatter),
        smooth: b(c.get("smooth")).unwrap_or(false),
        hole_size: if kind == ChartKind::Pie { n(c.get("holeSize")).unwrap_or(if ty == Some("donut") { DONUT_HOLE_DEFAULT } else { 0.0 }) } else { 0.0 },
        explode: n(c.get("explode")).unwrap_or(0.0),
        filled: b(c.get("filled")).unwrap_or(false),
        num_lines: n(c.get("numLines")).unwrap_or(1.0).max(1.0),
    }
}
