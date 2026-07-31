//! `<w:sectPr>` - page geometry, margins, columns, page numbering, borders.

use roxmltree::{Document as XmlDoc, Node};

use crate::converters::docx::model::{LineNumbering, PageBorder, SectionColumns, SectionInfo};
use crate::converters::docx::xml::{attr_val, child, local};

/// twips -> CSS px (1440 twips per inch / 96 px per inch). Word IGNORES any
/// value >= 0x8000 and renders 0 — a 16-bit leftover, explicit in LibreOffice
/// `ConversionHelper::convertTwipToMm100_Limited` (ConversionHelper.cxx:423);
/// without this guard a corrupt file yields margins several metres wide.
fn twips_px(v: Option<String>, dflt: f64) -> f64 {
    match v.and_then(|s| s.trim().parse::<f64>().ok()) {
        Some(t) if t.abs() >= 32768.0 => 0.0,
        Some(t) => (t / 15.0).round(),
        None => dflt,
    }
}

/// True when this `<w:sectPr>` really describes a section of the body, i.e. it
/// sits either in `w:body` (last section) or in `w:body/w:p/w:pPr` (a section
/// break). This rules out the nested `<w:sectPr>` of `<w:sectPrChange>`, which
/// carries the layout as it was BEFORE a tracked change and — being a child of
/// its own parent — comes AFTER it in document order: a plain `rfind` over the
/// descendants would restore the old layout of any document with change
/// tracking enabled on the page setup.
fn is_body_section(n: &Node<'_, '_>) -> bool {
    let Some(parent) = n.parent() else {
        return false;
    };
    match local(&parent) {
        "body" => true,
        "pPr" => parent
            .parent()
            .filter(|p| local(p) == "p")
            .and_then(|p| p.parent())
            .is_some_and(|b| local(&b) == "body"),
        _ => false,
    }
}

/// Every section of the document, in reading order. Word stores the layout of
/// section N in the LAST paragraph of that section (`<w:p><w:pPr><w:sectPr>`);
/// only the LAST section lives in `<w:body><w:sectPr>`. So `sections[i]`
/// describes the slice that PRECEDES the i-th `sectPr`. Cf. LibreOffice
/// `SectionPropertyMap::CloseSectionGroup` (PropertyMap.cxx:1641).
// TODO(read/mod.rs): drop this allow once the multi-section import is wired.
#[allow(dead_code)]
pub(crate) fn parse_all_sections(document_xml: &str) -> Vec<SectionInfo> {
    let Ok(doc) = XmlDoc::parse(document_xml) else {
        return vec![SectionInfo::default()];
    };
    let all: Vec<Node> = doc.descendants().filter(|n| local(n) == "sectPr").collect();
    let mut out: Vec<SectionInfo> = all
        .iter()
        .filter(|n| is_body_section(n))
        .map(section_from_node)
        .collect();
    if out.is_empty() {
        // Malformed file: keep the last `sectPr` that is not a revision record
        // rather than dropping the layout altogether.
        if let Some(n) = all.iter().rev().find(|n| {
            !n.ancestors()
                .skip(1)
                .any(|a| matches!(local(&a), "sectPrChange" | "pPrChange"))
        }) {
            out.push(section_from_node(n));
        }
    }
    if out.is_empty() {
        out.push(SectionInfo::default());
    }
    out
}

/// BODY page layout = the last section. Facade kept for the callers that only need
/// a single page layout.
pub(crate) fn parse_section_props(document_xml: &str) -> SectionInfo {
    parse_all_sections(document_xml).pop().unwrap_or_default()
}

/// Read ONE `<w:sectPr>`. Only direct children are inspected (never
/// `descendants()`) so a nested `<w:sectPrChange>` can never bleed in.
fn section_from_node(sect: &Node<'_, '_>) -> SectionInfo {
    let mut s = SectionInfo::default();

    if let Some(mar) = child(sect, "pgMar") {
        // A NEGATIVE top/bottom margin means "fixed margin even if the header
        // overlaps it", not a negative margin: LibreOffice stores abs() plus a
        // dynamic-height flag (SetTopMargin, PropertyMap.hxx:408). Without
        // overlapping rendering, the absolute value is the faithful fallback.
        s.margin_top = twips_px(attr_val(&mar, "top"), 96.0).abs();
        s.margin_right = twips_px(attr_val(&mar, "right"), 96.0);
        s.margin_bottom = twips_px(attr_val(&mar, "bottom"), 96.0).abs();
        s.margin_left = twips_px(attr_val(&mar, "left"), 96.0);
        s.gutter = twips_px(attr_val(&mar, "gutter"), 0.0);
        s.header_dist = twips_px(attr_val(&mar, "header"), 48.0);
        s.footer_dist = twips_px(attr_val(&mar, "footer"), 48.0);
    }

    if let Some(sz) = child(sect, "pgSz") {
        if attr_val(&sz, "orient").as_deref() == Some("landscape") {
            s.orientation = "landscape".into();
        }
        let w = attr_val(&sz, "w").and_then(|v| v.parse::<f64>().ok()).unwrap_or(11906.0);
        let h = attr_val(&sz, "h").and_then(|v| v.parse::<f64>().ok()).unwrap_or(16838.0);
        // Match on the (short side, long side) COUPLE, the way LibreOffice does
        // (PaperInfo::sloppyFitPageDimension, DomainMapper.cxx:2853). Comparing
        // only the longest side confused A5 (11906 tall) with the WIDTH of A4.
        let (short, long) = (w.min(h), w.max(h));
        let near = |a: f64, b: f64| (short - a).abs() < 300.0 && (long - b).abs() < 300.0;
        s.paper = if near(16838.0, 23811.0) {
            "a3".into()
        } else if near(12240.0, 20160.0) {
            "legal".into()
        } else if near(12240.0, 15840.0) {
            "letter".into()
        } else if near(8391.0, 11906.0) {
            "a5".into()
        } else {
            "a4".into()
        };
    }

    s.title_pg = child(sect, "titlePg").is_some();
    s.bidi = child(sect, "bidi").is_some();

    if let Some(va) = child(sect, "vAlign").and_then(|n| attr_val(&n, "val")) {
        s.v_align = match va.as_str() {
            "center" => "center",
            "bottom" => "bottom",
            "both" => "both",
            _ => "top",
        }
        .into();
    }
    if let Some(ty) = child(sect, "type").and_then(|n| attr_val(&n, "val")) {
        s.section_start = match ty.as_str() {
            "continuous" => "continuous",
            "evenPage" => "evenPage",
            "oddPage" => "oddPage",
            // Word 2013+ degrades `nextColumn` to a page break as soon as the
            // column count changes (PropertyMap.cxx:1648).
            _ => "nextPage",
        }
        .into();
    }

    // Header/footer parts, keyed by type (default | first | even).
    for r in sect.children().filter(|n| n.is_element()) {
        let map = match local(&r) {
            "headerReference" => &mut s.header_refs,
            "footerReference" => &mut s.footer_refs,
            _ => continue,
        };
        let kind = attr_val(&r, "type").unwrap_or_else(|| "default".into());
        if let Some(id) = attr_val(&r, "id") {
            map.insert(kind, id);
        }
    }

    s.columns = parse_section_columns(sect);
    s.line_numbers = parse_line_numbering(sect);
    s.page_border = parse_page_borders(sect);

    if let Some(pn) = child(sect, "pgNumType") {
        s.page_num_start = attr_val(&pn, "start").and_then(|v| v.trim().parse::<i32>().ok());
        if let Some(fmt) = attr_val(&pn, "fmt") {
            // Frontend `PageNumFormat` values (DocumentEditorPage.tsx).
            s.page_num_fmt = match fmt.as_str() {
                "upperRoman" => "roman-upper",
                "lowerRoman" => "roman-lower",
                "upperLetter" => "alpha-upper",
                "lowerLetter" => "alpha-lower",
                _ => "arabic",
            }
            .into();
        }
    }
    s
}

/// `<w:cols>` — count, gutter, per-column widths, separator line.
/// Cf. SectionColumnHandler.cxx:40 and DomainMapper.cxx:2902.
fn parse_section_columns(sect: &Node<'_, '_>) -> SectionColumns {
    let mut c = SectionColumns::default();
    let Some(cols) = child(sect, "cols") else {
        return c;
    };

    // Missing `num` means 1; Word treats `num == 1` as "no columns"
    // (DomainMapper.cxx:2911: `nColumnCount = num == 1 ? 0 : num`).
    // Word's own upper bound is 45 columns.
    let num = attr_val(&cols, "num")
        .and_then(|v| v.trim().parse::<u16>().ok())
        .unwrap_or(1)
        .clamp(1, 45);
    // `equalWidth` defaults to true (OOXML boolean: 1/0/true/false/on/off).
    let equal = attr_val(&cols, "equalWidth")
        .map(|v| !matches!(v.as_str(), "0" | "false" | "off"))
        .unwrap_or(true);
    c.space = twips_px(attr_val(&cols, "space"), 48.0);
    c.sep = attr_val(&cols, "sep")
        .map(|v| matches!(v.as_str(), "1" | "true" | "on"))
        .unwrap_or(false);

    let mut widths = Vec::new();
    let mut spaces = Vec::new();
    for col in cols.children().filter(|n| local(n) == "col") {
        widths.push(twips_px(attr_val(&col, "w"), 0.0));
        // `space` of the LAST `<w:col>` is optional (docxattributeoutput.cxx:10627).
        spaces.push(twips_px(attr_val(&col, "space"), c.space));
    }

    // Individual widths are honoured ONLY when equalWidth=0 AND there are as
    // many <w:col> as `num` — otherwise Word/LO spread evenly
    // (PropertyMap.cxx:849).
    let use_custom = !equal && !widths.is_empty() && widths.len() == num as usize;
    c.count = num;
    c.equal_width = !use_custom;
    if use_custom {
        c.widths = widths;
        c.spaces = spaces;
    }
    c
}

/// `<w:lnNumType countBy start distance restart>` — line numbering. Word makes
/// it a PER-SECTION setting, Writer a GLOBAL one (DomainMapper.cxx:1213); our
/// model is global too, so a caller keeps the first section that enables it.
fn parse_line_numbering(sect: &Node<'_, '_>) -> Option<LineNumbering> {
    let ln = child(sect, "lnNumType")?;
    let count_by = attr_val(&ln, "countBy")
        .and_then(|v| v.trim().parse::<i32>().ok())
        .unwrap_or(1);
    if count_by <= 0 {
        return None; // countBy = 0 disables numbering (DomainMapper.cxx:2810)
    }
    Some(LineNumbering {
        count_by,
        // Word is 0-based, our display 1-based (PropertyMap.cxx:1710).
        start: attr_val(&ln, "start")
            .and_then(|v| v.trim().parse::<i32>().ok())
            .unwrap_or(0)
            .saturating_add(1),
        distance: twips_px(attr_val(&ln, "distance"), 0.0),
        restart: match attr_val(&ln, "restart").as_deref() {
            Some("newPage") => "newPage",
            Some("continuous") => "continuous",
            _ => "newSection",
        }
        .into(),
    })
}

/// `<w:pgBorders>` — page frame. The first side that is not `w:val="none"`
/// gives the style, since our frontend `PageBorderDef` has a single style for
/// the four sides; sides set to `none` are skipped like PageBordersHandler.cxx:122.
fn parse_page_borders(sect: &Node<'_, '_>) -> Option<PageBorder> {
    let pb = child(sect, "pgBorders")?;
    let side = ["top", "left", "bottom", "right"]
        .iter()
        .filter_map(|k| child(&pb, k))
        .find(|n| !matches!(attr_val(n, "val").as_deref(), Some("none") | Some("nil")))?;

    let val = attr_val(&side, "val").unwrap_or_else(|| "single".into());
    let style = match val.as_str() {
        "dashed" | "dashSmallGap" | "dotDash" | "dotDotDash" => "dashed",
        "dotted" => "dotted",
        "double" | "thinThickSmallGap" | "thickThinSmallGap" | "doubleWave" => "double",
        _ => "solid",
    };
    // `w:sz` is in eighths of a point; `w:space` in points (0..31), measured
    // from the page edge or from the text depending on `offsetFrom`
    // (PageBordersHandler.cxx:67, SetBorderDistance PropertyMap.cxx:736).
    let sz = attr_val(&side, "sz")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(4.0);
    let space_pt = attr_val(&side, "space")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(24.0);
    let color = attr_val(&side, "color").unwrap_or_else(|| "auto".into());
    let hex = color.trim_start_matches('#');
    let color = if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        format!("#{hex}")
    } else {
        "#000000".into() // "auto" and any junk fall back to black, like Word
    };

    Some(PageBorder {
        color,
        width: (sz / 8.0 * 96.0 / 72.0).max(1.0).round(), // 1/8 pt -> px
        style: style.into(),
        margin: (space_pt.clamp(0.0, 31.0) * 96.0 / 72.0).round(), // pt -> px
        offset_from: match attr_val(&pb, "offsetFrom").as_deref() {
            Some("page") => "page",
            _ => "text",
        }
        .into(),
        display: match attr_val(&pb, "display").as_deref() {
            Some("firstPage") => "firstPage",
            Some("notFirstPage") => "notFirstPage",
            _ => "allPages",
        }
        .into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wrap section XML into a minimal `w:document`.
    fn doc_with(body_extra: &str, body_sect: &str) -> String {
        format!(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>{body_extra}{body_sect}</w:body></w:document>"#
        )
    }

    #[test]
    fn sect_pr_change_does_not_override_the_current_layout() {
        // The nested <w:sectPr> of <w:sectPrChange> holds the layout BEFORE the
        // tracked change and comes after its parent in document order.
        let xml = doc_with(
            "",
            r#"<w:sectPr>
                 <w:pgSz w:w="11906" w:h="16838"/>
                 <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/>
                 <w:sectPrChange w:id="1" w:author="a">
                   <w:sectPr>
                     <w:pgSz w:w="12240" w:h="15840" w:orient="landscape"/>
                     <w:pgMar w:top="2880" w:right="2880" w:bottom="2880" w:left="2880"/>
                   </w:sectPr>
                 </w:sectPrChange>
               </w:sectPr>"#,
        );
        let s = parse_section_props(&xml);
        assert_eq!(s.paper, "a4");
        assert_eq!(s.orientation, "portrait");
        assert_eq!(s.margin_top, 96.0);
        assert_eq!(parse_all_sections(&xml).len(), 1);
    }

    #[test]
    fn a5_is_not_mistaken_for_a4() {
        let xml = doc_with("", r#"<w:sectPr><w:pgSz w:w="8391" w:h="11906"/></w:sectPr>"#);
        assert_eq!(parse_section_props(&xml).paper, "a5");
        let xml = doc_with("", r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr>"#);
        assert_eq!(parse_section_props(&xml).paper, "a4");
        // Landscape A5: the couple is orientation independent.
        let xml = doc_with(
            "",
            r#"<w:sectPr><w:pgSz w:w="11906" w:h="8391" w:orient="landscape"/></w:sectPr>"#,
        );
        let s = parse_section_props(&xml);
        assert_eq!(s.paper, "a5");
        assert_eq!(s.orientation, "landscape");
        // Letter and legal share their width, only the couple separates them.
        let xml = doc_with("", r#"<w:sectPr><w:pgSz w:w="12240" w:h="20160"/></w:sectPr>"#);
        assert_eq!(parse_section_props(&xml).paper, "legal");
    }

    #[test]
    fn out_of_range_twips_are_ignored_and_negative_margins_flipped() {
        let xml = doc_with(
            "",
            r#"<w:sectPr><w:pgMar w:top="-1440" w:right="60000" w:bottom="-720" w:left="1440"/></w:sectPr>"#,
        );
        let s = parse_section_props(&xml);
        assert_eq!(s.margin_top, 96.0); // abs(-1440 twips) = 96 px
        assert_eq!(s.margin_right, 0.0); // >= 0x8000: Word renders 0
        assert_eq!(s.margin_bottom, 48.0);
        assert_eq!(s.margin_left, 96.0);
    }

    #[test]
    fn columns_widths_need_equal_width_zero_and_a_matching_count() {
        let cols = |attrs: &str, inner: &str| {
            doc_with("", &format!(r#"<w:sectPr><w:cols {attrs}>{inner}</w:cols></w:sectPr>"#))
        };
        // equalWidth defaults to true -> even spread, widths dropped.
        let s = parse_section_props(&cols(
            r#"w:num="2" w:space="720""#,
            r#"<w:col w:w="4000"/><w:col w:w="4000"/>"#,
        ));
        assert_eq!(s.columns.count, 2);
        assert_eq!(s.columns.space, 48.0);
        assert!(s.columns.equal_width && s.columns.widths.is_empty());
        // equalWidth=0 with the right number of <w:col> -> widths honoured.
        let s = parse_section_props(&cols(
            r#"w:num="2" w:space="720" w:equalWidth="0" w:sep="1""#,
            r#"<w:col w:w="3000" w:space="720"/><w:col w:w="5000"/>"#,
        ));
        assert!(!s.columns.equal_width);
        assert_eq!(s.columns.widths, vec![200.0, 333.0]);
        assert_eq!(s.columns.spaces, vec![48.0, 48.0]);
        assert!(s.columns.sep);
        // equalWidth=0 but a mismatched count -> even spread (PropertyMap.cxx:849).
        let s = parse_section_props(&cols(r#"w:num="3" w:equalWidth="0""#, r#"<w:col w:w="3000"/>"#));
        assert_eq!(s.columns.count, 3);
        assert!(s.columns.equal_width && s.columns.widths.is_empty());
    }

    #[test]
    fn page_numbering_line_numbering_and_borders() {
        let xml = doc_with(
            "",
            r#"<w:sectPr>
                 <w:pgNumType w:start="5" w:fmt="lowerRoman"/>
                 <w:lnNumType w:countBy="2" w:start="0" w:distance="360" w:restart="newPage"/>
                 <w:pgBorders w:offsetFrom="page" w:display="firstPage">
                   <w:top w:val="none"/>
                   <w:left w:val="double" w:sz="24" w:space="24" w:color="FF0000"/>
                 </w:pgBorders>
               </w:sectPr>"#,
        );
        let s = parse_section_props(&xml);
        assert_eq!(s.page_num_start, Some(5));
        assert_eq!(s.page_num_fmt, "roman-lower");
        let ln = s.line_numbers.expect("lnNumType");
        assert_eq!(ln.count_by, 2);
        assert_eq!(ln.start, 1); // Word 0-based -> our 1-based
        assert_eq!(ln.distance, 24.0);
        assert_eq!(ln.restart, "newPage");
        let b = s.page_border.expect("pgBorders");
        assert_eq!(b.style, "double"); // the `none` top side is skipped
        assert_eq!(b.color, "#FF0000");
        assert_eq!(b.width, 4.0); // 24/8 pt = 3 pt = 4 px
        assert_eq!(b.margin, 32.0); // 24 pt = 32 px
        assert_eq!(b.offset_from, "page");
        assert_eq!(b.display, "firstPage");
        // countBy="0" disables line numbering.
        let xml = doc_with("", r#"<w:sectPr><w:lnNumType w:countBy="0"/></w:sectPr>"#);
        assert!(parse_section_props(&xml).line_numbers.is_none());
    }

    #[test]
    fn every_section_is_read_in_document_order() {
        let xml = doc_with(
            r#"<w:p><w:pPr><w:sectPr>
                 <w:pgSz w:w="11906" w:h="16838"/>
               </w:sectPr></w:pPr></w:p>
               <w:p><w:pPr><w:sectPr>
                 <w:type w:val="continuous"/>
                 <w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/>
                 <w:cols w:num="2"/>
               </w:sectPr></w:pPr></w:p>"#,
            r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:titlePg/></w:sectPr>"#,
        );
        let secs = parse_all_sections(&xml);
        assert_eq!(secs.len(), 3);
        assert_eq!(secs[0].paper, "a4");
        assert_eq!(secs[1].orientation, "landscape");
        assert_eq!(secs[1].section_start, "continuous");
        assert_eq!(secs[1].columns.count, 2);
        assert_eq!(secs[2].paper, "letter");
        assert!(secs[2].title_pg);
        // The facade keeps returning the body section.
        assert_eq!(parse_section_props(&xml).paper, "letter");
    }

    #[test]
    fn header_and_footer_references_are_kept_per_type() {
        let xml = doc_with(
            "",
            r#"<w:sectPr xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
                 <w:headerReference w:type="default" r:id="rId4"/>
                 <w:headerReference w:type="first" r:id="rId5"/>
                 <w:footerReference r:id="rId6"/>
               </w:sectPr>"#,
        );
        let s = parse_section_props(&xml);
        assert_eq!(s.header_refs.get("default").map(String::as_str), Some("rId4"));
        assert_eq!(s.header_refs.get("first").map(String::as_str), Some("rId5"));
        assert_eq!(s.footer_refs.get("default").map(String::as_str), Some("rId6"));
    }

    /// `word/document.xml` of the first corpus file with this basename.
    /// `KUBUNO_DOCX_CORPUS` points at a directory of real `.docx` (typically a
    /// LibreOffice checkout); the test is skipped when it is unset.
    fn corpus_document_xml(basename: &str) -> Option<String> {
        use std::io::Read;
        let root = std::env::var("KUBUNO_DOCX_CORPUS").ok()?;
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.file_name().is_some_and(|f| f == basename) {
                    let bytes = std::fs::read(&p).ok()?;
                    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
                    let mut entry = zip.by_name("word/document.xml").ok()?;
                    let mut xml = String::new();
                    entry.read_to_string(&mut xml).ok()?;
                    return Some(xml);
                }
            }
        }
        None
    }

    /// Real Word files from the LibreOffice corpus, one per fixed bug.
    #[test]
    fn corpus_files_prove_the_fixes() {
        // n830205.docx: 8 body sections, one of them carrying a
        // <w:sectPrChange> whose nested <w:sectPr> says `cols num="2"`
        // (equalWidth defaulting to true) while the live one says `num="1"`.
        if let Some(xml) = corpus_document_xml("n830205.docx") {
            let secs = parse_all_sections(&xml);
            assert_eq!(secs.len(), 8, "the revision sectPr must not become a section");
            assert_eq!(secs[2].columns.count, 1, "revision layout leaked into section 3");
            // Section 2 has genuine uneven columns: 2 x 4320 twips.
            assert!(!secs[1].columns.equal_width);
            assert_eq!(secs[1].columns.widths, vec![288.0, 288.0]);
            assert_eq!(secs[1].section_start, "continuous");
            // Body section: header reference + first page number.
            let last = secs.last().expect("body section");
            assert_eq!(last.paper, "letter");
            assert_eq!(last.page_num_start, Some(1));
            assert_eq!(last.header_refs.get("default").map(String::as_str), Some("rId11"));
        }
        // tdf119952_negativeMargins.docx: top="-1134" bottom="-1247", plus
        // first/even header and footer references.
        if let Some(xml) = corpus_document_xml("tdf119952_negativeMargins.docx") {
            let s = parse_section_props(&xml);
            assert_eq!(s.margin_top, 76.0);
            assert_eq!(s.margin_bottom, 83.0);
            assert!(s.title_pg);
            assert_eq!(s.header_refs.len(), 3); // default + even + first
            assert_eq!(s.footer_refs.get("first").map(String::as_str), Some("rId12"));
        }
        // tdf46940_dontEquallyDistributeColumns.docx: `equalWidth="true"` and
        // `sep="true"` spelled as words, and a landscape section.
        if let Some(xml) = corpus_document_xml("tdf46940_dontEquallyDistributeColumns.docx") {
            let secs = parse_all_sections(&xml);
            let two = secs.iter().find(|s| s.columns.count == 2).expect("2-column section");
            assert!(two.columns.equal_width && two.columns.widths.is_empty());
            assert!(two.columns.sep);
            assert!(secs.iter().any(|s| s.orientation == "landscape"));
        }
    }

    #[test]
    fn a_document_without_any_sect_pr_falls_back_to_the_defaults() {
        let s = parse_section_props("<w:document><w:body/></w:document>");
        assert_eq!(s.paper, "a4");
        assert!(!s.is_custom());
        assert_eq!(parse_all_sections("pas du XML").len(), 1);
    }
}
