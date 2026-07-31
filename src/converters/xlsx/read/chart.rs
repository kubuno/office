//! Chart part (xl/charts/chartN.xml) parsing.
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use super::super::util::{attr, col_to_idx, idx_to_col, split_ref};

// Expand a chart cat/val reference (possibly sheet-qualified, $-anchored, a range
// or a comma-separated multi-area) into a flat list of bare cell refs ("R18").
pub fn normalize_refs(f: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in f.split(',') {
        // Drop a wrapping (...) multi-area union, the sheet prefix and $ anchors.
        let body = part.trim().trim_matches(|c| c == '(' || c == ')').rsplit('!').next().unwrap_or("").replace(['$', '(', ')'], "");
        if let Some((a, z)) = body.split_once(':') {
            if let (Some((ca, ra)), Some((cz, rz))) = (split_ref(a), split_ref(z)) {
                let (c1, c2) = (col_to_idx(&ca).min(col_to_idx(&cz)), col_to_idx(&ca).max(col_to_idx(&cz)));
                let (r1, r2) = (ra.min(rz), ra.max(rz));
                for r in r1..=r2 { for c in c1..=c2 { out.push(format!("{}{}", idx_to_col(c), r)); } }
            }
        } else if !body.is_empty() {
            out.push(body.to_uppercase());
        }
    }
    out
}

/// One `<c:ser>`: its cached/literal name plus the raw `<c:f>` reference strings
/// of its category (`c:cat`/`c:xVal`) and value (`c:val`/`c:yVal`) sides.
#[derive(Default)]
pub struct ChartSeries {
    pub name: Option<String>,
    pub cats: Vec<String>,
    pub vals: Vec<String>,
    /// Series-level colour from `<c:spPr>`, "#RRGGBB" (resolved at end of parse:
    /// line stroke for line-drawn families, fill otherwise).
    pub color: Option<String>,
    color_fill: Option<String>,  // spPr solidFill/gradFill colour
    color_ln:   Option<String>,  // spPr > a:ln stroke colour
    /// Host plot element family: "line" when the series lives in a lineChart
    /// (used to tag combo-chart series), None otherwise.
    pub kind: Option<String>,
    /// Explicit per-series `<c:smooth>` value.
    pub smooth: Option<bool>,
    /// Explicit per-series `<c:marker><c:symbol>` (true unless "none").
    pub symbols: Option<bool>,
}

/// One parsed axis, resolved into the X/Y slots at the end of the parse.
#[derive(Default)]
struct AxisParsed {
    cat:   bool,            // catAx/dateAx (category-like)
    ser:   bool,            // serAx (3D series axis — ignored)
    pos:   Option<String>,  // axPos val: "b" | "t" | "l" | "r"
    title: String,
    grid:  bool,            // majorGridlines present
    min:   Option<f64>,     // manual c:scaling bounds
    max:   Option<f64>,
}

/// Resolved axis output (only what the internal model keeps).
pub struct AxisOut {
    pub title: Option<String>,
    pub min:   Option<f64>,
    pub max:   Option<f64>,
}

// Parsed chart structure + presentation options.
pub struct ChartParsed {
    pub ctype:       String,
    pub title:       Option<String>,   // chart title (rich-text runs or cached strRef)
    pub grouping:    Option<String>,   // "stacked" | "percentStacked" (only when set)
    pub series:      Vec<ChartSeries>, // per-series structure, in document order
    pub legend:      bool,
    pub legend_pos:  Option<String>,   // "right" | "left" | "top" | "bottom"
    pub data_labels: Option<String>,   // "value" | "percent" | "category" | None
    pub colors:      Vec<String>,      // per-point "#RRGGBB" (from c:dPt), in idx order; may be empty
    pub hole_size:   Option<f64>,      // doughnut inner radius, 0..1
    pub explode:     Option<f64>,      // pie slice offset, fraction of the radius
    pub filled:      bool,             // radarStyle val="filled"
    pub smooth:      Option<bool>,     // any explicit c:smooth / smooth scatterStyle
    pub symbols:     Option<bool>,     // any explicit marker symbol / *Marker scatterStyle
    pub lines:       Option<bool>,     // scatter: style draws connecting lines
    pub axis_x:      Option<AxisOut>,  // category axis (or scatter's bottom value axis)
    pub axis_y:      Option<AxisOut>,  // value axis
    pub grid_x:      bool,
    pub grid_y:      bool,
    // ── Chart-area formatting (chartSpace-level c:spPr / c:txPr) ──
    pub fill:         Option<String>,  // "#RRGGBB" | "none"
    pub border:       Option<String>,  // "#RRGGBB" | "none"
    pub border_width: Option<f64>,     // outline width in px
    pub font:         Option<String>,  // default typeface of the chart's text
}

// Axis elements whose <c:title> must not be mistaken for the chart title.
const AXIS_ELEMS: [&[u8]; 4] = [b"catAx", b"valAx", b"dateAx", b"serAx"];

// Plot elements (chart-type containers under c:plotArea).
const PLOT_ELEMS: [&[u8]; 14] = [
    b"barChart", b"bar3DChart", b"lineChart", b"line3DChart", b"pieChart", b"pie3DChart",
    b"doughnutChart", b"ofPieChart", b"areaChart", b"area3DChart", b"scatterChart",
    b"bubbleChart", b"radarChart", b"stockChart",
];

fn has(stack: &[Vec<u8>], name: &[u8]) -> bool { stack.iter().any(|e| e == name) }
fn in_axis(stack: &[Vec<u8>]) -> bool { stack.iter().any(|e| AXIS_ELEMS.contains(&e.as_slice())) }

// True when the ancestor context is exactly `c:chartSpace > <child> > …`, i.e.
// the CHART AREA's own properties — as opposed to the identically named
// elements nested under plotArea, a series or an axis.
fn space_child(stack: &[Vec<u8>], child: &[u8]) -> bool {
    stack.first().map(|n| n.as_slice()) == Some(b"chartSpace".as_slice())
        && stack.get(1).map(|n| n.as_slice()) == Some(child)
}

// Mutable parse state shared by the Start/Empty arms.
struct ParseState {
    plots:         Vec<String>,  // plot element local names, in document order
    bar_dir:       String,
    grouping:      Option<String>,
    legend:        bool,
    legend_pos:    Option<String>,
    show_val:      bool,
    show_pct:      bool,
    show_cat:      bool,
    series:        Vec<ChartSeries>,
    colors:        Vec<String>,
    hole_size:     Option<f64>,
    explode:       Option<f64>,
    radar_style:   Option<String>,
    scatter_style: Option<String>,
    axes:          Vec<AxisParsed>,
    cur_axis:      Option<AxisParsed>,
    // Chart-area formatting (chartSpace > spPr / txPr).
    space_fill:    Option<String>,
    space_border:  Option<String>,
    space_ln_w:    Option<f64>,
    space_font:    Option<String>,
}

// Elements under <c:ser> whose spPr colours are NOT the series colour.
const NON_SERIES_COLOR_CTX: [&[u8]; 6] = [b"dPt", b"marker", b"dLbls", b"tx", b"trendline", b"errBars"];

// Handle an element's opening tag (shared by Start and Empty events). `stack`
// is the ancestor context (the element itself is not on it yet).
fn on_element(st: &mut ParseState, e: &BytesStart, stack: &[Vec<u8>]) {
    let name = e.local_name();
    let name = name.as_ref();
    if PLOT_ELEMS.contains(&name) {
        st.plots.push(String::from_utf8_lossy(name).into_owned());
        return;
    }
    match name {
        b"barDir" => if let Some(v) = attr(e, b"val") { st.bar_dir = v; },
        // First grouping wins (the primary plot type's).
        b"grouping" if st.grouping.is_none() => st.grouping = attr(e, b"val"),
        b"legend" => st.legend = true,
        b"legendPos" => st.legend_pos = attr(e, b"val").and_then(|v| match v.as_str() {
            "r" | "tr" => Some("right".to_string()),
            "l" => Some("left".to_string()),
            "t" => Some("top".to_string()),
            "b" => Some("bottom".to_string()),
            _ => None,
        }),
        b"showVal" if attr(e, b"val").as_deref() == Some("1") => st.show_val = true,
        b"showPercent" if attr(e, b"val").as_deref() == Some("1") => st.show_pct = true,
        b"showCatName" if attr(e, b"val").as_deref() == Some("1") => st.show_cat = true,
        b"ser" => {
            // Tag the series with its host plot family (combo charts mix them).
            let kind = stack.iter().rev().find(|n| PLOT_ELEMS.contains(&n.as_slice()))
                .and_then(|n| match n.as_slice() {
                    b"lineChart" | b"line3DChart" => Some("line".to_string()),
                    _ => None,
                });
            st.series.push(ChartSeries { kind, ..ChartSeries::default() });
        }
        // ── Chart-area formatting ──
        // <c:spPr> directly under <c:chartSpace>: the chart area's fill and outline.
        b"noFill" if space_child(stack, b"spPr") => {
            let slot = if has(stack, b"ln") { &mut st.space_border } else { &mut st.space_fill };
            if slot.is_none() { *slot = Some("none".to_string()); }
        }
        b"ln" if space_child(stack, b"spPr") => {
            if let Some(w) = attr(e, b"w").and_then(|v| v.parse::<f64>().ok()) {
                st.space_ln_w = Some(((w / 12_700.0) * 10.0).round() / 10.0);
            }
        }
        // <c:txPr> default run properties: only the latin typeface is modelled.
        b"latin" if space_child(stack, b"txPr") => {
            // "+mn-lt" / "+mj-lt" are theme references, not a concrete typeface.
            if let Some(f) = attr(e, b"typeface").filter(|f| !f.starts_with('+') && !f.is_empty()) {
                if st.space_font.is_none() { st.space_font = Some(f); }
            }
        }
        b"srgbClr" if space_child(stack, b"spPr") => {
            if let Some(v) = attr(e, b"val") {
                let slot = if has(stack, b"ln") { &mut st.space_border } else { &mut st.space_fill };
                if slot.is_none() { *slot = Some(format!("#{}", v.to_uppercase())); }
            }
        }
        b"srgbClr" => {
            if has(stack, b"dPt") {
                if let Some(v) = attr(e, b"val") { st.colors.push(format!("#{}", v.to_uppercase())); }
            } else if has(stack, b"ser") && has(stack, b"spPr")
                && !NON_SERIES_COLOR_CTX.iter().any(|c| has(stack, c)) {
                if let (Some(s), Some(v)) = (st.series.last_mut(), attr(e, b"val")) {
                    let slot = if has(stack, b"ln") { &mut s.color_ln } else { &mut s.color_fill };
                    if slot.is_none() { *slot = Some(format!("#{}", v.to_uppercase())); }
                }
            }
        }
        b"holeSize" => st.hole_size = attr(e, b"val").and_then(|v| v.parse::<f64>().ok()).map(|v| v / 100.0),
        b"explosion" if st.explode.is_none() =>
            st.explode = attr(e, b"val").and_then(|v| v.parse::<f64>().ok()).map(|v| v / 100.0),
        b"radarStyle" => st.radar_style = attr(e, b"val"),
        b"scatterStyle" => st.scatter_style = attr(e, b"val"),
        // <c:smooth/> defaults to val="1" per the schema.
        b"smooth" if has(stack, b"ser") => {
            if let Some(s) = st.series.last_mut() { s.smooth = Some(attr(e, b"val").as_deref() != Some("0")); }
        }
        b"symbol" if has(stack, b"ser") && has(stack, b"marker") => {
            if let (Some(s), Some(v)) = (st.series.last_mut(), attr(e, b"val")) { s.symbols = Some(v != "none"); }
        }
        b"catAx" | b"dateAx" => st.cur_axis = Some(AxisParsed { cat: true, ..AxisParsed::default() }),
        b"valAx" => st.cur_axis = Some(AxisParsed::default()),
        b"serAx" => st.cur_axis = Some(AxisParsed { ser: true, ..AxisParsed::default() }),
        b"axPos" if in_axis(stack) => {
            if let Some(a) = st.cur_axis.as_mut() { a.pos = attr(e, b"val"); }
        }
        b"majorGridlines" if in_axis(stack) => {
            if let Some(a) = st.cur_axis.as_mut() { a.grid = true; }
        }
        b"max" if in_axis(stack) && has(stack, b"scaling") => {
            if let Some(a) = st.cur_axis.as_mut() { a.max = attr(e, b"val").and_then(|v| v.parse().ok()); }
        }
        b"min" if in_axis(stack) && has(stack, b"scaling") => {
            if let Some(a) = st.cur_axis.as_mut() { a.min = attr(e, b"val").and_then(|v| v.parse().ok()); }
        }
        _ => {}
    }
}

// Map the primary plot element to the internal chart type.
fn primary_type(plot: &str, bar_dir: &str) -> String {
    match plot {
        "pieChart" | "pie3DChart" | "ofPieChart" => "pie",
        "doughnutChart" => "donut",
        "barChart" | "bar3DChart" if bar_dir == "bar" => "hbar",
        "barChart" | "bar3DChart" => "bar",
        "lineChart" | "line3DChart" => "line",
        "areaChart" | "area3DChart" => "area",
        "scatterChart" | "bubbleChart" => "scatter",
        "radarChart" => "radar",
        // Stock charts share the cat/val series shape of a line chart; the
        // internal model renders them as lines (closest supported type).
        "stockChart" => "line",
        _ => "bar",
    }.to_string()
}

// Aggregate explicit per-series booleans into a chart-level flag: None when no
// series states one, else true if any does.
fn agg_flag(series: &[ChartSeries], get: impl Fn(&ChartSeries) -> Option<bool>) -> Option<bool> {
    let known: Vec<bool> = series.iter().filter_map(&get).collect();
    if known.is_empty() { None } else { Some(known.iter().any(|b| *b)) }
}

fn axis_out(a: &AxisParsed) -> Option<AxisOut> {
    let title = { let t = a.title.trim(); if t.is_empty() { None } else { Some(t.to_string()) } };
    if title.is_none() && a.min.is_none() && a.max.is_none() { return None; }
    Some(AxisOut { title, min: a.min, max: a.max })
}

pub fn parse_chart_xml(xml: &str) -> ChartParsed {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut st = ParseState {
        plots: Vec::new(), bar_dir: String::new(), grouping: None,
        legend: false, legend_pos: None, show_val: false, show_pct: false, show_cat: false,
        series: Vec::new(), colors: Vec::new(),
        hole_size: None, explode: None, radar_style: None, scatter_style: None,
        axes: Vec::new(), cur_axis: None,
        space_fill: None, space_border: None, space_ln_w: None, space_font: None,
    };
    let mut title = String::new();
    // Element stack of local names (Start events only — Empty events own no End).
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.local_name().as_ref().to_vec();
                on_element(&mut st, &e, &stack);
                if matches!(name.as_slice(), b"f" | b"v" | b"t") { text.clear(); }
                stack.push(name);
            }
            Ok(Event::Empty(e)) => on_element(&mut st, &e, &stack),
            Ok(Event::Text(e)) => {
                if matches!(stack.last().map(|n| n.as_slice()), Some(b"f") | Some(b"v") | Some(b"t")) {
                    text.push_str(&e.unescape().unwrap_or_default());
                }
            }
            Ok(Event::End(e)) => {
                let name = e.local_name().as_ref().to_vec();
                if stack.last() == Some(&name) { stack.pop(); }
                let ctx = &stack[..];
                match name.as_slice() {
                    b"f" => {
                        let f = std::mem::take(&mut text);
                        if !f.is_empty() {
                            if has(ctx, b"cat") || has(ctx, b"xVal") {
                                if let Some(s) = st.series.last_mut() { s.cats.push(f); }
                            } else if has(ctx, b"val") || has(ctx, b"yVal") {
                                if let Some(s) = st.series.last_mut() { s.vals.push(f); }
                            }
                        }
                    }
                    b"v" | b"t" => {
                        // Title text: literal runs (<a:t>) or cached string refs (<c:v>).
                        let v = std::mem::take(&mut text);
                        if v.is_empty() {
                        } else if name == b"v" && has(ctx, b"ser") && has(ctx, b"tx") && !has(ctx, b"cat") && !has(ctx, b"val") {
                            // Series name: literal <c:tx><c:v> or cached <c:strCache><c:pt><c:v>.
                            if let Some(s) = st.series.last_mut() {
                                if s.name.is_none() { s.name = Some(v); }
                            }
                        } else if in_axis(ctx) && has(ctx, b"title") {
                            // Axis title (rich-text run or cached string reference).
                            if let Some(a) = st.cur_axis.as_mut() { a.title.push_str(&v); }
                        } else if has(ctx, b"title") && !has(ctx, b"ser") {
                            // Chart title.
                            title.push_str(&v);
                        }
                    }
                    b"catAx" | b"valAx" | b"dateAx" | b"serAx" => {
                        if let Some(a) = st.cur_axis.take() { st.axes.push(a); }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    // Chart type: a plotArea mixing bar + line plots is a combo chart; else the
    // first plot element wins.
    let has_bar = st.plots.iter().any(|p| p == "barChart" || p == "bar3DChart");
    let has_line = st.plots.iter().any(|p| p == "lineChart" || p == "line3DChart");
    let primary = st.plots.first().map(String::as_str).unwrap_or("");
    let ctype = if has_bar && has_line { "combo".to_string() } else { primary_type(primary, &st.bar_dir) };

    // Chart-level line styling flags.
    let mut smooth = agg_flag(&st.series, |s| s.smooth);
    let mut symbols = agg_flag(&st.series, |s| s.symbols);
    let mut lines: Option<bool> = None;
    if ctype == "scatter" {
        match st.scatter_style.as_deref() {
            Some("lineMarker") => { lines = Some(true); symbols = symbols.or(Some(true)); }
            Some("line") => { lines = Some(true); symbols = symbols.or(Some(false)); }
            Some("marker") => { lines = Some(false); symbols = symbols.or(Some(true)); }
            Some("smoothMarker") => { lines = Some(true); smooth = smooth.or(Some(true)); symbols = symbols.or(Some(true)); }
            Some("smooth") => { lines = Some(true); smooth = smooth.or(Some(true)); symbols = symbols.or(Some(false)); }
            _ => {}
        }
    }
    let filled = ctype == "radar" && st.radar_style.as_deref() == Some("filled");
    if ctype == "radar" && st.radar_style.as_deref() == Some("marker") { symbols = symbols.or(Some(true)); }

    // Resolve each series' colour: line-drawn series carry it on the stroke,
    // area-like ones on the fill (a bare border colour is not a series colour).
    let chart_is_line = matches!(ctype.as_str(), "line" | "scatter") || (ctype == "radar" && !filled);
    for s in &mut st.series {
        let line_drawn = chart_is_line || s.kind.as_deref() == Some("line");
        s.color = if line_drawn { s.color_ln.take().or_else(|| s.color_fill.take()) } else { s.color_fill.take() };
    }

    // Resolve parsed axes into the X (category / bottom) and Y (value) slots.
    let cat_axes: Vec<&AxisParsed> = st.axes.iter().filter(|a| a.cat).collect();
    let val_axes: Vec<&AxisParsed> = st.axes.iter().filter(|a| !a.cat && !a.ser).collect();
    let (x_axis, y_axis) = if let Some(cat) = cat_axes.first() {
        (Some(*cat), val_axes.first().copied())
    } else {
        // Scatter/bubble: two value axes, discriminated by position (bottom/top = X).
        let xi = val_axes.iter().position(|a| matches!(a.pos.as_deref(), Some("b") | Some("t"))).unwrap_or(0);
        let x = val_axes.get(xi).copied();
        let y = val_axes.iter().enumerate().find(|(i, _)| *i != xi).map(|(_, a)| *a);
        (x, y)
    };
    let (axis_x, axis_y) = (x_axis.and_then(axis_out), y_axis.and_then(axis_out));
    let grid_x = x_axis.map(|a| a.grid).unwrap_or(false);
    let grid_y = y_axis.map(|a| a.grid).unwrap_or(false);

    let data_labels = if st.show_pct { Some("percent".to_string()) }
        else if st.show_val { Some("value".to_string()) }
        else if st.show_cat { Some("category".to_string()) }
        else { None };
    let grouping = st.grouping.filter(|g| g == "stacked" || g == "percentStacked");
    let title = { let t = title.trim().to_string(); if t.is_empty() { None } else { Some(t) } };
    let space_border = st.space_border.take();
    // A width without a colour would export an invisible outline of that width;
    // keep it only when the chart area really has a stroke.
    let border_width = st.space_ln_w.filter(|_| space_border.as_deref().is_some_and(|b| b != "none"));
    let hole_size = if ctype == "donut" { st.hole_size } else { None };
    let explode = if ctype == "pie" || ctype == "donut" { st.explode } else { None };
    ChartParsed {
        ctype, title, grouping, series: st.series, legend: st.legend, legend_pos: st.legend_pos,
        data_labels, colors: st.colors, hole_size, explode, filled,
        smooth, symbols, lines, axis_x, axis_y, grid_x, grid_y,
        fill: st.space_fill, border: space_border, border_width, font: st.space_font,
    }
}
