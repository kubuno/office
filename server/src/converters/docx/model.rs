//! Data shared across the DOCX reader: page layout, font context, block folding.

use std::collections::HashMap;

use serde_json::Value;

use crate::converters::types::PmNode;

/// Section columns (`<w:cols>`). Individual widths are honoured ONLY when
/// `equalWidth="0"` AND there are as many `<w:col>` as `@num` — otherwise Word
/// and Writer both spread the columns evenly (LibreOffice
/// writerfilter SectionPropertyMap::ApplyColumnProperties, PropertyMap.cxx:849).
#[derive(Clone, Debug)]
pub struct SectionColumns {
    pub count: u16,
    /// Default gutter between columns, in px.
    pub space: f64,
    pub equal_width: bool,
    /// Per-column widths in px — empty when `equal_width`.
    pub widths: Vec<f64>,
    /// Per-column gutters in px (gutter to the RIGHT of each column).
    pub spaces: Vec<f64>,
    /// Draw a separator line between columns (`w:sep`).
    pub sep: bool,
}

impl Default for SectionColumns {
    fn default() -> Self {
        // 720 twips = 0.5 inch = 48 px is Word's default (SectionColumnHandler.cxx:31).
        SectionColumns {
            count: 1,
            space: 48.0,
            equal_width: true,
            widths: Vec::new(),
            spaces: Vec::new(),
            sep: false,
        }
    }
}

/// Page border (`<w:pgBorders>`), one style for the four sides — that is all our
/// frontend `PageBorderDef` can render.
#[derive(Clone, Debug)]
pub struct PageBorder {
    /// "#rrggbb".
    pub color: String,
    /// Line width in px (`w:sz` is in eighths of a point).
    pub width: f64,
    /// "solid" | "dashed" | "dotted" | "double".
    pub style: String,
    /// Distance from the page edge or from the text, in px (`w:space`, in points).
    pub margin: f64,
    /// "page" | "text" — changes the meaning of `margin`.
    pub offset_from: String,
    /// "allPages" | "firstPage" | "notFirstPage".
    pub display: String,
}

/// Line numbering (`<w:lnNumType>`).
#[derive(Clone, Debug)]
pub struct LineNumbering {
    /// Number every Nth line; 0 disables numbering (so we store None instead).
    pub count_by: i32,
    /// First number. Word is 0-based, our model is 1-based (PropertyMap.cxx:1710).
    pub start: i32,
    /// Distance from the left margin in px (0 = automatic).
    pub distance: f64,
    /// "newPage" | "newSection" | "continuous".
    pub restart: String,
}

/// Section page layout (margins + orientation + paper size) read from a
/// `<w:sectPr>`. DOCX margins are in twips (1440 = 1 inch = 96 px).
#[derive(Clone)]
pub struct SectionInfo {
    pub margin_top: f64,
    pub margin_right: f64,
    pub margin_bottom: f64,
    pub margin_left: f64,
    pub orientation: String, // "portrait" | "landscape"
    pub paper: String,       // "a4" | "a5" | "a3" | "letter" | "legal"
    pub gutter: f64,         // binding gutter (px)
    pub header_dist: f64,    // header distance from the top edge (px)
    pub footer_dist: f64,    // footer distance from the bottom edge (px)
    pub title_pg: bool,      // "Different first page" (<w:titlePg>)
    pub v_align: String,     // vertical alignment: "top"|"center"|"bottom"|"both"
    pub section_start: String, // "nextPage"|"continuous"|"evenPage"|"oddPage"
    // ── Additions (full page-layout import) ──────────────────────────────────
    /// `<w:cols>` — columns of this section.
    pub columns: SectionColumns,
    /// `<w:pgNumType w:start>` — first page number of the section.
    pub page_num_start: Option<i32>,
    /// `<w:pgNumType w:fmt>` mapped onto the frontend `PageNumFormat` values:
    /// "arabic" | "roman-upper" | "roman-lower" | "alpha-upper" | "alpha-lower".
    pub page_num_fmt: String,
    /// `<w:lnNumType>` — None when absent or disabled (`countBy="0"`).
    pub line_numbers: Option<LineNumbering>,
    /// `<w:pgBorders>` — None when absent or every side is `w:val="none"`.
    pub page_border: Option<PageBorder>,
    /// `<w:bidi>` — right-to-left section.
    pub bidi: bool,
    /// Different header/footer on odd and even pages. In OOXML this is a
    /// DOCUMENT setting (`w:evenAndOddHeaders` in settings.xml), not a section
    /// one, but it is carried here because that is what the writer has in hand.
    pub even_odd_headers: bool,
    /// Track changes is ON when the document opens (`w:trackRevisions` in
    /// settings.xml — NOT `w:trackChanges`, which Word ignores silently).
    /// A document setting, carried here for the same reason as the flag above.
    pub track_changes: bool,
    /// `rId` of the header parts, keyed by type: "default" | "first" | "even".
    pub header_refs: HashMap<String, String>,
    /// `rId` of the footer parts, keyed by type: "default" | "first" | "even".
    pub footer_refs: HashMap<String, String>,
}

impl Default for SectionInfo {
    fn default() -> Self {
        SectionInfo {
            margin_top: 96.0,
            margin_right: 96.0,
            margin_bottom: 96.0,
            margin_left: 96.0,
            orientation: "portrait".into(),
            paper: "a4".into(),
            gutter: 0.0,
            header_dist: 48.0,
            footer_dist: 48.0,
            title_pg: false,
            v_align: "top".into(),
            section_start: "nextPage".into(),
            columns: SectionColumns::default(),
            page_num_start: None,
            page_num_fmt: "arabic".into(),
            line_numbers: None,
            page_border: None,
            bidi: false,
            even_odd_headers: false,
            track_changes: false,
            header_refs: HashMap::new(),
            footer_refs: HashMap::new(),
        }
    }
}

impl SectionInfo {
    /// `true` when the section differs from the defaults (triggers the wrapper).
    pub fn is_custom(&self) -> bool {
        let d = SectionInfo::default();
        (self.margin_top - d.margin_top).abs() > 0.5
            || (self.margin_right - d.margin_right).abs() > 0.5
            || (self.margin_bottom - d.margin_bottom).abs() > 0.5
            || (self.margin_left - d.margin_left).abs() > 0.5
            || self.orientation != d.orientation
            || self.paper != d.paper
            || self.gutter.abs() > 0.5
            || self.title_pg
            || self.v_align != d.v_align
            || self.section_start != d.section_start
            || self.columns.count > 1
            || self.page_num_start.is_some()
            || self.line_numbers.is_some()
            || self.page_border.is_some()
            // Not layout, but carried by the same envelope: a document whose only
            // non-default trait is one of these would otherwise lose it, since a
            // plain body is stored without an envelope at all.
            || self.even_odd_headers
            || self.track_changes
    }
}

/// Fonts resolved for the document: the theme's major/minor faces (`a:fontScheme`)
/// and the default font (`w:docDefaults`). Word often references fonts through the
/// theme (asciiTheme=minorHAnsi = body = Calibri) and body runs carry no `rFonts`
/// (they inherit the default); without this resolution the text falls back to Arial.
pub(crate) struct FontCtx {
    pub(crate) major: String,        // major theme face (headings) — e.g. "Calibri Light"
    pub(crate) minor: String,        // minor theme face (body) — e.g. "Calibri"
    pub(crate) default: Option<String>, // document default font (docDefaults)
    pub(crate) default_size: f64,    // default size in pt (docDefaults sz; Word = 11)
    // Inherited paragraph properties (the way Word does it): docDefaults < named
    // style < direct pPr. `para_default` = `<w:docDefaults><w:pPrDefault>`;
    // `para_styles` = RESOLVED pPr (basedOn chain included) by styleId;
    // `para_style_default` = the style applied when the paragraph has no `<w:pStyle>`.
    pub(crate) para_default: serde_json::Map<String, Value>,
    pub(crate) para_styles: HashMap<String, serde_json::Map<String, Value>>,
    pub(crate) para_style_default: Option<String>,
    // styleId → heading level (resolved through the style NAME, locale-robust).
    pub(crate) heading_levels: HashMap<String, u8>,
    // styleId → resolved RUN properties (bold/italic/size/font/colour of the style).
    pub(crate) run_styles: HashMap<String, serde_json::Map<String, Value>>,
}

/// A body-level block, before consecutive list items are folded into lists.
pub(crate) enum Block {
    Node(PmNode),
    Item { ordered: bool, ilvl: u8, para: PmNode },
}
