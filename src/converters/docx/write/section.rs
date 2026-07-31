//! `<w:sectPr>` — page geometry at the end of the body.

use crate::converters::docx::model::{LineNumbering, PageBorder, SectionColumns, SectionInfo};
use crate::converters::docx::xml::escape_xml_attr;

/// px -> twips (1440 twips per inch, 96 px per inch).
fn tw(px: f64) -> i64 {
    (px * 15.0).round() as i64
}

/// Page size in twips for a paper name, already swapped for landscape.
/// Values are the ones Word writes; LibreOffice recognises a paper by the
/// (short side, long side) couple (PaperInfo::sloppyFitPageDimension,
/// writerfilter DomainMapper.cxx:2853).
fn paper_twips(paper: &str, landscape: bool) -> (i64, i64) {
    let (w, h): (i64, i64) = match paper {
        "a3" => (16838, 23811),
        "a5" => (8391, 11906),
        "letter" => (12240, 15840),
        "legal" => (12240, 20160),
        _ => (11906, 16838), // a4
    };
    if landscape {
        (h, w)
    } else {
        (w, h)
    }
}

/// `<w:pgSz>` — paper size and orientation.
fn render_pg_sz(s: &SectionInfo) -> String {
    let landscape = s.orientation == "landscape";
    let (w, h) = paper_twips(&s.paper, landscape);
    let orient = if landscape {
        r#" w:orient="landscape""#
    } else {
        ""
    };
    format!(r#"<w:pgSz w:w="{w}" w:h="{h}"{orient}/>"#)
}

/// `<w:pgMar>` — every distance in twips. `header`/`footer` are measured from
/// the page edge (NOT from the body), `gutter` is the binding allowance.
fn render_pg_mar(s: &SectionInfo) -> String {
    format!(
        r#"<w:pgMar w:top="{}" w:right="{}" w:bottom="{}" w:left="{}" w:header="{}" w:footer="{}" w:gutter="{}"/>"#,
        tw(s.margin_top),
        tw(s.margin_right),
        tw(s.margin_bottom),
        tw(s.margin_left),
        tw(s.header_dist),
        tw(s.footer_dist),
        tw(s.gutter.max(0.0)),
    )
}

/// `<w:pgBorders>` — page frame. `w:sz` is in EIGHTHS of a point and `w:space`
/// in points, capped at 31 by Word (PageBordersHandler.cxx:67, SetBorderDistance
/// PropertyMap.cxx:736). One style for the four sides, which is what our model
/// carries.
fn render_pg_borders(b: &PageBorder) -> String {
    let val = match b.style.as_str() {
        "dashed" => "dashed",
        "dotted" => "dotted",
        "double" => "double",
        _ => "single",
    };
    // px -> pt -> eighths of a point; Word accepts 2..96 (1/4 pt .. 12 pt).
    let sz = ((b.width * 72.0 / 96.0 * 8.0).round() as i64).clamp(2, 96);
    let space = ((b.margin * 72.0 / 96.0).round() as i64).clamp(0, 31);
    let color = escape_xml_attr(b.color.trim_start_matches('#'));
    let side = |tag: &str| {
        format!(r#"<w:{tag} w:val="{val}" w:sz="{sz}" w:space="{space}" w:color="{color}"/>"#)
    };
    format!(
        r#"<w:pgBorders w:offsetFrom="{}" w:display="{}">{}{}{}{}</w:pgBorders>"#,
        escape_xml_attr(&b.offset_from),
        escape_xml_attr(&b.display),
        side("top"),
        side("left"),
        side("bottom"),
        side("right"),
    )
}

/// `<w:lnNumType>` — line numbering. Word counts from 0 while our model is
/// 1-based, hence the -1 (docxattributeoutput.cxx:7560).
fn render_ln_num_type(l: &LineNumbering) -> String {
    if l.count_by <= 0 {
        return String::new(); // countBy = 0 means "disabled"
    }
    let mut a = format!(
        r#" w:countBy="{}" w:restart="{}""#,
        l.count_by,
        escape_xml_attr(&l.restart)
    );
    if l.distance > 0.0 {
        a.push_str(&format!(r#" w:distance="{}""#, tw(l.distance)));
    }
    if l.start > 1 {
        a.push_str(&format!(r#" w:start="{}""#, l.start - 1));
    }
    format!("<w:lnNumType{a}/>")
}

/// `<w:pgNumType>` — page numbering restart value and format.
fn render_pg_num_type(start: Option<i32>, fmt: &str) -> String {
    if start.is_none() && (fmt.is_empty() || fmt == "arabic") {
        return String::new();
    }
    let word_fmt = match fmt {
        "roman-upper" => "upperRoman",
        "roman-lower" => "lowerRoman",
        "alpha-upper" => "upperLetter",
        "alpha-lower" => "lowerLetter",
        _ => "decimal",
    };
    let start_attr = start
        .map(|n| format!(r#" w:start="{n}""#))
        .unwrap_or_default();
    format!(r#"<w:pgNumType{start_attr} w:fmt="{word_fmt}"/>"#)
}

/// `<w:cols>` — column count, gutter, separator line and, when the widths are
/// unequal, one `<w:col>` per column. Individual widths are honoured only when
/// `equalWidth="0"` AND there are exactly `num` children (PropertyMap.cxx:849);
/// the `w:space` of the LAST `<w:col>` is omitted (docxattributeoutput.cxx:10627).
fn render_cols(c: &SectionColumns) -> String {
    if c.count <= 1 {
        return String::new(); // num == 1 means "no columns" for Word
    }
    let sep = if c.sep { "1" } else { "0" };
    let custom = !c.equal_width && c.widths.len() == c.count as usize;
    if !custom {
        return format!(
            r#"<w:cols w:num="{}" w:space="{}" w:equalWidth="1" w:sep="{sep}"/>"#,
            c.count,
            tw(c.space)
        );
    }
    let last = c.widths.len() - 1;
    let mut inner = String::new();
    for (i, w) in c.widths.iter().enumerate() {
        if i == last {
            inner.push_str(&format!(r#"<w:col w:w="{}"/>"#, tw(*w)));
        } else {
            let sp = c.spaces.get(i).copied().unwrap_or(c.space);
            inner.push_str(&format!(
                r#"<w:col w:w="{}" w:space="{}"/>"#,
                tw(*w),
                tw(sp)
            ));
        }
    }
    format!(
        r#"<w:cols w:num="{}" w:equalWidth="0" w:sep="{sep}">{inner}</w:cols>"#,
        c.count
    )
}

/// Serialise a section. The child ORDER is normative: LibreOffice reorders its
/// buffer explicitly in DocxAttributeOutput::StartSection
/// (sw/source/filter/ww8/docxattributeoutput.cxx:7452) —
/// type -> pgSz -> pgMar -> pgBorders -> lnNumType -> pgNumType -> cols ->
/// vAlign -> titlePg -> bidi. Word ignores (or refuses) any other order.
// Not wired to the pipeline yet (WIP docx writer) — kept for the upcoming
// section serialisation call sites.
#[allow(dead_code)]
pub(crate) fn render_sect_pr(s: &SectionInfo) -> String {
    render_sect_pr_with(s, "")
}

/// Same, with the header/footer references. They come FIRST inside `sectPr`:
/// that is their position in the normative child order.
pub(crate) fn render_sect_pr_with(s: &SectionInfo, refs: &str) -> String {
    let mut x = String::from("<w:sectPr>");
    x.push_str(refs);

    // `nextPage` is the default; writing it back is noise.
    if s.section_start != "nextPage" && !s.section_start.is_empty() {
        x.push_str(&format!(
            r#"<w:type w:val="{}"/>"#,
            escape_xml_attr(&s.section_start)
        ));
    }

    x.push_str(&render_pg_sz(s));
    x.push_str(&render_pg_mar(s));

    if let Some(b) = &s.page_border {
        x.push_str(&render_pg_borders(b));
    }
    if let Some(l) = &s.line_numbers {
        x.push_str(&render_ln_num_type(l));
    }
    x.push_str(&render_pg_num_type(s.page_num_start, &s.page_num_fmt));
    x.push_str(&render_cols(&s.columns));

    if s.v_align != "top" && !s.v_align.is_empty() {
        x.push_str(&format!(
            r#"<w:vAlign w:val="{}"/>"#,
            escape_xml_attr(&s.v_align)
        ));
    }
    if s.title_pg {
        x.push_str("<w:titlePg/>");
    }
    if s.bidi {
        x.push_str("<w:bidi/>");
    }

    x.push_str("</w:sectPr>");
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a4_portrait_is_the_default() {
        let x = render_sect_pr(&SectionInfo::default());
        assert!(x.contains(r#"<w:pgSz w:w="11906" w:h="16838"/>"#));
        assert!(x.contains(r#"w:top="1440""#));
        assert!(!x.contains("<w:type"));
        assert!(!x.contains("<w:cols"));
    }

    #[test]
    fn landscape_swaps_the_page_dimensions() {
        let s = SectionInfo {
            orientation: "landscape".into(),
            paper: "letter".into(),
            ..SectionInfo::default()
        };
        let x = render_sect_pr(&s);
        assert!(x.contains(r#"<w:pgSz w:w="15840" w:h="12240" w:orient="landscape"/>"#));
    }

    #[test]
    fn unequal_columns_omit_the_last_space() {
        let c = SectionColumns {
            count: 3,
            equal_width: false,
            widths: vec![100.0, 200.0, 300.0],
            spaces: vec![24.0, 24.0, 24.0],
            sep: true,
            ..SectionColumns::default()
        };
        let s = SectionInfo {
            columns: c,
            ..SectionInfo::default()
        };
        let x = render_sect_pr(&s);
        assert!(x.contains(r#"<w:cols w:num="3" w:equalWidth="0" w:sep="1">"#));
        assert!(x.contains(r#"<w:col w:w="1500" w:space="360"/>"#));
        assert!(x.contains(r#"<w:col w:w="4500"/></w:cols>"#));
    }

    #[test]
    fn children_follow_the_normative_order() {
        let s = SectionInfo {
            section_start: "continuous".into(),
            v_align: "center".into(),
            title_pg: true,
            bidi: true,
            page_num_start: Some(3),
            line_numbers: Some(LineNumbering {
                count_by: 5,
                start: 1,
                distance: 0.0,
                restart: "newPage".into(),
            }),
            page_border: Some(PageBorder {
                color: "#ff0000".into(),
                width: 2.0,
                style: "solid".into(),
                margin: 24.0,
                offset_from: "text".into(),
                display: "allPages".into(),
            }),
            columns: SectionColumns {
                count: 2,
                ..SectionColumns::default()
            },
            ..SectionInfo::default()
        };
        let x = render_sect_pr(&s);
        let at = |tag: &str| x.find(tag).unwrap_or(usize::MAX);
        assert!(at("<w:type") < at("<w:pgSz"));
        assert!(at("<w:pgSz") < at("<w:pgMar"));
        assert!(at("<w:pgMar") < at("<w:pgBorders"));
        assert!(at("<w:pgBorders") < at("<w:lnNumType"));
        assert!(at("<w:lnNumType") < at("<w:pgNumType"));
        assert!(at("<w:pgNumType") < at("<w:cols"));
        assert!(at("<w:cols") < at("<w:vAlign"));
        assert!(at("<w:vAlign") < at("<w:titlePg"));
        assert!(at("<w:titlePg") < at("<w:bidi"));
    }
}
