use std::io::{Cursor, Write};
use zip::{write::FileOptions, ZipArchive, ZipWriter};

use crate::errors::{OfficeError, Result};
use super::types::{PmMark, PmNode};

// ─── Export: ProseMirror JSON → ODT ─────────────────────────────────────────

pub fn export_odt(doc: &PmNode, title: &str) -> Result<Vec<u8>> {
    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);
    let opts = FileOptions::<()>::default().compression_method(zip::CompressionMethod::Deflated);
    let opts_uncompressed = FileOptions::<()>::default().compression_method(zip::CompressionMethod::Stored);

    // mimetype MUST be first and uncompressed
    zip.start_file("mimetype", opts_uncompressed)?;
    zip.write_all(b"application/vnd.oasis.opendocument.text")?;

    zip.start_file("META-INF/manifest.xml", opts)?;
    zip.write_all(MANIFEST_XML.as_bytes())?;

    zip.start_file("meta.xml", opts)?;
    let escaped_title = escape_xml(title);
    zip.write_all(format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
    xmlns:dc="http://purl.org/dc/elements/1.1/">
  <office:meta>
    <dc:title>{escaped_title}</dc:title>
  </office:meta>
</office:document-meta>"#).as_bytes())?;

    zip.start_file("styles.xml", opts)?;
    zip.write_all(STYLES_XML.as_bytes())?;

    zip.start_file("content.xml", opts)?;
    let body = render_body(doc);
    let content = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content
    xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
    xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
    xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
    xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0">
<office:body>
<office:text>
{body}
</office:text>
</office:body>
</office:document-content>"#);
    zip.write_all(content.as_bytes())?;

    let result = zip.finish()?;
    Ok(result.into_inner())
}

fn render_body(doc: &PmNode) -> String {
    let mut out = String::new();
    for child in doc.children() {
        out.push_str(&render_block(child));
    }
    if out.is_empty() {
        out.push_str("<text:p text:style-name=\"Text_20_Body\"/>");
    }
    out
}

fn render_block(node: &PmNode) -> String {
    match node.node_type.as_str() {
        "paragraph" => render_paragraph(node, "Text_20_Body"),
        "heading" => {
            let style = match node.heading_level() {
                1 => "Heading_20_1",
                2 => "Heading_20_2",
                3 => "Heading_20_3",
                4 => "Heading_20_4",
                5 => "Heading_20_5",
                _ => "Heading_20_6",
            };
            render_paragraph(node, style)
        }
        "bulletList" => render_list(node, "List_20_Bullet"),
        "orderedList" => render_list(node, "List_20_Number"),
        "blockquote" => {
            let mut out = String::new();
            for child in node.children() {
                out.push_str(&render_paragraph(child, "Quotations"));
            }
            out
        }
        "codeBlock" => render_code_block(node),
        _ => String::new(),
    }
}

fn render_paragraph(node: &PmNode, style: &str) -> String {
    let content = render_inline_content(node.children());
    format!(r#"<text:p text:style-name="{style}">{content}</text:p>"#)
}

fn render_code_block(node: &PmNode) -> String {
    let text: String = node.children().iter().filter_map(|n| n.text.as_deref()).collect::<Vec<_>>().join("\n");
    if text.is_empty() {
        return r#"<text:p text:style-name="Preformatted_20_Text"/>"#.to_string();
    }
    // Toutes les lignes dans un seul paragraphe séparées par <text:line-break/>
    // → round-trip sans fragmentation
    let inner: String = text.split('\n')
        .enumerate()
        .map(|(i, line)| {
            if i == 0 { escape_xml(line) } else { format!("<text:line-break/>{}", escape_xml(line)) }
        })
        .collect();
    format!(r#"<text:p text:style-name="Preformatted_20_Text">{inner}</text:p>"#)
}

fn render_list(node: &PmNode, style: &str) -> String {
    let items: String = node.children().iter().map(|item| render_list_item(item, style)).collect();
    format!(r#"<text:list text:style-name="{style}">{items}</text:list>"#)
}

fn render_list_item(node: &PmNode, _list_style: &str) -> String {
    let mut content = String::new();
    for child in node.children() {
        match child.node_type.as_str() {
            "paragraph" => {
                let inline = render_inline_content(child.children());
                content.push_str(&format!(r#"<text:p text:style-name="List_20_Contents">{inline}</text:p>"#));
            }
            "bulletList" => content.push_str(&render_list(child, "List_20_Bullet")),
            "orderedList" => content.push_str(&render_list(child, "List_20_Number")),
            _ => content.push_str(&render_block(child)),
        }
    }
    format!(r#"<text:list-item>{content}</text:list-item>"#)
}

fn render_inline_content(nodes: &[PmNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node.node_type.as_str() {
            "text" => {
                if let Some(text) = &node.text {
                    let marks = node.marks.as_deref().unwrap_or(&[]);
                    out.push_str(&render_span(text, marks));
                }
            }
            "hardBreak" => {
                out.push_str("<text:line-break/>");
            }
            _ => {}
        }
    }
    out
}

fn render_span(text: &str, marks: &[PmMark]) -> String {
    if marks.is_empty() {
        return escape_xml(text);
    }
    let style = marks_to_style(marks);
    let escaped = escape_xml(text);
    format!(r#"<text:span text:style-name="{style}">{escaped}</text:span>"#)
}

fn marks_to_style(marks: &[PmMark]) -> String {
    let mut parts = Vec::new();
    for mark in marks {
        match mark.mark_type.as_str() {
            "bold"      => parts.push("Bold"),
            "italic"    => parts.push("Italic"),
            "underline" => parts.push("Underline"),
            "strike"    => parts.push("Strikethrough"),
            "code"      => parts.push("Code_20_Char"),
            _           => {}
        }
    }
    if parts.is_empty() {
        "Default_20_Style".into()
    } else {
        parts.join("_")
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ─── Import: ODT → ProseMirror JSON ─────────────────────────────────────────

pub fn import_odt(data: &[u8]) -> Result<PmNode> {
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|e| OfficeError::Conversion(format!("ZIP invalide: {e}")))?;

    let xml = {
        let mut file = archive.by_name("content.xml")
            .map_err(|_| OfficeError::Conversion("content.xml introuvable".into()))?;
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut file, &mut buf)
            .map_err(|e| OfficeError::Conversion(format!("Lecture échouée: {e}")))?;
        buf
    };

    parse_odt_xml(&xml)
}

fn parse_odt_xml(xml: &str) -> Result<PmNode> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut nodes: Vec<PmNode> = Vec::new();
    let mut current_runs: Vec<PmNode> = Vec::new();
    let mut current_text = String::new();
    let mut current_style_name = String::new();

    // Span/run stack for nested text:span
    let mut span_marks_stack: Vec<Vec<PmMark>> = Vec::new();

    let mut in_text = false;
    let mut in_list = false;
    let mut in_list_item = false;
    let mut list_style_name = String::new();

    // List item accumulator
    let mut list_items: Vec<PmNode> = Vec::new();
    let mut current_item_paras: Vec<PmNode> = Vec::new();

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let ns_name = e.name();
                let local = std::str::from_utf8(ns_name.local_name().into_inner()).unwrap_or("").to_string();

                match local.as_str() {
                    "p" => {
                        in_text = true;
                        current_runs.clear();
                        current_style_name = String::new();
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"style-name" {
                                current_style_name = String::from_utf8_lossy(&attr.value).into_owned();
                            }
                        }
                    }
                    "list" => {
                        in_list = true;
                        list_items.clear();
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"style-name" {
                                list_style_name = String::from_utf8_lossy(&attr.value).into_owned();
                            }
                        }
                    }
                    "list-item" => {
                        in_list_item = true;
                        current_item_paras.clear();
                    }
                    "span" if in_text => {
                        // Push current accumulated text as run first
                        if !current_text.is_empty() {
                            let marks = span_marks_stack.last().cloned().unwrap_or_default();
                            current_runs.push(PmNode::text(&current_text, marks));
                            current_text.clear();
                        }
                        let mut style_name = String::new();
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"style-name" {
                                style_name = String::from_utf8_lossy(&attr.value).into_owned();
                            }
                        }
                        let marks = style_to_marks(&style_name);
                        span_marks_stack.push(marks);
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let local = std::str::from_utf8(e.name().local_name().into_inner()).unwrap_or("").to_string();
                if local == "line-break" && in_text {
                    if !current_text.is_empty() {
                        let marks = span_marks_stack.last().cloned().unwrap_or_default();
                        current_runs.push(PmNode::text(&current_text, marks));
                        current_text.clear();
                    }
                    current_runs.push(PmNode::hard_break());
                }
            }
            Ok(Event::End(ref e)) => {
                let local = std::str::from_utf8(e.name().local_name().into_inner()).unwrap_or("").to_string();
                match local.as_str() {
                    "span" if in_text && !span_marks_stack.is_empty() => {
                        if !current_text.is_empty() {
                            let marks = span_marks_stack.last().cloned().unwrap_or_default();
                            current_runs.push(PmNode::text(&current_text, marks));
                            current_text.clear();
                        }
                        span_marks_stack.pop();
                    }
                    "p" if in_text => {
                        in_text = false;
                        if !current_text.is_empty() {
                            let marks = span_marks_stack.last().cloned().unwrap_or_default();
                            current_runs.push(PmNode::text(&current_text, marks));
                            current_text.clear();
                        }
                        span_marks_stack.clear();

                        let runs = std::mem::take(&mut current_runs);
                        let pm_node = style_name_to_node(&current_style_name, runs);

                        if in_list_item {
                            current_item_paras.push(pm_node);
                        } else if in_list {
                            // standalone paragraph inside list context (unusual)
                            list_items.push(PmNode::list_item(vec![pm_node]));
                        } else {
                            nodes.push(pm_node);
                        }
                    }
                    "list-item" => {
                        in_list_item = false;
                        let paras = std::mem::take(&mut current_item_paras);
                        list_items.push(PmNode::list_item(paras));
                    }
                    "list" => {
                        in_list = false;
                        let items = std::mem::take(&mut list_items);
                        let list_node = if list_style_name.contains("Number") {
                            PmNode::ordered_list(items)
                        } else {
                            PmNode::bullet_list(items)
                        };
                        nodes.push(list_node);
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if in_text {
                    let t = e.unescape().unwrap_or_default();
                    current_text.push_str(&t);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(OfficeError::Conversion(format!("XML parse error: {e}"))),
            _ => {}
        }
        buf.clear();
    }

    if nodes.is_empty() {
        nodes.push(PmNode::paragraph(vec![]));
    }

    Ok(PmNode::doc(nodes))
}

fn style_name_to_node(style: &str, runs: Vec<PmNode>) -> PmNode {
    match style {
        "Heading_20_1" | "Heading 1" => PmNode::heading(1, runs),
        "Heading_20_2" | "Heading 2" => PmNode::heading(2, runs),
        "Heading_20_3" | "Heading 3" => PmNode::heading(3, runs),
        "Heading_20_4" | "Heading 4" => PmNode::heading(4, runs),
        "Heading_20_5" | "Heading 5" => PmNode::heading(5, runs),
        "Heading_20_6" | "Heading 6" => PmNode::heading(6, runs),
        "Quotations" | "Quotation" => PmNode::blockquote(vec![PmNode::paragraph(runs)]),
        "Preformatted_20_Text" | "Preformatted Text" => {
            // Les <text:line-break/> ont été convertis en hardBreak nodes ;
            // on les traduit en \n pour reconstituer le bloc de code original.
            let text: String = runs.iter().map(|n| {
                if n.node_type == "hardBreak" { "\n".to_string() }
                else { n.text.as_deref().unwrap_or("").to_string() }
            }).collect();
            PmNode::code_block(text)
        }
        _ => PmNode::paragraph(runs),
    }
}

fn style_to_marks(style: &str) -> Vec<PmMark> {
    let mut marks = Vec::new();
    let lower = style.to_lowercase();
    if lower.contains("bold") { marks.push(PmMark { mark_type: "bold".into(), attrs: None }); }
    if lower.contains("italic") { marks.push(PmMark { mark_type: "italic".into(), attrs: None }); }
    if lower.contains("underline") { marks.push(PmMark { mark_type: "underline".into(), attrs: None }); }
    if lower.contains("strikethrough") || lower.contains("strike") {
        marks.push(PmMark { mark_type: "strike".into(), attrs: None });
    }
    if lower.contains("code") { marks.push(PmMark { mark_type: "code".into(), attrs: None }); }
    marks
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use zip::ZipArchive;
    use std::io::{Cursor, Read};

    // ── helpers ───────────────────────────────────────────────────────────────

    fn plain(t: &str) -> PmNode { PmNode::text(t, vec![]) }

    fn bold(t: &str) -> PmNode {
        PmNode::text(t, vec![PmMark { mark_type: "bold".into(), attrs: None }])
    }

    fn italic(t: &str) -> PmNode {
        PmNode::text(t, vec![PmMark { mark_type: "italic".into(), attrs: None }])
    }

    fn para(children: Vec<PmNode>) -> PmNode { PmNode::paragraph(children) }
    fn doc(nodes: Vec<PmNode>) -> PmNode { PmNode::doc(nodes) }

    fn roundtrip(pm: &PmNode) -> PmNode {
        let bytes = export_odt(pm, "Test").expect("export_odt failed");
        import_odt(&bytes).expect("import_odt failed")
    }

    fn collect_texts(node: &PmNode) -> Vec<String> {
        let mut out = Vec::new();
        fn walk(n: &PmNode, out: &mut Vec<String>) {
            if let Some(t) = &n.text { out.push(t.clone()); }
            for c in n.children() { walk(c, out); }
        }
        walk(node, &mut out);
        out
    }

    // ── structure ZIP ─────────────────────────────────────────────────────────

    #[test]
    fn export_produces_valid_zip() {
        let pm = doc(vec![para(vec![plain("hello")])]);
        let bytes = export_odt(&pm, "Mon titre").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(bytes)).expect("ZIP invalide");
        assert!(archive.by_name("mimetype").is_ok());
        assert!(archive.by_name("content.xml").is_ok());
        assert!(archive.by_name("META-INF/manifest.xml").is_ok());
        assert!(archive.by_name("styles.xml").is_ok());
    }

    #[test]
    fn mimetype_entry_is_correct() {
        let pm = doc(vec![para(vec![plain("x")])]);
        let bytes = export_odt(&pm, "T").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut mime = archive.by_name("mimetype").unwrap();
        let mut s = String::new();
        mime.read_to_string(&mut s).unwrap();
        assert_eq!(s, "application/vnd.oasis.opendocument.text");
    }

    #[test]
    fn xml_special_chars_escaped() {
        let pm = doc(vec![para(vec![plain("a & <b> \"c\"")])]);
        let bytes = export_odt(&pm, "T").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut f = archive.by_name("content.xml").unwrap();
        let mut xml = String::new();
        f.read_to_string(&mut xml).unwrap();
        assert!(xml.contains("&amp;"), "& doit être échappé en &amp;");
        assert!(xml.contains("&lt;"),  "< doit être échappé en &lt;");
        assert!(xml.contains("&quot;"), "\" doit être échappé en &quot;");
    }

    // ── round-trips corrects ──────────────────────────────────────────────────

    #[test]
    fn paragraph_text_roundtrip() {
        let pm = doc(vec![para(vec![plain("Bonjour le monde")])]);
        let result = roundtrip(&pm);
        let texts = collect_texts(&result);
        assert!(texts.contains(&"Bonjour le monde".to_string()));
    }

    #[test]
    fn multiple_paragraphs_roundtrip() {
        let pm = doc(vec![
            para(vec![plain("Premier")]),
            para(vec![plain("Deuxième")]),
        ]);
        let result = roundtrip(&pm);
        assert_eq!(result.children().len(), 2);
        let texts = collect_texts(&result);
        assert!(texts.contains(&"Premier".to_string()));
        assert!(texts.contains(&"Deuxième".to_string()));
    }

    #[test]
    fn heading_level_roundtrip() {
        for level in 1u8..=6 {
            let pm = doc(vec![PmNode::heading(level, vec![plain(&format!("Titre {level}"))])]);
            let result = roundtrip(&pm);
            let h = &result.children()[0];
            assert_eq!(h.node_type, "heading", "niveau {level}");
            assert_eq!(h.heading_level(), level, "niveau {level}");
            assert!(collect_texts(&result).contains(&format!("Titre {level}")));
        }
    }

    #[test]
    fn bold_mark_roundtrip() {
        let pm = doc(vec![para(vec![bold("gras")])]);
        let result = roundtrip(&pm);
        let para_node = &result.children()[0];
        let text_node = &para_node.children()[0];
        assert!(text_node.has_mark("bold"), "le mark bold doit être préservé");
        assert_eq!(text_node.text.as_deref(), Some("gras"));
    }

    #[test]
    fn italic_mark_roundtrip() {
        let pm = doc(vec![para(vec![italic("italique")])]);
        let result = roundtrip(&pm);
        let text_node = &result.children()[0].children()[0];
        assert!(text_node.has_mark("italic"));
    }

    #[test]
    fn mixed_inline_marks_roundtrip() {
        let pm = doc(vec![para(vec![
            plain("normal "),
            bold("gras "),
            italic("italique"),
        ])]);
        let result = roundtrip(&pm);
        let texts = collect_texts(&result);
        assert!(texts.iter().any(|t| t.contains("normal")));
        assert!(texts.iter().any(|t| t.contains("gras")));
        assert!(texts.iter().any(|t| t.contains("italique")));
    }

    #[test]
    fn bullet_list_roundtrip() {
        let pm = doc(vec![PmNode::bullet_list(vec![
            PmNode::list_item(vec![para(vec![plain("item 1")])]),
            PmNode::list_item(vec![para(vec![plain("item 2")])]),
        ])]);
        let result = roundtrip(&pm);
        assert_eq!(result.children().len(), 1);
        let list = &result.children()[0];
        assert_eq!(list.node_type, "bulletList");
        assert_eq!(list.children().len(), 2);
        let texts = collect_texts(&result);
        assert!(texts.contains(&"item 1".to_string()));
        assert!(texts.contains(&"item 2".to_string()));
    }

    #[test]
    fn ordered_list_roundtrip() {
        let pm = doc(vec![PmNode::ordered_list(vec![
            PmNode::list_item(vec![para(vec![plain("a")])]),
            PmNode::list_item(vec![para(vec![plain("b")])]),
        ])]);
        let result = roundtrip(&pm);
        let list = &result.children()[0];
        assert_eq!(list.node_type, "orderedList");
    }

    #[test]
    fn blockquote_roundtrip() {
        let pm = doc(vec![PmNode::blockquote(vec![para(vec![plain("citation")])])]);
        let result = roundtrip(&pm);
        let bq = &result.children()[0];
        assert_eq!(bq.node_type, "blockquote");
        assert!(collect_texts(&result).contains(&"citation".to_string()));
    }

    #[test]
    fn empty_doc_roundtrip() {
        let pm = doc(vec![]);
        let result = roundtrip(&pm);
        // Un doc vide doit produire au moins un paragraphe vide (non-crash)
        assert!(!result.children().is_empty());
    }

    // ── limitation documentée : code block multi-ligne ────────────────────────

    #[test]
    fn single_line_codeblock_roundtrip() {
        let pm = doc(vec![PmNode::code_block("print('hello')")]);
        let result = roundtrip(&pm);
        assert_eq!(result.children()[0].node_type, "codeBlock");
        assert!(collect_texts(&result).contains(&"print('hello')".to_string()));
    }

    #[test]
    fn multiline_codeblock_roundtrip() {
        let pm = doc(vec![PmNode::code_block("ligne1\nligne2\nligne3")]);
        let result = roundtrip(&pm);
        // Un seul codeBlock avec le texte multi-ligne préservé
        assert_eq!(result.children().len(), 1, "un seul codeBlock attendu");
        assert_eq!(result.children()[0].node_type, "codeBlock");
        let texts = collect_texts(&result);
        assert_eq!(texts[0], "ligne1\nligne2\nligne3");
    }
}

// ─── Static XML templates ────────────────────────────────────────────────────

const MANIFEST_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0">
  <manifest:file-entry manifest:media-type="application/vnd.oasis.opendocument.text" manifest:full-path="/"/>
  <manifest:file-entry manifest:media-type="text/xml" manifest:full-path="content.xml"/>
  <manifest:file-entry manifest:media-type="text/xml" manifest:full-path="styles.xml"/>
  <manifest:file-entry manifest:media-type="text/xml" manifest:full-path="meta.xml"/>
</manifest:manifest>"#;

const STYLES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles
    xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
    xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
    xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
    xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0">
<office:styles>
  <style:style style:name="Text_20_Body" style:family="paragraph">
    <style:paragraph-properties fo:margin-bottom="0.212cm"/>
    <style:text-properties fo:font-size="12pt"/>
  </style:style>
  <style:style style:name="Heading_20_1" style:family="paragraph">
    <style:text-properties fo:font-size="24pt" fo:font-weight="bold"/>
  </style:style>
  <style:style style:name="Heading_20_2" style:family="paragraph">
    <style:text-properties fo:font-size="18pt" fo:font-weight="bold"/>
  </style:style>
  <style:style style:name="Heading_20_3" style:family="paragraph">
    <style:text-properties fo:font-size="14pt" fo:font-weight="bold"/>
  </style:style>
  <style:style style:name="Heading_20_4" style:family="paragraph">
    <style:text-properties fo:font-size="13pt" fo:font-weight="bold"/>
  </style:style>
  <style:style style:name="Heading_20_5" style:family="paragraph">
    <style:text-properties fo:font-size="12pt" fo:font-weight="bold"/>
  </style:style>
  <style:style style:name="Heading_20_6" style:family="paragraph">
    <style:text-properties fo:font-size="12pt" fo:font-weight="bold"/>
  </style:style>
  <style:style style:name="Quotations" style:family="paragraph">
    <style:paragraph-properties fo:margin-left="1cm"/>
    <style:text-properties fo:font-style="italic"/>
  </style:style>
  <style:style style:name="Preformatted_20_Text" style:family="paragraph">
    <style:text-properties style:font-name="Courier New" fo:font-size="10pt"/>
  </style:style>
  <style:style style:name="List_20_Contents" style:family="paragraph">
    <style:text-properties fo:font-size="12pt"/>
  </style:style>
  <style:style style:name="Bold" style:family="text">
    <style:text-properties fo:font-weight="bold"/>
  </style:style>
  <style:style style:name="Italic" style:family="text">
    <style:text-properties fo:font-style="italic"/>
  </style:style>
  <style:style style:name="Underline" style:family="text">
    <style:text-properties style:text-underline-style="solid"/>
  </style:style>
  <style:style style:name="Strikethrough" style:family="text">
    <style:text-properties style:text-line-through-style="solid"/>
  </style:style>
  <style:style style:name="Code_20_Char" style:family="text">
    <style:text-properties style:font-name="Courier New" fo:font-size="10pt"/>
  </style:style>
  <style:style style:name="Bold_Italic" style:family="text">
    <style:text-properties fo:font-weight="bold" fo:font-style="italic"/>
  </style:style>
  <style:style style:name="Bold_Underline" style:family="text">
    <style:text-properties fo:font-weight="bold" style:text-underline-style="solid"/>
  </style:style>
  <style:style style:name="Italic_Underline" style:family="text">
    <style:text-properties fo:font-style="italic" style:text-underline-style="solid"/>
  </style:style>
  <style:style style:name="Bold_Italic_Underline" style:family="text">
    <style:text-properties fo:font-weight="bold" fo:font-style="italic" style:text-underline-style="solid"/>
  </style:style>
</office:styles>
<text:list-style style:name="List_20_Bullet">
  <text:list-level-style-bullet text:level="1" text:bullet-char="•"/>
  <text:list-level-style-bullet text:level="2" text:bullet-char="◦"/>
  <text:list-level-style-bullet text:level="3" text:bullet-char="▪"/>
</text:list-style>
<text:list-style style:name="List_20_Number">
  <text:list-level-style-number text:level="1" style:num-format="1" style:num-suffix="."/>
  <text:list-level-style-number text:level="2" style:num-format="a" style:num-suffix="."/>
  <text:list-level-style-number text:level="3" style:num-format="i" style:num-suffix="."/>
</text:list-style>
</office:document-styles>"#;
