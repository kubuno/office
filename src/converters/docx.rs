use std::io::{Cursor, Write};
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

pub fn import_docx(data: &[u8]) -> Result<PmNode> {
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|e| OfficeError::Conversion(format!("ZIP invalide: {e}")))?;

    let xml = {
        let mut file = archive.by_name("word/document.xml")
            .map_err(|_| OfficeError::Conversion("word/document.xml introuvable".into()))?;
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut file, &mut buf)
            .map_err(|e| OfficeError::Conversion(format!("Lecture échouée: {e}")))?;
        buf
    };

    parse_docx_xml(&xml)
}

fn parse_docx_xml(xml: &str) -> Result<PmNode> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut nodes: Vec<PmNode> = Vec::new();
    let mut current_runs: Vec<PmNode> = Vec::new();
    let mut current_marks: Vec<PmMark> = Vec::new();
    let mut current_text = String::new();

    // Paragraph state
    let mut in_para = false;
    let mut in_run = false;
    let mut in_rpr = false;
    let mut para_style = "Normal".to_string();

    // List state
    let mut list_num_id: Option<String> = None;
    let mut list_ilvl: u8 = 0;
    let mut in_num_pr = false;
    let mut in_para_pr = false;

    // Nesting stack for lists
    let mut list_stack: Vec<(String, u8, Vec<PmNode>)> = Vec::new(); // (list_type, ilvl, items)

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = std::str::from_utf8(e.name().local_name().into_inner()).unwrap_or("").to_string();
                match name.as_str() {
                    "p" => {
                        in_para = true;
                        para_style = "Normal".to_string();
                        list_num_id = None;
                        list_ilvl = 0;
                        current_runs.clear();
                    }
                    "pPr" => { in_para_pr = true; }
                    "pStyle" if in_para_pr => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"val" {
                                para_style = String::from_utf8_lossy(&attr.value).into_owned();
                            }
                        }
                    }
                    "numId" if in_num_pr => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"val" {
                                list_num_id = Some(String::from_utf8_lossy(&attr.value).into_owned());
                            }
                        }
                    }
                    "ilvl" if in_num_pr => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"val" {
                                list_ilvl = String::from_utf8_lossy(&attr.value).parse().unwrap_or(0);
                            }
                        }
                    }
                    "numPr" if in_para_pr => { in_num_pr = true; }
                    "r" if in_para => {
                        in_run = true;
                        current_marks.clear();
                        current_text.clear();
                    }
                    "rPr" if in_run => { in_rpr = true; }
                    "b" if in_rpr => {
                        current_marks.push(PmMark { mark_type: "bold".into(), attrs: None });
                    }
                    "i" if in_rpr => {
                        current_marks.push(PmMark { mark_type: "italic".into(), attrs: None });
                    }
                    "u" if in_rpr => {
                        current_marks.push(PmMark { mark_type: "underline".into(), attrs: None });
                    }
                    "strike" if in_rpr => {
                        current_marks.push(PmMark { mark_type: "strike".into(), attrs: None });
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let name = std::str::from_utf8(e.name().local_name().into_inner()).unwrap_or("").to_string();
                match name.as_str() {
                    "br" if in_run => {
                        if !current_text.is_empty() {
                            current_runs.push(PmNode::text(&current_text, current_marks.clone()));
                            current_text.clear();
                        }
                        current_runs.push(PmNode::hard_break());
                    }
                    "b" if in_rpr => {
                        current_marks.push(PmMark { mark_type: "bold".into(), attrs: None });
                    }
                    "i" if in_rpr => {
                        current_marks.push(PmMark { mark_type: "italic".into(), attrs: None });
                    }
                    "u" if in_rpr => {
                        current_marks.push(PmMark { mark_type: "underline".into(), attrs: None });
                    }
                    "strike" if in_rpr => {
                        current_marks.push(PmMark { mark_type: "strike".into(), attrs: None });
                    }
                    "pStyle" if in_para_pr => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"val" {
                                para_style = String::from_utf8_lossy(&attr.value).into_owned();
                            }
                        }
                    }
                    "numId" if in_num_pr => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"val" {
                                list_num_id = Some(String::from_utf8_lossy(&attr.value).into_owned());
                            }
                        }
                    }
                    "ilvl" if in_num_pr => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"val" {
                                list_ilvl = String::from_utf8_lossy(&attr.value).parse().unwrap_or(0);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = std::str::from_utf8(e.name().local_name().into_inner()).unwrap_or("").to_string();
                match name.as_str() {
                    "pPr" => { in_para_pr = false; }
                    "numPr" => { in_num_pr = false; }
                    "rPr" => { in_rpr = false; }
                    "t" => {} // text already captured in Text event
                    "r" if in_run => {
                        in_run = false;
                        if !current_text.is_empty() {
                            current_runs.push(PmNode::text(&current_text, current_marks.clone()));
                            current_text.clear();
                        }
                    }
                    "p" if in_para => {
                        in_para = false;
                        let runs = std::mem::take(&mut current_runs);

                        if let Some(ref num_id) = list_num_id.clone() {
                            // This paragraph is a list item
                            let list_type = if num_id == "1" { "bulletList" } else { "orderedList" };
                            let item = PmNode::list_item(vec![PmNode::paragraph(runs)]);

                            if list_stack.is_empty() || list_stack.last().map(|(_, il, _)| *il) != Some(list_ilvl) {
                                // Flush existing list if level changed
                                while list_stack.last().map(|(_, il, _)| *il).unwrap_or(0) > list_ilvl {
                                    if let Some((ltype, _, items)) = list_stack.pop() {
                                        let list_node = if ltype == "bulletList" {
                                            PmNode::bullet_list(items)
                                        } else {
                                            PmNode::ordered_list(items)
                                        };
                                        if let Some((_, _, parent_items)) = list_stack.last_mut() {
                                            if let Some(last) = parent_items.last_mut() {
                                                if let Some(ref mut c) = last.content {
                                                    c.push(list_node);
                                                }
                                            }
                                        } else {
                                            nodes.push(list_node);
                                        }
                                    }
                                }
                                if list_stack.last().map(|(_, il, _)| *il) != Some(list_ilvl) {
                                    list_stack.push((list_type.to_string(), list_ilvl, vec![item]));
                                } else if let Some((_, _, items)) = list_stack.last_mut() {
                                    items.push(item);
                                }
                            } else if let Some((_, _, items)) = list_stack.last_mut() {
                                items.push(item);
                            }
                        } else {
                            // Flush pending list
                            flush_lists(&mut list_stack, &mut nodes);

                            let pm_node = match para_style.as_str() {
                                "Heading1" => PmNode::heading(1, runs),
                                "Heading2" => PmNode::heading(2, runs),
                                "Heading3" => PmNode::heading(3, runs),
                                "Heading4" => PmNode::heading(4, runs),
                                "Heading5" => PmNode::heading(5, runs),
                                "Heading6" => PmNode::heading(6, runs),
                                "Quote"    => PmNode::blockquote(vec![PmNode::paragraph(runs)]),
                                "Code"     => {
                                    let text: String = runs.iter().filter_map(|n| n.text.as_deref()).collect::<Vec<_>>().join("");
                                    PmNode::code_block(text)
                                }
                                _ => PmNode::paragraph(runs),
                            };
                            nodes.push(pm_node);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if in_run {
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

    // Flush remaining list
    flush_lists(&mut list_stack, &mut nodes);

    if nodes.is_empty() {
        nodes.push(PmNode::paragraph(vec![]));
    }

    Ok(PmNode::doc(nodes))
}

fn flush_lists(stack: &mut Vec<(String, u8, Vec<PmNode>)>, nodes: &mut Vec<PmNode>) {
    while let Some((ltype, _, items)) = stack.pop() {
        let list_node = if ltype == "bulletList" {
            PmNode::bullet_list(items)
        } else {
            PmNode::ordered_list(items)
        };
        if let Some((_, _, parent_items)) = stack.last_mut() {
            if let Some(last) = parent_items.last_mut() {
                if let Some(ref mut c) = last.content {
                    c.push(list_node);
                }
            }
        } else {
            nodes.push(list_node);
        }
    }
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
