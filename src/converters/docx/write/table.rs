//! `<w:tbl>` — table serialisation.
//!
//! The ProseMirror model stores a table as rows of *real* cells only: vertical
//! merge continuations are absorbed into the `rowspan` of the cell above and
//! never appear as nodes (some rows also carry `merged` placeholders). OOXML is
//! the opposite: every grid slot of every row MUST exist as a `<w:tc>`, the
//! continuations carrying `<w:vMerge w:val="continue"/>`
//! (docxattributeoutput.cxx:4992-5001). So the first thing we do is rebuild the
//! occupancy grid, exactly like the canvas engine's `layoutTable` does.
//!
//! Two schema constraints drive the rest of the file:
//!  * `w:tblPr` and `w:tcPr` are strict sequences — LibreOffice goes as far as
//!    buffering then reordering the children (docxtableexport.cxx:163-182
//!    `aOrder`). Word refuses to open a file whose properties are out of order.
//!  * a cell must end with a `<w:p>`, and a nested table must be followed by one
//!    (docxattributeoutput.cxx:864-872, "MS Word insists on that").

use serde_json::{json, Value};

use crate::converters::types::PmNode;

use super::ctx::ExportCtx;

// ─── Units ───────────────────────────────────────────────────────────────────
// 1440 twips = 1 inch = 96 CSS px  →  1 px = 15 twips.

/// px → twips.
fn px_tw(px: f64) -> i64 {
    (px * 15.0).round() as i64
}

/// A1 content width of the default A4 layout (pgSz 11906 − pgMar 1440 × 2), in
/// twips. Used only when the table carries no explicit column widths.
const CONTENT_TW: f64 = 9026.0;

/// Smallest column width we will write; a degenerate `gridCol` makes Word
/// collapse the whole grid.
const MIN_COL_TW: i64 = 120;

/// Sanitise a `#rrggbb` (or `rrggbb`) colour into the bare uppercase hex form
/// OOXML expects. Anything else — `rgb()`, a named colour, garbage — is
/// rejected rather than written out and rejected by Word.
fn hex6(s: &str) -> Option<String> {
    let t = s.trim().trim_start_matches('#');
    (t.len() == 6 && t.chars().all(|c| c.is_ascii_hexdigit())).then(|| t.to_uppercase())
}

/// A cell as placed on the reconstructed occupancy grid.
struct ExpCell<'a> {
    node: &'a PmNode,
    /// Row index.
    r: usize,
    /// First grid column covered.
    c0: usize,
    cspan: usize,
    rspan: usize,
}

/// Rebuild the occupancy grid the way the canvas engine does (`layoutTable`):
/// `merged` placeholders are skipped and a `rowspan` reserves its columns for
/// the following rows. Returns the placed cells and the column count.
fn build_export_grid(rows: &[PmNode]) -> (Vec<ExpCell<'_>>, usize) {
    let mut placed: Vec<ExpCell> = Vec::new();
    // occupied[c] = number of rows still covered by a rowspan on column c.
    let mut occupied: Vec<usize> = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        let mut c = 0usize;
        for cell in row.children() {
            let a = cell.attrs.as_ref();
            if a.and_then(|v| v.get("merged")).and_then(|v| v.as_bool()).unwrap_or(false) {
                continue;
            }
            let num = |k: &str| {
                a.and_then(|v| v.get(k)).and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize
            };
            while occupied.get(c).copied().unwrap_or(0) > 0 {
                c += 1;
            }
            let cspan = num("colspan");
            let rspan = num("rowspan");
            if occupied.len() < c + cspan {
                occupied.resize(c + cspan, 0);
            }
            for slot in occupied.iter_mut().skip(c).take(cspan) {
                *slot = rspan;
            }
            placed.push(ExpCell { node: cell, r, c0: c, cspan, rspan });
            c += cspan;
        }
        for slot in occupied.iter_mut() {
            *slot = slot.saturating_sub(1);
        }
    }
    let ncols = placed.iter().map(|p| p.c0 + p.cspan).max().unwrap_or(1).max(1);
    (placed, ncols)
}

/// A `{ w, s, c }` border spec (px / dash style / hex colour) →
/// `<w:top w:val="single" w:sz=".." w:color=".."/>`.
///
/// UNIT TRAP: `w:sz` is in **eighths of a point**, not twips and not px.
/// LibreOffice reads it as `sz * 5 / 2` twips (BorderHandler.cxx:58-61); the
/// inverse is `sz = px * 15 * 2 / 5 = px * 6`.
///
/// `None` means the border is explicitly absent, which is NOT the same as
/// omitting the element (that would inherit the table default), hence `nil`.
fn border_el(tag: &str, spec: Option<&Value>) -> String {
    let Some(spec) = spec else {
        return format!(r#"<w:{tag} w:val="nil" w:sz="0" w:space="0" w:color="auto"/>"#);
    };
    let w = spec.get("w").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let sz = ((w * 6.0).round() as i64).clamp(2, 96);
    let val = match spec.get("s").and_then(|v| v.as_str()).unwrap_or("solid") {
        "dashed" => "dashed",
        "dotted" => "dotted",
        _ => "single",
    };
    let color = spec
        .get("c")
        .and_then(|v| v.as_str())
        .and_then(hex6)
        .unwrap_or_else(|| "000000".into());
    format!(r#"<w:{tag} w:val="{val}" w:sz="{sz}" w:space="0" w:color="{color}"/>"#)
}

/// `<w:tblCellMar>` / `<w:tcMar>` body: the four sides, in schema order.
fn cell_mar_body(top: f64, left: f64, bottom: f64, right: f64) -> String {
    format!(
        concat!(
            r#"<w:top w:w="{}" w:type="dxa"/><w:left w:w="{}" w:type="dxa"/>"#,
            r#"<w:bottom w:w="{}" w:type="dxa"/><w:right w:w="{}" w:type="dxa"/>"#
        ),
        px_tw(top).max(0),
        px_tw(left).max(0),
        px_tw(bottom).max(0),
        px_tw(right).max(0)
    )
}

/// A `table` node -> `<w:tbl>`. Returns an empty string for anything we cannot
/// serialise: a table must never be emitted half-formed, Word would repair it.
pub(crate) fn render_table(node: &PmNode, ctx: &mut ExportCtx) -> String {
    let rows = node.children();
    if rows.is_empty() {
        return String::new();
    }
    let (placed, ncols) = build_export_grid(rows);
    if placed.is_empty() {
        return String::new();
    }

    let a = node.attrs.clone().unwrap_or_else(|| json!({}));
    let get_f = |k: &str| a.get(k).and_then(|v| v.as_f64());
    let get_s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(str::to_string);
    let get_b = |k: &str| a.get(k).and_then(|v| v.as_bool()).unwrap_or(false);

    // Column widths, in twips. Missing or inconsistent → even split of the
    // content width, which is what Word does for an auto-fitted table.
    let col_tw: Vec<i64> = match a.get("colWidths").and_then(|v| v.as_array()) {
        Some(ws) if ws.len() == ncols && ws.iter().all(|v| v.as_f64().unwrap_or(0.0) > 0.0) => ws
            .iter()
            .map(|v| px_tw(v.as_f64().unwrap_or(0.0)).max(MIN_COL_TW))
            .collect(),
        _ => {
            let each = (CONTENT_TW / ncols as f64).round() as i64;
            vec![each.max(MIN_COL_TW); ncols]
        }
    };
    let total_tw: i64 = col_tw.iter().sum();

    // ── w:tblPr — STRICT schema sequence:
    //    tblStyle, tblpPr, tblOverlap, bidiVisual, tblStyleRowBandSize,
    //    tblStyleColBandSize, tblW, jc, tblCellSpacing, tblInd, tblBorders,
    //    shd, tblLayout, tblCellMar, tblLook, tblPrChange.
    let mut pr = String::new();

    // tblpPr — floating table (TablePositionHandler.cxx).
    if get_s("tableWrap").as_deref() == Some("around") {
        let d = |k: &str, dflt: f64| px_tw(get_f(k).unwrap_or(dflt)).max(0);
        pr.push_str(&format!(
            concat!(
                r#"<w:tblpPr w:leftFromText="{}" w:rightFromText="{}" w:topFromText="{}""#,
                r#" w:bottomFromText="{}" w:vertAnchor="text" w:horzAnchor="text""#,
                r#" w:tblpX="{}" w:tblpY="1"/>"#
            ),
            d("wrapDistLeft", 12.0),
            d("wrapDistRight", 12.0),
            d("wrapDistTop", 4.0),
            d("wrapDistBottom", 8.0),
            px_tw(get_f("tableIndent").unwrap_or(0.0)).max(0)
        ));
    }

    // tblW
    pr.push_str(&format!(r#"<w:tblW w:w="{total_tw}" w:type="dxa"/>"#));

    // jc (ConversionHelper.cxx:473-492).
    match get_s("tableAlign").as_deref() {
        Some("center") => pr.push_str(r#"<w:jc w:val="center"/>"#),
        Some("right") => pr.push_str(r#"<w:jc w:val="right"/>"#),
        _ => {}
    }

    // tblCellSpacing. ECMA-376 §17.4.44: the value applies on BOTH sides of an
    // edge, so the visible gap is twice `w`. LibreOffice drops this element
    // entirely (DomainMapperTableManager.cxx:412-417, "To-Do: Not yet
    // preserved") — our engine renders it, so we can do better on round-trip.
    if let Some(sp) = get_f("cellSpacing").filter(|v| *v > 0.0) {
        pr.push_str(&format!(r#"<w:tblCellSpacing w:w="{}" w:type="dxa"/>"#, px_tw(sp / 2.0)));
    }

    // tblInd — written even at 0 (docxtableexport.cxx:487-489).
    let indent = get_f("tableIndent").filter(|v| *v > 0.0).unwrap_or(0.0);
    pr.push_str(&format!(r#"<w:tblInd w:w="{}" w:type="dxa"/>"#, px_tw(indent).max(0)));

    // tblBorders — the table-level default, applied to the six sides. A 'plain'
    // table has none, and the per-cell overrides below still win.
    let plain = get_s("tableStyle").as_deref() == Some("plain");
    let default_border = (!plain).then(|| {
        json!({
            "w": get_f("tableBorderWidth").filter(|v| *v > 0.0).unwrap_or(1.0),
            "s": get_s("tableBorderStyle").unwrap_or_else(|| "solid".into()),
            "c": get_s("tableBorderColor").unwrap_or_else(|| "#bdc1c6".into()),
        })
    });
    pr.push_str("<w:tblBorders>");
    for tag in ["top", "left", "bottom", "right", "insideH", "insideV"] {
        pr.push_str(&border_el(tag, default_border.as_ref()));
    }
    pr.push_str("</w:tblBorders>");

    // tblLayout — LibreOffice always writes "fixed" (docxtableexport.cxx:243-244)
    // for want of an internal autofit; our engine implements both, so we honour
    // the real value.
    let layout = if get_s("tableLayout").as_deref() == Some("fixed") { "fixed" } else { "autofit" };
    pr.push_str(&format!(r#"<w:tblLayout w:type="{layout}"/>"#));

    // tblCellMar — defaults mirror the frontend's (2/6/2/6 px).
    pr.push_str("<w:tblCellMar>");
    pr.push_str(&cell_mar_body(
        get_f("cellMarginTop").unwrap_or(2.0),
        get_f("cellMarginLeft").unwrap_or(6.0),
        get_f("cellMarginBottom").unwrap_or(2.0),
        get_f("cellMarginRight").unwrap_or(6.0),
    ));
    pr.push_str("</w:tblCellMar>");

    // tblLook — MS errata default 0x4A0 (DomainMapperTableManager.cxx:96).
    pr.push_str(
        r#"<w:tblLook w:val="04A0" w:firstRow="1" w:lastRow="0" w:firstColumn="1" w:lastColumn="0" w:noHBand="0" w:noVBand="1"/>"#,
    );

    // ── w:tblGrid, immediately after w:tblPr (docxtableexport.cxx:496-508).
    let mut grid = String::from("<w:tblGrid>");
    for w in &col_tw {
        grid.push_str(&format!(r#"<w:gridCol w:w="{w}"/>"#));
    }
    grid.push_str("</w:tblGrid>");

    // ── Rows.
    let header_rows = a
        .get("headerRows")
        .and_then(|v| v.as_u64())
        .unwrap_or(if get_b("headerRepeat") { 1 } else { 0 }) as usize;
    let row_h: Vec<f64> = a
        .get("rowHeights")
        .and_then(|v| v.as_array())
        .map(|xs| xs.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect())
        .unwrap_or_default();
    let row_mode: Vec<String> = a
        .get("rowHeightModes")
        .and_then(|v| v.as_array())
        .map(|xs| xs.iter().map(|v| v.as_str().unwrap_or("atleast").to_string()).collect())
        .unwrap_or_default();
    let row_cant_split: Vec<bool> = a
        .get("rowCantSplit")
        .and_then(|v| v.as_array())
        .map(|xs| xs.iter().map(|v| v.as_bool().unwrap_or(false)).collect())
        .unwrap_or_default();

    let mut body = String::new();
    for r in 0..rows.len() {
        // w:trPr is a repeatable choice, order is free; we follow the order
        // LibreOffice emits (tblHeader, trHeight, cantSplit).
        let mut trpr = String::new();
        if r < header_rows {
            trpr.push_str(r#"<w:tblHeader w:val="true"/>"#);
        }
        if let Some(h) = row_h.get(r).copied().filter(|v| *v > 0.0) {
            // hRule="exact" = fixed height, "atLeast" = minimum
            // (MeasureHandler.cxx:71-77, TablePropertiesHandler.cxx:73-89).
            let rule = if row_mode.get(r).map(String::as_str) == Some("exactly") {
                "exact"
            } else {
                "atLeast"
            };
            trpr.push_str(&format!(r#"<w:trHeight w:val="{}" w:hRule="{rule}"/>"#, px_tw(h)));
        }
        if row_cant_split.get(r).copied().unwrap_or(false) {
            trpr.push_str(r#"<w:cantSplit w:val="true"/>"#);
        }
        body.push_str("<w:tr>");
        if !trpr.is_empty() {
            body.push_str(&format!("<w:trPr>{trpr}</w:trPr>"));
        }

        // Every grid slot must be materialised as a <w:tc>.
        let mut c = 0usize;
        while c < ncols {
            if let Some(p) = placed.iter().find(|p| p.r == r && p.c0 == c) {
                let hi = (p.c0 + p.cspan).min(ncols).max(c + 1);
                let vmerge = (p.rspan > 1).then_some("restart");
                body.push_str(&render_table_cell(p.node, &col_tw[c..hi], hi - c, vmerge, ctx));
                c = hi;
            } else if let Some(p) = placed
                .iter()
                .find(|p| p.r < r && r < p.r + p.rspan && p.c0 <= c && c < p.c0 + p.cspan)
            {
                // Vertical-merge continuation: absent from the PM model, but it
                // MUST exist in the XML (docxattributeoutput.cxx:4992-5001).
                let hi = (p.c0 + p.cspan).min(ncols).max(c + 1);
                let empty = PmNode::paragraph(vec![]);
                let cell = PmNode {
                    node_type: "tableCell".into(),
                    attrs: None,
                    content: Some(vec![empty]),
                    marks: None,
                    text: None,
                };
                body.push_str(&render_table_cell(&cell, &col_tw[c..hi], hi - c, Some("continue"), ctx));
                c = hi;
            } else {
                // Hole in the grid (w:gridBefore/gridAfter equivalent): a row
                // shorter than the grid still needs its slots filled.
                let cell = PmNode {
                    node_type: "tableCell".into(),
                    attrs: None,
                    content: Some(vec![PmNode::paragraph(vec![])]),
                    marks: None,
                    text: None,
                };
                body.push_str(&render_table_cell(&cell, &col_tw[c..c + 1], 1, None, ctx));
                c += 1;
            }
        }
        body.push_str("</w:tr>");
    }

    format!("<w:tbl><w:tblPr>{pr}</w:tblPr>{grid}{body}</w:tbl>")
}

/// `<w:tc>`. `w:tcPr` children follow the CT_TcPr sequence, in the same order
/// LibreOffice emits them (docxattributeoutput.cxx:4969-5018):
/// tcW, gridSpan, vMerge, tcBorders, shd, tcMar, textDirection, vAlign.
fn render_table_cell(
    cell: &PmNode,
    col_tw: &[i64],
    cspan: usize,
    vmerge: Option<&str>,
    ctx: &mut ExportCtx,
) -> String {
    let a = cell.attrs.clone().unwrap_or_else(|| json!({}));
    let mut pr = String::new();

    // tcW — the sum of the grid columns this cell covers.
    pr.push_str(&format!(r#"<w:tcW w:w="{}" w:type="dxa"/>"#, col_tw.iter().sum::<i64>()));

    // gridSpan — horizontal merge.
    if cspan > 1 {
        pr.push_str(&format!(r#"<w:gridSpan w:val="{cspan}"/>"#));
    }

    // vMerge — "restart" on the head cell, "continue" on every row it covers.
    if let Some(v) = vmerge {
        pr.push_str(&format!(r#"<w:vMerge w:val="{v}"/>"#));
    }

    // tcBorders — per-side overrides. A key mapped to `null` means "explicitly
    // no border" and must be written as `nil`; an absent key inherits the table
    // default, so it is left out (lcl_mergeBorder's non-overwriting Insert,
    // DomainMapperTableHandler.cxx:126-201).
    if let Some(b) = a.get("cellBorders").filter(|v| v.is_object()) {
        let mut sides = String::new();
        for (tag, key) in [("top", "t"), ("left", "l"), ("bottom", "b"), ("right", "r")] {
            match b.get(key) {
                Some(Value::Null) => sides.push_str(&border_el(tag, None)),
                Some(spec) if spec.is_object() => sides.push_str(&border_el(tag, Some(spec))),
                _ => {}
            }
        }
        if !sides.is_empty() {
            pr.push_str(&format!("<w:tcBorders>{sides}</w:tcBorders>"));
        }
    }

    // shd — cell background.
    if let Some(bg) = a.get("cellBg").and_then(|v| v.as_str()).and_then(hex6) {
        pr.push_str(&format!(r#"<w:shd w:val="clear" w:color="auto" w:fill="{bg}"/>"#));
    }

    // tcMar — per-cell margin override. The model only carries table-level
    // margins today, so this is written solely when a cell actually overrides
    // them (all four sides required, as OOXML has no partial inheritance here).
    let cm = |k: &str| a.get(k).and_then(|v| v.as_f64());
    if let (Some(t), Some(l), Some(bm), Some(rm)) = (
        cm("cellMarginTop"),
        cm("cellMarginLeft"),
        cm("cellMarginBottom"),
        cm("cellMarginRight"),
    ) {
        pr.push_str(&format!("<w:tcMar>{}</w:tcMar>", cell_mar_body(t, l, bm, rm)));
    }

    // textDirection — rotated cell text.
    match a.get("cellDir").and_then(|v| v.as_i64()).unwrap_or(0) {
        90 => pr.push_str(r#"<w:textDirection w:val="tbRl"/>"#),
        270 => pr.push_str(r#"<w:textDirection w:val="btLr"/>"#),
        _ => {}
    }

    // vAlign — "top" is the default and is left implicit.
    match a.get("cellVAlign").and_then(|v| v.as_str()) {
        Some("center") => pr.push_str(r#"<w:vAlign w:val="center"/>"#),
        Some("bottom") => pr.push_str(r#"<w:vAlign w:val="bottom"/>"#),
        _ => {}
    }

    let mut inner = String::new();
    let mut last_was_table = false;
    for ch in cell.children() {
        inner.push_str(&super::document::render_block(ch, ctx));
        last_was_table = ch.node_type == "table";
    }
    // A cell must end with a w:p, and a nested table must be followed by one —
    // Word repairs the file otherwise (docxattributeoutput.cxx:864-872).
    if inner.is_empty() || last_was_table {
        inner.push_str("<w:p/>");
    }
    format!("<w:tc><w:tcPr>{pr}</w:tcPr>{inner}</w:tc>")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `attrs` is the table node's attribute object; each cell is
    /// `(colspan, rowspan, attrs)`.
    fn table(attrs: Value, rows: &[&[(u64, u64, Value)]]) -> PmNode {
        let rows: Vec<PmNode> = rows
            .iter()
            .map(|cells| {
                let cells: Vec<PmNode> = cells
                    .iter()
                    .map(|(cs, rs, extra)| {
                        let mut a = json!({ "colspan": cs, "rowspan": rs });
                        if let (Some(dst), Some(src)) = (a.as_object_mut(), extra.as_object()) {
                            for (k, v) in src {
                                dst.insert(k.clone(), v.clone());
                            }
                        }
                        PmNode {
                            node_type: "tableCell".into(),
                            attrs: Some(a),
                            content: Some(vec![PmNode::paragraph(vec![])]),
                            marks: None,
                            text: None,
                        }
                    })
                    .collect();
                PmNode {
                    node_type: "tableRow".into(),
                    attrs: None,
                    content: Some(cells),
                    marks: None,
                    text: None,
                }
            })
            .collect();
        PmNode {
            node_type: "table".into(),
            attrs: Some(attrs),
            content: Some(rows),
            marks: None,
            text: None,
        }
    }

    fn render(node: &PmNode) -> String {
        render_table(node, &mut ExportCtx::new())
    }

    /// Index of a substring, panicking with a readable message instead of
    /// unwrapping blindly.
    fn at(hay: &str, needle: &str) -> usize {
        match hay.find(needle) {
            Some(i) => i,
            None => panic!("missing {needle} in {hay}"),
        }
    }

    #[test]
    fn a_plain_table_round_trips_grid_and_cells() {
        let x = render(&table(json!({}), &[&[(1, 1, json!({})), (1, 1, json!({}))]]));
        assert!(x.starts_with("<w:tbl><w:tblPr>"));
        assert_eq!(x.matches("<w:gridCol").count(), 2);
        assert_eq!(x.matches("<w:tc>").count(), 2);
        // Every cell ends with a paragraph, otherwise Word repairs the file.
        assert!(x
            .split("</w:tc>")
            .take(2)
            .all(|seg| seg.ends_with("</w:p>") || seg.ends_with("<w:p/>")));
        // No explicit widths: the content width is split evenly (9026 / 2).
        assert!(x.contains(r#"<w:gridCol w:w="4513"/>"#));
        assert!(x.contains(r#"<w:tblW w:w="9026" w:type="dxa"/>"#));
    }

    #[test]
    fn tbl_pr_children_follow_the_normative_order() {
        let x = render(&table(
            json!({
                "tableWrap": "around", "tableAlign": "center", "cellSpacing": 4.0,
                "tableIndent": 20.0, "tableLayout": "fixed",
            }),
            &[&[(1, 1, json!({}))]],
        ));
        let order = [
            "<w:tblpPr", "<w:tblW", "<w:jc", "<w:tblCellSpacing", "<w:tblInd", "<w:tblBorders",
            "<w:tblLayout", "<w:tblCellMar", "<w:tblLook", "</w:tblPr><w:tblGrid",
        ];
        let mut prev = 0;
        for tag in order {
            let i = at(&x, tag);
            assert!(i > prev, "{tag} out of order in {x}");
            prev = i;
        }
        assert!(x.contains(r#"<w:tblLayout w:type="fixed"/>"#));
    }

    #[test]
    fn tc_pr_children_follow_the_normative_order() {
        let x = render(&table(
            json!({}),
            &[&[(
                2,
                1,
                json!({
                    "cellBg": "#FF0000", "cellVAlign": "bottom", "cellDir": 90,
                    "cellBorders": { "t": { "w": 1.0, "s": "solid", "c": "#123456" }, "b": null },
                    "cellMarginTop": 1.0, "cellMarginLeft": 2.0,
                    "cellMarginBottom": 3.0, "cellMarginRight": 4.0,
                }),
            )]],
        ));
        let order = [
            "<w:tcW", "<w:gridSpan", "<w:tcBorders", "<w:shd", "<w:tcMar", "<w:textDirection",
            "<w:vAlign", "</w:tcPr>",
        ];
        let mut prev = 0;
        for tag in order {
            let i = at(&x, tag);
            assert!(i > prev, "{tag} out of order in {x}");
            prev = i;
        }
        // An explicit `null` side means "no border", not "inherit".
        assert!(x.contains(r#"<w:bottom w:val="nil""#));
        assert!(x.contains(r#"<w:shd w:val="clear" w:color="auto" w:fill="FF0000"/>"#));
        // Absent sides are omitted so the table default applies.
        assert!(!x.contains("<w:left w:val=\"nil\"/></w:tcBorders>"));
    }

    #[test]
    fn border_width_is_in_eighths_of_a_point() {
        // 1 px = 15 twips = 6 eighths of a point (BorderHandler.cxx:58-61).
        let x = render(&table(
            json!({ "tableBorderWidth": 2.0, "tableBorderColor": "#00ff00", "tableBorderStyle": "dashed" }),
            &[&[(1, 1, json!({}))]],
        ));
        assert!(x.contains(r#"<w:top w:val="dashed" w:sz="12" w:space="0" w:color="00FF00"/>"#));
    }

    #[test]
    fn a_plain_table_writes_nil_borders() {
        let x = render(&table(json!({ "tableStyle": "plain" }), &[&[(1, 1, json!({}))]]));
        assert_eq!(x.matches(r#"w:val="nil""#).count(), 6);
    }

    #[test]
    fn vertical_merges_materialise_their_continuations() {
        // 2x2 grid whose left cell spans both rows: the model omits the
        // continuation, OOXML requires it.
        let t = table(
            json!({ "colWidths": [100.0, 200.0] }),
            &[&[(1, 2, json!({})), (1, 1, json!({}))], &[(1, 1, json!({}))]],
        );
        let x = render(&t);
        assert!(x.contains(r#"<w:vMerge w:val="restart"/>"#));
        assert!(x.contains(r#"<w:vMerge w:val="continue"/>"#));
        // Both rows carry two cells once the grid is rebuilt.
        assert_eq!(x.matches("<w:tc>").count(), 4);
        // The continuation sits in the FIRST column, the real cell in the second.
        let cont = at(&x, r#"<w:vMerge w:val="continue"/>"#);
        let row2 = at(&x, "</w:tr><w:tr>");
        assert!(cont > row2);
        assert!(x.contains(r#"<w:gridCol w:w="1500"/>"#));
        assert!(x.contains(r#"<w:gridCol w:w="3000"/>"#));
    }

    #[test]
    fn horizontal_merges_sum_the_grid_columns() {
        let x = render(&table(
            json!({ "colWidths": [100.0, 100.0, 100.0] }),
            &[&[(3, 1, json!({}))], &[(1, 1, json!({})), (2, 1, json!({}))]],
        ));
        assert!(x.contains(r#"<w:gridSpan w:val="3"/>"#));
        assert!(x.contains(r#"<w:gridSpan w:val="2"/>"#));
        assert!(x.contains(r#"<w:tcW w:w="4500" w:type="dxa"/>"#));
        assert!(x.contains(r#"<w:tcW w:w="3000" w:type="dxa"/>"#));
    }

    #[test]
    fn rows_carry_header_repeat_and_heights() {
        let x = render(&table(
            json!({
                "headerRows": 1, "rowHeights": [40.0, 0.0],
                "rowHeightModes": ["exactly", "atleast"], "rowCantSplit": [false, true],
            }),
            &[&[(1, 1, json!({}))], &[(1, 1, json!({}))]],
        ));
        assert_eq!(x.matches(r#"<w:tblHeader w:val="true"/>"#).count(), 1);
        assert!(x.contains(r#"<w:trHeight w:val="600" w:hRule="exact"/>"#));
        assert!(x.contains(r#"<w:cantSplit w:val="true"/>"#));
    }

    #[test]
    fn a_short_row_still_fills_the_grid() {
        let x = render(&table(
            json!({}),
            &[&[(1, 1, json!({})), (1, 1, json!({}))], &[(1, 1, json!({}))]],
        ));
        assert_eq!(x.matches("<w:tc>").count(), 4);
    }

    #[test]
    fn an_empty_table_is_dropped_rather_than_half_formed() {
        let empty = PmNode {
            node_type: "table".into(),
            attrs: None,
            content: Some(vec![]),
            marks: None,
            text: None,
        };
        assert!(render(&empty).is_empty());
    }
}
