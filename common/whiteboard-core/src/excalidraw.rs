//! Excalidraw (`.excalidraw` JSON) import - a port of `office/web/src/whiteboard-excalidraw.ts`.
//!
//! Maps the subset of Excalidraw scene elements the board consumes onto its own [`WbElement`] / [`Stroke`]
//! model. Ids and the stroke timestamp come from the caller (the core reads no clock, no random source).

use serde_json::{json, Value as Json};

use crate::model::{Stroke, WbElement};

const DEFAULT_STROKE: &str = "#1e1e1e";
const DEFAULT_FILL: &str = "transparent";

/// The result of an import.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Imported {
    pub elements: Vec<WbElement>,
    pub strokes: Vec<Stroke>,
    /// A parse failure (the web returns the elements it got plus an error string).
    pub error: Option<String>,
}

fn num(v: &Json, key: &str) -> f64 {
    v.get(key).and_then(Json::as_f64).unwrap_or(0.0)
}

fn opt_num(v: &Json, key: &str) -> Option<f64> {
    v.get(key).and_then(Json::as_f64)
}

/// Radians → degrees for the board's rotation field (`radToDeg`).
fn rad_to_deg(angle: Option<f64>) -> f64 {
    match angle {
        Some(a) if a.is_finite() => a * 180.0 / std::f64::consts::PI,
        _ => 0.0,
    }
}

/// Excalidraw opacity is 0..100; the board uses 0..1 (`normalizeOpacity`).
fn normalize_opacity(o: Option<f64>) -> f64 {
    match o {
        Some(o) if o.is_finite() => (o / 100.0).clamp(0.0, 1.0),
        _ => 1.0,
    }
}

/// Maps an Excalidraw arrowhead name onto the board's `ArrowHead` union (`mapArrowhead`).
fn map_arrowhead(head: Option<&Json>, fallback: &str) -> String {
    match head {
        Some(Json::Null) => "none".to_string(),
        Some(Json::String(s)) => match s.as_str() {
            "arrow" | "triangle" => "triangle",
            "bar" | "open" => "open",
            "dot" | "circle" => "circle",
            "none" => "none",
            _ => fallback,
        }
        .to_string(),
        _ => fallback.to_string(),
    }
}

/// Maps an Excalidraw text alignment onto the board's allowed values (`mapTextAlign`).
fn map_text_align(align: Option<&str>) -> &'static str {
    match align {
        Some("center") => "center",
        Some("right") => "right",
        _ => "left",
    }
}

/// Converts an Excalidraw scene (a JSON string or value) into board elements and strokes. `gen_id` mints a
/// fresh id per produced element; `now` is the stroke `createdAt` (ms).
pub fn excalidraw_to_whiteboard(data: &Json, mut gen_id: impl FnMut() -> String, now: f64) -> Imported {
    let scene = data;
    if !scene.is_object() {
        return Imported { error: Some("Invalid Excalidraw data".into()), ..Default::default() };
    }
    let raw = match scene.get("elements").and_then(Json::as_array) {
        Some(a) => a,
        None => {
            if scene.get("type").and_then(Json::as_str) == Some("excalidraw") {
                return Imported::default();
            }
            return Imported { error: Some("No Excalidraw elements found".into()), ..Default::default() };
        }
    };
    let files = scene.get("files").cloned().unwrap_or(Json::Null);
    let mut out = Imported::default();
    let mut z_index = 0i64;

    for el in raw {
        if !el.is_object() || el.get("isDeleted").and_then(Json::as_bool) == Some(true) {
            continue;
        }
        let x = num(el, "x");
        let y = num(el, "y");
        let width = num(el, "width");
        let height = num(el, "height");
        let rotation = rad_to_deg(opt_num(el, "angle"));
        let opacity = normalize_opacity(opt_num(el, "opacity"));
        let stroke = el.get("strokeColor").and_then(Json::as_str).filter(|s| !s.is_empty()).unwrap_or(DEFAULT_STROKE).to_string();
        let stroke_width = opt_num(el, "strokeWidth").unwrap_or(1.0);
        let ty = el.get("type").and_then(Json::as_str).unwrap_or("");

        match ty {
            "rectangle" | "ellipse" | "diamond" => {
                let kind = match ty {
                    "ellipse" => "circle",
                    "diamond" => "diamond",
                    _ => "rect",
                };
                let bg = el.get("backgroundColor").and_then(Json::as_str).unwrap_or("");
                let fill = if !bg.is_empty() && bg != "transparent" { bg } else { DEFAULT_FILL };
                let v = json!({
                    "id": gen_id(), "type": "shape", "x": x, "y": y, "width": width, "height": height,
                    "rotation": rotation, "opacity": opacity, "zIndex": z_index, "locked": false,
                    "kind": kind, "fill": fill, "stroke": stroke, "strokeWidth": stroke_width,
                });
                z_index += 1;
                push(&mut out, v);
            }
            "text" => {
                let v = json!({
                    "id": gen_id(), "type": "text", "x": x, "y": y, "width": width, "height": height,
                    "rotation": rotation, "opacity": opacity, "zIndex": z_index, "locked": false,
                    "text": el.get("text").and_then(Json::as_str).unwrap_or(""),
                    "fontSize": opt_num(el, "fontSize").unwrap_or(20.0),
                    "fontWeight": "normal", "color": stroke,
                    "textAlign": map_text_align(el.get("textAlign").and_then(Json::as_str)),
                });
                z_index += 1;
                push(&mut out, v);
            }
            "arrow" | "line" => {
                let pts = el.get("points").and_then(Json::as_array).cloned().unwrap_or_default();
                let first = pts.first().cloned().unwrap_or(json!([0, 0]));
                let last = pts.last().cloned().unwrap_or(json!([width, height]));
                let px = |p: &Json, i: usize| p.as_array().and_then(|a| a.get(i)).and_then(Json::as_f64).unwrap_or(0.0);
                let is_arrow = ty == "arrow";
                let v = json!({
                    "id": gen_id(), "type": "arrow",
                    "startX": x + px(&first, 0), "startY": y + px(&first, 1),
                    "endX": x + px(&last, 0), "endY": y + px(&last, 1),
                    "color": stroke, "width": stroke_width,
                    "style": if is_arrow && pts.len() > 2 { "curved" } else { "straight" },
                    "startArrow": if is_arrow { map_arrowhead(el.get("startArrowhead"), "none") } else { "none".into() },
                    "endArrow": if is_arrow { map_arrowhead(el.get("endArrowhead"), "triangle") } else { "none".into() },
                    "zIndex": z_index, "opacity": opacity,
                });
                z_index += 1;
                push(&mut out, v);
            }
            "freedraw" => {
                let pts = el.get("points").and_then(Json::as_array).cloned().unwrap_or_default();
                let mut flat: Vec<f64> = Vec::new();
                for p in &pts {
                    if let Some(a) = p.as_array() {
                        flat.push(x + a.first().and_then(Json::as_f64).unwrap_or(0.0));
                        flat.push(y + a.get(1).and_then(Json::as_f64).unwrap_or(0.0));
                    }
                }
                if flat.len() >= 4 {
                    let s = json!({
                        "id": gen_id(), "points": flat, "color": stroke, "width": stroke_width,
                        "opacity": opacity, "tool": "pen", "userId": "import", "createdAt": now,
                    });
                    if let Ok(stroke) = serde_json::from_value::<Stroke>(s) {
                        out.strokes.push(stroke);
                    }
                    z_index += 1;
                }
            }
            "image" => {
                let data_url = el
                    .get("fileId")
                    .and_then(Json::as_str)
                    .and_then(|fid| files.get(fid))
                    .and_then(|f| f.get("dataURL"))
                    .and_then(Json::as_str)
                    .unwrap_or("");
                if !data_url.is_empty() {
                    let v = json!({
                        "id": gen_id(), "type": "image", "x": x, "y": y, "width": width, "height": height,
                        "rotation": rotation, "opacity": opacity, "zIndex": z_index, "locked": false,
                        "src": data_url, "natural_width": width, "natural_height": height,
                    });
                    z_index += 1;
                    push(&mut out, v);
                }
            }
            "frame" => {
                let title = el.get("name").and_then(Json::as_str).filter(|s| !s.is_empty()).unwrap_or("Frame");
                let v = json!({
                    "id": gen_id(), "type": "frame", "x": x, "y": y, "width": width, "height": height,
                    "rotation": rotation, "opacity": opacity, "zIndex": z_index, "locked": false,
                    "title": title, "color": stroke,
                });
                z_index += 1;
                push(&mut out, v);
            }
            _ => {}
        }
    }
    out
}

fn push(out: &mut Imported, v: Json) {
    if let Ok(e) = serde_json::from_value::<WbElement>(v) {
        out.elements.push(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn counter() -> impl FnMut() -> String {
        let n = Cell::new(0);
        move || {
            n.set(n.get() + 1);
            format!("id{}", n.get())
        }
    }

    #[test]
    fn maps_a_rectangle_and_an_arrow() {
        let scene = json!({"type":"excalidraw","elements":[
            {"type":"rectangle","x":10,"y":20,"width":30,"height":40,"backgroundColor":"#ff0000","opacity":50},
            {"type":"arrow","x":0,"y":0,"points":[[0,0],[100,50]],"endArrowhead":"triangle"}
        ]});
        let r = excalidraw_to_whiteboard(&scene, counter(), 1000.0);
        assert_eq!(r.error, None);
        assert_eq!(r.elements.len(), 2);
        match &r.elements[0] {
            WbElement::Shape(s) => {
                assert_eq!(s.kind, "rect");
                assert_eq!(s.fill, "#ff0000");
                assert!((s.base.opacity - 0.5).abs() < 1e-9);
            }
            _ => panic!("expected a shape"),
        }
        match &r.elements[1] {
            WbElement::Arrow(a) => {
                assert_eq!(a.end_x, 100.0);
                assert_eq!(a.end_arrow, "triangle");
            }
            _ => panic!("expected an arrow"),
        }
    }

    #[test]
    fn freedraw_becomes_a_stroke() {
        let scene = json!({"elements":[{"type":"freedraw","x":5,"y":5,"points":[[0,0],[1,1],[2,2]]}]});
        let r = excalidraw_to_whiteboard(&scene, counter(), 42.0);
        assert_eq!(r.strokes.len(), 1);
        assert_eq!(r.strokes[0].points, vec![5.0, 5.0, 6.0, 6.0, 7.0, 7.0]);
        assert_eq!(r.strokes[0].created_at, 42.0);
    }

    #[test]
    fn non_object_is_an_error() {
        assert!(excalidraw_to_whiteboard(&json!(3), counter(), 0.0).error.is_some());
    }
}
