/// Convertisseur ODS (OpenDocument Spreadsheet) — export/import.
/// Format : ZIP contenant mimetype + META-INF/manifest.xml + content.xml
use std::collections::HashMap;
use std::io::{Cursor, Write};

use anyhow::Result;
use serde_json::Value;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

// ── Structures internes ────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct OdsSheetData {
    pub name:        String,
    /// cells: map "A1" → (value_str, formula_opt)
    pub cells:       HashMap<String, OdsCell>,
    pub col_widths:  HashMap<String, f64>,
    pub row_heights: HashMap<i32, f64>,
    pub frozen_rows: i32,
    pub frozen_cols: i32,
}

#[derive(Debug)]
pub struct OdsCell {
    pub value:   Option<String>,
    pub formula: Option<String>,
}

// ── Parser de clé cellule ("A1" → (col_idx 0-based, row_idx 0-based)) ────────

fn parse_cell_key(key: &str) -> Option<(usize, usize)> {
    let col_end = key.find(|c: char| c.is_ascii_digit())?;
    let col_str = &key[..col_end];
    let row_str = &key[col_end..];
    let col_idx: usize = col_str.chars().fold(0usize, |acc, c| {
        acc * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1)
    }) - 1;
    let row_idx: usize = row_str.parse::<usize>().ok()?.checked_sub(1)?;
    Some((col_idx, row_idx))
}

fn col_idx_to_letter(mut idx: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (idx % 26) as u8) as char);
        if idx < 26 { break; }
        idx = idx / 26 - 1;
    }
    s
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
}

// ── Export ODS ────────────────────────────────────────────────────────────────

const MANIFEST_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"
                   manifest:version="1.2">
  <manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/>
  <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
</manifest:manifest>"#;

pub fn export_ods(title: &str, sheets: &[OdsSheetData]) -> Result<Vec<u8>> {
    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);

    // 1. mimetype — doit être le premier fichier, sans compression
    zip.start_file("mimetype", SimpleFileOptions::default().compression_method(CompressionMethod::Stored))?;
    zip.write_all(b"application/vnd.oasis.opendocument.spreadsheet")?;

    // 2. META-INF/manifest.xml
    zip.start_file("META-INF/manifest.xml", SimpleFileOptions::default())?;
    zip.write_all(MANIFEST_XML.as_bytes())?;

    // 3. content.xml
    let content = build_content_xml(title, sheets);
    zip.start_file("content.xml", SimpleFileOptions::default())?;
    zip.write_all(content.as_bytes())?;

    let cursor = zip.finish()?;
    Ok(cursor.into_inner())
}

fn build_content_xml(title: &str, sheets: &[OdsSheetData]) -> String {
    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content
    xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
    xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"
    xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
    xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
    xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"
    xmlns:of="urn:oasis:names:tc:opendocument:xmlns:of:1.2"
    office:version="1.2">
  <office:body>
    <office:spreadsheet>
"#);

    xml.push_str(&format!("<!-- {} -->\n", escape_xml(title)));

    for sheet in sheets {
        xml.push_str(&format!(
            "      <table:table table:name=\"{}\">\n",
            escape_xml(&sheet.name)
        ));

        // Compute dimensions
        let max_col = sheet.cells.keys()
            .filter_map(|k| parse_cell_key(k))
            .map(|(c, _)| c)
            .max()
            .unwrap_or(0);
        let max_row = sheet.cells.keys()
            .filter_map(|k| parse_cell_key(k))
            .map(|(_, r)| r)
            .max()
            .unwrap_or(0);

        // Column declarations
        for ci in 0..=max_col {
            let col_letter = col_idx_to_letter(ci);
            let width_pt = sheet.col_widths.get(&col_letter).copied().unwrap_or(64.0);
            xml.push_str(&format!(
                "        <table:table-column table:style-name=\"co{ci}\" />\n"
            ));
            let _ = width_pt; // used for style in a real impl
        }

        // Rows
        for ri in 0..=max_row {
            xml.push_str("        <table:table-row>\n");
            for ci in 0..=max_col {
                let key = format!("{}{}", col_idx_to_letter(ci), ri + 1);
                if let Some(cell) = sheet.cells.get(&key) {
                    let val = cell.value.as_deref().unwrap_or("");
                    if let Some(formula) = &cell.formula {
                        // ODS formula prefix: "of:="
                        xml.push_str(&format!(
                            "          <table:table-cell table:formula=\"of:={}\" office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>\n",
                            escape_xml(formula),
                            escape_xml(val)
                        ));
                    } else if val.is_empty() {
                        xml.push_str("          <table:table-cell />\n");
                    } else if val.parse::<f64>().is_ok() {
                        xml.push_str(&format!(
                            "          <table:table-cell office:value-type=\"float\" office:value=\"{val}\"><text:p>{val}</text:p></table:table-cell>\n"
                        ));
                    } else {
                        xml.push_str(&format!(
                            "          <table:table-cell office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>\n",
                            escape_xml(val)
                        ));
                    }
                } else {
                    xml.push_str("          <table:table-cell />\n");
                }
            }
            xml.push_str("        </table:table-row>\n");
        }

        xml.push_str("      </table:table>\n");
    }

    xml.push_str("    </office:spreadsheet>\n  </office:body>\n</office:document-content>\n");
    xml
}

// ── Import ODS ────────────────────────────────────────────────────────────────

/// Parse un fichier ODS en retournant une liste de feuilles avec leur données.
/// Retourne une liste de (sheet_name, cells) où cells est un HashMap "A1" → valeur_string.
pub fn import_ods(bytes: &[u8]) -> Result<Vec<(String, HashMap<String, Value>)>> {
    use std::io::Read;

    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)?;

    // Trouver content.xml
    let content_xml = {
        let mut file = archive.by_name("content.xml")?;
        let mut buf  = String::new();
        file.read_to_string(&mut buf)?;
        buf
    };

    parse_content_xml(&content_xml)
}

fn parse_content_xml(xml: &str) -> Result<Vec<(String, HashMap<String, Value>)>> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut sheets: Vec<(String, HashMap<String, Value>)> = Vec::new();
    let mut current_sheet: Option<(String, HashMap<String, Value>)> = None;
    let mut current_row    = 0usize;
    let mut current_col    = 0usize;
    let mut in_cell        = false;
    let mut in_text        = false;
    let mut cell_text      = String::new();
    let mut cell_formula: Option<String> = None;
    let mut row_repeat     = 1usize;
    let mut col_repeat     = 1usize;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let local = e.local_name();
                let local_str = std::str::from_utf8(local.as_ref()).unwrap_or("");
                match local_str {
                    "table" => {
                        // Save previous sheet
                        if let Some(s) = current_sheet.take() {
                            sheets.push(s);
                        }
                        let name = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.local_name().as_ref() == b"name")
                            .map(|a| String::from_utf8_lossy(&a.value).into_owned())
                            .unwrap_or_else(|| format!("Feuille {}", sheets.len() + 1));
                        current_sheet = Some((name, HashMap::new()));
                        current_row = 0;
                        current_col = 0;
                    }
                    "table-row" => {
                        row_repeat = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.local_name().as_ref() == b"number-rows-repeated")
                            .and_then(|a| std::str::from_utf8(&a.value).ok()?.parse().ok())
                            .unwrap_or(1);
                        current_col = 0;
                    }
                    "table-cell" | "covered-table-cell" => {
                        col_repeat = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.local_name().as_ref() == b"number-columns-repeated")
                            .and_then(|a| std::str::from_utf8(&a.value).ok()?.parse().ok())
                            .unwrap_or(1);
                        cell_formula = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.local_name().as_ref() == b"formula")
                            .map(|a| {
                                let s = String::from_utf8_lossy(&a.value).into_owned();
                                // Strip "of:=" prefix
                                s.trim_start_matches("of:=").trim_start_matches("oooc:=").to_string()
                            });
                        in_cell   = true;
                        cell_text = String::new();
                    }
                    "p" if in_cell => { in_text = true; }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) if in_text => {
                cell_text.push_str(&e.unescape().unwrap_or_default());
            }
            Ok(Event::End(e)) => {
                let local_name = e.local_name();
                let local = std::str::from_utf8(local_name.as_ref()).unwrap_or("");
                match local {
                    "p"    => { in_text = false; }
                    "table-cell" | "covered-table-cell" => {
                        if in_cell {
                            if !cell_text.is_empty() || cell_formula.is_some() {
                                if let Some((_, ref mut cells)) = current_sheet {
                                    let col_letter = col_idx_to_letter(current_col);
                                    let key        = format!("{}{}", col_letter, current_row + 1);
                                    let cell_val = if let Some(ref f) = cell_formula {
                                        serde_json::json!({ "f": f, "v": cell_text })
                                    } else {
                                        serde_json::json!({ "v": cell_text })
                                    };
                                    cells.insert(key, cell_val);
                                }
                            }
                            current_col += col_repeat;
                            in_cell      = false;
                            col_repeat   = 1;
                            cell_formula = None;
                        }
                    }
                    "table-row" => {
                        current_row += row_repeat;
                        row_repeat   = 1;
                    }
                    "table" => {
                        if let Some(s) = current_sheet.take() {
                            sheets.push(s);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("ODS XML parse error: {e}")),
            _ => {}
        }
    }

    Ok(sheets)
}
