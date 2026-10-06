//! `word/styles.xml` — the document style table.
//!
//! Three rules drive this generator:
//!
//! * `<w:docDefaults>` MUST exist. Without it Word silently applies its own
//!   defaults (Calibri 11 pt) to everything we do not style explicitly, so an
//!   exported document never looks like the editor.
//! * The children of `<w:style>` follow the order imposed by `CT_Style`
//!   (ECMA-376 §17.7.4.17, same order as LibreOffice's writer
//!   `docxattributeoutput.cxx:7228-7276`): name, basedOn, next, link,
//!   autoRedefine, uiPriority, semiHidden, unhideWhenUsed, qFormat, locked,
//!   rsid, then pPr and rPr. Word "repairs" — i.e. rewrites — a file whose
//!   children are out of order. `StyleDef` below makes that order structural.
//! * A style without `<w:qFormat/>` does NOT appear in Word's style gallery
//!   (`docxattributeoutput.cxx:7259-7273`) — the usual cause of the "my styles
//!   disappeared" report.
//!
//! `w:name` of a BUILT-IN style is its canonical lowercase English name
//! (`heading 1`, `footnote text`, `toc 1` — see LibreOffice
//! `sw/source/filter/ww8/styles.cxx:30-125`) whatever the UI language: every
//! reader keys the semantics off the NAME, whereas `w:styleId` is only a
//! reference key for `pStyle`/`rStyle`/`basedOn`/`next`/`link` and may be
//! localised. Our own additions carry `w:customStyle="1"` and a free-form name.
//!
//! Every style referenced elsewhere by the exporter must be declared here, or
//! the reference dangles: `Normal`, `Heading1..6` (`write/document.rs`),
//! `Quote` (blockquotes), `Code`/`CodeChar` (code blocks and inline code),
//! `Hyperlink` (links), `ListParagraph` (`write/list.rs`).

/// Body font of the default paragraph, mirroring the editor's `Normal` style
/// (Arial 11 pt, 1.15 line spacing, no space after — `DEFAULT_STYLES` in
/// `DocumentEditorPage.tsx`). Sizes below are in half-points, spacing in twips.
const BODY_FONT: &str = "Arial";
/// 11 pt, in half-points.
const BODY_SZ: u32 = 22;
/// 1.15 line spacing: `w:line` is 240 × the multiplier when `w:lineRule="auto"`.
const BODY_LINE: u32 = 276;
/// Monospaced family used by code blocks and by the inline `code` mark.
const MONO_FONT: &str = "Courier New";

/// Built-in headings: styleId, canonical name, linked character style, its
/// name, size in half-points. Sizes mirror the editor's heading styles.
const HEADINGS: [(&str, &str, &str, &str, u32); 6] = [
    ("Heading1", "heading 1", "Heading1Char", "Heading 1 Char", 48),
    ("Heading2", "heading 2", "Heading2Char", "Heading 2 Char", 36),
    ("Heading3", "heading 3", "Heading3Char", "Heading 3 Char", 28),
    ("Heading4", "heading 4", "Heading4Char", "Heading 4 Char", 26),
    ("Heading5", "heading 5", "Heading5Char", "Heading 5 Char", 24),
    ("Heading6", "heading 6", "Heading6Char", "Heading 6 Char", 24),
];

/// Table-of-contents entry styles: styleId and canonical lowercase name.
const TOC_LEVELS: [(&str, &str); 9] = [
    ("TOC1", "toc 1"),
    ("TOC2", "toc 2"),
    ("TOC3", "toc 3"),
    ("TOC4", "toc 4"),
    ("TOC5", "toc 5"),
    ("TOC6", "toc 6"),
    ("TOC7", "toc 7"),
    ("TOC8", "toc 8"),
    ("TOC9", "toc 9"),
];

/// One `<w:style>`. The fields are declared — and rendered — in the exact order
/// `CT_Style` mandates for its children, so a malformed style cannot be built.
struct StyleDef {
    /// `w:type`: `paragraph`, `character`, `table` or `numbering`.
    kind: &'static str,
    /// `w:styleId`, the key used by `pStyle`/`rStyle`/`basedOn`/`next`/`link`.
    id: &'static str,
    /// `w:default="1"`: style applied when nothing else is referenced. At most
    /// one per type; "the last instance shall be used" if several.
    default: bool,
    /// `w:customStyle="1"`: not a Word built-in, so `name` is free-form.
    custom: bool,
    /// `w:name` — canonical lowercase English name for built-ins.
    name: &'static str,
    based_on: Option<&'static str>,
    next: Option<&'static str>,
    /// `w:link`: paragraph/character pair. Both sides must point at each other.
    link: Option<&'static str>,
    auto_redefine: bool,
    ui_priority: Option<u32>,
    semi_hidden: bool,
    unhide_when_used: bool,
    /// Gallery visibility. See the module comment.
    q_format: bool,
    /// Inner XML of `<w:pPr>`, without the wrapper; empty omits the element.
    /// Child order there is `CT_PPrBase` — `w:outlineLvl` comes last.
    ppr: String,
    /// Inner XML of `<w:rPr>`, without the wrapper; empty omits the element.
    /// Child order there is `CT_RPr`: rFonts, b, i, color, sz, szCs, u, vertAlign.
    rpr: String,
}

impl StyleDef {
    fn new(kind: &'static str, id: &'static str, name: &'static str) -> Self {
        Self {
            kind,
            id,
            default: false,
            custom: false,
            name,
            based_on: None,
            next: None,
            link: None,
            auto_redefine: false,
            ui_priority: None,
            semi_hidden: false,
            unhide_when_used: false,
            q_format: false,
            ppr: String::new(),
            rpr: String::new(),
        }
    }

    fn para(id: &'static str, name: &'static str) -> Self {
        Self::new("paragraph", id, name)
    }

    fn chr(id: &'static str, name: &'static str) -> Self {
        Self::new("character", id, name)
    }

    fn default_style(mut self) -> Self {
        self.default = true;
        self
    }

    fn custom_style(mut self) -> Self {
        self.custom = true;
        self
    }

    fn based_on(mut self, v: &'static str) -> Self {
        self.based_on = Some(v);
        self
    }

    fn next_style(mut self, v: &'static str) -> Self {
        self.next = Some(v);
        self
    }

    fn link_to(mut self, v: &'static str) -> Self {
        self.link = Some(v);
        self
    }

    fn auto_redefine(mut self) -> Self {
        self.auto_redefine = true;
        self
    }

    fn ui(mut self, v: u32) -> Self {
        self.ui_priority = Some(v);
        self
    }

    /// `semiHidden` + `unhideWhenUsed`: hidden from the gallery until the
    /// document actually uses the style (footnotes, comments, TOC…).
    fn hidden_until_used(mut self) -> Self {
        self.semi_hidden = true;
        self.unhide_when_used = true;
        self
    }

    fn unhide_when_used(mut self) -> Self {
        self.unhide_when_used = true;
        self
    }

    /// `semiHidden` alone: Word marks the character half of a linked pair this
    /// way — it is never picked from the gallery, only through its paragraph
    /// style.
    fn semi_hidden_only(mut self) -> Self {
        self.semi_hidden = true;
        self
    }

    fn qformat(mut self) -> Self {
        self.q_format = true;
        self
    }

    fn ppr(mut self, v: impl Into<String>) -> Self {
        self.ppr = v.into();
        self
    }

    fn rpr(mut self, v: impl Into<String>) -> Self {
        self.rpr = v.into();
        self
    }

    fn render(&self, out: &mut String) {
        out.push_str(&format!(r#"  <w:style w:type="{}""#, self.kind));
        if self.default {
            out.push_str(r#" w:default="1""#);
        }
        if self.custom {
            out.push_str(r#" w:customStyle="1""#);
        }
        out.push_str(&format!(
            " w:styleId=\"{}\">\n    <w:name w:val=\"{}\"/>",
            self.id, self.name
        ));
        if let Some(v) = self.based_on {
            out.push_str(&format!(r#"<w:basedOn w:val="{v}"/>"#));
        }
        if let Some(v) = self.next {
            out.push_str(&format!(r#"<w:next w:val="{v}"/>"#));
        }
        if let Some(v) = self.link {
            out.push_str(&format!(r#"<w:link w:val="{v}"/>"#));
        }
        if self.auto_redefine {
            out.push_str("<w:autoRedefine/>");
        }
        if let Some(v) = self.ui_priority {
            out.push_str(&format!(r#"<w:uiPriority w:val="{v}"/>"#));
        }
        if self.semi_hidden {
            out.push_str("<w:semiHidden/>");
        }
        if self.unhide_when_used {
            out.push_str("<w:unhideWhenUsed/>");
        }
        if self.q_format {
            out.push_str("<w:qFormat/>");
        }
        if !self.ppr.is_empty() {
            out.push_str(&format!("\n    <w:pPr>{}</w:pPr>", self.ppr));
        }
        if !self.rpr.is_empty() {
            out.push_str(&format!("\n    <w:rPr>{}</w:rPr>", self.rpr));
        }
        out.push_str("\n  </w:style>\n");
    }
}

/// `<w:docDefaults>`: the weakest formatting layer, applied to anything the
/// styles and the direct formatting leave undefined. Word falls back to its own
/// Calibri 11 pt when this element is missing.
fn doc_defaults() -> String {
    format!(
        r#"  <w:docDefaults>
    <w:rPrDefault>
      <w:rPr>
        <w:rFonts w:ascii="{BODY_FONT}" w:hAnsi="{BODY_FONT}" w:eastAsia="{BODY_FONT}" w:cs="{BODY_FONT}"/>
        <w:sz w:val="{BODY_SZ}"/><w:szCs w:val="{BODY_SZ}"/>
      </w:rPr>
    </w:rPrDefault>
    <w:pPrDefault>
      <w:pPr>
        <w:spacing w:after="0" w:line="{BODY_LINE}" w:lineRule="auto"/>
      </w:pPr>
    </w:pPrDefault>
  </w:docDefaults>
"#
    )
}

/// The whole style table, in the order Word itself writes it: the four
/// `w:default` styles first, then the gallery styles.
fn styles() -> Vec<StyleDef> {
    let mut v = vec![
        // The default of each type. `TableNormal` and `NoList` carry no
        // formatting on purpose: they are the "no table style" / "no numbering"
        // markers every Word document declares, not inert leftovers.
        StyleDef::para("Normal", "Normal").default_style().qformat(),
        StyleDef::chr("DefaultParagraphFont", "Default Paragraph Font")
            .default_style()
            .ui(1)
            .hidden_until_used(),
        StyleDef::new("table", "TableNormal", "Normal Table")
            .default_style()
            .ui(99)
            .hidden_until_used(),
        StyleDef::new("numbering", "NoList", "No List")
            .default_style()
            .ui(99)
            .hidden_until_used(),
    ];

    // Headings. `outlineLvl` is what feeds Word's navigation pane and its table
    // of contents; `link` exposes the matching character style so a heading can
    // be applied to part of a line.
    for (lvl, (id, name, char_id, char_name, sz)) in HEADINGS.iter().enumerate() {
        let rpr = format!(r#"<w:b/><w:sz w:val="{sz}"/><w:szCs w:val="{sz}"/>"#);
        v.push(
            StyleDef::para(id, name)
                .based_on("Normal")
                .next_style("Normal")
                .link_to(char_id)
                .ui(9)
                .qformat()
                .ppr(format!(
                    r#"<w:keepNext/><w:keepLines/><w:spacing w:before="240" w:after="60"/><w:outlineLvl w:val="{lvl}"/>"#
                ))
                .rpr(rpr.as_str()),
        );
        // "Heading N Char" is not one of Word's built-in NAMES, so the linked
        // character half must be flagged `w:customStyle="1"` — that is exactly
        // what Word itself writes for these pairs.
        v.push(
            StyleDef::chr(char_id, char_name)
                .custom_style()
                .link_to(id)
                .ui(9)
                .rpr(rpr.as_str()),
        );
    }

    v.push(
        StyleDef::para("Title", "Title")
            .based_on("Normal")
            .next_style("Normal")
            .ui(10)
            .qformat()
            .ppr(r#"<w:spacing w:before="60" w:after="90"/>"#)
            .rpr(r#"<w:color w:val="202124"/><w:sz w:val="56"/><w:szCs w:val="56"/>"#),
    );
    v.push(
        StyleDef::para("Subtitle", "Subtitle")
            .based_on("Normal")
            .next_style("Normal")
            .ui(11)
            .qformat()
            .ppr(r#"<w:spacing w:after="180"/>"#)
            .rpr(r#"<w:i/><w:color w:val="5F6368"/><w:sz w:val="30"/><w:szCs w:val="30"/>"#),
    );
    v.push(
        StyleDef::para("NoSpacing", "No Spacing")
            .based_on("Normal")
            .ui(1)
            .qformat()
            .ppr(r#"<w:spacing w:after="0" w:line="240" w:lineRule="auto"/>"#),
    );
    v.push(
        StyleDef::para("Quote", "Quote")
            .based_on("Normal")
            .next_style("Normal")
            .ui(29)
            .qformat()
            .ppr(r#"<w:spacing w:before="120" w:after="120"/><w:ind w:left="720"/>"#)
            .rpr(r#"<w:i/><w:color w:val="595959"/>"#),
    );
    // Applied to every list paragraph by `write/list.rs`. No `w:ind` here: the
    // indent of each level comes from `word/numbering.xml`, and duplicating it
    // would shift the list one extra step to the right.
    v.push(
        StyleDef::para("ListParagraph", "List Paragraph")
            .based_on("Normal")
            .ui(34)
            .qformat()
            .ppr("<w:contextualSpacing/>"),
    );

    // Code block and its linked character style — the one `write/run.rs`
    // references for the inline `code` mark. Not Word built-ins, hence
    // `w:customStyle="1"`.
    let mono = format!(
        r#"<w:rFonts w:ascii="{MONO_FONT}" w:hAnsi="{MONO_FONT}" w:cs="{MONO_FONT}"/><w:sz w:val="20"/><w:szCs w:val="20"/>"#
    );
    v.push(
        StyleDef::para("Code", "Code")
            .custom_style()
            .based_on("Normal")
            .next_style("Normal")
            .link_to("CodeChar")
            .ui(35)
            .qformat()
            .ppr(r#"<w:shd w:val="clear" w:color="auto" w:fill="F5F5F5"/><w:spacing w:after="0" w:line="240" w:lineRule="auto"/>"#)
            .rpr(mono.as_str()),
    );
    v.push(
        StyleDef::chr("CodeChar", "Code Char")
            .custom_style()
            .link_to("Code")
            .ui(35)
            .rpr(mono.as_str()),
    );
    v.push(
        StyleDef::chr("Hyperlink", "Hyperlink")
            .ui(99)
            .unhide_when_used()
            .rpr(r#"<w:color w:val="1155CC"/><w:u w:val="single"/>"#),
    );

    // Footnotes and comments: declared so the references the exporter may emit
    // never dangle. Both text styles are linked to their character style, the
    // way Word writes them.
    v.push(
        StyleDef::para("FootnoteText", "footnote text")
            .based_on("Normal")
            .link_to("FootnoteTextChar")
            .ui(99)
            .hidden_until_used()
            .ppr(r#"<w:spacing w:after="0" w:line="240" w:lineRule="auto"/>"#)
            .rpr(r#"<w:sz w:val="20"/><w:szCs w:val="20"/>"#),
    );
    v.push(
        StyleDef::chr("FootnoteTextChar", "Footnote Text Char")
            .custom_style()
            .based_on("DefaultParagraphFont")
            .link_to("FootnoteText")
            .ui(99)
            .semi_hidden_only()
            .rpr(r#"<w:sz w:val="20"/><w:szCs w:val="20"/>"#),
    );
    v.push(
        StyleDef::chr("FootnoteReference", "footnote reference")
            .based_on("DefaultParagraphFont")
            .ui(99)
            .hidden_until_used()
            .rpr(r#"<w:vertAlign w:val="superscript"/>"#),
    );
    // Endnotes: same pair as footnotes. `endnotes.xml` references these two, and
    // a dangling style reference is what makes Word fall back to Normal.
    v.push(
        StyleDef::chr("EndnoteReference", "endnote reference")
            .based_on("DefaultParagraphFont")
            .ui(99)
            .hidden_until_used()
            .rpr(r#"<w:vertAlign w:val="superscript"/>"#),
    );
    v.push(
        StyleDef::para("EndnoteText", "endnote text")
            .based_on("Normal")
            .ui(99)
            .hidden_until_used()
            .ppr(r#"<w:spacing w:after="0" w:line="240" w:lineRule="auto"/>"#)
            .rpr(r#"<w:sz w:val="20"/><w:szCs w:val="20"/>"#),
    );
    v.push(
        StyleDef::para("CommentText", "annotation text")
            .based_on("Normal")
            .link_to("CommentTextChar")
            .ui(99)
            .hidden_until_used()
            .ppr(r#"<w:spacing w:after="0" w:line="240" w:lineRule="auto"/>"#)
            .rpr(r#"<w:sz w:val="20"/><w:szCs w:val="20"/>"#),
    );
    v.push(
        StyleDef::chr("CommentTextChar", "Comment Text Char")
            .custom_style()
            .based_on("DefaultParagraphFont")
            .link_to("CommentText")
            .ui(99)
            .semi_hidden_only()
            .rpr(r#"<w:sz w:val="20"/><w:szCs w:val="20"/>"#),
    );
    v.push(
        StyleDef::chr("CommentReference", "annotation reference")
            .based_on("DefaultParagraphFont")
            .ui(99)
            .hidden_until_used()
            .rpr(r#"<w:sz w:val="16"/><w:szCs w:val="16"/>"#),
    );

    // Table of contents. `TOCHeading` is what Word applies to the "Contents"
    // title; `TOC1..9` format the entries, indented one step per level.
    v.push(
        StyleDef::para("TOCHeading", "TOC Heading")
            .based_on("Heading1")
            .next_style("Normal")
            .ui(39)
            .hidden_until_used()
            .qformat()
            .ppr(r#"<w:spacing w:before="240" w:after="0"/><w:outlineLvl w:val="9"/>"#),
    );
    for (i, (id, name)) in TOC_LEVELS.iter().enumerate() {
        let ind = 220 * i as u32;
        let ppr = if ind == 0 {
            r#"<w:spacing w:after="100"/>"#.to_string()
        } else {
            format!(r#"<w:spacing w:after="100"/><w:ind w:left="{ind}"/>"#)
        };
        v.push(
            StyleDef::para(id, name)
                .based_on("Normal")
                .next_style("Normal")
                .auto_redefine()
                .ui(39)
                .unhide_when_used()
                .ppr(ppr),
        );
    }
    v
}

/// `word/styles.xml` for an exported document.
pub(crate) fn build_styles_xml() -> String {
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
"#,
    );
    s.push_str(&doc_defaults());
    for st in styles() {
        st.render(&mut s);
    }
    s.push_str("</w:styles>");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use roxmltree::Document as XmlDoc;
    use std::collections::HashSet;

    /// The order `CT_Style` mandates for the children of `<w:style>`.
    const CHILD_ORDER: [&str; 13] = [
        "name",
        "basedOn",
        "next",
        "link",
        "autoRedefine",
        "uiPriority",
        "semiHidden",
        "unhideWhenUsed",
        "qFormat",
        "locked",
        "rsid",
        "pPr",
        "rPr",
    ];

    #[test]
    fn xml_is_well_formed_and_has_doc_defaults() {
        let xml = build_styles_xml();
        let doc = XmlDoc::parse(&xml).expect("styles.xml must be well-formed XML");
        assert_eq!(doc.root_element().tag_name().name(), "styles");
        let defaults = doc
            .descendants()
            .find(|n| n.tag_name().name() == "docDefaults")
            .expect("docDefaults");
        assert!(defaults
            .descendants()
            .any(|n| n.tag_name().name() == "rPrDefault"));
        assert!(defaults
            .descendants()
            .any(|n| n.tag_name().name() == "pPrDefault"));
    }

    #[test]
    fn every_style_child_follows_the_schema_order() {
        let xml = build_styles_xml();
        let doc = XmlDoc::parse(&xml).expect("well-formed");
        for st in doc.descendants().filter(|n| n.tag_name().name() == "style") {
            let mut last = 0usize;
            for c in st.children().filter(|n| n.is_element()) {
                let name = c.tag_name().name();
                let pos = CHILD_ORDER
                    .iter()
                    .position(|k| *k == name)
                    .unwrap_or_else(|| panic!("unexpected child <w:{name}> in a style"));
                assert!(pos >= last, "child <w:{name}> is out of schema order");
                last = pos;
            }
        }
    }

    #[test]
    fn styles_referenced_by_the_exporter_all_exist() {
        let xml = build_styles_xml();
        let doc = XmlDoc::parse(&xml).expect("well-formed");
        let ids: HashSet<&str> = doc
            .descendants()
            .filter(|n| n.tag_name().name() == "style")
            .filter_map(|n| n.attribute(("http://schemas.openxmlformats.org/wordprocessingml/2006/main", "styleId")))
            .collect();
        for want in [
            "Normal",
            "Heading1",
            "Heading2",
            "Heading3",
            "Heading4",
            "Heading5",
            "Heading6",
            "Quote",
            "Code",
            "CodeChar",
            "Hyperlink",
            "ListParagraph",
            "FootnoteReference",
            "FootnoteText",
            "EndnoteReference",
            "EndnoteText",
            "CommentReference",
            "CommentText",
            "TOCHeading",
            "TOC1",
            "TOC9",
        ] {
            assert!(ids.contains(want), "missing style {want}");
        }
        // basedOn / next / link must never dangle.
        for st in doc.descendants().filter(|n| n.tag_name().name() == "style") {
            for c in st.children().filter(|n| n.is_element()) {
                if matches!(c.tag_name().name(), "basedOn" | "next" | "link") {
                    let v = c
                        .attribute((
                            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
                            "val",
                        ))
                        .unwrap_or_default();
                    assert!(ids.contains(v), "dangling reference to {v}");
                }
            }
        }
    }

    #[test]
    fn headings_are_gallery_visible_and_carry_an_outline_level() {
        let xml = build_styles_xml();
        for lvl in 0..6 {
            assert!(xml.contains(&format!(r#"<w:outlineLvl w:val="{lvl}"/>"#)));
        }
        assert!(xml.contains(r#"<w:name w:val="heading 1"/>"#));
        assert!(!xml.contains(r#"<w:name w:val="Heading 1"/>"#));
        // Inert numbering styles must be gone.
        assert!(!xml.contains("ListBullet"));
        assert!(!xml.contains("ListNumber"));
    }
}
