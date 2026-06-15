/// Convertisseur SVG pour les diagrammes Kubuno.
/// Rend les shapes et connectors d'une page de diagramme en SVG.
use anyhow::Result;
use serde_json::Value;

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
}

pub struct DiagramPageData {
    pub name:     String,
    pub bg_color: String,
    pub width:    i32,
    pub height:   i32,
    pub data:     Value,
}

/// Exporte toutes les pages d'un diagramme en un seul SVG multi-page (pages empilées verticalement).
pub fn export_svg(title: &str, pages: &[DiagramPageData]) -> Result<Vec<u8>> {
    if pages.is_empty() {
        return Ok(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_vec());
    }

    let total_w = pages.iter().map(|p| p.width).max().unwrap_or(800);
    let total_h: i32 = pages.iter().map(|p| p.height + 40).sum(); // 40px gap between pages

    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"
     width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}">
  <title>{}</title>
  <defs>
    <marker id="arrowEnd" markerWidth="10" markerHeight="7" refX="9" refY="3.5" orient="auto">
      <polygon points="0 0, 10 3.5, 0 7" fill="#6c8ebf"/>
    </marker>
  </defs>
"##,
        escape_xml(title)
    );

    let mut offset_y = 0i32;
    for (page_idx, page) in pages.iter().enumerate() {
        svg.push_str(&format!(
            "  <g id=\"page-{}\" transform=\"translate(0,{})\">\n",
            page_idx, offset_y
        ));

        // Background
        svg.push_str(&format!(
            "    <rect width=\"{}\" height=\"{}\" fill=\"{}\"/>\n",
            page.width, page.height, escape_xml(&page.bg_color)
        ));

        // Page label
        svg.push_str(&format!(
            "    <text x=\"8\" y=\"16\" font-size=\"11\" font-family=\"Inter,sans-serif\" fill=\"#888\">{}</text>\n",
            escape_xml(&page.name)
        ));

        let shapes     = page.data.get("shapes").and_then(|v| v.as_array());
        let connectors = page.data.get("connectors").and_then(|v| v.as_array());

        // Build shape center map for connectors
        let mut shape_centers: std::collections::HashMap<&str, (f64, f64)> = std::collections::HashMap::new();
        if let Some(shapes) = shapes {
            for shape in shapes {
                if let (Some(id), Some(x), Some(y), Some(w), Some(h)) = (
                    shape.get("id").and_then(|v| v.as_str()),
                    shape.get("x").and_then(|v| v.as_f64()),
                    shape.get("y").and_then(|v| v.as_f64()),
                    shape.get("w").and_then(|v| v.as_f64()),
                    shape.get("h").and_then(|v| v.as_f64()),
                ) {
                    shape_centers.insert(id, (x + w / 2.0, y + h / 2.0));
                }
            }
        }

        // Draw connectors first (below shapes)
        if let Some(connectors) = connectors {
            for conn in connectors {
                render_connector(&mut svg, conn, &shape_centers);
            }
        }

        // Draw shapes
        if let Some(shapes) = page.data.get("shapes").and_then(|v| v.as_array()) {
            for shape in shapes {
                render_shape(&mut svg, shape);
            }
        }

        svg.push_str("  </g>\n");
        offset_y += page.height + 40;
    }

    svg.push_str("</svg>\n");
    Ok(svg.into_bytes())
}

fn render_shape(svg: &mut String, shape: &Value) {
    let x = shape.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y = shape.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let w = shape.get("w").and_then(|v| v.as_f64()).unwrap_or(100.0);
    let h = shape.get("h").and_then(|v| v.as_f64()).unwrap_or(50.0);
    let shape_type = shape.get("type").and_then(|v| v.as_str()).unwrap_or("rect");
    let label = shape.get("label").and_then(|v| v.as_str()).unwrap_or("");
    let style = shape.get("style");
    let fill   = style.and_then(|s| s.get("fillColor")).and_then(|v| v.as_str()).unwrap_or("#dae8fc");
    let stroke = style.and_then(|s| s.get("strokeColor")).and_then(|v| v.as_str()).unwrap_or("#6c8ebf");
    let stroke_w = style.and_then(|s| s.get("strokeWidth")).and_then(|v| v.as_f64()).unwrap_or(1.5);

    let shape_svg = if shape_type.contains("ellipse") || shape_type.contains("circle") {
        format!(
            "    <ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/>\n",
            x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0, escape_xml(fill), escape_xml(stroke), stroke_w
        )
    } else if shape_type.contains("diamond") || shape_type.contains("rhombus") {
        let points = format!(
            "{},{} {},{} {},{} {},{}",
            x + w / 2.0, y,
            x + w, y + h / 2.0,
            x + w / 2.0, y + h,
            x, y + h / 2.0
        );
        format!(
            "    <polygon points=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/>\n",
            points, escape_xml(fill), escape_xml(stroke), stroke_w
        )
    } else {
        format!(
            "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"4\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/>\n",
            x, y, w, h, escape_xml(fill), escape_xml(stroke), stroke_w
        )
    };
    svg.push_str(&shape_svg);

    // Label
    if !label.is_empty() {
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\" text-anchor=\"middle\" dominant-baseline=\"middle\" font-size=\"12\" font-family=\"Inter,sans-serif\" fill=\"#000000\">{}</text>\n",
            x + w / 2.0, y + h / 2.0, escape_xml(label)
        ));
    }
}

fn render_connector(
    svg: &mut String,
    conn: &Value,
    shape_centers: &std::collections::HashMap<&str, (f64, f64)>,
) {
    let src_id = conn.get("sourceId").and_then(|v| v.as_str());
    let tgt_id = conn.get("targetId").and_then(|v| v.as_str());

    let src = src_id
        .and_then(|id| shape_centers.get(id).copied())
        .or_else(|| {
            let p = conn.get("sourcePoint")?;
            Some((p.get("x")?.as_f64()?, p.get("y")?.as_f64()?))
        });
    let tgt = tgt_id
        .and_then(|id| shape_centers.get(id).copied())
        .or_else(|| {
            let p = conn.get("targetPoint")?;
            Some((p.get("x")?.as_f64()?, p.get("y")?.as_f64()?))
        });

    if let (Some((x1, y1)), Some((x2, y2))) = (src, tgt) {
        let style   = conn.get("style");
        let color   = style.and_then(|s| s.get("strokeColor")).and_then(|v| v.as_str()).unwrap_or("#6c8ebf");
        let sw      = style.and_then(|s| s.get("strokeWidth")).and_then(|v| v.as_f64()).unwrap_or(1.5);
        let arrow   = style.and_then(|s| s.get("arrowEnd")).and_then(|v| v.as_str()).unwrap_or("block");
        let label   = conn.get("label").and_then(|v| v.as_str()).unwrap_or("");
        let marker  = if arrow != "none" { " marker-end=\"url(#arrowEnd)\"" } else { "" };

        // Waypoints
        let waypoints: Vec<(f64, f64)> = conn.get("waypoints")
            .and_then(|v| v.as_array())
            .map(|pts| {
                pts.iter().filter_map(|p| {
                    Some((p.get("x")?.as_f64()?, p.get("y")?.as_f64()?))
                }).collect()
            })
            .unwrap_or_default();

        let points_str = if waypoints.is_empty() {
            format!("{x1},{y1} {x2},{y2}")
        } else {
            let mut pts = format!("{x1},{y1}");
            for (wx, wy) in &waypoints {
                pts.push_str(&format!(" {wx},{wy}"));
            }
            pts.push_str(&format!(" {x2},{y2}"));
            pts
        };

        svg.push_str(&format!(
            "    <polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" {}/>\n",
            points_str, escape_xml(color), sw, marker.trim()
        ));

        if !label.is_empty() {
            let mid_x = (x1 + x2) / 2.0;
            let mid_y = (y1 + y2) / 2.0;
            svg.push_str(&format!(
                "    <text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-size=\"11\" font-family=\"Inter,sans-serif\" fill=\"#555\">{}</text>\n",
                mid_x, mid_y - 4.0, escape_xml(label)
            ));
        }
    }
}
