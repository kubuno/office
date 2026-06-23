use std::collections::HashMap;
use std::io::{Cursor, Write};

use base64::Engine;
use roxmltree::{Document as XmlDoc, Node};
use serde_json::{json, Value};
use zip::{write::FileOptions, ZipArchive, ZipWriter};

use crate::errors::{OfficeError, Result};
use super::types::{PmMark, PmNode};

// ─── Export: ProseMirror JSON → DOCX ────────────────────────────────────────

pub fn export_docx(doc: &PmNode, title: &str) -> Result<Vec<u8>> {
    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);
    let opts = FileOptions::<()>::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", opts)?;
    zip.write_all(CONTENT_TYPES.as_bytes())?;

    zip.start_file("_rels/.rels", opts)?;
    zip.write_all(RELS.as_bytes())?;

    zip.start_file("word/_rels/document.xml.rels", opts)?;
    zip.write_all(DOC_RELS.as_bytes())?;

    zip.start_file("word/styles.xml", opts)?;
    zip.write_all(STYLES_XML.as_bytes())?;

    zip.start_file("docProps/core.xml", opts)?;
    let escaped_title = escape_xml(title);
    zip.write_all(format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties"
    xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>{escaped_title}</dc:title>
</cp:coreProperties>"#).as_bytes())?;

    zip.start_file("word/document.xml", opts)?;
    let body = render_body(doc);
    let doc_xml = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:wpc="http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas"
    xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<w:body>
{body}
<w:sectPr>
  <w:pgSz w:w="11906" w:h="16838"/>
  <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/>
</w:sectPr>
</w:body>
</w:document>"#);
    zip.write_all(doc_xml.as_bytes())?;

    let result = zip.finish()?;
    Ok(result.into_inner())
}

fn render_body(doc: &PmNode) -> String {
    let mut out = String::new();
    for child in doc.children() {
        out.push_str(&render_block(child));
    }
    if out.is_empty() {
        out.push_str("<w:p/>");
    }
    out
}

fn render_block(node: &PmNode) -> String {
    match node.node_type.as_str() {
        "paragraph" => render_paragraph(node, "Normal"),
        "heading" => {
            let style = match node.heading_level() {
                1 => "Heading1",
                2 => "Heading2",
                3 => "Heading3",
                4 => "Heading4",
                5 => "Heading5",
                _ => "Heading6",
            };
            render_paragraph(node, style)
        }
        "bulletList" | "orderedList" => render_list(node),
        "listItem" => render_list_item(node),
        "blockquote" => {
            let mut out = String::new();
            for child in node.children() {
                out.push_str(&render_paragraph_with_style(child, "Quote"));
            }
            out
        }
        "codeBlock" => render_code_block(node),
        _ => String::new(),
    }
}

fn render_paragraph(node: &PmNode, style: &str) -> String {
    render_paragraph_with_style(node, style)
}

fn render_paragraph_with_style(node: &PmNode, style: &str) -> String {
    let runs = render_inline_content(node.children());
    format!(
        r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr>{runs}</w:p>"#,
        style = style,
        runs = runs
    )
}

fn render_code_block(node: &PmNode) -> String {
    let text: String = node.children().iter().filter_map(|n| n.text.as_deref()).collect::<Vec<_>>().join("\n");
    let mut out = String::new();
    for line in text.split('\n') {
        let escaped = escape_xml(line);
        out.push_str(&format!(
            r#"<w:p><w:pPr><w:pStyle w:val="Code"/></w:pPr><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
            escaped
        ));
    }
    if out.is_empty() {
        out.push_str(r#"<w:p><w:pPr><w:pStyle w:val="Code"/></w:pPr></w:p>"#);
    }
    out
}

fn render_list(node: &PmNode) -> String {
    let num_id = if node.node_type == "bulletList" { "1" } else { "2" };
    let mut out = String::new();
    for item in node.children() {
        out.push_str(&render_list_item_with_num(item, num_id, 0));
    }
    out
}

fn render_list_item(node: &PmNode) -> String {
    render_list_item_with_num(node, "1", 0)
}

fn render_list_item_with_num(node: &PmNode, num_id: &str, ilvl: u8) -> String {
    let mut out = String::new();
    for child in node.children() {
        match child.node_type.as_str() {
            "paragraph" => {
                let runs = render_inline_content(child.children());
                out.push_str(&format!(
                    r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="{ilvl}"/><w:numId w:val="{num_id}"/></w:numPr></w:pPr>{runs}</w:p>"#,
                    ilvl = ilvl,
                    num_id = num_id,
                    runs = runs
                ));
            }
            "bulletList" => {
                for sub in child.children() {
                    out.push_str(&render_list_item_with_num(sub, "1", ilvl + 1));
                }
            }
            "orderedList" => {
                for sub in child.children() {
                    out.push_str(&render_list_item_with_num(sub, "2", ilvl + 1));
                }
            }
            _ => out.push_str(&render_block(child)),
        }
    }
    out
}

fn render_inline_content(nodes: &[PmNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node.node_type.as_str() {
            "text" => {
                if let Some(text) = &node.text {
                    out.push_str(&render_run(text, node.marks.as_deref().unwrap_or(&[])));
                }
            }
            "hardBreak" => {
                out.push_str(r#"<w:r><w:br/></w:r>"#);
            }
            _ => {}
        }
    }
    out
}

fn render_run(text: &str, marks: &[PmMark]) -> String {
    let mut rpr = String::new();
    for mark in marks {
        match mark.mark_type.as_str() {
            "bold"      => rpr.push_str("<w:b/>"),
            "italic"    => rpr.push_str("<w:i/>"),
            "underline" => rpr.push_str("<w:u w:val=\"single\"/>"),
            "strike"    => rpr.push_str("<w:strike/>"),
            "code"      => rpr.push_str("<w:rStyle w:val=\"CodeChar\"/>"),
            "link" => {
                // Link marks are handled inline; simplified here
                rpr.push_str("<w:rStyle w:val=\"Hyperlink\"/>");
            }
            _ => {}
        }
    }
    let rpr_xml = if rpr.is_empty() { String::new() } else { format!("<w:rPr>{rpr}</w:rPr>") };

    // Preserve leading/trailing spaces with xml:space="preserve"
    let escaped = escape_xml(text);
    format!(
        r#"<w:r>{rpr_xml}<w:t xml:space="preserve">{escaped}</w:t></w:r>"#
    )
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ─── Import: DOCX → ProseMirror JSON ────────────────────────────────────────
//
// DOM-based parser (roxmltree). Handles run formatting (bold/italic/underline/
// strike, color, font size & family, highlight, super/subscript), paragraph
// alignment, headings & block styles, hyperlinks (resolved via document rels),
// nested ordered/bulleted lists (type detected from numbering.xml) and tables.

/// Import a DOCX → (corps, en-tête, pied). En-tête/pied = `Option<doc PM>` (None si
/// absent ou vide). Le handler construit l'enveloppe multi-page quand l'un existe.
pub fn import_docx(data: &[u8]) -> Result<(PmNode, Option<PmNode>, Option<PmNode>, SectionInfo)> {
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|e| OfficeError::Conversion(format!("ZIP invalide: {e}")))?;

    let document_xml = read_zip_entry(&mut archive, "word/document.xml")
        .ok_or_else(|| OfficeError::Conversion("word/document.xml introuvable".into()))?;
    let rels = read_zip_entry(&mut archive, "word/_rels/document.xml.rels")
        .as_deref()
        .map(parse_rels)
        .unwrap_or_default();
    let numbering = read_zip_entry(&mut archive, "word/numbering.xml")
        .as_deref()
        .map(parse_numbering)
        .unwrap_or_default();
    // Thème : palette de couleurs (`schemeClr`) + polices majeure/mineure (`fontScheme`).
    let theme_xml = read_zip_entry(&mut archive, "word/theme/theme1.xml");
    let theme = theme_xml.as_deref().map(parse_theme).unwrap_or_default();
    let (major, minor) = theme_xml.as_deref().map(parse_theme_fonts).unwrap_or_default();
    // Police par défaut du document (docDefaults de styles.xml) résolue via le thème.
    let styles_xml = read_zip_entry(&mut archive, "word/styles.xml");
    let default_font = styles_xml.as_deref().and_then(|s| parse_default_font(s, &major, &minor));
    let default_size = styles_xml.as_deref().and_then(parse_default_size);
    // Propriétés de paragraphe héritées (docDefaults + styles nommés).
    let para_default = styles_xml.as_deref().map(parse_para_default).unwrap_or_default();
    let (para_styles, para_style_default) =
        styles_xml.as_deref().map(parse_para_styles).unwrap_or_default();
    // Replis : police par défaut d'un DOCX = Calibri, taille = 11 pt (cf. consigne).
    let fonts = FontCtx {
        major,
        minor,
        default: Some(default_font.unwrap_or_else(|| "Calibri".to_string())),
        default_size: default_size.unwrap_or(11.0),
        para_default,
        para_styles,
        para_style_default,
    };
    // Images embarquées du corps : `rId → data-URL` (lues depuis word/media/…).
    let media = build_media_map(&mut archive, &rels, "word");

    let body = parse_document(&document_xml, &rels, &numbering, &theme, &fonts, &media)?;
    let header = parse_hf_part(&mut archive, &document_xml, &rels, &numbering, &theme, &fonts, "headerReference", "hdr");
    let footer = parse_hf_part(&mut archive, &document_xml, &rels, &numbering, &theme, &fonts, "footerReference", "ftr");
    let section = parse_section_props(&document_xml);
    Ok((body, header, footer, section))
}

/// Mise en page de section (marges + orientation + format papier) lue dans le
/// `<w:sectPr>` du corps. Les marges DOCX sont en twips (1440 = 1 pouce = 96 px).
#[derive(Clone)]
pub struct SectionInfo {
    pub margin_top: f64,
    pub margin_right: f64,
    pub margin_bottom: f64,
    pub margin_left: f64,
    pub orientation: String, // "portrait" | "landscape"
    pub paper: String,       // "a4" | "a5" | "a3" | "letter" | "legal"
    pub gutter: f64,         // reliure (px)
    pub header_dist: f64,    // distance en-tête depuis le bord haut (px)
    pub footer_dist: f64,    // distance pied depuis le bord bas (px)
    pub title_pg: bool,      // « Première page différente » (<w:titlePg>)
    pub v_align: String,     // alignement vertical : "top"|"center"|"bottom"|"both"
    pub section_start: String, // "nextPage"|"continuous"|"evenPage"|"oddPage"
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
        }
    }
}

impl SectionInfo {
    /// `true` si la section diffère des valeurs par défaut (déclenche l'enveloppe).
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
    }
}

/// Lit le dernier `<w:sectPr>` du document (mise en page du corps) : marges
/// `<w:pgMar>` (twips→px, dont reliure + distances en-tête/pied) + orientation/format
/// `<w:pgSz>` + `<w:titlePg>` (1ʳᵉ page différente) + `<w:vAlign>` + `<w:type>`.
fn parse_section_props(document_xml: &str) -> SectionInfo {
    let mut s = SectionInfo::default();
    let doc = match XmlDoc::parse(document_xml) {
        Ok(d) => d,
        Err(_) => return s,
    };
    // Le sectPr du corps est le dernier de la liste (les autres sont des sauts de section).
    let Some(sect) = doc.descendants().rfind(|n| local(n) == "sectPr") else {
        return s;
    };
    const TWIP_PER_PX: f64 = 15.0; // 1440 twips/pouce ÷ 96 px/pouce
    if let Some(mar) = sect.children().find(|n| local(n) == "pgMar") {
        let twp = |name: &str, dflt: f64| {
            attr_val(&mar, name)
                .and_then(|v| v.parse::<f64>().ok())
                .map(|t| (t / TWIP_PER_PX).round())
                .unwrap_or(dflt)
        };
        s.margin_top = twp("top", 96.0);
        s.margin_right = twp("right", 96.0);
        s.margin_bottom = twp("bottom", 96.0);
        s.margin_left = twp("left", 96.0);
        s.gutter = twp("gutter", 0.0);
        s.header_dist = twp("header", 48.0);
        s.footer_dist = twp("footer", 48.0);
    }
    if let Some(sz) = sect.children().find(|n| local(n) == "pgSz") {
        if attr_val(&sz, "orient").as_deref() == Some("landscape") {
            s.orientation = "landscape".into();
        }
        // Format papier d'après les dimensions (twips). Compare la plus grande
        // dimension : A4≈16838, A3≈23811, A5≈11906, Letter≈15840, Legal≈20160.
        let w = attr_val(&sz, "w").and_then(|v| v.parse::<f64>().ok()).unwrap_or(11906.0);
        let h = attr_val(&sz, "h").and_then(|v| v.parse::<f64>().ok()).unwrap_or(16838.0);
        let long = w.max(h);
        let near = |t: f64| (long - t).abs() < 500.0;
        s.paper = if near(23811.0) { "a3".into() }
            else if near(20160.0) { "legal".into() }
            else if near(15840.0) { "letter".into() }
            else if near(11906.0) { "a5".into() }
            else { "a4".into() };
    }
    if sect.children().any(|n| local(&n) == "titlePg") {
        s.title_pg = true;
    }
    if let Some(va) = sect.children().find(|n| local(n) == "vAlign").and_then(|n| attr_val(&n, "val")) {
        s.v_align = match va.as_str() {
            "center" => "center",
            "bottom" => "bottom",
            "both" => "both",
            _ => "top",
        }
        .into();
    }
    if let Some(ty) = sect.children().find(|n| local(n) == "type").and_then(|n| attr_val(&n, "val")) {
        s.section_start = match ty.as_str() {
            "continuous" => "continuous",
            "evenPage" => "evenPage",
            "oddPage" => "oddPage",
            _ => "nextPage",
        }
        .into();
    }
    s
}

/// Map a theme colour scheme (`<a:clrScheme>`) name → hex (`accent6` → `70AD47`).
fn parse_theme(xml: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let doc = match XmlDoc::parse(xml) {
        Ok(d) => d,
        Err(_) => return map,
    };
    if let Some(scheme) = doc.descendants().find(|n| local(n) == "clrScheme") {
        for slot in scheme.children().filter(|n| n.is_element()) {
            let name = local(&slot).to_string();
            // Chaque slot porte un srgbClr (val) ou un sysClr (lastClr).
            let hex = slot
                .children()
                .find(|n| n.is_element())
                .and_then(|c| match local(&c) {
                    "srgbClr" => attr_val(&c, "val"),
                    "sysClr" => attr_val(&c, "lastClr").or_else(|| attr_val(&c, "val")),
                    _ => None,
                });
            if let Some(h) = hex {
                map.insert(name, h);
            }
        }
    }
    map
}

/// Polices résolues pour le document : majeure/mineure du thème (`a:fontScheme`) et
/// police par défaut (`w:docDefaults`). Word référence souvent les polices par thème
/// (asciiTheme=minorHAnsi = corps = Calibri) et les runs du corps n'ont pas de
/// `rFonts` (héritage du défaut) ; sans résolution le texte retombe sur Arial.
struct FontCtx {
    major: String,        // thème majeur (titres) — ex. « Calibri Light »
    minor: String,        // thème mineur (corps) — ex. « Calibri »
    default: Option<String>, // police par défaut du document (docDefaults)
    default_size: f64,    // taille par défaut en pt (docDefaults sz ; Word = 11)
    // Propriétés de paragraphe héritées (façon Word) : docDefaults < style nommé <
    // pPr direct. `para_default` = `<w:docDefaults><w:pPrDefault>` ; `para_styles` =
    // pPr RÉSOLU (chaîne basedOn incluse) par styleId ; `para_style_default` = style
    // par défaut appliqué quand le paragraphe n'a pas de `<w:pStyle>`.
    para_default: serde_json::Map<String, Value>,
    para_styles: HashMap<String, serde_json::Map<String, Value>>,
    para_style_default: Option<String>,
}

/// Lit les polices latines majeure/mineure du `<a:fontScheme>` de theme1.xml.
fn parse_theme_fonts(xml: &str) -> (String, String) {
    let doc = match XmlDoc::parse(xml) {
        Ok(d) => d,
        Err(_) => return (String::new(), String::new()),
    };
    let latin = |which: &str| -> String {
        doc.descendants()
            .find(|n| local(n) == which)
            .and_then(|n| n.children().find(|c| local(c) == "latin"))
            .and_then(|c| attr_val(&c, "typeface"))
            .unwrap_or_default()
    };
    (latin("majorFont"), latin("minorFont"))
}

/// Police par défaut du document : `<w:docDefaults><w:rPrDefault>…<w:rFonts>` (nom
/// littéral `w:ascii`, sinon `w:asciiTheme` résolu via le thème major/minor).
fn parse_default_font(xml: &str, major: &str, minor: &str) -> Option<String> {
    let doc = XmlDoc::parse(xml).ok()?;
    let dd = doc.descendants().find(|n| local(n) == "docDefaults")?;
    let rfonts = dd
        .descendants()
        .find(|n| local(n) == "rPrDefault")
        .and_then(|rp| rp.descendants().find(|n| local(n) == "rFonts"))?;
    if let Some(f) = attr_val(&rfonts, "ascii").filter(|s| !s.is_empty()) {
        return Some(f);
    }
    let th = attr_val(&rfonts, "asciiTheme").or_else(|| attr_val(&rfonts, "hAnsiTheme"))?;
    let f = if th.starts_with("major") { major } else { minor };
    (!f.is_empty()).then(|| f.to_string())
}

/// Taille par défaut du document : `<w:docDefaults>…<w:sz w:val="…"/>` (demi-points → pt).
fn parse_default_size(xml: &str) -> Option<f64> {
    let doc = XmlDoc::parse(xml).ok()?;
    let dd = doc.descendants().find(|n| local(n) == "docDefaults")?;
    let sz = dd
        .descendants()
        .find(|n| local(n) == "rPrDefault")
        .and_then(|rp| rp.descendants().find(|n| local(n) == "sz"))?;
    attr_val(&sz, "val")
        .and_then(|v| v.parse::<f64>().ok())
        .map(|hp| hp / 2.0)
}

/// Résout la police d'un run depuis son `<w:rFonts>` : nom littéral `ascii`/`hAnsi`,
/// sinon référence de thème `asciiTheme`/`hAnsiTheme` (major*/minor* → thème).
fn resolve_run_font(m: &Node<'_, '_>, fonts: &FontCtx) -> Option<String> {
    if let Some(f) = attr_val(m, "ascii").or_else(|| attr_val(m, "hAnsi")).filter(|s| !s.is_empty()) {
        return Some(f);
    }
    let th = attr_val(m, "asciiTheme").or_else(|| attr_val(m, "hAnsiTheme"))?;
    let f = if th.starts_with("major") { &fonts.major } else { &fonts.minor };
    (!f.is_empty()).then(|| f.clone())
}

/// Read a zip entry as UTF-8 text; returns None if missing or unreadable.
fn read_zip_entry(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<String> {
    let mut file = archive.by_name(name).ok()?;
    let mut buf = String::new();
    std::io::Read::read_to_string(&mut file, &mut buf).ok()?;
    Some(buf)
}

/// Read a zip entry as raw bytes (images, …).
fn read_zip_bytes(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<Vec<u8>> {
    let mut file = archive.by_name(name).ok()?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut buf).ok()?;
    Some(buf)
}

/// MIME type d'une image d'après son extension de fichier.
fn image_mime(name: &str) -> Option<&'static str> {
    match name.rsplit('.').next().map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("bmp") => Some("image/bmp"),
        Some("webp") => Some("image/webp"),
        Some("svg") => Some("image/svg+xml"),
        Some("tif" | "tiff") => Some("image/tiff"),
        _ => None,
    }
}

/// Construit une map `rId → data-URL` pour les relations IMAGE d'une partie
/// (document/header/footer) : lit chaque blob média du zip et l'encode en base64.
/// Les data-URLs voyagent dans le contenu (pas de dépendance à un fichier externe).
fn build_media_map(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    rels: &HashMap<String, String>,
    base_dir: &str,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (rid, target) in rels {
        let Some(mime) = image_mime(target) else { continue };
        // Cible relative au dossier de la partie (word/), ou absolue (/word/media/…).
        let part = if let Some(abs) = target.strip_prefix('/') {
            abs.to_string()
        } else {
            format!("{base_dir}/{}", target.trim_start_matches("./"))
        };
        if let Some(bytes) = read_zip_bytes(archive, &part) {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            map.insert(rid.clone(), format!("data:{mime};base64,{b64}"));
        }
    }
    map
}

/// Local name of an element, namespace-agnostic (handles the `w:` prefix).
fn local<'a>(n: &Node<'a, '_>) -> &'a str {
    n.tag_name().name()
}

/// First attribute matching `name` by local name (handles `w:val`, `r:id`, …).
fn attr_val(n: &Node<'_, '_>, name: &str) -> Option<String> {
    n.attributes()
        .find(|a| a.name() == name)
        .map(|a| a.value().to_string())
}

/// First direct child element with the given local name.
fn child<'a, 'd>(n: &Node<'a, 'd>, name: &str) -> Option<Node<'a, 'd>> {
    n.children().find(|c| c.is_element() && local(c) == name)
}

/// Map relationship id → target URL (for hyperlinks).
fn parse_rels(xml: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(doc) = XmlDoc::parse(xml) {
        for n in doc.descendants().filter(|n| local(n) == "Relationship") {
            if let (Some(id), Some(target)) = (attr_val(&n, "Id"), attr_val(&n, "Target")) {
                map.insert(id, target);
            }
        }
    }
    map
}

/// Map numId → is the list ordered? Bullets and "none" formats are unordered.
fn parse_numbering(xml: &str) -> HashMap<String, bool> {
    let mut result = HashMap::new();
    let doc = match XmlDoc::parse(xml) {
        Ok(d) => d,
        Err(_) => return result,
    };

    // abstractNumId → ordered? (decided by the level-0 numFmt)
    let mut abstract_ordered: HashMap<String, bool> = HashMap::new();
    for an in doc.descendants().filter(|n| local(n) == "abstractNum") {
        let aid = match attr_val(&an, "abstractNumId") {
            Some(v) => v,
            None => continue,
        };
        let mut ordered = true;
        for lvl in an.descendants().filter(|n| local(n) == "lvl") {
            if attr_val(&lvl, "ilvl").as_deref() != Some("0") {
                continue;
            }
            if let Some(fmt) = child(&lvl, "numFmt").and_then(|n| attr_val(&n, "val")) {
                ordered = fmt != "bullet" && fmt != "none";
            }
            break;
        }
        abstract_ordered.insert(aid, ordered);
    }

    // numId → abstractNumId → ordered
    for num in doc.descendants().filter(|n| local(n) == "num") {
        let nid = match attr_val(&num, "numId") {
            Some(v) => v,
            None => continue,
        };
        if let Some(anid) = child(&num, "abstractNumId").and_then(|n| attr_val(&n, "val")) {
            let ordered = abstract_ordered.get(&anid).copied().unwrap_or(false);
            result.insert(nid, ordered);
        }
    }
    result
}

/// A body-level block, before consecutive list items are folded into lists.
enum Block {
    Node(PmNode),
    Item { ordered: bool, ilvl: u8, para: PmNode },
}

#[allow(clippy::too_many_arguments)]
fn parse_document(
    xml: &str,
    rels: &HashMap<String, String>,
    numbering: &HashMap<String, bool>,
    theme: &HashMap<String, String>,
    fonts: &FontCtx,
    media: &HashMap<String, String>,
) -> Result<PmNode> {
    let doc = XmlDoc::parse(xml)
        .map_err(|e| OfficeError::Conversion(format!("XML invalide: {e}")))?;
    let body = doc
        .descendants()
        .find(|n| local(n) == "body")
        .ok_or_else(|| OfficeError::Conversion("w:body introuvable".into()))?;
    Ok(parse_container_doc(&body, rels, numbering, theme, fonts, media))
}

/// Parse a block container (`w:body`, `w:hdr`, `w:ftr`, table cell…) into a
/// ProseMirror `doc` node: paragraphs/tables + DrawingML shapes, lists folded.
#[allow(clippy::too_many_arguments)]
fn parse_container_doc(
    root: &Node<'_, '_>,
    rels: &HashMap<String, String>,
    numbering: &HashMap<String, bool>,
    theme: &HashMap<String, String>,
    fonts: &FontCtx,
    media: &HashMap<String, String>,
) -> PmNode {
    let mut blocks: Vec<Block> = Vec::new();
    for node in root.children().filter(|n| n.is_element()) {
        match local(&node) {
            "p" => {
                // Formes DrawingML (`<w:drawing>`) → nœuds image bloc (alt `kbshape:`).
                let drawings = parse_drawings(&node, theme, media);
                let block = parse_paragraph(&node, rels, numbering, fonts, theme, media);
                let para_empty = matches!(&block, Block::Node(n)
                    if n.node_type == "paragraph" && n.content.as_ref().is_none_or(|c| c.is_empty()));
                // Si le paragraphe ne porte QUE des formes, ne pas garder le paragraphe vide.
                if !para_empty || drawings.is_empty() {
                    blocks.push(block);
                }
                for d in drawings {
                    blocks.push(Block::Node(d));
                }
            }
            "tbl" => blocks.push(Block::Node(parse_table(&node, rels, numbering, fonts, theme, media))),
            _ => {}
        }
    }

    let mut nodes = fold_blocks(blocks);
    if nodes.is_empty() {
        nodes.push(PmNode::paragraph(vec![]));
    }
    PmNode::doc(nodes)
}

/// A ProseMirror doc holding only empty paragraphs is considered empty.
fn pm_is_empty(doc: &PmNode) -> bool {
    doc.content.as_ref().is_none_or(|c| {
        c.iter().all(|n| {
            n.node_type == "paragraph" && n.content.as_ref().is_none_or(|cc| cc.is_empty())
        })
    })
}

/// Parse a referenced header/footer part (`<w:headerReference>`/`<w:footerReference>`
/// in the body `sectPr` → rels → `word/headerN.xml`). Returns None if absent/empty.
#[allow(clippy::too_many_arguments)]
fn parse_hf_part(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    document_xml: &str,
    rels: &HashMap<String, String>,
    numbering: &HashMap<String, bool>,
    theme: &HashMap<String, String>,
    fonts: &FontCtx,
    ref_tag: &str,
    root_tag: &str,
) -> Option<PmNode> {
    let doc = XmlDoc::parse(document_xml).ok()?;
    let refs: Vec<Node> = doc.descendants().filter(|n| local(n) == ref_tag).collect();
    // Préférer la référence « default » (sinon la première).
    let rid = refs
        .iter()
        .find(|n| attr_val(n, "type").as_deref() == Some("default"))
        .or_else(|| refs.first())
        .and_then(|n| attr_val(n, "id"))?;
    let target = rels.get(&rid)?;
    let part_name = target.trim_start_matches('/').trim_start_matches("./").to_string();
    let part = format!("word/{part_name}");
    let xml = read_zip_entry(archive, &part)?;
    // Relations PROPRES à cette partie (hyperliens/images de l'en-tête/pied).
    let hf_rels = read_zip_entry(archive, &format!("word/_rels/{part_name}.rels"))
        .as_deref()
        .map(parse_rels)
        .unwrap_or_default();
    let hf_media = build_media_map(archive, &hf_rels, "word");
    let pdoc = XmlDoc::parse(&xml).ok()?;
    let root = pdoc.descendants().find(|n| local(n) == root_tag)?;
    let pm = parse_container_doc(&root, &hf_rels, numbering, theme, fonts, &hf_media);
    if pm_is_empty(&pm) {
        None
    } else {
        Some(pm)
    }
}

// English Metric Units per CSS pixel (914400 EMU/inch ÷ 96 px/inch).
const EMU_PER_PX: f64 = 9525.0;

/// Extract DrawingML preset shapes (`<w:drawing>` → `<a:prstGeom>`) from a
/// paragraph as block-level image nodes carrying `alt="kbshape:{kind,fill,stroke}"`
/// and no `src` — the editor regenerates the SVG from the alt at render time.
/// Raster pictures (`<pic:pic>` without preset geometry) are ignored here.
fn parse_drawings(
    p: &Node<'_, '_>,
    theme: &HashMap<String, String>,
    media: &HashMap<String, String>,
) -> Vec<PmNode> {
    let mut out = Vec::new();
    for d in p.descendants().filter(|n| local(n) == "drawing") {
        // Seuls les objets ANCRÉS (`<wp:anchor>`) deviennent des blocs flottants. Les
        // objets « alignés sur le texte » (`<wp:inline>`) sont émis INLINE par parse_run
        // (token-image dans le flux, façon caractère).
        for frame in d.descendants().filter(|n| local(n) == "anchor") {
            let is_anchor = local(&frame) == "anchor";

            // Taille depuis <wp:extent cx= cy=> (EMU → px). Commune image/forme.
            let (mut w, mut h) = (240.0_f64, 180.0_f64);
            if let Some(ext) = frame.descendants().find(|n| local(n) == "extent") {
                if let Some(cx) = attr_val(&ext, "cx").and_then(|v| v.parse::<f64>().ok()) {
                    w = (cx / EMU_PER_PX).round().max(8.0);
                }
                if let Some(cy) = attr_val(&ext, "cy").and_then(|v| v.parse::<f64>().ok()) {
                    h = (cy / EMU_PER_PX).round().max(8.0);
                }
            }

            let mut attrs = serde_json::Map::new();
            attrs.insert("width".into(), json!(w));
            attrs.insert("height".into(), json!(h));

            // Rotation du dessin (`<a:xfrm rot="…">`, en 60000es de degré, sens
            // horaire) — Word fait pivoter l'objet ; sinon une photo prise à
            // l'horizontale s'affiche couchée. `wp:extent` reste la taille NON
            // tournée (notre moteur réserve l'AABB tournée au layout).
            if let Some(rot) = frame
                .descendants()
                .find(|n| local(n) == "xfrm" && n.has_attribute("rot"))
                .and_then(|x| attr_val(&x, "rot"))
                .and_then(|v| v.parse::<f64>().ok())
            {
                let deg = (rot / 60000.0).rem_euclid(360.0);
                if deg.abs() > 0.01 {
                    attrs.insert("rotation".into(), json!(deg));
                }
            }

            // 1) Image matricielle : <a:blip r:embed="rIdX"> → data URL depuis le média.
            let blip_src = frame
                .descendants()
                .find(|n| local(n) == "blip")
                .and_then(|b| attr_val(&b, "embed"))
                .and_then(|rid| media.get(&rid).cloned());

            if let Some(src) = blip_src {
                attrs.insert("src".into(), json!(src));
                attrs.insert("alt".into(), json!(""));
            } else if let Some(prst) = frame
                .descendants()
                .find(|n| local(n) == "prstGeom")
                .and_then(|g| attr_val(&g, "prst"))
            {
                // 2) Forme à géométrie prédéfinie → rendue via `kbshape:`.
                let kind = prst_to_kind(&prst);

                // Remplissage / contour : d'abord `spPr` explicite, sinon référence de
                // style (`wps:style > fillRef/lnRef`), avec résolution des couleurs de thème.
                let sppr = frame.descendants().find(|n| local(n) == "spPr");
                let style = frame.descendants().find(|n| local(n) == "style");
                let fill = sppr
                    .as_ref()
                    .and_then(|sp| sp.children().find(|n| n.is_element() && local(n) == "solidFill"))
                    .and_then(|sf| first_color_hex(&sf, theme))
                    .or_else(|| style.as_ref().and_then(|st| st.children().find(|n| local(n) == "fillRef")).and_then(|fr| first_color_hex(&fr, theme)))
                    .unwrap_or_else(|| default_shape_fill(kind).to_string());
                let stroke = sppr
                    .as_ref()
                    .and_then(|sp| sp.children().find(|n| n.is_element() && local(n) == "ln"))
                    .and_then(|ln| first_color_hex(&ln, theme))
                    .or_else(|| style.as_ref().and_then(|st| st.children().find(|n| local(n) == "lnRef")).and_then(|lr| first_color_hex(&lr, theme)))
                    .unwrap_or_else(|| "#1a73e8".to_string());

                // Épaisseur du contour : `<a:ln w="EMU">` (12700 EMU = 1 pt ; px = EMU÷9525).
                // Stockée en FRACTION de la plus petite dimension (invariante à la
                // résolution de génération du SVG). Absente → défaut côté frontend.
                let sw_frac = sppr
                    .as_ref()
                    .and_then(|sp| sp.children().find(|n| n.is_element() && local(n) == "ln"))
                    .and_then(|ln| attr_val(&ln, "w"))
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|emu| (emu / EMU_PER_PX) / w.min(h));

                // alt = `kbshape:` + encodeURIComponent(JSON) — symétrique du frontend.
                let mut params = json!({ "kind": kind, "fill": fill, "stroke": stroke });
                if let Some(f) = sw_frac {
                    params["sw"] = json!((f * 10000.0).round() / 10000.0);
                }
                let alt = format!("kbshape:{}", urlencoding::encode(&params.to_string()));
                attrs.insert("src".into(), json!(""));
                attrs.insert("alt".into(), json!(alt));
            } else {
                continue; // ni image ni forme à géométrie connue
            }

            // Objet ANCRÉ (flottant) : habillage + décalage de position (EMU → px).
            if is_anchor {
                let behind = attr_val(&frame, "behindDoc").as_deref() == Some("1");
                let wrap = if frame.descendants().any(|n| local(&n) == "wrapNone") {
                    if behind { "behind" } else { "front" }
                } else if frame.descendants().any(|n| local(&n) == "wrapTopAndBottom") {
                    "topBottom"
                } else if frame.descendants().any(|n| {
                    matches!(local(&n), "wrapSquare" | "wrapTight" | "wrapThrough")
                }) {
                    "square"
                } else if behind {
                    "behind"
                } else {
                    "front"
                };
                let off = |which: &str| -> f64 {
                    frame
                        .descendants()
                        .find(|n| local(n) == which)
                        .and_then(|pos| pos.children().find(|c| local(c) == "posOffset"))
                        .and_then(|o| o.text())
                        .and_then(|t| t.trim().parse::<f64>().ok())
                        .map(|emu| (emu / EMU_PER_PX).round())
                        .unwrap_or(0.0)
                };
                attrs.insert("align".into(), json!("left"));
                attrs.insert("wrap".into(), json!(wrap));
                attrs.insert("wrapX".into(), json!(off("positionH").max(0.0)));
                attrs.insert("wrapY".into(), json!(off("positionV")));
            } else {
                attrs.insert("align".into(), json!("center"));
            }

            out.push(PmNode {
                node_type: "image".into(),
                attrs: Some(Value::Object(attrs)),
                content: None,
                marks: None,
                text: None,
            });
        }
    }
    out
}

/// First colour child of a fill/line/ref node → hex, resolving theme `schemeClr`.
fn first_color_hex(parent: &Node<'_, '_>, theme: &HashMap<String, String>) -> Option<String> {
    let c = parent
        .descendants()
        .find(|n| matches!(local(n), "srgbClr" | "schemeClr" | "sysClr"))?;
    let base = match local(&c) {
        "srgbClr" => attr_val(&c, "val")?,
        "sysClr" => attr_val(&c, "lastClr")?,
        "schemeClr" => {
            let raw = attr_val(&c, "val")?;
            // Le mappage couleur du thème : tx1↔dk1, bg1↔lt1, tx2↔dk2, bg2↔lt2.
            let key = match raw.as_str() {
                "tx1" => "dk1", "bg1" => "lt1", "tx2" => "dk2", "bg2" => "lt2",
                other => other,
            };
            theme.get(key)?.clone()
        }
        _ => return None,
    };
    // Modificateurs de couleur (shade/tint/lumMod/lumOff) — ex. le CONTOUR d'une
    // forme = accent + shade → plus foncé que le remplissage (sinon bordure invisible).
    Some(apply_color_mods(&base, &c))
}

/// Applique les modificateurs OOXML (`a:shade`/`a:tint`/`a:lumMod`/`a:lumOff`) portés
/// par un élément couleur. Valeurs en millièmes de % (50000 = 50%). Approximation
/// linéaire (suffisante pour distinguer remplissage/contour).
fn apply_color_mods(hex: &str, c: &Node<'_, '_>) -> String {
    let h = hex.trim_start_matches('#');
    let parse = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0) as f64;
    let (mut r, mut g, mut b) = (parse(0), parse(2), parse(4));
    let val = |name: &str| -> Option<f64> {
        c.children()
            .find(|n| local(n) == name)
            .and_then(|n| attr_val(&n, "val"))
            .and_then(|v| v.parse::<f64>().ok())
            .map(|x| x / 100_000.0)
    };
    if let Some(s) = val("shade") {
        r *= s; g *= s; b *= s;                       // assombrit vers le noir
    }
    if let Some(t) = val("tint") {
        r = r * t + 255.0 * (1.0 - t);                // éclaircit vers le blanc
        g = g * t + 255.0 * (1.0 - t);
        b = b * t + 255.0 * (1.0 - t);
    }
    if let Some(lm) = val("lumMod") {
        r *= lm; g *= lm; b *= lm;
    }
    if let Some(lo) = val("lumOff") {
        r += 255.0 * lo; g += 255.0 * lo; b += 255.0 * lo;
    }
    let cl = |x: f64| x.round().clamp(0.0, 255.0) as u8;
    format!("#{:02X}{:02X}{:02X}", cl(r), cl(g), cl(b))
}

/// Les connecteurs/traits n'ont pas de remplissage ; les autres formes prennent
/// la teinte par défaut de l'éditeur si le DOCX ne précise pas de couleur.
fn default_shape_fill(kind: &str) -> &'static str {
    match kind {
        "line" | "lineArrow" | "lineDouble" | "elbowConnector" | "elbowArrow"
        | "elbowDoubleArrow" | "curveConnector" | "curveArrow" | "curveDoubleArrow" | "curve" => "none",
        _ => "#dbe7ff",
    }
}

/// Géométrie prédéfinie OOXML (`a:prstGeom@prst`) → ShapeKind de l'éditeur.
/// Repli : `rect` (jamais d'échec d'import sur une forme inconnue).
fn prst_to_kind(prst: &str) -> &'static str {
    match prst {
        // Rectangles
        "rect" => "rect", "roundRect" => "roundRect", "snip1Rect" => "snipRect",
        "snip2SameRect" => "snip2SameRect", "snip2DiagRect" => "snip2DiagRect", "snipRoundRect" => "snipRoundRect",
        "round1Rect" => "roundRect1", "round2SameRect" => "round2SameRect", "round2DiagRect" => "round2DiagRect", "plaque" => "plaque",
        // Formes de base
        "ellipse" | "oval" => "ellipse", "triangle" => "triangle", "rtTriangle" => "rtTriangle",
        "parallelogram" => "parallelogram", "trapezoid" | "trapezoid2" => "trapezoid", "diamond" => "diamond",
        "pentagon" => "pentagon", "hexagon" => "hexagon", "heptagon" => "heptagon", "octagon" => "octagon",
        "decagon" => "decagon", "dodecagon" => "dodecagon",
        "pie" | "pieWedge" => "pie", "chord" => "chord", "teardrop" => "teardrop",
        "frame" => "frame", "halfFrame" => "halfFrame", "corner" => "corner", "diagStripe" => "diagStripe",
        "plus" => "cross", "bevel" => "bevel", "can" => "cylinder", "cube" => "cube",
        "blockArc" => "blockArc", "foldedCorner" => "foldedCorner",
        "heart" => "heart", "lightningBolt" => "lightning", "sun" => "sun", "moon" => "moon",
        "cloud" => "cloud", "smileyFace" => "smiley", "arc" => "arc", "donut" => "donut", "noSmoking" => "noSymbol",
        "leftBrace" => "leftBrace", "rightBrace" => "rightBrace", "leftBracket" => "leftBracket", "rightBracket" => "rightBracket",
        "bracePair" => "doubleBrace", "bracketPair" => "doubleBracket",
        // Flèches pleines
        "rightArrow" => "arrow", "leftArrow" => "arrowLeft", "upArrow" => "arrowUp", "downArrow" => "arrowDown",
        "leftRightArrow" => "arrowLeftRight", "upDownArrow" => "arrowUpDown", "quadArrow" => "arrowQuad",
        "leftRightUpArrow" => "leftRightUpArrow", "bentArrow" => "bentArrow", "bentUpArrow" => "bentUpArrow",
        "uturnArrow" => "uTurnArrow", "curvedRightArrow" => "curvedRightArrow", "curvedLeftArrow" => "curvedLeftArrow",
        "curvedUpArrow" => "curvedUpArrow", "curvedDownArrow" => "curvedDownArrow",
        "stripedRightArrow" => "stripedRightArrow", "notchedRightArrow" => "notchedArrow",
        "homePlate" => "pentagonArrow", "chevron" => "chevron", "circularArrow" => "circularArrow",
        "rightArrowCallout" => "rightArrowCallout", "leftArrowCallout" => "leftArrowCallout",
        "upArrowCallout" => "upArrowCallout", "downArrowCallout" => "downArrowCallout",
        // Formes d'équation
        "mathPlus" => "mathPlus", "mathMinus" => "mathMinus", "mathMultiply" => "mathMultiply",
        "mathDivide" => "mathDivide", "mathEqual" => "mathEqual", "mathNotEqual" => "mathNotEqual",
        // Organigrammes
        "flowChartProcess" => "flowProcess", "flowChartAlternateProcess" => "flowAltProcess",
        "flowChartDecision" => "flowDecision", "flowChartInputOutput" => "flowData",
        "flowChartPredefinedProcess" => "flowPredefined", "flowChartInternalStorage" => "flowInternal",
        "flowChartDocument" => "flowDocument", "flowChartMultidocument" => "flowMultidoc",
        "flowChartTerminator" => "flowTerminator", "flowChartPreparation" => "flowPreparation",
        "flowChartManualInput" => "flowManualInput", "flowChartManualOperation" => "flowManualOp",
        "flowChartConnector" => "flowConnector", "flowChartOffpageConnector" => "flowOffPage",
        "flowChartPunchedCard" => "flowCard", "flowChartPunchedTape" => "flowPunchedTape",
        "flowChartSummingJunction" => "flowSumming", "flowChartOr" => "flowOr",
        "flowChartCollate" => "flowCollate", "flowChartSort" => "flowSort",
        "flowChartExtract" => "flowExtract", "flowChartMerge" => "flowMerge",
        "flowChartOnlineStorage" => "flowStored", "flowChartDelay" => "flowDelay",
        "flowChartMagneticTape" => "flowSequential", "flowChartMagneticDisk" => "flowMagneticDisk",
        "flowChartMagneticDrum" => "flowDirectAccess", "flowChartDisplay" => "flowDisplay",
        // Étoiles et bannières
        "star4" => "star4", "star5" => "star", "star6" => "star6", "star7" => "star7",
        "star8" => "star8", "star10" => "star10", "star12" => "star12", "star16" => "star16",
        "star24" => "star24", "star32" => "star32",
        "irregularSeal1" => "explosion1", "irregularSeal2" => "explosion2",
        "ribbon" => "ribbonDown", "ribbon2" => "ribbon", "ellipseRibbon" | "ellipseRibbon2" => "ribbonCurved",
        "verticalScroll" => "scrollV", "horizontalScroll" => "scrollH", "wave" => "wave", "doubleWave" => "doubleWave",
        // Bulles et légendes
        "wedgeRectCallout" => "calloutRect", "wedgeRoundRectCallout" => "calloutRoundRect",
        "wedgeEllipseCallout" => "calloutOval", "cloudCallout" => "calloutCloud",
        "borderCallout1" | "callout1" => "lineCallout", "borderCallout2" | "callout2" => "calloutLine2",
        // Traits / connecteurs
        "line" | "straightConnector1" => "line",
        "bentConnector2" | "bentConnector3" | "bentConnector4" | "bentConnector5" => "elbowConnector",
        "curvedConnector2" | "curvedConnector3" | "curvedConnector4" | "curvedConnector5" => "curveConnector",
        _ => "rect",
    }
}

#[allow(clippy::too_many_arguments)]
fn parse_paragraph(
    p: &Node<'_, '_>,
    rels: &HashMap<String, String>,
    numbering: &HashMap<String, bool>,
    fonts: &FontCtx,
    theme: &HashMap<String, String>,
    media: &HashMap<String, String>,
) -> Block {
    let mut style = String::new();
    let mut num: Option<(String, u8)> = None;
    // Toutes les propriétés de paragraphe (`<w:pPr>`) → attributs de l'éditeur
    // (alignement, retraits, espacement, interligne, enchaînements…).
    let ppr_attrs = if let Some(ppr) = child(p, "pPr") {
        if let Some(ps) = child(&ppr, "pStyle") {
            style = attr_val(&ps, "val").unwrap_or_default();
        }
        if let Some(numpr) = child(&ppr, "numPr") {
            let numid = child(&numpr, "numId").and_then(|n| attr_val(&n, "val"));
            let ilvl = child(&numpr, "ilvl")
                .and_then(|n| attr_val(&n, "val"))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0u8);
            if let Some(id) = numid {
                num = Some((id, ilvl));
            }
        }
        parse_ppr_attrs(&ppr)
    } else {
        serde_json::Map::new()
    };

    let inline = parse_inline(p, rels, fonts, theme, media);
    // Propriétés EFFECTIVES = docDefaults < style nommé (ou style par défaut) < pPr direct.
    let eff = effective_para_attrs(fonts, &style, ppr_attrs);

    // List item: defer wrapping into a list to fold_blocks.
    if let Some((numid, ilvl)) = num {
        let ordered = numbering.get(&numid).copied().unwrap_or(false);
        let mut para = PmNode::paragraph(inline);
        apply_attrs(&mut para, eff);
        return Block::Item { ordered, ilvl, para };
    }

    // Heading / block style.
    let node = if let Some(level) = heading_level(&style) {
        let mut n = PmNode::heading(level, inline);
        apply_attrs(&mut n, eff);
        n
    } else if style.eq_ignore_ascii_case("Quote") || style.eq_ignore_ascii_case("IntenseQuote") {
        let mut pp = PmNode::paragraph(inline);
        apply_attrs(&mut pp, eff);
        PmNode::blockquote(vec![pp])
    } else if style == "Code" || style.eq_ignore_ascii_case("HTMLPreformatted") {
        let text: String = inline
            .iter()
            .filter_map(|n| n.text.as_deref())
            .collect::<Vec<_>>()
            .join("");
        PmNode::code_block(text)
    } else {
        let mut n = PmNode::paragraph(inline);
        apply_attrs(&mut n, eff);
        n
    };
    Block::Node(node)
}

/// `<w:pPr>` → attributs de paragraphe de l'éditeur. Retraits/espacement DOCX en
/// twips (1440/pouce = 96 px ⇒ ÷15). Interligne : `lineRule="auto"` = multiplicateur
/// (240 = simple), sinon (`atLeast`/`exact`) valeur absolue en px.
fn parse_ppr_attrs(ppr: &Node<'_, '_>) -> serde_json::Map<String, Value> {
    const TWIP_PER_PX: f64 = 15.0;
    let mut m = serde_json::Map::new();
    let twip_px = |v: Option<String>| v.and_then(|s| s.parse::<f64>().ok()).map(|t| (t / TWIP_PER_PX).round());

    if let Some(a) = child(ppr, "jc").and_then(|jc| attr_val(&jc, "val")).as_deref().and_then(map_align) {
        m.insert("textAlign".into(), json!(a));
    }
    if let Some(ind) = child(ppr, "ind") {
        if let Some(px) = twip_px(attr_val(&ind, "left").or_else(|| attr_val(&ind, "start"))) {
            if px != 0.0 { m.insert("indentLeft".into(), json!(px)); }
        }
        if let Some(px) = twip_px(attr_val(&ind, "right").or_else(|| attr_val(&ind, "end"))) {
            if px != 0.0 { m.insert("indentRight".into(), json!(px)); }
        }
        // 1ʳᵉ ligne positive ; suspendu (hanging) = retrait négatif de 1ʳᵉ ligne.
        if let Some(fl) = twip_px(attr_val(&ind, "firstLine")).filter(|v| *v != 0.0) {
            m.insert("indentFirstLine".into(), json!(fl));
        } else if let Some(hg) = twip_px(attr_val(&ind, "hanging")).filter(|v| *v != 0.0) {
            m.insert("indentFirstLine".into(), json!(-hg));
        }
    }
    if let Some(sp) = child(ppr, "spacing") {
        if let Some(px) = twip_px(attr_val(&sp, "before")) {
            m.insert("spaceBefore".into(), json!(px));
        }
        if let Some(px) = twip_px(attr_val(&sp, "after")) {
            m.insert("spaceAfter".into(), json!(px));
        }
        if let Some(line) = attr_val(&sp, "line").and_then(|v| v.parse::<f64>().ok()) {
            match attr_val(&sp, "lineRule").as_deref() {
                Some("exact") | Some("exactly") => {
                    m.insert("lineSpacingMode".into(), json!("exactly"));
                    m.insert("lineSpacingPt".into(), json!((line / TWIP_PER_PX).round()));
                }
                Some("atLeast") => {
                    m.insert("lineSpacingMode".into(), json!("atLeast"));
                    m.insert("lineSpacingPt".into(), json!((line / TWIP_PER_PX).round()));
                }
                _ => {
                    // auto : 240 = interligne simple → multiplicateur.
                    m.insert("lineHeight".into(), json!((line / 240.0 * 100.0).round() / 100.0));
                }
            }
        }
    }
    // Enchaînements + exceptions (drapeaux ON sauf val falsey).
    let flag = |name: &str| child(ppr, name).map(|n| bool_on(&n)).unwrap_or(false);
    if flag("keepNext") { m.insert("keepNext".into(), json!(true)); }
    if flag("keepLines") { m.insert("keepLines".into(), json!(true)); }
    if flag("pageBreakBefore") { m.insert("pageBreakBefore".into(), json!(true)); }
    if flag("contextualSpacing") { m.insert("contextualSpacing".into(), json!(true)); }
    if flag("suppressLineNumbers") { m.insert("suppressLineNumbers".into(), json!(true)); }
    if flag("suppressAutoHyphens") { m.insert("dontHyphenate".into(), json!(true)); }
    if flag("mirrorIndents") { m.insert("mirrorIndents".into(), json!(true)); }
    // widowControl : par défaut activé côté éditeur → ne stocke que la désactivation explicite.
    if let Some(wc) = child(ppr, "widowControl") {
        if !bool_on(&wc) { m.insert("widowControl".into(), json!(false)); }
    }
    // Niveau hiérarchique : `<w:outlineLvl val="0">` = Niveau 1.
    if let Some(ol) = child(ppr, "outlineLvl").and_then(|n| attr_val(&n, "val")).and_then(|v| v.parse::<i64>().ok()) {
        m.insert("outlineLevel".into(), json!(ol + 1));
    }
    m
}

/// Fusionne des attributs dans un nœud (préserve les existants, ex. `level`).
fn apply_attrs(node: &mut PmNode, attrs: serde_json::Map<String, Value>) {
    if attrs.is_empty() {
        return;
    }
    let mut obj = match node.attrs.take() {
        Some(Value::Object(m)) => m,
        _ => serde_json::Map::new(),
    };
    for (k, v) in attrs {
        obj.insert(k, v);
    }
    node.attrs = Some(Value::Object(obj));
}

/// Propriétés de paragraphe PAR DÉFAUT du document (`<w:docDefaults><w:pPrDefault>
/// <w:pPr>`) — ex. Word applique « Après 8 pt » + interligne 1,16 à TOUT paragraphe.
fn parse_para_default(styles_xml: &str) -> serde_json::Map<String, Value> {
    XmlDoc::parse(styles_xml)
        .ok()
        .and_then(|d| {
            d.descendants()
                .find(|n| local(n) == "pPrDefault")
                .and_then(|pd| child(&pd, "pPr").map(|p| parse_ppr_attrs(&p)))
        })
        .unwrap_or_default()
}

/// Styles de paragraphe (`<w:style w:type="paragraph">`) → pPr RÉSOLU (chaîne
/// `<w:basedOn>` aplatie, racine→feuille) par styleId, + le styleId par défaut
/// (`w:default="1"`). Permet d'hériter espacement/interligne/retraits du style nommé.
fn parse_para_styles(
    styles_xml: &str,
) -> (HashMap<String, serde_json::Map<String, Value>>, Option<String>) {
    let doc = match XmlDoc::parse(styles_xml) {
        Ok(d) => d,
        Err(_) => return (HashMap::new(), None),
    };
    // styleId → (basedOn, pPr propre)
    let mut raw: HashMap<String, (Option<String>, serde_json::Map<String, Value>)> = HashMap::new();
    let mut default_id = None;
    for st in doc.descendants().filter(|n| local(n) == "style") {
        if attr_val(&st, "type").as_deref() != Some("paragraph") {
            continue;
        }
        let Some(id) = attr_val(&st, "styleId") else { continue };
        if matches!(attr_val(&st, "default").as_deref(), Some("1") | Some("true")) {
            default_id = Some(id.clone());
        }
        let based = child(&st, "basedOn").and_then(|b| attr_val(&b, "val"));
        let ppr = child(&st, "pPr").map(|p| parse_ppr_attrs(&p)).unwrap_or_default();
        raw.insert(id, (based, ppr));
    }
    // Aplatit chaque chaîne basedOn (racine appliquée en premier, feuille écrase).
    let mut resolved = HashMap::new();
    for id in raw.keys() {
        let mut chain = Vec::new();
        let mut cur = Some(id.clone());
        let mut guard = 0;
        while let Some(c) = cur {
            if !raw.contains_key(&c) || chain.contains(&c) || guard > 25 {
                break;
            }
            chain.push(c.clone());
            cur = raw[&c].0.clone();
            guard += 1;
        }
        let mut m = serde_json::Map::new();
        for c in chain.iter().rev() {
            for (k, v) in &raw[c].1 {
                m.insert(k.clone(), v.clone());
            }
        }
        resolved.insert(id.clone(), m);
    }
    (resolved, default_id)
}

/// Propriétés de paragraphe EFFECTIVES = docDefaults < style nommé (ou défaut) <
/// pPr direct. `direct` est consommé (les valeurs explicites priment).
fn effective_para_attrs(
    fonts: &FontCtx,
    style_id: &str,
    direct: serde_json::Map<String, Value>,
) -> serde_json::Map<String, Value> {
    let mut eff = fonts.para_default.clone();
    let sid = if !style_id.is_empty() {
        Some(style_id.to_string())
    } else {
        fonts.para_style_default.clone()
    };
    if let Some(sid) = sid {
        if let Some(sm) = fonts.para_styles.get(&sid) {
            for (k, v) in sm {
                eff.insert(k.clone(), v.clone());
            }
        }
    }
    for (k, v) in direct {
        eff.insert(k, v);
    }
    eff
}

/// Inline content of a paragraph: runs and hyperlinks.
fn parse_inline(
    parent: &Node<'_, '_>,
    rels: &HashMap<String, String>,
    fonts: &FontCtx,
    theme: &HashMap<String, String>,
    media: &HashMap<String, String>,
) -> Vec<PmNode> {
    let mut out = Vec::new();
    for node in parent.children().filter(|n| n.is_element()) {
        match local(&node) {
            "r" => parse_run(&node, &[], &mut out, fonts, theme, media),
            "hyperlink" => {
                let href = attr_val(&node, "id")
                    .and_then(|rid| rels.get(&rid).cloned())
                    .or_else(|| attr_val(&node, "anchor").map(|a| format!("#{a}")));
                let extra: Vec<PmMark> = href
                    .map(|h| PmMark {
                        mark_type: "link".into(),
                        attrs: Some(json!({ "href": h })),
                    })
                    .into_iter()
                    .collect();
                for r in node.children().filter(|n| n.is_element() && local(n) == "r") {
                    parse_run(&r, &extra, &mut out, fonts, theme, media);
                }
            }
            _ => {}
        }
    }
    out
}

/// A single run: build marks from rPr, then emit text / breaks / tabs / inline images.
#[allow(clippy::too_many_arguments)]
fn parse_run(
    r: &Node<'_, '_>,
    extra_marks: &[PmMark],
    out: &mut Vec<PmNode>,
    fonts: &FontCtx,
    theme: &HashMap<String, String>,
    media: &HashMap<String, String>,
) {
    let mut marks: Vec<PmMark> = extra_marks.to_vec();
    let mut text_style = serde_json::Map::new();

    if let Some(rpr) = child(r, "rPr") {
        for m in rpr.children().filter(|n| n.is_element()) {
            match local(&m) {
                "b" if bool_on(&m) => marks.push(simple_mark("bold")),
                "i" if bool_on(&m) => marks.push(simple_mark("italic")),
                "u" if attr_val(&m, "val").as_deref() != Some("none") => {
                    marks.push(simple_mark("underline"))
                }
                "strike" if bool_on(&m) => marks.push(simple_mark("strike")),
                "vertAlign" => match attr_val(&m, "val").as_deref() {
                    Some("superscript") => marks.push(simple_mark("superscript")),
                    Some("subscript") => marks.push(simple_mark("subscript")),
                    _ => {}
                },
                "color" => {
                    if let Some(c) = attr_val(&m, "val") {
                        if c != "auto" {
                            text_style.insert(
                                "color".into(),
                                json!(format!("#{}", c.trim_start_matches('#'))),
                            );
                        }
                    }
                }
                "sz" => {
                    if let Some(v) = attr_val(&m, "val").and_then(|v| v.parse::<f64>().ok()) {
                        // DOCX half-points → points.
                        text_style.insert("fontSize".into(), json!(v / 2.0));
                    }
                }
                "rFonts" => {
                    // Nom littéral OU référence de thème (asciiTheme="minorHAnsi"…).
                    if let Some(f) = resolve_run_font(&m, fonts) {
                        text_style.insert("fontFamily".into(), json!(f));
                    }
                }
                "highlight" => {
                    if let Some(c) = attr_val(&m, "val") {
                        if c != "none" {
                            marks.push(PmMark {
                                mark_type: "highlight".into(),
                                attrs: Some(json!({ "color": highlight_color(&c) })),
                            });
                        }
                    }
                }
                // Trame de fond du texte (`<w:shd w:fill="RRGGBB">`) = couleur
                // d'arrière-plan arbitraire (distincte de `highlight`, limité aux
                // couleurs nommées). Rendue via le mark `highlight` (fond du texte).
                "shd" => {
                    if let Some(fill) = attr_val(&m, "fill") {
                        let f = fill.trim_start_matches('#');
                        if !f.eq_ignore_ascii_case("auto") && f.len() == 6 && f != "FFFFFF" {
                            marks.push(PmMark {
                                mark_type: "highlight".into(),
                                attrs: Some(json!({ "color": format!("#{f}") })),
                            });
                        }
                    }
                }
                "rStyle" if attr_val(&m, "val").as_deref() == Some("CodeChar") => {
                    marks.push(simple_mark("code"))
                }
                _ => {}
            }
        }
    }

    // Aucune police explicite sur le run → police par défaut du document (souvent
    // « Calibri » via le thème mineur). Sans ça, le corps retombe sur Arial.
    if !text_style.contains_key("fontFamily") {
        if let Some(def) = &fonts.default {
            text_style.insert("fontFamily".into(), json!(def));
        }
    }
    // Idem pour la TAILLE : un run sans `<w:sz>` hérite de la taille par défaut.
    if !text_style.contains_key("fontSize") {
        text_style.insert("fontSize".into(), json!(fonts.default_size));
    }

    if !text_style.is_empty() {
        marks.push(PmMark {
            mark_type: "textStyle".into(),
            attrs: Some(Value::Object(text_style)),
        });
    }

    for c in r.children().filter(|n| n.is_element()) {
        match local(&c) {
            "t" => {
                let txt = c.text().unwrap_or("");
                if !txt.is_empty() {
                    out.push(PmNode::text(txt, marks.clone()));
                }
            }
            "br" | "cr" => out.push(PmNode::hard_break()),
            "tab" => out.push(PmNode::text("\t", marks.clone())),
            // Dessin « aligné sur le texte » (`<wp:inline>`) → nœud inlineImage dans le flux.
            "drawing" => {
                if let Some(frame) = c.descendants().find(|n| local(n) == "inline") {
                    if let Some(node) = inline_image_node(&frame, theme, media) {
                        out.push(node);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Nœud `inlineImage` (atom inline, « aligné sur le texte ») depuis un `<wp:inline>` :
/// image matricielle (blip→data-URL) ou forme (`kbshape:`), rotation `<a:xfrm rot>`.
fn inline_image_node(
    frame: &Node<'_, '_>,
    theme: &HashMap<String, String>,
    media: &HashMap<String, String>,
) -> Option<PmNode> {
    let (mut w, mut h) = (200.0_f64, 150.0_f64);
    if let Some(ext) = frame.descendants().find(|n| local(n) == "extent") {
        if let Some(cx) = attr_val(&ext, "cx").and_then(|v| v.parse::<f64>().ok()) {
            w = (cx / EMU_PER_PX).round().max(4.0);
        }
        if let Some(cy) = attr_val(&ext, "cy").and_then(|v| v.parse::<f64>().ok()) {
            h = (cy / EMU_PER_PX).round().max(4.0);
        }
    }
    let mut attrs = serde_json::Map::new();
    attrs.insert("width".into(), json!(w));
    attrs.insert("height".into(), json!(h));
    if let Some(rot) = frame
        .descendants()
        .find(|n| local(n) == "xfrm" && n.has_attribute("rot"))
        .and_then(|x| attr_val(&x, "rot"))
        .and_then(|v| v.parse::<f64>().ok())
    {
        let deg = (rot / 60000.0).rem_euclid(360.0);
        if deg.abs() > 0.01 {
            attrs.insert("rotation".into(), json!(deg));
        }
    }
    // Image matricielle (blip) sinon forme à géométrie prédéfinie.
    let blip_src = frame
        .descendants()
        .find(|n| local(n) == "blip")
        .and_then(|b| attr_val(&b, "embed"))
        .and_then(|rid| media.get(&rid).cloned());
    if let Some(src) = blip_src {
        attrs.insert("src".into(), json!(src));
        attrs.insert("alt".into(), json!(""));
    } else if let Some(prst) = frame
        .descendants()
        .find(|n| local(n) == "prstGeom")
        .and_then(|g| attr_val(&g, "prst"))
    {
        let kind = prst_to_kind(&prst);
        let sppr = frame.descendants().find(|n| local(n) == "spPr");
        let style = frame.descendants().find(|n| local(n) == "style");
        let fill = sppr
            .as_ref()
            .and_then(|sp| sp.children().find(|n| n.is_element() && local(n) == "solidFill"))
            .and_then(|sf| first_color_hex(&sf, theme))
            .or_else(|| style.as_ref().and_then(|st| st.children().find(|n| local(n) == "fillRef")).and_then(|fr| first_color_hex(&fr, theme)))
            .unwrap_or_else(|| default_shape_fill(kind).to_string());
        let stroke = sppr
            .as_ref()
            .and_then(|sp| sp.children().find(|n| n.is_element() && local(n) == "ln"))
            .and_then(|ln| first_color_hex(&ln, theme))
            .or_else(|| style.as_ref().and_then(|st| st.children().find(|n| local(n) == "lnRef")).and_then(|lr| first_color_hex(&lr, theme)))
            .unwrap_or_else(|| "#1a73e8".to_string());
        let params = json!({ "kind": kind, "fill": fill, "stroke": stroke });
        attrs.insert("src".into(), json!(""));
        attrs.insert("alt".into(), json!(format!("kbshape:{}", urlencoding::encode(&params.to_string()))));
    } else {
        return None;
    }
    Some(PmNode {
        node_type: "inlineImage".into(),
        attrs: Some(Value::Object(attrs)),
        content: None,
        marks: None,
        text: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn parse_table(
    tbl: &Node<'_, '_>,
    rels: &HashMap<String, String>,
    numbering: &HashMap<String, bool>,
    fonts: &FontCtx,
    theme: &HashMap<String, String>,
    media: &HashMap<String, String>,
) -> PmNode {
    let mut rows = Vec::new();
    for tr in tbl.children().filter(|n| n.is_element() && local(n) == "tr") {
        let mut cells = Vec::new();
        for tc in tr.children().filter(|n| n.is_element() && local(n) == "tc") {
            let tcpr = child(&tc, "tcPr");
            let colspan = tcpr
                .as_ref()
                .and_then(|p| child(p, "gridSpan"))
                .and_then(|g| attr_val(&g, "val"))
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(1);
            // vMerge without val="restart" is a continuation cell; skip it.
            let is_continuation = tcpr
                .as_ref()
                .and_then(|p| child(p, "vMerge"))
                .map(|v| attr_val(&v, "val").as_deref() != Some("restart"))
                .unwrap_or(false);
            if is_continuation {
                continue;
            }

            let mut content = Vec::new();
            for ch in tc.children().filter(|n| n.is_element()) {
                match local(&ch) {
                    "p" => {
                        let node = match parse_paragraph(&ch, rels, numbering, fonts, theme, media) {
                            Block::Node(n) => n,
                            Block::Item { para, .. } => para,
                        };
                        content.push(node);
                    }
                    "tbl" => content.push(parse_table(&ch, rels, numbering, fonts, theme, media)),
                    _ => {}
                }
            }
            if content.is_empty() {
                content.push(PmNode::paragraph(vec![]));
            }
            cells.push(PmNode {
                node_type: "tableCell".into(),
                attrs: Some(json!({ "colspan": colspan, "rowspan": 1 })),
                content: Some(content),
                marks: None,
                text: None,
            });
        }
        if !cells.is_empty() {
            rows.push(PmNode {
                node_type: "tableRow".into(),
                attrs: None,
                content: Some(cells),
                marks: None,
                text: None,
            });
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
    PmNode {
        node_type: "table".into(),
        attrs: None,
        content: Some(rows),
        marks: None,
        text: None,
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn simple_mark(t: &str) -> PmMark {
    PmMark { mark_type: t.to_string(), attrs: None }
}

/// Toggle properties (b/i/strike) default ON; an explicit falsey val disables.
fn bool_on(n: &Node<'_, '_>) -> bool {
    !matches!(
        attr_val(n, "val").as_deref(),
        Some("0") | Some("false") | Some("off")
    )
}

fn map_align(jc: &str) -> Option<&'static str> {
    match jc {
        "center" => Some("center"),
        "right" | "end" => Some("right"),
        "both" | "distribute" => Some("justify"),
        "left" | "start" => Some("left"),
        _ => None,
    }
}

/// "Heading1".."Heading6" / "Heading 2" / "Title" → ProseMirror heading level.
fn heading_level(style: &str) -> Option<u8> {
    let s = style.to_lowercase().replace(' ', "");
    if s == "title" {
        return Some(1);
    }
    s.strip_prefix("heading")
        .and_then(|rest| rest.parse::<u8>().ok())
        .filter(|l| (1..=6).contains(l))
}

/// DOCX named highlight colour → editor hex (highlight mark `color` attr).
fn highlight_color(name: &str) -> String {
    let hex = match name {
        "yellow" => "#fff59d",
        "green" => "#a5d6a7",
        "cyan" => "#80deea",
        "magenta" => "#f48fb1",
        "blue" => "#90caf9",
        "red" => "#ef9a9a",
        "darkBlue" => "#5c6bc0",
        "darkCyan" => "#26a69a",
        "darkGreen" => "#66bb6a",
        "darkMagenta" => "#ab47bc",
        "darkRed" => "#e57373",
        "darkYellow" => "#fbc02d",
        "darkGray" => "#9e9e9e",
        "lightGray" => "#e0e0e0",
        "black" => "#bdbdbd",
        _ => "#fff59d",
    };
    hex.to_string()
}

/// Fold consecutive list items into properly nested bullet/ordered lists.
fn fold_blocks(blocks: Vec<Block>) -> Vec<PmNode> {
    struct ListFrame {
        ordered: bool,
        items: Vec<PmNode>,
    }

    /// Close open list frames down to `keep`, attaching each closed list to its
    /// parent item (or to the output when at the top level).
    fn close_lists(stack: &mut Vec<ListFrame>, keep: usize, out: &mut Vec<PmNode>) {
        while stack.len() > keep {
            let frame = stack.pop().expect("len > keep ≥ 0");
            let list_node = if frame.ordered {
                PmNode::ordered_list(frame.items)
            } else {
                PmNode::bullet_list(frame.items)
            };
            if let Some(parent) = stack.last_mut() {
                if let Some(last_item) = parent.items.last_mut() {
                    last_item.content.get_or_insert_with(Vec::new).push(list_node);
                } else {
                    parent.items.push(PmNode::list_item(vec![list_node]));
                }
            } else {
                out.push(list_node);
            }
        }
    }

    let mut out: Vec<PmNode> = Vec::new();
    let mut stack: Vec<ListFrame> = Vec::new();

    for block in blocks {
        match block {
            Block::Node(node) => {
                close_lists(&mut stack, 0, &mut out);
                out.push(node);
            }
            Block::Item { ordered, ilvl, para } => {
                let depth = ilvl as usize;
                // Collapse anything deeper than this item's level.
                close_lists(&mut stack, depth + 1, &mut out);
                // A list-type change at the same level starts a fresh list.
                if stack.len() == depth + 1 && stack[depth].ordered != ordered {
                    close_lists(&mut stack, depth, &mut out);
                }
                // Open intermediate levels up to this depth.
                while stack.len() < depth + 1 {
                    stack.push(ListFrame { ordered, items: Vec::new() });
                }
                stack[depth].items.push(PmNode::list_item(vec![para]));
            }
        }
    }
    close_lists(&mut stack, 0, &mut out);
    out
}

// ─── Static XML templates ────────────────────────────────────────────────────

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
</Types>"#;

const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
</Relationships>"#;

const DOC_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;

const STYLES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="Normal" w:default="1">
    <w:name w:val="Normal"/>
    <w:rPr><w:sz w:val="24"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading1">
    <w:name w:val="heading 1"/>
    <w:pPr><w:outlineLvl w:val="0"/></w:pPr>
    <w:rPr><w:b/><w:sz w:val="48"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading2">
    <w:name w:val="heading 2"/>
    <w:pPr><w:outlineLvl w:val="1"/></w:pPr>
    <w:rPr><w:b/><w:sz w:val="36"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading3">
    <w:name w:val="heading 3"/>
    <w:pPr><w:outlineLvl w:val="2"/></w:pPr>
    <w:rPr><w:b/><w:sz w:val="28"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading4">
    <w:name w:val="heading 4"/>
    <w:pPr><w:outlineLvl w:val="3"/></w:pPr>
    <w:rPr><w:b/><w:sz w:val="26"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading5">
    <w:name w:val="heading 5"/>
    <w:pPr><w:outlineLvl w:val="4"/></w:pPr>
    <w:rPr><w:b/><w:sz w:val="24"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading6">
    <w:name w:val="heading 6"/>
    <w:pPr><w:outlineLvl w:val="5"/></w:pPr>
    <w:rPr><w:b/><w:sz w:val="24"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Quote">
    <w:name w:val="Quote"/>
    <w:pPr><w:ind w:left="720"/></w:pPr>
    <w:rPr><w:i/><w:color w:val="595959"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Code">
    <w:name w:val="Code"/>
    <w:rPr><w:rFonts w:ascii="Courier New" w:hAnsi="Courier New"/><w:sz w:val="20"/></w:rPr>
  </w:style>
  <w:style w:type="character" w:styleId="CodeChar">
    <w:name w:val="Code Char"/>
    <w:rPr><w:rFonts w:ascii="Courier New" w:hAnsi="Courier New"/><w:sz w:val="20"/></w:rPr>
  </w:style>
  <w:style w:type="character" w:styleId="Hyperlink">
    <w:name w:val="Hyperlink"/>
    <w:rPr><w:color w:val="1155CC"/><w:u w:val="single"/></w:rPr>
  </w:style>
  <w:style w:type="numbering" w:styleId="ListBullet">
    <w:name w:val="List Bullet"/>
  </w:style>
  <w:style w:type="numbering" w:styleId="ListNumber">
    <w:name w:val="List Number"/>
  </w:style>
</w:styles>"#;
