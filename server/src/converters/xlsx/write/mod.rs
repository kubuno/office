//! XLSX writer — assembles a workbook from the internal `.kbcal` content JSON.
//!
//! Entry point: [`export_xlsx`]. Parts produced today:
//!   [Content_Types].xml, _rels/.rels, docProps/{core,app}.xml,
//!   xl/workbook.xml (+ rels), xl/styles.xml, xl/sharedStrings.xml,
//!   xl/theme/theme1.xml, xl/worksheets/sheetN.xml.
//!
//! Extension map for later agents (one file per part family):
//!   - `worksheet.rs`  cell data + per-sheet sections (add `write_*` hooks there)
//!   - `styles.rs`     style dedup registry (add dxfs there for CF)
//!   - `strings.rs`    shared string table
//!   - `workbook.rs`   workbook part (defined names, calcPr)
//!   - `drawing.rs`    per-sheet drawing part (pictures + chart frames + media)
//!   - `chart.rs`      chart parts (xl/charts/chartN.xml)
//!   - `comments.rs`   cell notes (xl/commentsN.xml + companion VML drawing)
use std::collections::BTreeSet;
use std::io::{Cursor, Write};

use anyhow::Result;
use serde_json::Value;
use uuid::Uuid;
use zip::{write::SimpleFileOptions, ZipWriter};

use super::util::esc_xml;

pub mod chart;
pub mod comments;
pub mod condfmt;
pub mod drawing;
pub mod shape;
pub mod strings;
pub mod styles;
pub mod workbook;
pub mod worksheet;

use strings::SharedStrings;
use styles::StyleRegistry;

/// Ordered sheet metadata (SQL `spreadsheet_sheets` rows): the content file is
/// keyed by sheet UUID and carries no ordering of its own.
#[derive(Debug, Clone)]
pub struct SheetExportMeta {
    pub id:   Uuid,
    pub name: String,
}

/// Characters Excel forbids in sheet names.
const BAD_SHEET_CHARS: &[char] = &[':', '\\', '/', '?', '*', '[', ']'];

// Sanitize + uniquify sheet names: ≤31 chars, no forbidden chars, non-empty,
// unique (case-insensitive, as Excel compares).
fn sanitize_sheet_names(metas: &[SheetExportMeta]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::with_capacity(metas.len());
    for (i, m) in metas.iter().enumerate() {
        let mut base: String = m.name.chars()
            .map(|c| if BAD_SHEET_CHARS.contains(&c) { ' ' } else { c })
            .collect::<String>().trim().to_string();
        if base.is_empty() { base = format!("Feuille {}", i + 1); }
        if base.chars().count() > 31 { base = base.chars().take(31).collect(); }
        let mut name = base.clone();
        let mut n = 2;
        while seen.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
            let suffix = format!(" ({n})");
            let keep = 31usize.saturating_sub(suffix.chars().count());
            name = format!("{}{suffix}", base.chars().take(keep).collect::<String>());
            n += 1;
        }
        seen.push(name.clone());
        out.push(name);
    }
    out
}

// ── Static / templated parts ─────────────────────────────────────────────────

// MIME type of an exported media extension (see drawing::decode_data_url).
fn media_content_type(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn content_types_xml(
    sheet_count: usize,
    drawing_count: usize,
    chart_paths: &[String],
    media_exts: &BTreeSet<String>,
    comment_count: usize,
) -> String {
    let mut out = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
        "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
        "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
        "<Default Extension=\"xml\" ContentType=\"application/xml\"/>",
        "<Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>",
        "<Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>",
        "<Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/>",
        "<Override PartName=\"/xl/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>",
        "<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>",
        "<Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/>",
    ));
    for ext in media_exts {
        out.push_str(&format!(
            "<Default Extension=\"{ext}\" ContentType=\"{}\"/>", media_content_type(ext)
        ));
    }
    if comment_count > 0 {
        // The VML companion of the comment parts is declared by extension.
        out.push_str("<Default Extension=\"vml\" ContentType=\"application/vnd.openxmlformats-officedocument.vmlDrawing\"/>");
    }
    for n in 1..=comment_count {
        out.push_str(&format!(
            "<Override PartName=\"/xl/comments{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml\"/>"
        ));
    }
    for i in 1..=sheet_count {
        out.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{i}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
        ));
    }
    for d in 1..=drawing_count {
        out.push_str(&format!(
            "<Override PartName=\"/xl/drawings/drawing{d}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/>"
        ));
    }
    for path in chart_paths {
        out.push_str(&format!(
            "<Override PartName=\"/{path}\" ContentType=\"application/vnd.openxmlformats-officedocument.drawingml.chart+xml\"/>"
        ));
    }
    out.push_str("</Types>");
    out
}

const ROOT_RELS: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
    "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>",
    "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>",
    "<Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties\" Target=\"docProps/app.xml\"/>",
    "</Relationships>",
);

fn workbook_rels_xml(sheet_count: usize) -> String {
    let mut out = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    ));
    for i in 1..=sheet_count {
        out.push_str(&format!(
            "<Relationship Id=\"rId{i}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{i}.xml\"/>"
        ));
    }
    out.push_str(&format!(
        "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
        sheet_count + 1
    ));
    out.push_str(&format!(
        "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"sharedStrings.xml\"/>",
        sheet_count + 2
    ));
    out.push_str(&format!(
        "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" Target=\"theme/theme1.xml\"/>",
        sheet_count + 3
    ));
    out.push_str("</Relationships>");
    out
}

// Sheet-level .rels part: external hyperlink targets + the sheet's drawing +
// its comment parts (legacy notes + companion VML drawing).
fn sheet_rels_xml(
    rels: &[(String, String)],
    drawing_target: Option<&str>,
    comment_part: Option<usize>,
) -> String {
    let mut out = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    ));
    for (rid, target) in rels {
        out.push_str(&format!(
            "<Relationship Id=\"{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"{}\" TargetMode=\"External\"/>",
            esc_xml(rid), esc_xml(target)
        ));
    }
    if let Some(target) = drawing_target {
        out.push_str(&format!(
            "<Relationship Id=\"{DRAWING_RID}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing\" Target=\"{target}\"/>"
        ));
    }
    if let Some(n) = comment_part {
        out.push_str(&format!(
            "<Relationship Id=\"{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments\" Target=\"../comments{n}.xml\"/>",
            comments::COMMENTS_RID
        ));
        out.push_str(&format!(
            "<Relationship Id=\"{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing\" Target=\"../drawings/vmlDrawing{n}.vml\"/>",
            comments::VML_RID
        ));
    }
    out.push_str("</Relationships>");
    out
}

/// Relationship id of a sheet's drawing part (namespaced away from the
/// hyperlink ids "rIdHlN" produced by the worksheet builder).
const DRAWING_RID: &str = "rIdDrw1";

// Insert a late CT_Worksheet child (`<drawing/>`, `<legacyDrawing/>`) into a
// finished worksheet part: these precede tableParts and extLst, and everything
// the worksheet builder emits today comes before them. Call order matters —
// each injected tag lands after the previously injected one (schema order:
// drawing, then legacyDrawing).
fn inject_tag(xml: &str, tag: &str) -> String {
    for marker in ["<tableParts", "<extLst"] {
        if let Some(p) = xml.find(marker) {
            return format!("{}{}{}", &xml[..p], tag, &xml[p..]);
        }
    }
    xml.replacen("</worksheet>", &format!("{tag}</worksheet>"), 1)
}

// Qualify every comma-separated range of a per-sheet print definition with the
// (exported) sheet name: "$A$1:$D$5,$F$1" → "'Sheet'!$A$1:$D$5,'Sheet'!$F$1".
fn qualify_ranges(sheet_name: &str, ranges: &str) -> String {
    let q = workbook::quote_sheet_name(sheet_name);
    ranges.split(',')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        // Defensive: strip a stray qualifier if one was stored.
        .map(|p| p.rsplit_once('!').map(|(_, r)| r).unwrap_or(p))
        .map(|p| format!("{q}!{p}"))
        .collect::<Vec<_>>()
        .join(",")
}

// "A1:C9" → "$A$1:$C$9" (already-absolute endpoints are left untouched).
fn absolutize_range(r: &str) -> String {
    r.split(':').map(|part| {
        let part = part.trim();
        if part.starts_with('$') { return part.to_string(); }
        match part.find(|c: char| c.is_ascii_digit()) {
            Some(p) if p > 0 => format!("${}${}", &part[..p], &part[p..]),
            _ => part.to_string(),
        }
    }).collect::<Vec<_>>().join(":")
}

fn core_props_xml(title: &str) -> String {
    format!(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
        "<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" ",
        "xmlns:dc=\"http://purl.org/dc/elements/1.1/\">",
        "<dc:title>{}</dc:title><dc:creator>Kubuno Office</dc:creator>",
        "</cp:coreProperties>"
    ), esc_xml(title))
}

const APP_PROPS: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
    "<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\">",
    "<Application>Kubuno Office</Application>",
    "</Properties>",
);

// Minimal but schema-complete Office theme (clrScheme + fontScheme + fmtScheme
// are all required children of themeElements).
const THEME_XML: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
    "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"Office\">",
    "<a:themeElements>",
    "<a:clrScheme name=\"Office\">",
    "<a:dk1><a:sysClr val=\"windowText\" lastClr=\"000000\"/></a:dk1>",
    "<a:lt1><a:sysClr val=\"window\" lastClr=\"FFFFFF\"/></a:lt1>",
    "<a:dk2><a:srgbClr val=\"1F497D\"/></a:dk2>",
    "<a:lt2><a:srgbClr val=\"EEECE1\"/></a:lt2>",
    "<a:accent1><a:srgbClr val=\"4F81BD\"/></a:accent1>",
    "<a:accent2><a:srgbClr val=\"C0504D\"/></a:accent2>",
    "<a:accent3><a:srgbClr val=\"9BBB59\"/></a:accent3>",
    "<a:accent4><a:srgbClr val=\"8064A2\"/></a:accent4>",
    "<a:accent5><a:srgbClr val=\"4BACC6\"/></a:accent5>",
    "<a:accent6><a:srgbClr val=\"F79646\"/></a:accent6>",
    "<a:hlink><a:srgbClr val=\"0000FF\"/></a:hlink>",
    "<a:folHlink><a:srgbClr val=\"800080\"/></a:folHlink>",
    "</a:clrScheme>",
    "<a:fontScheme name=\"Office\">",
    "<a:majorFont><a:latin typeface=\"Cambria\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont>",
    "<a:minorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont>",
    "</a:fontScheme>",
    "<a:fmtScheme name=\"Office\">",
    "<a:fillStyleLst>",
    "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>",
    "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>",
    "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>",
    "</a:fillStyleLst>",
    "<a:lnStyleLst>",
    "<a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln>",
    "<a:ln w=\"25400\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln>",
    "<a:ln w=\"38100\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln>",
    "</a:lnStyleLst>",
    "<a:effectStyleLst>",
    "<a:effectStyle><a:effectLst/></a:effectStyle>",
    "<a:effectStyle><a:effectLst/></a:effectStyle>",
    "<a:effectStyle><a:effectLst/></a:effectStyle>",
    "</a:effectStyleLst>",
    "<a:bgFillStyleLst>",
    "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>",
    "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>",
    "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>",
    "</a:bgFillStyleLst>",
    "</a:fmtScheme>",
    "</a:themeElements>",
    "</a:theme>",
);

// ── Entry point ──────────────────────────────────────────────────────────────

/// Export a spreadsheet to a .xlsx byte buffer.
///
/// - `title`        workbook title (docProps only; the filename is the caller's business)
/// - `file_content` the parsed `.kbcal` content file `{ version, sheets: { uuid: sheet_data }, names }`
/// - `sheets_meta`  ordered sheet list (id + display name), from SQL `ORDER BY position`
pub fn export_xlsx(title: &str, file_content: &Value, sheets_meta: &[SheetExportMeta]) -> Result<Vec<u8>> {
    if sheets_meta.is_empty() {
        anyhow::bail!("export xlsx: aucune feuille");
    }
    let names = sanitize_sheet_names(sheets_meta);
    let empty = Value::Object(serde_json::Map::new());

    // Per-sheet data (draft order = SQL order) gathered first so the active
    // (first visible) tab is known before the worksheet parts are built.
    let datas: Vec<&Value> = sheets_meta.iter()
        .map(|meta| file_content.get("sheets").and_then(|s| s.get(meta.id.to_string())).unwrap_or(&empty))
        .collect();
    let mut hidden: Vec<bool> = datas.iter()
        .map(|d| d.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false))
        .collect();
    // Excel requires at least one visible sheet.
    if hidden.iter().all(|&h| h) { hidden[0] = false; }
    let active_tab = hidden.iter().position(|&h| !h).unwrap_or(0);

    let mut styles = StyleRegistry::new();
    let mut sst = SharedStrings::new();

    // Worksheets first: they feed the style registry and the shared strings.
    let mut sheet_parts: Vec<worksheet::WorksheetPart> = Vec::with_capacity(sheets_meta.len());
    for (i, data) in datas.iter().enumerate() {
        sheet_parts.push(worksheet::build_worksheet_xml(data, &mut styles, &mut sst, i == active_tab));
    }

    // Per-sheet drawings (pictures + charts). Media and chart part numbers are
    // workbook-global; drawing parts are numbered over sheets that have one.
    let mut media_no = 1usize;
    let mut chart_no = 1usize;
    let mut sheet_drawings: Vec<Option<(usize, drawing::SheetDrawing)>> = Vec::with_capacity(datas.len());
    let mut drawing_count = 0usize;
    for (i, data) in datas.iter().enumerate() {
        match drawing::build_sheet_drawing(data, &names[i], &mut media_no, &mut chart_no) {
            Some(d) => {
                drawing_count += 1;
                sheet_parts[i].xml = inject_tag(&sheet_parts[i].xml, &format!("<drawing r:id=\"{DRAWING_RID}\"/>"));
                sheet_drawings.push(Some((drawing_count, d)));
            }
            None => sheet_drawings.push(None),
        }
    }

    // Per-sheet cell notes (xl/commentsN.xml + companion VML drawing part).
    // Injected after <drawing/> so <legacyDrawing/> keeps the schema order.
    let mut sheet_comments: Vec<Option<(usize, comments::SheetComments)>> = Vec::with_capacity(datas.len());
    let mut comment_count = 0usize;
    for (i, data) in datas.iter().enumerate() {
        match comments::build_sheet_comments(data) {
            Some(c) => {
                comment_count += 1;
                sheet_parts[i].xml = inject_tag(
                    &sheet_parts[i].xml,
                    &format!("<legacyDrawing r:id=\"{}\"/>", comments::VML_RID),
                );
                sheet_comments.push(Some((comment_count, c)));
            }
            None => sheet_comments.push(None),
        }
    }
    let chart_paths: Vec<String> = sheet_drawings.iter().flatten()
        .flat_map(|(_, d)| d.charts.iter().map(|(p, _)| p.clone()))
        .collect();
    let media_exts: BTreeSet<String> = sheet_drawings.iter().flatten()
        .flat_map(|(_, d)| d.media.iter())
        .filter_map(|(p, _)| p.rsplit('.').next().map(|e| e.to_string()))
        .collect();

    // Workbook-level defined names ({ NAME: "=definition" } → strip the '=').
    let mut defined: Vec<(String, String)> = file_content
        .get("names").and_then(|n| n.as_object())
        .map(|m| m.iter()
            .filter_map(|(k, v)| v.as_str().map(|d| (k.clone(), d.trim_start_matches('=').to_string())))
            .filter(|(_, d)| !d.is_empty())
            .collect())
        .unwrap_or_default();
    defined.sort();

    // Sheet-scoped print builtins: qualify each stored range with its sheet name.
    let mut local_names: Vec<(String, usize, String)> = Vec::new();
    for (i, data) in datas.iter().enumerate() {
        for (key, builtin) in [("print_area", "_xlnm.Print_Area"), ("print_titles", "_xlnm.Print_Titles")] {
            if let Some(ranges) = data.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                local_names.push((builtin.to_string(), i, qualify_ranges(&names[i], ranges)));
            }
        }
        // The autoFilter range is mirrored as the hidden _FilterDatabase builtin
        // (Excel convention; LibreOffice needs it to rebuild its filter range).
        if let Some(af) = data.get("auto_filter").and_then(|v| v.as_str()).filter(|s| s.contains(':')) {
            local_names.push(("_xlnm._FilterDatabase".to_string(), i, qualify_ranges(&names[i], &absolutize_range(af))));
        }
    }

    let wb_sheets: Vec<workbook::WbSheetEntry> = names.iter().enumerate()
        .map(|(i, n)| workbook::WbSheetEntry { name: n.clone(), rid: format!("rId{}", i + 1), hidden: hidden[i] })
        .collect();
    let workbook_xml = workbook::build_workbook_xml(&wb_sheets, active_tab, &defined, &local_names);

    // ── ZIP assembly ([Content_Types].xml first, by convention) ──
    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);
    let opts = SimpleFileOptions::default();

    let put = |zip: &mut ZipWriter<Cursor<Vec<u8>>>, name: &str, content: &str| -> Result<()> {
        zip.start_file(name, opts)?;
        zip.write_all(content.as_bytes())?;
        Ok(())
    };
    let put_bytes = |zip: &mut ZipWriter<Cursor<Vec<u8>>>, name: &str, content: &[u8]| -> Result<()> {
        zip.start_file(name, opts)?;
        zip.write_all(content)?;
        Ok(())
    };

    put(&mut zip, "[Content_Types].xml",
        &content_types_xml(sheet_parts.len(), drawing_count, &chart_paths, &media_exts, comment_count))?;
    put(&mut zip, "_rels/.rels", ROOT_RELS)?;
    put(&mut zip, "docProps/core.xml", &core_props_xml(title))?;
    put(&mut zip, "docProps/app.xml", APP_PROPS)?;
    put(&mut zip, "xl/workbook.xml", &workbook_xml)?;
    put(&mut zip, "xl/_rels/workbook.xml.rels", &workbook_rels_xml(sheet_parts.len()))?;
    put(&mut zip, "xl/styles.xml", &styles.build_xml())?;
    put(&mut zip, "xl/sharedStrings.xml", &sst.build_xml())?;
    put(&mut zip, "xl/theme/theme1.xml", THEME_XML)?;
    for (i, part) in sheet_parts.iter().enumerate() {
        put(&mut zip, &format!("xl/worksheets/sheet{}.xml", i + 1), &part.xml)?;
        let drawing_target = sheet_drawings[i].as_ref()
            .map(|(d, _)| format!("../drawings/drawing{d}.xml"));
        let comment_part = sheet_comments[i].as_ref().map(|(n, _)| *n);
        if !part.rels.is_empty() || drawing_target.is_some() || comment_part.is_some() {
            put(&mut zip, &format!("xl/worksheets/_rels/sheet{}.xml.rels", i + 1),
                &sheet_rels_xml(&part.rels, drawing_target.as_deref(), comment_part))?;
        }
    }
    for (n, c) in sheet_comments.iter().flatten() {
        put(&mut zip, &format!("xl/comments{n}.xml"), &c.comments_xml)?;
        put(&mut zip, &format!("xl/drawings/vmlDrawing{n}.vml"), &c.vml_xml)?;
    }
    for (d, dr) in sheet_drawings.iter().flatten() {
        put(&mut zip, &format!("xl/drawings/drawing{d}.xml"), &dr.xml)?;
        put(&mut zip, &format!("xl/drawings/_rels/drawing{d}.xml.rels"), &dr.rels)?;
        for (path, xml) in &dr.charts { put(&mut zip, path, xml)?; }
        for (path, bytes) in &dr.media { put_bytes(&mut zip, path, bytes)?; }
    }

    let cursor = zip.finish()?;
    Ok(cursor.into_inner())
}
