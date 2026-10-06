//! Comment parts writer: xl/commentsN.xml (legacy cell notes) plus the
//! companion VML drawing part (xl/drawings/vmlDrawingN.vml).
//!
//! Excel requires the VML part: the note shapes (position, size, visibility)
//! live there, referenced from the worksheet through `<legacyDrawing r:id>`.
//! Without it Excel triggers a silent "repair" and drops the notes
//! (LibreOffice exports the same pair — sc/source/filter/excel/xeescher.cxx,
//! XclExpNote). Shapes are hidden by default, anchored next to their cell.
use serde_json::Value;

use super::super::util::{col_to_idx, esc_xml, split_ref};

/// Relationship ids used in the sheet .rels part (namespaced away from the
/// hyperlink "rIdHlN" and drawing "rIdDrw1" ids).
pub const COMMENTS_RID: &str = "rIdCmt1";
pub const VML_RID: &str = "rIdVml1";

/// The two generated parts for one sheet's notes.
pub struct SheetComments {
    pub comments_xml: String,
    pub vml_xml:      String,
}

/// Build the comment parts of one sheet from its internal (snake_case)
/// sheet_data JSON, or `None` when no cell carries a note (`c` key).
pub fn build_sheet_comments(data: &Value) -> Option<SheetComments> {
    let cells = data.get("cells")?.as_object()?;
    // (row, col_idx, ref, text) — sorted for a deterministic part.
    let mut notes: Vec<(i32, usize, &String, &str)> = cells.iter()
        .filter_map(|(k, c)| {
            let text = c.get("c")?.as_str()?;
            if text.trim().is_empty() { return None }
            let (col, row) = split_ref(k)?;
            Some((row, col_to_idx(&col), k, text))
        })
        .collect();
    if notes.is_empty() { return None }
    notes.sort_unstable_by_key(|&(row, col, _, _)| (row, col));

    // ── xl/commentsN.xml ──
    // The author is not modelled internally (a note is plain text) — a single
    // empty author entry satisfies the schema without inventing content.
    let mut cm = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
        "<comments xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
        "<authors><author/></authors><commentList>",
    ));
    for (_, _, key, text) in &notes {
        cm.push_str(&format!(
            "<comment ref=\"{}\" authorId=\"0\"><text><t xml:space=\"preserve\">{}</t></text></comment>",
            esc_xml(key), esc_xml(text)
        ));
    }
    cm.push_str("</commentList></comments>");

    // ── xl/drawings/vmlDrawingN.vml ──
    let mut vml = String::from(concat!(
        "<xml xmlns:v=\"urn:schemas-microsoft-com:vml\"",
        " xmlns:o=\"urn:schemas-microsoft-com:office:office\"",
        " xmlns:x=\"urn:schemas-microsoft-com:office:excel\">",
        "<o:shapelayout v:ext=\"edit\"><o:idmap v:ext=\"edit\" data=\"1\"/></o:shapelayout>",
        "<v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\"",
        " path=\"m,l,21600r21600,l21600,xe\">",
        "<v:stroke joinstyle=\"miter\"/><v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/>",
        "</v:shapetype>",
    ));
    for (i, (row, col, _, _)) in notes.iter().enumerate() {
        // 0-based cell coordinates; the anchor places the (hidden) note box
        // one column to the right, spanning ~2 columns × ~3 rows:
        // col1, dx1, row1, dy1, col2, dx2, row2, dy2 (offsets in px).
        let (r0, c0) = ((row - 1).max(0) as usize, *col);
        let anchor = format!(
            "{}, 15, {}, 2, {}, 31, {}, 9",
            c0 + 1, r0.saturating_sub(1), c0 + 3, r0 + 3
        );
        vml.push_str(&format!(concat!(
            "<v:shape id=\"_x0000_s{}\" type=\"#_x0000_t202\"",
            " style=\"position:absolute;margin-left:0;margin-top:0;width:96pt;height:55.5pt;z-index:{};visibility:hidden\"",
            " fillcolor=\"#ffffe1\" o:insetmode=\"auto\">",
            "<v:fill color2=\"#ffffe1\"/>",
            "<v:shadow on=\"t\" color=\"black\" obscured=\"t\"/>",
            "<v:path o:connecttype=\"none\"/>",
            "<v:textbox style=\"mso-direction-alt:auto\"><div style=\"text-align:left\"></div></v:textbox>",
            "<x:ClientData ObjectType=\"Note\">",
            "<x:MoveWithCells/><x:SizeWithCells/>",
            "<x:Anchor>{}</x:Anchor>",
            "<x:AutoFill>False</x:AutoFill>",
            "<x:Row>{}</x:Row><x:Column>{}</x:Column>",
            "</x:ClientData></v:shape>"),
            1025 + i, i + 1, anchor, r0, c0
        ));
    }
    vml.push_str("</xml>");

    Some(SheetComments { comments_xml: cm, vml_xml: vml })
}
