//! XLSX (Office Open XML Spreadsheet) converter — import and export.
//!
//! A .xlsx file is a ZIP archive of XML parts:
//!   - xl/sharedStrings.xml        shared string table (cells of type "s" index into it)
//!   - xl/workbook.xml             sheet list (order + r:id) + defined names
//!   - xl/_rels/workbook.xml.rels  r:id → worksheet path
//!   - xl/worksheets/sheetN.xml    cells, merged ranges, column widths, row heights
//!   - xl/styles.xml               numFmts, fonts, fills, borders, cellXfs
//!   - xl/theme/theme1.xml         colour scheme (for theme colours)
//!
//! Layout of this module (each area lives in its own file so features can be
//! extended independently):
//!   - `util`  shared helpers (refs, colours, units, escaping, tables)
//!   - `read`  the importer, one sub-module per part family
//!   - `write` the exporter, one sub-module per generated part
//!
//! Import covers values, formulas, defined names, merged cells, column/row sizes
//! and a faithful subset of cell styling (font weight/italic/underline/size/colour,
//! fill background, borders, alignment) so the rendered sheet resembles the original.
use std::collections::HashMap;
use std::io::Cursor;

use anyhow::Result;
use serde_json::{json, Map, Value};

pub mod read;
pub mod util;
pub mod shape_presets;
pub mod write;

pub use write::{export_xlsx, SheetExportMeta};

// ── Public output structures ─────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct XlsxSheet {
    pub name:        String,
    pub cells:       HashMap<String, Value>, // "A1" → { v?, f?, s?, link? }
    pub merges:      Vec<String>,            // "A1:B2"
    pub col_widths:  HashMap<String, f64>,   // column letter → px
    pub row_heights: HashMap<i32, f64>,      // 1-based row → px
    pub cond_formats: Vec<Value>,            // [{ ranges:[..], rules:[{type,op,formulas,dxf,stop}] }]
    pub show_gridlines: bool,                // sheetView showGridLines (default true)
    pub default_row_height: Option<f64>,     // sheetFormatPr defaultRowHeight → px
    pub default_col_width: Option<f64>,      // sheetFormatPr defaultColWidth (chars) → px
    pub images:      Vec<Value>,             // embedded pictures (anchor in grid coords + data URL)
    pub charts:      Vec<Value>,             // embedded charts (anchor + type + cat/val refs)
    pub shapes:      Vec<Value>,             // drawing shapes (pixel box + preset geometry + style + caption)
    pub frozen_rows: i64,                    // sheetView <pane state="frozen"> ySplit
    pub frozen_cols: i64,                    // sheetView <pane state="frozen"> xSplit
    pub hidden:      bool,                   // workbook <sheet state="hidden|veryHidden">
    pub tab_color:   Option<String>,         // sheetPr <tabColor> → "#RRGGBB"
    pub protection:  Option<Value>,          // { algorithmName, hashValue, saltValue, spinCount } (ECMA-376 hash)
    pub auto_filter: Option<String>,         // <autoFilter ref> range (filter state itself is not modelled)
    pub validations: Vec<Value>,             // DVBlock JSON: { ranges:[..], rule:{ crit, reject?, help? } }
    pub row_groups:  Vec<Value>,             // outline groups { start, end, collapsed } (rows 1-based)
    pub col_groups:  Vec<Value>,             // outline groups { start, end, collapsed } (cols 0-based)
    pub print_area:  Option<String>,         // _xlnm.Print_Area ranges, sheet qualifier stripped ("$A$1:$D$20[,…]")
    pub print_titles: Option<String>,        // _xlnm.Print_Titles ranges, sheet qualifier stripped
}

#[derive(Debug, Default)]
pub struct XlsxWorkbook {
    pub sheets:        Vec<XlsxSheet>,
    pub defined_names: Vec<(String, String)>, // (name, formula like "='Sheet'!$A$1")
}

// ── Import entry point ───────────────────────────────────────────────────────

pub fn import_xlsx(bytes: &[u8]) -> Result<XlsxWorkbook> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)?;

    let shared = util::read_zip_text(&mut archive, "xl/sharedStrings.xml")
        .map(|x| read::strings::parse_shared_strings(&x)).unwrap_or_default();
    // Real theme palette (modern Excel themes differ from the 2007 defaults).
    let theme = util::read_zip_text(&mut archive, "xl/theme/theme1.xml")
        .and_then(|x| util::parse_theme_colors(&x));
    let styles = util::read_zip_text(&mut archive, "xl/styles.xml")
        .map(|x| read::styles::parse_styles(&x, theme)).unwrap_or_default();
    let wb_xml = util::read_zip_text(&mut archive, "xl/workbook.xml")
        .ok_or_else(|| anyhow::anyhow!("xl/workbook.xml manquant"))?;
    let (sheet_refs, defined_names, local_names) = read::workbook::parse_workbook(&wb_xml);
    let wb_rels_xml = util::read_zip_text(&mut archive, "xl/_rels/workbook.xml.rels");
    let rels = wb_rels_xml.as_deref()
        .map(read::workbook::parse_rels).unwrap_or_default();
    // Threaded-comment authors (workbook-level part; fixed path as fallback for
    // producers that omit the person relationship).
    let persons = wb_rels_xml.as_deref()
        .map(read::workbook::parse_typed_rels).unwrap_or_default()
        .values()
        .find(|(_, ty, _)| ty.ends_with("/person"))
        .map(|(t, _, _)| util::resolve_path("xl/workbook.xml", t))
        .or_else(|| Some("xl/persons/person.xml".to_string()))
        .and_then(|p| util::read_zip_text(&mut archive, &p))
        .map(|x| read::comments::parse_persons(&x))
        .unwrap_or_default();

    let mut wb = XlsxWorkbook { sheets: Vec::new(), defined_names };
    // workbook sheet index (localSheetId space) → index into wb.sheets (a sheet
    // whose part is unreadable is skipped, shifting the indices).
    let mut idx_map: HashMap<usize, usize> = HashMap::new();
    for (orig_idx, entry) in sheet_refs.into_iter().enumerate() {
        let target = rels.get(&entry.rid).cloned().unwrap_or_default();
        let path = if target.starts_with("/xl/") { target.trim_start_matches('/').to_string() }
                   else if target.starts_with("xl/") { target }
                   else { format!("xl/{}", target.trim_start_matches("./")) };
        if let Some(xml) = util::read_zip_text(&mut archive, &path) {
            // Sheet-level rels: hyperlinks resolve `r:id` → external URL; the
            // comment parts (legacy + threaded) are located through them too.
            let typed_rels = sheet_rels_path(&path)
                .and_then(|p| util::read_zip_text(&mut archive, &p))
                .map(|x| read::workbook::parse_typed_rels(&x))
                .unwrap_or_default();
            let hlinks: HashMap<String, String> = typed_rels.iter()
                .filter(|(_, (_, ty, _))| ty.ends_with("/hyperlink"))
                .map(|(id, (t, _, _))| (id.clone(), t.clone()))
                .collect();
            let mut sheet = read::worksheet::parse_worksheet(&xml, &shared, &styles, &hlinks);
            sheet.name = entry.name;
            sheet.hidden = entry.hidden;
            // Shapes are stored as an explicit pixel box, so their EMU anchors are
            // resolved here — against the geometry the worksheet parse just produced.
            let geom = read::drawing::SheetGeom {
                col_widths:  &sheet.col_widths,
                row_heights: &sheet.row_heights,
                default_col_width:  sheet.default_col_width,
                default_row_height: sheet.default_row_height,
            };
            let (imgs, charts, shapes) = read::drawing::extract_sheet_drawings(&mut archive, &path, &geom);
            sheet.images = imgs;
            sheet.charts = charts;
            sheet.shapes = shapes;
            // Cell notes: legacy comments part, overridden by threaded comments
            // (the legacy note is then only a compatibility placeholder).
            let part_of = |suffix: &str| typed_rels.values()
                .find(|(_, ty, _)| ty.ends_with(suffix))
                .map(|(t, _, _)| util::resolve_path(&path, t));
            let legacy = part_of("/comments")
                .and_then(|p| util::read_zip_text(&mut archive, &p))
                .map(|x| read::comments::parse_comments(&x)).unwrap_or_default();
            let threads = part_of("/threadedComment")
                .and_then(|p| util::read_zip_text(&mut archive, &p))
                .map(|x| read::comments::parse_threaded_comments(&x)).unwrap_or_default();
            for (r, text) in read::comments::merge_comments(legacy, threads, &persons) {
                let entry = sheet.cells.entry(r).or_insert_with(|| Value::Object(Map::new()));
                if let Some(obj) = entry.as_object_mut() { obj.insert("c".into(), json!(text)); }
            }
            wb.sheets.push(sheet);
            idx_map.insert(orig_idx, wb.sheets.len() - 1);
        }
    }
    if wb.sheets.is_empty() { anyhow::bail!("aucune feuille lisible dans le .xlsx"); }

    // Sheet-scoped defined names: Print_Area / Print_Titles builtins attach to
    // their sheet; other local names are folded into the workbook-level list
    // (the internal model has no sheet scope) when the name is still free.
    for ln in local_names {
        let Some(&si) = idx_map.get(&ln.sheet) else { continue };
        match ln.name.as_str() {
            "_xlnm.Print_Area" => wb.sheets[si].print_area = non_empty(strip_sheet_qualifiers(&ln.def)),
            "_xlnm.Print_Titles" => wb.sheets[si].print_titles = non_empty(strip_sheet_qualifiers(&ln.def)),
            _ => {
                if !wb.defined_names.iter().any(|(n, _)| n.eq_ignore_ascii_case(&ln.name)) {
                    wb.defined_names.push((ln.name, format!("={}", ln.def)));
                }
            }
        }
    }
    Ok(wb)
}

// "xl/worksheets/sheet1.xml" → "xl/worksheets/_rels/sheet1.xml.rels".
fn sheet_rels_path(part: &str) -> Option<String> {
    let (dir, file) = part.rsplit_once('/')?;
    Some(format!("{dir}/_rels/{file}.rels"))
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

/// Strip the sheet qualifier from every comma-separated range of a defined-name
/// definition: "'My Sheet'!$A$1:$D$5,'My Sheet'!$F$1" → "$A$1:$D$5,$F$1".
/// Commas inside quoted sheet names are respected.
fn strip_sheet_qualifiers(def: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    for c in def.chars() {
        match c {
            '\'' => { in_q = !in_q; cur.push(c); }
            ',' if !in_q => { parts.push(std::mem::take(&mut cur)); }
            c => cur.push(c),
        }
    }
    parts.push(cur);
    parts.iter()
        .map(|p| p.rsplit_once('!').map(|(_, r)| r).unwrap_or(p).trim().to_string())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests;
