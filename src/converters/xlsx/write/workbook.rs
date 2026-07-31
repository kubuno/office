//! Workbook part builder (xl/workbook.xml).
use super::super::util::esc_xml;

/// One `<sheet>` entry of the workbook part.
pub struct WbSheetEntry {
    pub name:   String,
    pub rid:    String,
    pub hidden: bool,
}

/// Quote a sheet name for use inside a formula / defined-name definition:
/// simple names stay bare, anything else is wrapped in single quotes with
/// embedded quotes doubled.
pub fn quote_sheet_name(name: &str) -> String {
    let simple = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.chars().next().is_some_and(|c| c.is_ascii_digit());
    if simple { name.to_string() } else { format!("'{}'", name.replace('\'', "''")) }
}

/// Build xl/workbook.xml.
/// - `sheets` in tab order; `active_tab` = index of the selected (visible) tab.
/// - `defined_names` = workbook-scoped (NAME, definition WITHOUT the leading `=`).
/// - `local_names` = sheet-scoped names as (name, sheet index, definition) —
///   used for the `_xlnm.Print_Area` / `_xlnm.Print_Titles` builtins.
///
/// CT_Workbook child order: bookViews? → sheets (required) → definedNames? → calcPr?.
pub fn build_workbook_xml(
    sheets: &[WbSheetEntry],
    active_tab: usize,
    defined_names: &[(String, String)],
    local_names: &[(String, usize, String)],
) -> String {
    let mut out = String::with_capacity(512);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    out.push_str("<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">");
    out.push_str(&format!("<bookViews><workbookView activeTab=\"{active_tab}\"/></bookViews>"));
    out.push_str("<sheets>");
    for (i, s) in sheets.iter().enumerate() {
        let state = if s.hidden { " state=\"hidden\"" } else { "" };
        out.push_str(&format!(
            "<sheet name=\"{}\" sheetId=\"{}\"{state} r:id=\"{}\"/>",
            esc_xml(&s.name), i + 1, esc_xml(&s.rid)
        ));
    }
    out.push_str("</sheets>");
    if !defined_names.is_empty() || !local_names.is_empty() {
        out.push_str("<definedNames>");
        for (name, def) in defined_names {
            out.push_str(&format!(
                "<definedName name=\"{}\">{}</definedName>",
                esc_xml(name), esc_xml(def)
            ));
        }
        for (name, sheet_idx, def) in local_names {
            // Excel keeps the filter-database builtin hidden from the name manager.
            let hidden = if name == "_xlnm._FilterDatabase" { " hidden=\"1\"" } else { "" };
            out.push_str(&format!(
                "<definedName name=\"{}\" localSheetId=\"{sheet_idx}\"{hidden}>{}</definedName>",
                esc_xml(name), esc_xml(def)
            ));
        }
        out.push_str("</definedNames>");
    }
    // Cached formula results may be missing → ask for a full recalc on load.
    out.push_str("<calcPr calcId=\"124519\" fullCalcOnLoad=\"1\"/>");
    out.push_str("</workbook>");
    out
}
