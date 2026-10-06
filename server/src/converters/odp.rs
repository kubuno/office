/// Convertisseur ODP (OpenDocument Presentation) — export basique.
/// Chaque slide devient un <draw:page> avec ses éléments (texte, formes).
use std::io::{Cursor, Write};

use anyhow::Result;
use serde_json::Value;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
}

const ODP_MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"
                   manifest:version="1.2">
  <manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.presentation"/>
  <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
</manifest:manifest>"#;

pub struct OdpSlideData {
    pub position:   i32,
    pub background: Value,
    pub elements:   Value,
    pub notes:      String,
    pub width:      i32,
    pub height:     i32,
}

pub fn export_odp(title: &str, slides: &[OdpSlideData]) -> Result<Vec<u8>> {
    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);

    zip.start_file("mimetype", SimpleFileOptions::default().compression_method(CompressionMethod::Stored))?;
    zip.write_all(b"application/vnd.oasis.opendocument.presentation")?;

    zip.start_file("META-INF/manifest.xml", SimpleFileOptions::default())?;
    zip.write_all(ODP_MANIFEST.as_bytes())?;

    let content = build_odp_content(title, slides);
    zip.start_file("content.xml", SimpleFileOptions::default())?;
    zip.write_all(content.as_bytes())?;

    let cursor = zip.finish()?;
    Ok(cursor.into_inner())
}

fn build_odp_content(title: &str, slides: &[OdpSlideData]) -> String {
    // Use first slide dimensions or defaults
    let (w_mm, h_mm) = slides.first()
        .map(|s| (s.width as f64 / 96.0 * 25.4, s.height as f64 / 96.0 * 25.4))
        .unwrap_or((254.0, 190.5)); // 10" × 7.5"

    let mut xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content
    xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
    xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"
    xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0"
    xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
    xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
    xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0"
    xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"
    office:version="1.2">
  <office:automatic-styles>
    <style:style style:name="dp1" style:family="drawing-page"/>
    <style:style style:name="gr1" style:family="graphic">
      <style:graphic-properties draw:fill="solid" fo:border="none"/>
    </style:style>
    <style:style style:name="pr1" style:family="presentation">
      <style:graphic-properties draw:fill="none" draw:stroke="none"/>
    </style:style>
  </office:automatic-styles>
  <office:body>
    <office:presentation>
      <style:page-layout style:name="pl1">
        <style:page-layout-properties fo:page-width="{w_mm:.2}mm" fo:page-height="{h_mm:.2}mm"/>
      </style:page-layout>
"#);

    xml.push_str(&format!("      <!-- {} -->\n", escape_xml(title)));

    for (idx, slide) in slides.iter().enumerate() {
        let bg_color = slide.background
            .get("color")
            .and_then(|v| v.as_str())
            .unwrap_or("#ffffff");

        xml.push_str(&format!(
            "      <draw:page draw:name=\"Diapositive{}\" draw:style-name=\"dp1\" draw:master-page-name=\"Default\" presentation:presentation-page-layout-name=\"AL1T0\">\n",
            idx + 1
        ));
        xml.push_str(&format!(
            "        <!-- bg={} -->\n",
            escape_xml(bg_color)
        ));

        // Render elements
        if let Some(elements) = slide.elements.as_array() {
            for el in elements {
                render_element(&mut xml, el, slide.width, slide.height);
            }
        }

        // Notes
        if !slide.notes.trim().is_empty() {
            xml.push_str("        <presentation:notes>\n");
            xml.push_str("          <draw:frame presentation:style-name=\"pr1\" draw:layer=\"layout\"\n");
            xml.push_str(&format!("              svg:width=\"{w_mm:.2}mm\" svg:height=\"50mm\" svg:x=\"0mm\" svg:y=\"{h_mm:.2}mm\">\n"));
            xml.push_str("            <draw:text-box>\n");
            for line in slide.notes.lines() {
                xml.push_str(&format!("              <text:p>{}</text:p>\n", escape_xml(line)));
            }
            xml.push_str("            </draw:text-box>\n          </draw:frame>\n");
            xml.push_str("        </presentation:notes>\n");
        }

        xml.push_str("      </draw:page>\n");
    }

    xml.push_str("    </office:presentation>\n  </office:body>\n</office:document-content>\n");
    xml
}

fn render_element(xml: &mut String, el: &Value, slide_w: i32, slide_h: i32) {
    let el_type = el.get("type").and_then(|v| v.as_str()).unwrap_or("text");
    let x_px    = el.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y_px    = el.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let w_px    = el.get("width").and_then(|v| v.as_f64()).unwrap_or(200.0);
    let h_px    = el.get("height").and_then(|v| v.as_f64()).unwrap_or(50.0);
    let _ = (slide_w, slide_h);

    // Convert px → mm (96dpi)
    let x_mm = x_px / 96.0 * 25.4;
    let y_mm = y_px / 96.0 * 25.4;
    let w_mm = w_px / 96.0 * 25.4;
    let h_mm = h_px / 96.0 * 25.4;

    match el_type {
        "text" => {
            let content = el.get("content").and_then(|v| v.as_str()).unwrap_or("");
            xml.push_str(&format!(
                "        <draw:frame draw:style-name=\"gr1\" draw:layer=\"layout\"\n            svg:x=\"{x_mm:.2}mm\" svg:y=\"{y_mm:.2}mm\" svg:width=\"{w_mm:.2}mm\" svg:height=\"{h_mm:.2}mm\">\n"
            ));
            xml.push_str("          <draw:text-box>\n");
            for line in content.lines() {
                xml.push_str(&format!("            <text:p>{}</text:p>\n", escape_xml(line)));
            }
            xml.push_str("          </draw:text-box>\n        </draw:frame>\n");
        }
        "shape" => {
            let shape_type = el.get("shapeType").and_then(|v| v.as_str()).unwrap_or("rect");
            let fill = el.get("fill").and_then(|v| v.as_str()).unwrap_or("#e0e0e0");
            let label = el.get("label").and_then(|v| v.as_str()).unwrap_or("");
            let tag = if shape_type.contains("ellipse") || shape_type.contains("circle") {
                "draw:ellipse"
            } else {
                "draw:rect"
            };
            xml.push_str(&format!(
                "        <{tag} draw:style-name=\"gr1\" draw:layer=\"layout\"\n            svg:x=\"{x_mm:.2}mm\" svg:y=\"{y_mm:.2}mm\" svg:width=\"{w_mm:.2}mm\" svg:height=\"{h_mm:.2}mm\"\n            draw:fill-color=\"{}\">\n",
                escape_xml(fill)
            ));
            if !label.is_empty() {
                xml.push_str(&format!("          <text:p>{}</text:p>\n", escape_xml(label)));
            }
            xml.push_str(&format!("        </{tag}>\n"));
        }
        _ => {}
    }
}
