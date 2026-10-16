//! Printing a sheet (`web/src/sheetPrint.ts`, `PrintDialog.tsx`, `buildPrintGrid` `SA:3818-3931`).
//!
//! The web renders the print area as an HTML `<table>` (column widths, row heights, merges as spans, the
//! cell styles) scaled by [`print_scale`] and lets the browser paginate it. This module builds the same
//! grid ([`build_grid`]) and lays it out on pages ([`paginate`]) so a native printer can draw it: rows flow
//! from page to page with the repeated title rows (`<thead>` on the web) on each; a table wider than the
//! printable width is split into column bands (the browser would cut it off at the page's edge - the
//! desktop prints everything, Excel's way).

use std::collections::HashSet;

use crate::addr::{self, Rect};
use crate::editor::Editor;
use crate::grid::Geometry;
use crate::model::style::Style;

/// How the table is scaled (`PrintScaleMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleMode {
    Normal,
    FitWidth,
    FitPage,
    Custom,
}

/// The print options (`PrintOptions`, defaults `DEFAULT_PRINT_OPTIONS`).
#[derive(Clone, Debug, PartialEq)]
pub struct PrintOptions {
    pub landscape: bool,
    /// A [`PAGE_FORMATS`] id.
    pub paper: String,
    pub scale_mode: ScaleMode,
    /// Used with [`ScaleMode::Custom`], 10–400.
    pub scale_pct: f64,
    pub gridlines: bool,
    /// The A/B/C and 1/2/3 headers.
    pub headings: bool,
    /// The number of top rows repeated on every page (0–10).
    pub repeat_rows: usize,
    /// The uniform margin (0–40 mm).
    pub margin_mm: f64,
    pub center_h: bool,
    pub title: String,
}

impl Default for PrintOptions {
    fn default() -> Self {
        PrintOptions { landscape: false, paper: "a4".into(), scale_mode: ScaleMode::Normal, scale_pct: 100.0, gridlines: true, headings: false, repeat_rows: 0, margin_mm: 12.0, center_h: false, title: String::new() }
    }
}

/// A paper format (`PAGE_FORMATS` of `mathPages.ts`): id, name, width and height in mm (portrait).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaperFormat {
    pub id: &'static str,
    pub name: &'static str,
    pub w: f64,
    pub h: f64,
}

pub const PAGE_FORMATS: [PaperFormat; 6] = [
    PaperFormat { id: "a3", name: "A3", w: 297.0, h: 420.0 },
    PaperFormat { id: "a4", name: "A4", w: 210.0, h: 297.0 },
    PaperFormat { id: "a5", name: "A5", w: 148.0, h: 210.0 },
    PaperFormat { id: "b5", name: "B5", w: 176.0, h: 250.0 },
    PaperFormat { id: "letter", name: "Letter", w: 216.0, h: 279.0 },
    PaperFormat { id: "legal", name: "Legal", w: 216.0, h: 356.0 },
];

/// The web's pixels per millimetre (`paperPx(…, 3.78)`).
pub const PX_PER_MM: f64 = 3.78;
/// The row-header column and the column-header row of the printed table (`HEAD_W`, `HEAD_H`).
pub const HEAD_W: f64 = 42.0;
pub const HEAD_H: f64 = 22.0;

/// The format named `id` (A4 when unknown, as on the web).
pub fn paper(id: &str) -> PaperFormat {
    PAGE_FORMATS.iter().copied().find(|p| p.id == id).unwrap_or(PAGE_FORMATS[1])
}

/// `paperPx`: the sheet's size in px (rounded), honouring the orientation.
pub fn paper_px(id: &str, landscape: bool) -> (f64, f64) {
    let f = paper(id);
    let (w, h) = if landscape { (f.h, f.w) } else { (f.w, f.h) };
    ((w * PX_PER_MM).round(), (h * PX_PER_MM).round())
}

/// One printed cell (`PrintCell`): the text, the style (the cell's own `s` with the conditional format's
/// colours over it), numeric (right-aligned by default), the merge spans.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrintCell {
    pub text: String,
    pub style: Option<Style>,
    pub num: bool,
    pub cs: usize,
    pub rs: usize,
}

/// The print area (`PrintGrid`): visible columns and rows with their sizes in px at zoom 1, and the cells
/// (`None` where a merge covers the cell).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrintGrid {
    /// (letter, width).
    pub cols: Vec<(String, f64)>,
    /// (1-based row, height).
    pub rows: Vec<(u32, f64)>,
    pub cells: Vec<Vec<Option<PrintCell>>>,
    /// The sheet columns and rows of [`Self::cols`] and [`Self::rows`].
    pub col_index: Vec<u32>,
    pub row_index: Vec<u32>,
}

/// `buildPrintGrid(area)`: the selection when it is a range, else the used range; hidden columns (collapsed
/// groups) and hidden rows (filters, collapsed groups) skipped; at most 256 columns and 5 000 rows.
///
/// QUIRK (kept): the web prints `resolveValue` for every cell - the value WITHOUT its number format
/// (`SA:3851`, both branches of the ternary) - so a currency prints as its plain number.
pub fn build_grid(ed: &Editor, selection: bool) -> Option<PrintGrid> {
    let sheet = ed.sheet();
    let mut b: Option<Rect> = None;
    if selection && ed.selected {
        let r = ed.sel.rect();
        if r.c2 > r.c1 || r.r2 > r.r1 {
            b = Some(r);
        }
    }
    let b = b.unwrap_or_else(|| {
        let (mut max_c, mut max_r) = (0u32, 1u32);
        for k in sheet.cells.keys() {
            if let Some(p) = addr::Cell::parse(k) {
                max_c = max_c.max(p.col);
                max_r = max_r.max(p.row);
            }
        }
        Rect { c1: 0, r1: 1, c2: max_c, r2: max_r }
    });
    let (_, hidden_cols) = crate::grid::group_hidden(sheet);
    let hidden_rows = ed.hidden_rows();
    let geom = Geometry::new(sheet, 1.0, &HashSet::new(), &HashSet::new());
    let mut grid = PrintGrid::default();
    for c in b.c1..=b.c2 {
        if grid.cols.len() >= 256 {
            break;
        }
        if hidden_cols.contains(&c) {
            continue;
        }
        grid.cols.push((addr::index_to_col(c as i64), geom.col_width(c).round()));
        grid.col_index.push(c);
    }
    for r in b.r1..=b.r2 {
        if grid.rows.len() >= 5000 {
            break;
        }
        if hidden_rows.contains(&r) {
            continue;
        }
        grid.rows.push((r, geom.row_height(r).round()));
        grid.row_index.push(r);
    }
    if grid.cols.is_empty() || grid.rows.is_empty() {
        return None;
    }
    let merges = ed.merges();
    let pos_col = |c: u32| grid.col_index.iter().position(|&x| x == c);
    let pos_row = |r: u32| grid.row_index.iter().position(|&x| x == r);
    let (last_c, last_r) = (*grid.col_index.last().unwrap_or(&0), *grid.row_index.last().unwrap_or(&1));
    let mut cells = Vec::with_capacity(grid.rows.len());
    for (ri, &r) in grid.row_index.iter().enumerate() {
        let mut row = Vec::with_capacity(grid.cols.len());
        for (ci, &c) in grid.col_index.iter().enumerate() {
            let key = addr::key(c, r);
            if merges.covered.get(&key).is_some_and(|a| merges.anchors.contains_key(a)) {
                row.push(None);
                continue;
            }
            let cell = sheet.cells.get(&key);
            let shown = ed.shown(c, r);
            let mut style = cell.map(|x| x.style()).unwrap_or_default();
            let cf = ed.cond_style(&key);
            let has_style = cell.is_some_and(|x| x.s.is_some());
            if let Some(cf) = &cf {
                if let Some(bg) = &cf.bg {
                    style.bg = Some(bg.clone());
                }
                if let Some(col) = &cf.color {
                    style.color = Some(col.clone());
                }
                if let Some(v) = cf.bold {
                    style.bold = Some(v);
                }
                if let Some(v) = cf.italic {
                    style.italic = Some(v);
                }
            }
            let (mut cs, mut rs) = (1, 1);
            if let Some(anc) = merges.anchors.get(&key) {
                let c2 = anc.c2.min(last_c);
                let r2 = anc.r2.min(last_r);
                cs = (pos_col(c2).unwrap_or(ci) + 1).saturating_sub(ci).max(1);
                rs = (pos_row(r2).unwrap_or(ri) + 1).saturating_sub(ri).max(1);
            }
            let text = shown.raw_text.clone();
            if text.is_empty() && !has_style && cf.is_none() && cs == 1 && rs == 1 {
                row.push(Some(PrintCell { text: String::new(), ..Default::default() }));
                continue;
            }
            let styled = has_style || cf.is_some();
            row.push(Some(PrintCell { text, style: if styled { Some(style) } else { None }, num: shown.num.is_some(), cs, rs }));
        }
        cells.push(row);
    }
    let _ = (pos_col, pos_row);
    grid.cells = cells;
    Some(grid)
}

/// The table's width and height in px, the headers included (`tableWidth`, `tableHeight`).
pub fn table_size(grid: &PrintGrid, opt: &PrintOptions) -> (f64, f64) {
    let w = if opt.headings { HEAD_W } else { 0.0 } + grid.cols.iter().map(|c| c.1).sum::<f64>();
    let h = if opt.headings { HEAD_H } else { 0.0 } + grid.rows.iter().map(|r| r.1).sum::<f64>();
    (w, h)
}

/// `printScale`: the factor applied to the whole table.
pub fn print_scale(grid: &PrintGrid, opt: &PrintOptions) -> f64 {
    let (w, h) = paper_px(&opt.paper, opt.landscape);
    let mm = opt.margin_mm * PX_PER_MM;
    let (avail_w, avail_h) = (w - mm * 2.0, h - mm * 2.0);
    let (tw, th) = table_size(grid, opt);
    match opt.scale_mode {
        ScaleMode::Custom => (opt.scale_pct / 100.0).clamp(0.1, 4.0),
        ScaleMode::FitWidth => (avail_w / tw).min(1.0),
        ScaleMode::FitPage => (avail_w / tw).min(avail_h / th).min(1.0),
        ScaleMode::Normal => 1.0,
    }
}

/// One printed page: the grid's columns `cols` (a band) and rows `rows` (indexes into the grid), the title
/// rows repeated at its top, in px of the table (before the scale).
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub cols: std::ops::Range<usize>,
    pub repeat: std::ops::Range<usize>,
    pub rows: std::ops::Range<usize>,
}

/// Lays `grid` out on pages whose printable area is `avail_w` × `avail_h` px (the table's px, the scale
/// already divided out): the column bands first (left to right), each with every row band (top to
/// bottom), like the browser's flow of a tall table - and a band boundary never inside a merge's span.
pub fn paginate(grid: &PrintGrid, opt: &PrintOptions, avail_w: f64, avail_h: f64) -> Vec<Page> {
    let head_w = if opt.headings { HEAD_W } else { 0.0 };
    let head_h = if opt.headings { HEAD_H } else { 0.0 };
    let repeat = opt.repeat_rows.min(grid.rows.len());
    let repeat_h: f64 = grid.rows[..repeat].iter().map(|r| r.1).sum();
    // Columns: greedy bands; a merge spanning a band edge pulls the edge after it.
    let col_spans: Vec<usize> = (0..grid.cols.len())
        .map(|ci| grid.cells.iter().filter_map(|row| row.get(ci).and_then(|c| c.as_ref())).map(|c| ci + c.cs.max(1)).max().unwrap_or(ci + 1))
        .collect();
    let mut col_bands = Vec::new();
    let mut start = 0;
    while start < grid.cols.len() {
        let (mut end, mut w) = (start, head_w);
        let mut reach = start + 1;
        while end < grid.cols.len() {
            let cw = grid.cols[end].1;
            if end > start && w + cw > avail_w && end >= reach {
                break;
            }
            w += cw;
            reach = reach.max(col_spans[end]);
            end += 1;
        }
        let end = end.max(start + 1);
        col_bands.push(start..end);
        start = end;
    }
    let row_spans: Vec<usize> = (0..grid.rows.len())
        .map(|ri| grid.cells.get(ri).map(|row| row.iter().flatten().map(|c| ri + c.rs.max(1)).max().unwrap_or(ri + 1)).unwrap_or(ri + 1))
        .collect();
    let mut row_bands = Vec::new();
    let mut start = repeat;
    if start >= grid.rows.len() {
        row_bands.push(start..start);
    }
    while start < grid.rows.len() {
        let (mut end, mut h) = (start, head_h + repeat_h);
        let mut reach = start + 1;
        while end < grid.rows.len() {
            let rh = grid.rows[end].1;
            if end > start && h + rh > avail_h && end >= reach {
                break;
            }
            h += rh;
            reach = reach.max(row_spans[end]);
            end += 1;
        }
        let end = end.max(start + 1);
        row_bands.push(start..end);
        start = end;
    }
    let mut pages = Vec::new();
    for cols in &col_bands {
        for rows in &row_bands {
            pages.push(Page { cols: cols.clone(), repeat: 0..repeat, rows: rows.clone() });
        }
    }
    pages
}

/// The printable area of a page in table px: the paper less the margins, divided by the scale.
pub fn printable_area(opt: &PrintOptions, scale: f64) -> (f64, f64) {
    let (w, h) = paper_px(&opt.paper, opt.landscape);
    let mm = opt.margin_mm * PX_PER_MM;
    (((w - 2.0 * mm) / scale).max(1.0), ((h - 2.0 * mm) / scale).max(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::tests::editor;
    use serde_json::json;

    #[test]
    fn the_scale_follows_the_web() {
        let ed = editor(json!({"A1": {"v": 1}, "Z1": {"v": 2}}));
        let grid = build_grid(&ed, false).expect("a grid");
        assert_eq!(grid.cols.len(), 26);
        let mut opt = PrintOptions::default();
        assert_eq!(print_scale(&grid, &opt), 1.0);
        opt.scale_mode = ScaleMode::FitWidth;
        // A4 portrait: 794 px wide, 12 mm margins → 703.28 px for 26 × 100 px.
        let s = print_scale(&grid, &opt);
        assert!((s - (794.0 - 2.0 * 12.0 * 3.78) / 2600.0).abs() < 1e-9, "{s}");
        opt.scale_mode = ScaleMode::Custom;
        opt.scale_pct = 5.0;
        assert_eq!(print_scale(&grid, &opt), 0.1);
        assert_eq!(paper_px("letter", true), (1055.0, 816.0));
        assert_eq!(paper("nope").id, "a4");
    }

    #[test]
    fn values_print_unformatted_and_merges_span() {
        let mut ed = editor(json!({"A1": {"v": 1234.5, "s": {"numFmt": "currency"}}, "B1": {"v": "x"}, "A2": {"v": 2}}));
        ed.select_rect(addr::Cell::new(0, 1), addr::Cell::new(1, 1));
        ed.toggle_merge().expect("merge");
        let grid = build_grid(&ed, false).expect("a grid");
        let a1 = grid.cells[0][0].as_ref().expect("anchor");
        assert_eq!(a1.text, "1234.5");
        assert!(a1.num);
        assert_eq!((a1.cs, a1.rs), (2, 1));
        assert!(grid.cells[0][1].is_none(), "covered by the merge");
    }

    #[test]
    fn pages_repeat_the_title_rows_and_split_wide_tables() {
        let cells: serde_json::Map<String, serde_json::Value> = (1..=200).map(|r| (format!("A{r}"), json!({"v": r}))).chain(std::iter::once(("L1".to_string(), json!({"v": 0})))).collect();
        let ed = editor(serde_json::Value::Object(cells));
        let grid = build_grid(&ed, false).expect("a grid");
        let opt = PrintOptions { repeat_rows: 1, ..Default::default() };
        let (w, h) = printable_area(&opt, 1.0);
        let pages = paginate(&grid, &opt, w, h);
        // 12 columns × 100 px = 1200 px > 703 px: two column bands.
        let bands: HashSet<_> = pages.iter().map(|p| (p.cols.start, p.cols.end)).collect();
        assert_eq!(bands.len(), 2);
        assert!(pages.iter().all(|p| p.repeat == (0..1)));
        // Every row printed once per band, the first row only as the repeated title.
        let first_band: Vec<_> = pages.iter().filter(|p| p.cols.start == 0).collect();
        assert_eq!(first_band.first().map(|p| p.rows.start), Some(1));
        assert_eq!(first_band.last().map(|p| p.rows.end), Some(200));
        let per_page = ((h - 24.0) / 24.0).floor() as usize;
        assert_eq!(first_band[0].rows.len(), per_page);
    }
}
