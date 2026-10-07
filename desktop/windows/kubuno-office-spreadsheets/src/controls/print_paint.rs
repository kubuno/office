//! Draws a printed page of a sheet (`kubuno_office_sheets_core::print`) on a printer's (or the preview's)
//! `Graphics`, the way the web's print table looks (`web/src/sheetPrint.ts`, `tableHtml`/`cellStyleCss`):
//! the column and row headers in `#f1f3f4`/`#5f6368` 11 px semi-bold with `#c0c4c8` borders, the
//! gridlines in `#d0d0d0` (transparent without them), each edge's own border over it (1, 1.6 or 2.5 px for
//! the weights 1, 2, 3), the background, the text 13 px (or the cell's size) with a 1 × 4 px padding,
//! aligned right for numbers by default, at the bottom by default, wrapped or clipped to the cell.

use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Graphics, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::Rect;
use kubuno_office_sheets_core::model::style::{Align, Style, VAlign};
use kubuno_office_sheets_core::print::{Page, PrintGrid, PrintOptions, HEAD_H, HEAD_W};

const HEAD_BG: &str = "#f1f3f4";
const HEAD_FG: &str = "#5f6368";
const HEAD_LINE: &str = "#c0c4c8";
const GRID_LINE: &str = "#d0d0d0";
const TEXT: &str = "#202124";

fn color(c: &str) -> Color {
    Color::from_hex(c).unwrap_or(Color::BLACK)
}

/// Where a page's table starts and its scale, for a page whose printable area (margins removed) is
/// `area` (DIP).
pub fn table_origin(area: Rect, grid: &PrintGrid, page: &Page, opt: &PrintOptions, scale: f64) -> (f32, f32) {
    if !opt.center_h {
        return (area.left, area.top);
    }
    let w: f64 = if opt.headings { HEAD_W } else { 0.0 } + grid.cols[page.cols.clone()].iter().map(|c| c.1).sum::<f64>();
    let free = (area.right - area.left) as f64 - w * scale;
    (area.left + (free.max(0.0) / 2.0) as f32, area.top)
}

/// Paints `page` of `grid` with its top-left table corner at `origin`, scaled by `scale`.
pub fn paint_page(g: &Graphics<'_>, origin: (f32, f32), grid: &PrintGrid, page: &Page, opt: &PrintOptions, scale: f64) {
    g.with_saved(|g| {
        g.translate_transform(origin.0, origin.1);
        g.scale_transform(scale as f32, scale as f32);
        paint_table(g, grid, page, opt);
    });
}

fn paint_table(g: &Graphics<'_>, grid: &PrintGrid, page: &Page, opt: &PrintOptions) {
    let head_w = if opt.headings { HEAD_W } else { 0.0 };
    let head_h = if opt.headings { HEAD_H } else { 0.0 };
    // The columns of the band and their left edges.
    let cols: Vec<usize> = page.cols.clone().collect();
    let mut col_x = Vec::with_capacity(cols.len() + 1);
    let mut x = head_w;
    for &ci in &cols {
        col_x.push(x);
        x += grid.cols[ci].1;
    }
    col_x.push(x);
    let table_w = x;
    // The rows of the page: the repeated title rows, then the page's own.
    let rows: Vec<usize> = page.repeat.clone().chain(page.rows.clone()).collect();
    let mut row_y = Vec::with_capacity(rows.len() + 1);
    let mut y = head_h;
    for &ri in &rows {
        row_y.push(y);
        y += grid.rows[ri].1;
    }
    row_y.push(y);
    let fmt_center = StringFormat { alignment: StringAlignment::Center, line_alignment: StringAlignment::Center, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP };
    let head_font = Font::with_dip("Arial", 11.0, FontStyle::BOLD);
    let head_pen = Pen::new(color(HEAD_LINE), 1.0);
    if opt.headings {
        g.fill_rectangle(Brush::solid(color(HEAD_BG)), Rect::new(0.0, 0.0, table_w as f32, head_h as f32));
        g.fill_rectangle(Brush::solid(color(HEAD_BG)), Rect::new(0.0, head_h as f32, head_w as f32, y as f32));
        g.draw_rectangle(&head_pen, Rect::new(0.0, 0.0, head_w as f32, head_h as f32));
        for (i, &ci) in cols.iter().enumerate() {
            let r = Rect::new(col_x[i] as f32, 0.0, col_x[i + 1] as f32, head_h as f32);
            g.draw_rectangle(&head_pen, r);
            g.draw_string(&grid.cols[ci].0, &head_font, Brush::solid(color(HEAD_FG)), r, &fmt_center);
        }
        for (i, &ri) in rows.iter().enumerate() {
            let r = Rect::new(0.0, row_y[i] as f32, head_w as f32, row_y[i + 1] as f32);
            g.draw_rectangle(&head_pen, r);
            g.draw_string(&grid.rows[ri].0.to_string(), &head_font, Brush::solid(color(HEAD_FG)), r, &fmt_center);
        }
    }
    // Backgrounds and text first, borders over them (the collapsed borders of the table).
    let mut edges: Vec<(f32, f32, f32, f32, Color, f32)> = Vec::new();
    for (i, &ri) in rows.iter().enumerate() {
        let Some(line) = grid.cells.get(ri) else { continue };
        for (j, &ci) in cols.iter().enumerate() {
            let Some(Some(cell)) = line.get(ci) else { continue };
            // The span, clamped to the band and to the page's run of rows.
            let j2 = (j + cell.cs.max(1)).min(cols.len());
            let mut i2 = i + 1;
            while i2 < rows.len() && i2 - i < cell.rs.max(1) && rows[i2] == rows[i2 - 1] + 1 {
                i2 += 1;
            }
            let (x1, x2, y1, y2) = (col_x[j], col_x[j2], row_y[i], row_y[i2]);
            let rect = Rect::new(x1 as f32, y1 as f32, x2 as f32, y2 as f32);
            let st = cell.style.clone().unwrap_or_default();
            if let Some(bg) = st.bg.as_deref() {
                g.fill_rectangle(Brush::solid(color(bg)), rect);
            }
            let grid_edge = if opt.gridlines { Some(color(GRID_LINE)) } else { None };
            let edge = |c: &Option<String>, w: Option<f64>| -> Option<(Color, f32)> {
                match c {
                    Some(c) => Some((color(c), match w.map(|w| w as i64) {
                        Some(3) => 2.5,
                        Some(2) => 1.6,
                        _ => 1.0,
                    })),
                    None => grid_edge.map(|c| (c, 1.0)),
                }
            };
            let (l, t, r, b) = (x1 as f32, y1 as f32, x2 as f32, y2 as f32);
            if let Some((c, w)) = edge(&st.bt, st.btw) {
                edges.push((l, t, r, t, c, w));
            }
            if let Some((c, w)) = edge(&st.bb, st.bbw) {
                edges.push((l, b, r, b, c, w));
            }
            if let Some((c, w)) = edge(&st.bl, st.blw) {
                edges.push((l, t, l, b, c, w));
            }
            if let Some((c, w)) = edge(&st.br, st.brw) {
                edges.push((r, t, r, b, c, w));
            }
            if !cell.text.is_empty() {
                paint_text(g, &cell.text, &st, cell.num, rect);
            }
        }
    }
    for (x1, y1, x2, y2, c, w) in edges {
        g.draw_line(&Pen::new(c, w), PointF::new(x1, y1), PointF::new(x2, y2));
    }
}

/// The cell's text in its box (padding 1 × 4 px, `line-height: 1.25`).
fn paint_text(g: &Graphics<'_>, text: &str, st: &Style, num: bool, rect: Rect) {
    let size = st.font_size.unwrap_or(13.0) as f32;
    let mut fs = FontStyle::REGULAR.0;
    if st.is_bold() {
        fs |= FontStyle::BOLD.0;
    }
    if st.is_italic() {
        fs |= FontStyle::ITALIC.0;
    }
    if st.is_underline() {
        fs |= FontStyle::UNDERLINE.0;
    }
    if st.is_strike() {
        fs |= FontStyle::STRIKEOUT.0;
    }
    let family = st.font_family.as_deref().filter(|f| !f.is_empty()).unwrap_or("Arial");
    let font = Font::with_dip(family, size, FontStyle(fs));
    let align = match st.align {
        Some(Align::Center) => StringAlignment::Center,
        Some(Align::Right) => StringAlignment::Far,
        Some(Align::Left) => StringAlignment::Near,
        None if num => StringAlignment::Far,
        None => StringAlignment::Near,
    };
    let valign = match st.valign {
        Some(VAlign::Top) => StringAlignment::Near,
        Some(VAlign::Center) => StringAlignment::Center,
        _ => StringAlignment::Far,
    };
    let wrap = st.is_wrap();
    let flags = if wrap { StringFormatFlags::default() } else { StringFormatFlags::NO_WRAP };
    let fmt = StringFormat { alignment: align, line_alignment: valign, trimming: StringTrimming::None, flags };
    let inner = Rect::new(rect.left + 4.0, rect.top + 1.0, (rect.right - 4.0).max(rect.left + 4.0), (rect.bottom - 1.0).max(rect.top + 1.0));
    let ink = color(st.color.as_deref().unwrap_or(TEXT));
    g.with_saved(|g| {
        g.intersect_clip(rect);
        g.draw_string(text, &font, Brush::solid(ink), inner, &fmt);
    });
}
