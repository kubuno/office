//! `<w:tbl>` - tables: grid, rows, cells, and the `w:tblPr` / `w:trPr` / `w:tcPr`
//! geometry Word stores alongside them (borders, margins, spacing, layout,
//! repeated header rows, vertical merges).

use std::collections::HashMap;

use roxmltree::Node;
use serde_json::{json, Map, Value};

use crate::converters::docx::model::{Block, FontCtx};
use crate::converters::docx::xml::{attr_val, bool_on, child, local};
use crate::converters::types::PmNode;
use super::paragraph::parse_paragraph;
use super::revisions;
use super::fields::FieldState;
use super::notes::NoteTexts;

// ─── Units ───────────────────────────────────────────────────────────────────

/// 1440 twips = 1 inch = 96 CSS px, so 1 px = 15 twips. Rounded to 0.1 px:
/// the layout engine works in fractional pixels, no need for more precision.
fn tw_px(tw: f64) -> f64 {
    (tw / 15.0 * 10.0).round() / 10.0
}

/// Reads a twip-valued attribute. Word stores these in a signed 16-bit field and
/// silently ignores anything that overflows it, so a value at or above 0x8000 is
/// treated as 0 rather than as a gigantic measure (LibreOffice does the same in
/// `ConversionHelper::convertTwipToMm100_Limited`).
fn twips_attr(n: &Node<'_, '_>, name: &str) -> Option<f64> {
    let v = attr_val(n, name)?.trim().parse::<f64>().ok()?;
    Some(if v.abs() >= 32768.0 { 0.0 } else { v })
}

/// `w:tblW` / `w:tcW` / `w:tblInd` / `w:tblCellMar/*` and friends: the width in
/// pixels when it is an absolute measure. `None` for relative or automatic
/// widths (`type="pct"` is fiftieths of a percent, `type="auto"` means "derived
/// from the content") - those must never be read as twips.
fn width_px(n: &Node<'_, '_>) -> Option<f64> {
    match attr_val(n, "type").unwrap_or_else(|| "dxa".into()).as_str() {
        "dxa" => Some(tw_px(twips_attr(n, "w").unwrap_or(0.0))),
        "nil" => Some(0.0),
        _ => None,
    }
}

/// Parses a `CT_Border` (`w:top`, `w:insideH`, `w:tcBorders/w:left`, ...) into the
/// frontend `CellBorderSpec` shape `{ w, s, c }` = px / dash style / hex colour.
/// `None` means the border is explicitly absent (`w:val="none"` or `"nil"`).
///
/// `w:sz` is in EIGHTHS OF A POINT, not twips: `sz=4` is half a point, i.e.
/// `4 * 5 / 2 = 10` twips = 0.67 px. `w:color="auto"` (and any malformed value)
/// falls back to black. Word knows 200+ decorative border styles; the engine only
/// draws solid / dashed / dotted, so they collapse onto those three.
fn parse_border(b: &Node<'_, '_>) -> Option<Value> {
    let val = attr_val(b, "val").unwrap_or_else(|| "single".into());
    let style = match val.as_str() {
        "nil" | "none" => return None,
        "dotted" | "dotDash" | "dotDotDash" => "dotted",
        "dashed" | "dashSmallGap" | "dashDotStroked" => "dashed",
        _ => "solid",
    };
    let sz = attr_val(b, "sz")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(4.0);
    // sz eighths of a point = sz * 2.5 twips = sz / 6 px. Kept to 0.01 px: a
    // hairline (sz=4) is 0.67 px and rounding it to a tenth is already visible.
    let w = (sz / 6.0 * 100.0).round() / 100.0;
    let c = attr_val(b, "color")
        .map(|c| c.trim().trim_start_matches('#').to_string())
        .filter(|c| c.len() == 6 && c.chars().all(|ch| ch.is_ascii_hexdigit()))
        .map(|c| format!("#{}", c.to_lowercase()))
        .unwrap_or_else(|| "#000000".into());
    Some(json!({ "w": if w < 0.5 { 0.5 } else { w }, "s": style, "c": c }))
}

/// Reads a `w:tcBorders` element into the `cellBorders` map. An absent side is
/// left out of the map (inherit the table default); a `nil`/`none` side is stored
/// as JSON `null`, which the renderer treats as an explicit "no border" that can
/// veto the neighbour's inherited default. That distinction is load-bearing.
fn parse_cell_borders(tb: &Node<'_, '_>) -> Map<String, Value> {
    let mut cb = Map::new();
    for (key, names) in [
        ("t", ["top", "top"]),
        ("b", ["bottom", "bottom"]),
        ("l", ["left", "start"]),
        ("r", ["right", "end"]),
    ] {
        if let Some(e) = child(tb, names[0]).or_else(|| child(tb, names[1])) {
            cb.insert(key.into(), parse_border(&e).unwrap_or(Value::Null));
        }
    }
    cb
}

// ─── Grid model ──────────────────────────────────────────────────────────────

/// Column-aware grid model. Vertical-merge continuations (`w:vMerge` without
/// `val="restart"`) exist in the XML but are NOT emitted: they only serve to
/// compute the `rowspan` of the cell they continue. The frontend rebuilds the
/// grid from colspan/rowspan.
struct RawCell {
    col: u32,
    cspan: u32,
    rspan: u32,
    /// Vertical-merge continuation: absorbed by the cell above, never emitted.
    cont: bool,
    /// `w:tcW` in px, 0 when absent - fallback column widths when `w:tblGrid` is missing.
    tcw: f64,
    pm: Option<PmNode>,
}

/// Element children named `name`, looking THROUGH the structured-document tags
/// a producer may wrap them in: Word (and Google Docs, which writes the
/// `goog_rdk_*` tags) is free to put a `<w:sdt><w:sdtContent>` around a row or a
/// cell. Matching on direct children alone drops the whole row or cell — the
/// table then imports with missing columns and no error anywhere.
/// LibreOffice does the same in `DomainMapper::lcl_startSectionGroup`.
fn named_children<'a, 'i: 'a>(parent: &Node<'a, 'i>, name: &str) -> Vec<Node<'a, 'i>> {
    fn walk<'a, 'i: 'a>(parent: &Node<'a, 'i>, name: &str, depth: u8, out: &mut Vec<Node<'a, 'i>>) {
        for c in parent.children().filter(|n| n.is_element()) {
            match local(&c) {
                n if n == name => out.push(c),
                // A `w:sdt` holds its payload one level down, in `w:sdtContent`.
                "sdt" | "sdtContent" if depth < 6 => walk(&c, name, depth + 1, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(parent, name, 0, &mut out);
    out
}

/// Row-level properties (`w:trPr`), kept parallel to the grid rows.
struct RowMeta {
    height: f64,
    mode: &'static str,
    cant_split: bool,
    header: bool,
    /// `w:trPr/w:ins|w:del` — the row itself was inserted or deleted.
    revision: Option<Value>,
}

/// `w:gridBefore` / `w:gridAfter` leave real holes in the row: Word renders the
/// row as starting (or ending) short. The model has no notion of a hole, so the
/// gap becomes a borderless empty cell spanning the missing columns - which keeps
/// every other row's column indices, and therefore the vMerge matching, correct.
fn spacer_cell(col: u32, span: u32) -> RawCell {
    RawCell {
        col,
        cspan: span,
        rspan: 1,
        cont: false,
        tcw: 0.0,
        pm: Some(PmNode {
            node_type: "tableCell".into(),
            attrs: Some(json!({
                "colspan": span,
                "rowspan": 1,
                "cellBorders": { "t": null, "b": null, "l": null, "r": null },
            })),
            content: Some(vec![PmNode::paragraph(vec![])]),
            marks: None,
            text: None,
        }),
    }
}

/// `w:gridBefore` / `w:gridAfter` value, clamped so a corrupt file cannot make us
/// allocate a cell spanning thousands of columns.
fn grid_skip(trpr: Option<&Node<'_, '_>>, name: &str) -> u32 {
    trpr.and_then(|p| child(p, name))
        .and_then(|n| attr_val(&n, "val"))
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(0)
        .min(64)
}

// ─── Entry point ─────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub(crate) fn parse_table(
    tbl: &Node<'_, '_>,
    rels: &HashMap<String, String>,
    numbering: &HashMap<String, bool>,
    fonts: &FontCtx,
    theme: &HashMap<String, String>,
    media: &HashMap<String, String>,
    fs: &mut FieldState,
    notes: &NoteTexts,
) -> PmNode {
    let tblpr = child(tbl, "tblPr");
    let mut grid: Vec<Vec<RawCell>> = Vec::new();
    let mut metas: Vec<RowMeta> = Vec::new();

    // ── Pass 1: cells ────────────────────────────────────────────────────────
    for tr in named_children(tbl, "tr") {
        let trpr = child(&tr, "trPr");
        metas.push(parse_row_props(trpr.as_ref()));

        let mut row: Vec<RawCell> = Vec::new();
        let mut col = 0u32;
        let before = grid_skip(trpr.as_ref(), "gridBefore");
        if before > 0 {
            row.push(spacer_cell(col, before));
            col += before;
        }

        for tc in named_children(&tr, "tc") {
            let tcpr = child(&tc, "tcPr");
            let colspan = tcpr
                .as_ref()
                .and_then(|p| child(p, "gridSpan"))
                .and_then(|g| attr_val(&g, "val"))
                .and_then(|v| v.trim().parse::<u32>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(1)
                .min(256);
            let tcw = tcpr
                .as_ref()
                .and_then(|p| child(p, "tcW"))
                .and_then(|w| width_px(&w))
                .unwrap_or(0.0);
            let vmerge = tcpr.as_ref().and_then(|p| child(p, "vMerge"));
            let restart = vmerge
                .as_ref()
                .map(|v| attr_val(v, "val").as_deref() == Some("restart"))
                .unwrap_or(false);
            // Any `w:vMerge` that is not `restart` continues the merge above -
            // including a bare `<w:vMerge/>`. On the very first row there is
            // nothing above, so such a cell must be kept as a real cell.
            let cont = vmerge.is_some() && !restart && !grid.is_empty();
            let this_col = col;
            col += colspan;
            if cont {
                row.push(RawCell { col: this_col, cspan: colspan, rspan: 1, cont: true, tcw, pm: None });
                continue;
            }

            let mut content = Vec::new();
            for ch in tc.children().filter(|n| n.is_element()) {
                match local(&ch) {
                    "p" => {
                        let node = match parse_paragraph(&ch, rels, numbering, fonts, theme, media, fs, notes) {
                            Block::Node(n) => n,
                            Block::Item { para, .. } => para,
                        };
                        content.push(node);
                    }
                    "tbl" => content.push(parse_table(&ch, rels, numbering, fonts, theme, media, fs, notes)),
                    _ => {}
                }
            }
            if content.is_empty() {
                content.push(PmNode::paragraph(vec![]));
            }
            // Cell formatting: background (w:shd@fill), vertical alignment
            // (w:vAlign), text orientation (w:textDirection), per-side borders
            // (w:tcBorders). `rowspan` starts at 1 and is fixed up in pass 2.
            let mut cell_attrs = json!({ "colspan": colspan, "rowspan": 1 });
            if let Some(p) = tcpr.as_ref() {
                if let Some(fill) = child(p, "shd").and_then(|s| attr_val(&s, "fill")) {
                    let f = fill.trim();
                    if f.len() == 6 && f != "auto" && f.chars().all(|c| c.is_ascii_hexdigit()) {
                        cell_attrs["cellBg"] = json!(format!("#{}", f.to_lowercase()));
                    }
                }
                if let Some(va) = child(p, "vAlign").and_then(|v| attr_val(&v, "val")) {
                    if va == "center" || va == "bottom" {
                        cell_attrs["cellVAlign"] = json!(va);
                    }
                }
                if let Some(td) = child(p, "textDirection").and_then(|v| attr_val(&v, "val")) {
                    let dir = match td.as_str() {
                        "btLr" | "bt" => 270,
                        "tbRl" | "tb" | "tbRlV" => 90,
                        _ => 0,
                    };
                    if dir != 0 {
                        cell_attrs["cellDir"] = json!(dir);
                    }
                }
                if let Some(tb) = child(p, "tcBorders") {
                    let cb = parse_cell_borders(&tb);
                    if !cb.is_empty() {
                        cell_attrs["cellBorders"] = Value::Object(cb);
                    }
                }
                // `w:cellIns` / `w:cellDel`: the cell itself was added or removed
                // by a tracked change (distinct from its content being revised).
                if let Some(rev) = revisions::cell_revision(p) {
                    cell_attrs["cellRevision"] = rev.typed_attrs();
                }
            }
            row.push(RawCell {
                col: this_col,
                cspan: colspan,
                rspan: 1,
                cont: false,
                tcw,
                pm: Some(PmNode {
                    node_type: "tableCell".into(),
                    attrs: Some(cell_attrs),
                    content: Some(content),
                    marks: None,
                    text: None,
                }),
            });
        }

        let after = grid_skip(trpr.as_ref(), "gridAfter");
        if after > 0 {
            row.push(spacer_cell(col, after));
        }
        grid.push(row);
    }

    // ── Pass 2: rowspans ─────────────────────────────────────────────────────
    // A cell's rowspan is 1 + the number of following rows holding a continuation
    // whose column range covers it. Matching by range (not by exact column) keeps
    // merged-and-spanned cells aligned, and extending ANY non-continuation cell
    // (not only an explicit `restart`) recovers the files where the producer left
    // `w:vMerge w:val="restart"` off the head cell.
    let n_rows = grid.len();
    for r in 0..n_rows {
        for k in 0..grid[r].len() {
            if grid[r][k].cont {
                continue;
            }
            let c = grid[r][k].col;
            let mut span = 1u32;
            let mut rr = r + 1;
            while rr < n_rows
                && grid[rr]
                    .iter()
                    .any(|cell| cell.cont && cell.col <= c && c < cell.col + cell.cspan)
            {
                span += 1;
                rr += 1;
            }
            grid[r][k].rspan = span;
            if span > 1 {
                if let Some(pm) = grid[r][k].pm.as_mut() {
                    if let Some(attrs) = pm.attrs.as_mut() {
                        attrs["rowspan"] = json!(span);
                    }
                }
            }
        }
    }

    // ── Table-level properties ───────────────────────────────────────────────
    let mut attrs_map: Map<String, Value> = Map::new();
    let mut tbl_w_px: Option<f64> = None;
    if let Some(pr) = tblpr.as_ref() {
        parse_tbl_pr(pr, &mut attrs_map, &mut tbl_w_px);
    }
    // Borders are split between the table default (insideH/insideV) and explicit
    // per-cell overrides on the perimeter, because the renderer only knows those
    // two levels. Must run after pass 2: it needs the final rowspans.
    if let Some(borders) = tblpr.as_ref().and_then(|pr| child(pr, "tblBorders")) {
        apply_tbl_borders(&borders, &mut attrs_map, &mut grid);
    }
    if let Some(widths) = column_widths(tbl, &grid, tbl_w_px) {
        attrs_map.insert("colWidths".into(), json!(widths));
    }

    // ── Emission: real cells only (continuations excluded) ───────────────────
    let mut rows = Vec::new();
    let mut row_heights: Vec<f64> = Vec::new();
    let mut row_modes: Vec<&'static str> = Vec::new();
    let mut cant_split: Vec<bool> = Vec::new();
    let mut header_rows = 0u32;
    let mut header_open = true;
    for (row, meta) in grid.into_iter().zip(metas) {
        let cells: Vec<PmNode> = row.into_iter().filter_map(|c| c.pm).collect();
        if cells.is_empty() {
            continue;
        }
        rows.push(PmNode {
            node_type: "tableRow".into(),
            attrs: meta.revision.clone().map(|r| json!({ "rowRevision": r })),
            content: Some(cells),
            marks: None,
            text: None,
        });
        row_heights.push(meta.height);
        row_modes.push(meta.mode);
        cant_split.push(meta.cant_split);
        // Only the unbroken run of header rows starting at the top repeats;
        // Word itself stops honouring more than ten of them.
        if header_open && meta.header && header_rows < 10 {
            header_rows += 1;
        } else {
            header_open = false;
        }
    }
    if rows.is_empty() {
        rows.push(PmNode {
            node_type: "tableRow".into(),
            attrs: None,
            content: Some(vec![]),
            marks: None,
            text: None,
        });
    }
    if row_heights.iter().any(|h| *h > 0.0) {
        attrs_map.insert("rowHeights".into(), json!(row_heights));
        if row_modes.contains(&"exactly") {
            attrs_map.insert("rowHeightModes".into(), json!(row_modes));
        }
    }
    if cant_split.iter().any(|b| *b) {
        attrs_map.insert("rowCantSplit".into(), json!(cant_split));
    }
    if header_rows > 0 {
        attrs_map.insert("headerRows".into(), json!(header_rows));
        attrs_map.insert("headerRepeat".into(), json!(true));
    }

    PmNode {
        node_type: "table".into(),
        attrs: if attrs_map.is_empty() { None } else { Some(Value::Object(attrs_map)) },
        content: Some(rows),
        marks: None,
        text: None,
    }
}

// ─── Row properties ──────────────────────────────────────────────────────────

/// `w:trPr`: row height with its rule, "keep on one page", repeated header row.
fn parse_row_props(trpr: Option<&Node<'_, '_>>) -> RowMeta {
    let mut height = 0.0f64;
    let mut mode = "atleast";
    if let Some(th) = trpr.and_then(|p| child(p, "trHeight")) {
        // `w:hRule="exact"` pins the height; anything else (including an absent
        // rule) makes it a minimum. A negative legacy value also means "exact".
        let v = twips_attr(&th, "val").unwrap_or(0.0);
        if v < 0.0 {
            height = tw_px(-v);
            mode = "exactly";
        } else if v > 0.0 {
            height = tw_px(v);
        }
        if attr_val(&th, "hRule").as_deref() == Some("exact") {
            mode = "exactly";
        }
    }
    RowMeta {
        height,
        mode,
        cant_split: trpr.and_then(|p| child(p, "cantSplit")).map(|n| bool_on(&n)).unwrap_or(false),
        header: trpr.and_then(|p| child(p, "tblHeader")).map(|n| bool_on(&n)).unwrap_or(false),
        revision: trpr.and_then(revisions::row_revision).map(|r| r.typed_attrs()),
    }
}

// ─── Table properties ────────────────────────────────────────────────────────

/// `w:tblPr`, minus the borders: overall width, indent, alignment, layout mode,
/// default cell margins, cell spacing and floating-table wrapping.
fn parse_tbl_pr(pr: &Node<'_, '_>, attrs: &mut Map<String, Value>, tbl_w_px: &mut Option<f64>) {
    if let Some(e) = child(pr, "tblW") {
        // `pct` (fiftieths of a percent) has no meaning without the page width,
        // which this layer does not have: leave the engine in autofit.
        *tbl_w_px = width_px(&e).filter(|w| *w > 0.0);
    }
    if let Some(w) = child(pr, "tblInd").and_then(|e| width_px(&e)).filter(|w| *w > 0.0) {
        attrs.insert("tableIndent".into(), json!(w));
    }
    if let Some(jc) = child(pr, "jc").and_then(|n| attr_val(&n, "val")) {
        match jc.as_str() {
            "center" => {
                attrs.insert("tableAlign".into(), json!("center"));
            }
            "right" | "end" => {
                attrs.insert("tableAlign".into(), json!("right"));
            }
            _ => {}
        }
    }
    // `w:tblLayout w:type="fixed"` disables Word's AutoFit; the engine implements
    // both algorithms, so the real value is honoured rather than forced.
    if child(pr, "tblLayout").and_then(|n| attr_val(&n, "type")).as_deref() == Some("fixed") {
        attrs.insert("tableLayout".into(), json!("fixed"));
    }
    // Default inner padding of every cell (`w:tblCellMar`). `start`/`end` are the
    // LTR aliases of `left`/`right`.
    if let Some(cm) = child(pr, "tblCellMar") {
        for (key, names) in [
            ("cellMarginTop", ["top", "top"]),
            ("cellMarginBottom", ["bottom", "bottom"]),
            ("cellMarginLeft", ["left", "start"]),
            ("cellMarginRight", ["right", "end"]),
        ] {
            let side = child(&cm, names[0]).or_else(|| child(&cm, names[1]));
            if let Some(v) = side.and_then(|e| width_px(&e)).filter(|v| (0.0..=200.0).contains(v)) {
                attrs.insert(key.into(), json!(v));
            }
        }
    }
    // `w:tblCellSpacing` is applied on BOTH sides of every edge, so the visible
    // gap between two cells is twice the stored value.
    if let Some(sp) = child(pr, "tblCellSpacing").and_then(|e| width_px(&e)).filter(|v| *v > 0.0) {
        attrs.insert("cellSpacing".into(), json!((sp * 2.0 * 10.0).round() / 10.0));
    }
    // Floating table (`w:tblpPr`): the text wraps around it.
    if let Some(tp) = child(pr, "tblpPr") {
        attrs.insert("tableWrap".into(), json!("around"));
        for (key, name) in [
            ("wrapDistTop", "topFromText"),
            ("wrapDistBottom", "bottomFromText"),
            ("wrapDistLeft", "leftFromText"),
            ("wrapDistRight", "rightFromText"),
        ] {
            if let Some(v) = twips_attr(&tp, name).map(tw_px).filter(|v| *v >= 0.0) {
                attrs.insert(key.into(), json!(v));
            }
        }
        if let Some(v) = twips_attr(&tp, "tblpX").map(tw_px).filter(|v| *v > 0.0) {
            attrs.insert("tableIndent".into(), json!(v));
        }
    }
}

/// Splits `w:tblBorders` between the table-level default (taken from
/// `insideH`/`insideV`) and explicit per-cell overrides materialising the
/// perimeter. This is the inverse of what Word does internally, and it is the
/// only shape the renderer understands.
///
/// An explicit `w:tcBorders` side always wins over the table perimeter, matching
/// Word's "do not overwrite an already-set cell border" rule.
fn apply_tbl_borders(borders: &Node<'_, '_>, attrs: &mut Map<String, Value>, grid: &mut [Vec<RawCell>]) {
    // `None` = element absent (inherit); `Some(None)` = explicit "no border".
    let side = |name: &str| -> Option<Option<Value>> { child(borders, name).map(|e| parse_border(&e)) };
    match side("insideH").or_else(|| side("insideV")).flatten() {
        Some(spec) => {
            if let Some(w) = spec.get("w") {
                attrs.insert("tableBorderWidth".into(), w.clone());
            }
            if let Some(s) = spec.get("s") {
                attrs.insert("tableBorderStyle".into(), s.clone());
            }
            if let Some(c) = spec.get("c") {
                attrs.insert("tableBorderColor".into(), c.clone());
            }
        }
        // No inner border at all: 'plain' turns off the table default so only the
        // explicit per-cell borders below get drawn.
        None => {
            attrs.insert("tableStyle".into(), json!("plain"));
        }
    }

    let top = side("top");
    let bottom = side("bottom");
    let left = side("left").or_else(|| side("start"));
    let right = side("right").or_else(|| side("end"));
    if top.is_none() && bottom.is_none() && left.is_none() && right.is_none() {
        return;
    }
    let n_rows = grid.len();
    for (r, row) in grid.iter_mut().enumerate() {
        let last = row.len().saturating_sub(1);
        for (k, rc) in row.iter_mut().enumerate() {
            // A vertically merged cell also touches the bottom edge of the table.
            let on_bottom = r + rc.rspan.max(1) as usize >= n_rows;
            let Some(pm) = rc.pm.as_mut() else { continue };
            let Some(Value::Object(a)) = pm.attrs.as_mut() else { continue };
            let mut cb = match a.get("cellBorders") {
                Some(Value::Object(m)) => m.clone(),
                _ => Map::new(),
            };
            let mut put = |key: &str, v: &Option<Option<Value>>| {
                if cb.contains_key(key) {
                    return;
                }
                if let Some(spec) = v {
                    cb.insert(key.into(), spec.clone().unwrap_or(Value::Null));
                }
            };
            if r == 0 {
                put("t", &top);
            }
            if on_bottom {
                put("b", &bottom);
            }
            if k == 0 {
                put("l", &left);
            }
            if k == last {
                put("r", &right);
            }
            if !cb.is_empty() {
                a.insert("cellBorders".into(), Value::Object(cb));
            }
        }
    }
}

/// Column widths, in px. `w:tblGrid` is authoritative; a degenerate `gridCol`
/// must not throw the whole grid away, so widths are floored instead. When the
/// grid is missing entirely, `w:tcW` from the widest fully-measured row is used.
fn column_widths(tbl: &Node<'_, '_>, grid: &[Vec<RawCell>], tbl_w_px: Option<f64>) -> Option<Vec<f64>> {
    if let Some(grid_el) = child(tbl, "tblGrid") {
        let mut widths: Vec<f64> = grid_el
            .children()
            .filter(|n| n.is_element() && local(n) == "gridCol")
            .map(|g| tw_px(twips_attr(&g, "w").unwrap_or(0.0)).max(8.0))
            .collect();
        if !widths.is_empty() {
            // `w:tblW` governs the total; rescale when the grid disagrees with it
            // by more than a rounding error.
            if let Some(target) = tbl_w_px.filter(|t| *t > 20.0) {
                let sum: f64 = widths.iter().sum();
                if sum > 0.0 && (sum - target).abs() / target > 0.02 {
                    let k = target / sum;
                    widths = widths.iter().map(|w| (w * k * 10.0).round() / 10.0).collect();
                }
            }
            return Some(widths);
        }
    }
    let mut best: Vec<f64> = Vec::new();
    for row in grid {
        if row.is_empty() || row.iter().any(|c| c.tcw <= 0.0) {
            continue;
        }
        let mut ws = Vec::new();
        for c in row {
            let n = c.cspan.max(1);
            let each = c.tcw / f64::from(n);
            for _ in 0..n {
                ws.push((each * 10.0).round() / 10.0);
            }
        }
        if ws.len() > best.len() {
            best = ws;
        }
    }
    if best.is_empty() { None } else { Some(best) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> FontCtx {
        FontCtx {
            major: "Calibri Light".into(),
            minor: "Calibri".into(),
            default: Some("Calibri".into()),
            default_size: 11.0,
            para_default: serde_json::Map::new(),
            para_styles: HashMap::new(),
            para_style_default: None,
            heading_levels: HashMap::new(),
            run_styles: HashMap::new(),
        }
    }

    /// Parse a `<w:tbl>` snippet and return the resulting `table` node.
    fn table(inner: &str) -> PmNode {
        let xml = format!(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
                 <w:tbl>{inner}</w:tbl></w:document>"#
        );
        let doc = roxmltree::Document::parse(&xml).expect("xml");
        let tbl = doc.descendants().find(|n| local(n) == "tbl").expect("tbl");
        let empty = HashMap::new();
        parse_table(&tbl, &empty, &HashMap::new(), &ctx(), &empty, &empty, &mut FieldState::default(), &NoteTexts::default())
    }

    fn attr<'a>(n: &'a PmNode, k: &str) -> Option<&'a Value> {
        n.attrs.as_ref()?.get(k)
    }

    const CELL: &str = "<w:tc><w:p/></w:tc>";

    /// A tracked row/cell change lives in `w:trPr`/`w:tcPr`, NOT on the runs:
    /// reading only the runs loses an inserted-but-empty row entirely.
    #[test]
    fn tracked_row_and_cell_changes_survive_import() {
        let t = table(concat!(
            r#"<w:tr><w:trPr><w:ins w:id="7" w:author="Ada" w:date="2026-07-28T10:00:00Z"/></w:trPr>"#,
            r#"<w:tc><w:tcPr><w:cellDel w:id="8" w:author="Alan"/></w:tcPr><w:p/></w:tc></w:tr>"#,
        ));
        let row = &t.content.as_ref().expect("lignes")[0];
        let rev = attr(row, "rowRevision").expect("révision de ligne");
        assert_eq!(rev["type"], json!("insertion"));
        assert_eq!(rev["author"], json!("Ada"));
        let cell = &row.content.as_ref().expect("cellules")[0];
        let crev = attr(cell, "cellRevision").expect("révision de cellule");
        assert_eq!(crev["type"], json!("deletion"));
        assert_eq!(crev["author"], json!("Alan"));
    }

    /// The same, on a real Word file: `tdf157011` has both inserted and deleted
    /// columns, which is exactly the case an emptied cell would hide.
    #[test]
    fn tracked_columns_of_a_real_document() {
        let Ok(root) = std::env::var("KUBUNO_DOCX_CORPUS") else {
            eprintln!("KUBUNO_DOCX_CORPUS absent — test ignoré");
            return;
        };
        let path = std::path::Path::new(&root)
            .join("sw/qa/extras/ooxmlexport/data/tdf157011_ins_del_empty_cols.docx");
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("{} absent — test ignoré", path.display());
            return;
        };
        let (body, _, _, _) = crate::converters::docx::import_docx(&bytes).expect("import");
        let mut ins = 0;
        let mut del = 0;
        let mut stack = vec![&body];
        while let Some(n) = stack.pop() {
            if let Some(r) = n.attrs.as_ref().and_then(|a| a.get("cellRevision")) {
                match r.get("type").and_then(|v| v.as_str()) {
                    Some("insertion") => ins += 1,
                    Some("deletion") => del += 1,
                    _ => {}
                }
            }
            if let Some(c) = n.content.as_ref() {
                stack.extend(c.iter());
            }
        }
        assert!(ins > 0 && del > 0, "colonnes suivies perdues: {ins} insérées, {del} supprimées");
    }

    #[test]
    fn geometry_margins_spacing_and_layout() {
        let t = table(&format!(
            r#"<w:tblPr>
                 <w:tblW w:w="4500" w:type="dxa"/>
                 <w:tblInd w:w="720" w:type="dxa"/>
                 <w:jc w:val="center"/>
                 <w:tblLayout w:type="fixed"/>
                 <w:tblCellSpacing w:w="30" w:type="dxa"/>
                 <w:tblCellMar>
                   <w:top w:w="15" w:type="dxa"/><w:start w:w="150" w:type="dxa"/>
                   <w:bottom w:w="15" w:type="dxa"/><w:end w:w="150" w:type="dxa"/>
                 </w:tblCellMar>
               </w:tblPr>
               <w:tblGrid><w:gridCol w:w="1500"/><w:gridCol w:w="3000"/></w:tblGrid>
               <w:tr>{CELL}{CELL}</w:tr>"#
        ));
        assert_eq!(attr(&t, "tableIndent"), Some(&json!(48.0)));
        assert_eq!(attr(&t, "tableAlign"), Some(&json!("center")));
        assert_eq!(attr(&t, "tableLayout"), Some(&json!("fixed")));
        // 30 twips on each side of an edge = a 4 px visible gap.
        assert_eq!(attr(&t, "cellSpacing"), Some(&json!(4.0)));
        assert_eq!(attr(&t, "cellMarginTop"), Some(&json!(1.0)));
        assert_eq!(attr(&t, "cellMarginLeft"), Some(&json!(10.0)));
        assert_eq!(attr(&t, "cellMarginRight"), Some(&json!(10.0)));
        // 1500 + 3000 twips = 100 + 200 px, already matching tblW = 4500.
        assert_eq!(attr(&t, "colWidths"), Some(&json!([100.0, 200.0])));
    }

    #[test]
    fn twips_over_16_bits_are_ignored_like_word() {
        let t = table(&format!(
            r#"<w:tblPr><w:tblInd w:w="40000" w:type="dxa"/></w:tblPr>
               <w:tr>{CELL}</w:tr>"#
        ));
        assert_eq!(attr(&t, "tableIndent"), None);
    }

    #[test]
    fn borders_split_between_table_default_and_perimeter() {
        let t = table(&format!(
            r#"<w:tblPr><w:tblBorders>
                 <w:top w:val="single" w:sz="24" w:color="FF0000"/>
                 <w:bottom w:val="nil"/>
                 <w:insideH w:val="dashed" w:sz="4" w:color="auto"/>
               </w:tblBorders></w:tblPr>
               <w:tr>{CELL}</w:tr><w:tr>{CELL}</w:tr>"#
        ));
        // insideH drives the table-wide default: sz=4 eighths of a point = 0.67 px.
        assert_eq!(attr(&t, "tableBorderStyle"), Some(&json!("dashed")));
        assert_eq!(attr(&t, "tableBorderColor"), Some(&json!("#000000")));
        assert_eq!(attr(&t, "tableBorderWidth"), Some(&json!(0.67)));
        assert_eq!(attr(&t, "tableStyle"), None);
        let rows = t.children();
        let top = attr(&rows[0].children()[0], "cellBorders").expect("row 0 borders");
        // sz=24 = 3 pt = 60 twips = 4 px.
        assert_eq!(top["t"], json!({ "w": 4.0, "s": "solid", "c": "#ff0000" }));
        // An explicit `nil` must survive as JSON null so it can veto a neighbour.
        let bot = attr(&rows[1].children()[0], "cellBorders").expect("row 1 borders");
        assert_eq!(bot["b"], Value::Null);
        // The perimeter never leaks onto the inner edge.
        assert!(bot.get("t").is_none());
    }

    #[test]
    fn no_inner_border_falls_back_to_plain() {
        let t = table(&format!(
            r#"<w:tblPr><w:tblBorders><w:insideH w:val="none"/><w:insideV w:val="none"/>
               </w:tblBorders></w:tblPr><w:tr>{CELL}</w:tr>"#
        ));
        assert_eq!(attr(&t, "tableStyle"), Some(&json!("plain")));
    }

    #[test]
    fn tc_borders_win_over_the_table_perimeter() {
        let t = table(
            r#"<w:tblPr><w:tblBorders><w:top w:val="single" w:sz="8"/></w:tblBorders></w:tblPr>
               <w:tr><w:tc><w:tcPr><w:tcBorders><w:top w:val="nil"/></w:tcBorders></w:tcPr>
               <w:p/></w:tc></w:tr>"#,
        );
        let cb = attr(&t.children()[0].children()[0], "cellBorders").expect("borders");
        assert_eq!(cb["t"], Value::Null);
    }

    #[test]
    fn row_properties_heights_modes_and_header_run() {
        let t = table(&format!(
            r#"<w:tr><w:trPr><w:tblHeader/><w:trHeight w:val="600" w:hRule="exact"/>
                 <w:cantSplit/></w:trPr>{CELL}</w:tr>
               <w:tr><w:trPr><w:tblHeader/></w:trPr>{CELL}</w:tr>
               <w:tr>{CELL}</w:tr>
               <w:tr><w:trPr><w:tblHeader/></w:trPr>{CELL}</w:tr>"#
        ));
        assert_eq!(attr(&t, "headerRows"), Some(&json!(2)));
        assert_eq!(attr(&t, "headerRepeat"), Some(&json!(true)));
        assert_eq!(attr(&t, "rowHeights"), Some(&json!([40.0, 0.0, 0.0, 0.0])));
        assert_eq!(
            attr(&t, "rowHeightModes"),
            Some(&json!(["exactly", "atleast", "atleast", "atleast"]))
        );
        assert_eq!(attr(&t, "rowCantSplit"), Some(&json!([true, false, false, false])));
    }

    #[test]
    fn vmerge_spans_rows_even_without_an_explicit_restart() {
        // The head cell carries no w:vMerge at all - a common producer quirk.
        let t = table(&format!(
            r#"<w:tr>{CELL}{CELL}</w:tr>
               <w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc>{CELL}</w:tr>
               <w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc>{CELL}</w:tr>"#
        ));
        let rows = t.children();
        assert_eq!(attr(&rows[0].children()[0], "rowspan"), Some(&json!(3)));
        // Continuations are absorbed, so the following rows hold one cell each.
        assert_eq!(rows[1].children().len(), 1);
        assert_eq!(attr(&rows[1].children()[0], "rowspan"), Some(&json!(1)));
    }

    #[test]
    fn grid_before_becomes_a_borderless_spacer() {
        let t = table(&format!(
            r#"<w:tr>{CELL}{CELL}</w:tr>
               <w:tr><w:trPr><w:gridBefore w:val="1"/></w:trPr>{CELL}</w:tr>"#
        ));
        let row1 = t.children()[1].children();
        assert_eq!(row1.len(), 2);
        assert_eq!(attr(&row1[0], "colspan"), Some(&json!(1)));
        assert_eq!(attr(&row1[0], "cellBorders").and_then(|b| b.get("l")), Some(&Value::Null));
    }

    #[test]
    fn cell_widths_replace_a_missing_grid() {
        let t = table(
            r#"<w:tr>
                 <w:tc><w:tcPr><w:tcW w:w="1500" w:type="dxa"/></w:tcPr><w:p/></w:tc>
                 <w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:gridSpan w:val="2"/></w:tcPr><w:p/></w:tc>
               </w:tr>"#,
        );
        assert_eq!(attr(&t, "colWidths"), Some(&json!([100.0, 100.0, 100.0])));
    }

    #[test]
    fn percent_widths_are_never_read_as_twips() {
        let t = table(&format!(
            r#"<w:tblPr><w:tblW w:w="5000" w:type="pct"/><w:tblInd w:w="2500" w:type="pct"/></w:tblPr>
               <w:tblGrid><w:gridCol w:w="1500"/></w:tblGrid>
               <w:tr>{CELL}</w:tr>"#
        ));
        assert_eq!(attr(&t, "tableIndent"), None);
        assert_eq!(attr(&t, "colWidths"), Some(&json!([100.0])));
    }
}
