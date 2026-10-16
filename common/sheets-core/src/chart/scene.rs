//! The chart as drawing primitives - the elements the web's `ChartView` emits as SVG, in the same order
//! and with the same attribute values, so a platform painter draws exactly what the browser draws and the
//! golden test can compare the two lists element by element.
//!
//! Colours are kept as the web writes them, CSS variables included (`var(--color-surface-0, #ffffff)`): the
//! painter resolves them for its theme ([`resolve_color`]).

use crate::js;

/// An SVG element: tag, attributes, text.
pub type SvgElement = (&'static str, Vec<(&'static str, String)>, Option<String>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    /// No `text-anchor` attribute (SVG's default, start).
    Default,
    Start,
    Middle,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    Rect { x: f64, y: f64, w: f64, h: f64, rx: Option<f64>, fill: String, fill_opacity: Option<f64>, stroke: Option<String>, stroke_width: Option<f64>, tip: Option<String> },
    Line { x1: f64, y1: f64, x2: f64, y2: f64, stroke: String, stroke_opacity: Option<f64>, stroke_width: Option<f64> },
    /// An SVG path (`d` as the web formats it). `round`: 1 = round joins, 2 = round joins and caps.
    Path { d: String, fill: String, fill_opacity: Option<f64>, evenodd: bool, stroke: Option<String>, stroke_width: Option<f64>, round: u8, tip: Option<String> },
    Polygon { points: Vec<(f64, f64)>, fill: String, fill_opacity: Option<f64>, stroke: Option<String>, stroke_opacity: Option<f64> },
    Polyline { points: Vec<(f64, f64)>, stroke: String },
    Circle { cx: f64, cy: f64, r: f64, fill: String, stroke: Option<String>, stroke_opacity: Option<f64>, tip: Option<String> },
    Text { x: f64, y: f64, text: String, size: f64, bold: bool, anchor: Anchor, fill: String, rotate: Option<f64> },
    /// Clip the following primitives to this rectangle until [`Prim::Unclip`].
    Clip { x: f64, y: f64, w: f64, h: f64 },
    Unclip,
}

/// `f`: round to 2 decimals as the web's path builders do.
pub fn f2(n: f64) -> f64 {
    js::round(n * 100.0) / 100.0
}

/// A number as JavaScript prints it in an SVG attribute.
pub fn num(n: f64) -> String {
    js::to_string(n)
}

impl Prim {
    /// The SVG element this primitive is (tag, attributes in the web's order, text) - for the golden test
    /// and for an SVG export.
    pub fn svg(&self) -> Option<SvgElement> {
        let mut a: Vec<(&'static str, String)> = Vec::new();
        let tip = |a: &mut Vec<(&'static str, String)>, t: &Option<String>| {
            if let Some(t) = t {
                a.push(("title", t.clone()));
            }
        };
        Some(match self {
            Prim::Rect { x, y, w, h, rx, fill, fill_opacity, stroke, stroke_width, tip: t } => {
                a.extend([("x", num(*x)), ("y", num(*y)), ("width", num(*w)), ("height", num(*h))]);
                if let Some(r) = rx {
                    a.push(("rx", num(*r)));
                }
                a.push(("fill", fill.clone()));
                if let Some(o) = fill_opacity {
                    a.push(("fill-opacity", num(*o)));
                }
                if let Some(s) = stroke {
                    a.push(("stroke", s.clone()));
                }
                if let Some(w) = stroke_width {
                    a.push(("stroke-width", num(*w)));
                }
                tip(&mut a, t);
                ("rect", a, None)
            }
            Prim::Line { x1, y1, x2, y2, stroke, stroke_opacity, stroke_width } => {
                a.extend([("x1", num(*x1)), ("x2", num(*x2)), ("y1", num(*y1)), ("y2", num(*y2)), ("stroke", stroke.clone())]);
                if let Some(o) = stroke_opacity {
                    a.push(("stroke-opacity", num(*o)));
                }
                if let Some(w) = stroke_width {
                    a.push(("stroke-width", num(*w)));
                }
                ("line", a, None)
            }
            Prim::Path { d, fill, fill_opacity, evenodd, stroke, stroke_width, round, tip: t } => {
                a.push(("d", d.clone()));
                a.push(("fill", fill.clone()));
                if let Some(o) = fill_opacity {
                    a.push(("fill-opacity", num(*o)));
                }
                if *evenodd {
                    a.push(("fill-rule", "evenodd".into()));
                }
                if let Some(s) = stroke {
                    a.push(("stroke", s.clone()));
                }
                if let Some(w) = stroke_width {
                    a.push(("stroke-width", num(*w)));
                }
                if *round >= 1 {
                    a.push(("stroke-linejoin", "round".into()));
                }
                if *round >= 2 {
                    a.push(("stroke-linecap", "round".into()));
                }
                tip(&mut a, t);
                ("path", a, None)
            }
            Prim::Polygon { points, fill, fill_opacity, stroke, stroke_opacity } => {
                a.push(("points", points.iter().map(|(x, y)| format!("{},{}", num(*x), num(*y))).collect::<Vec<_>>().join(" ")));
                a.push(("fill", fill.clone()));
                if let Some(o) = fill_opacity {
                    a.push(("fill-opacity", num(*o)));
                }
                if let Some(s) = stroke {
                    a.push(("stroke", s.clone()));
                }
                if let Some(o) = stroke_opacity {
                    a.push(("stroke-opacity", num(*o)));
                }
                ("polygon", a, None)
            }
            Prim::Polyline { points, stroke } => {
                a.push(("points", points.iter().map(|(x, y)| format!("{},{}", num(*x), num(*y))).collect::<Vec<_>>().join(" ")));
                a.push(("fill", "none".into()));
                a.push(("stroke", stroke.clone()));
                ("polyline", a, None)
            }
            Prim::Circle { cx, cy, r, fill, stroke, stroke_opacity, tip: t } => {
                a.extend([("cx", num(*cx)), ("cy", num(*cy)), ("r", num(*r)), ("fill", fill.clone())]);
                if let Some(s) = stroke {
                    a.push(("stroke", s.clone()));
                }
                if let Some(o) = stroke_opacity {
                    a.push(("stroke-opacity", num(*o)));
                }
                tip(&mut a, t);
                ("circle", a, None)
            }
            Prim::Text { x, y, text, size, bold, anchor, fill, rotate } => {
                a.extend([("x", num(*x)), ("y", num(*y)), ("font-size", num(*size))]);
                if *bold {
                    a.push(("font-weight", "600".into()));
                }
                a.push(("fill", fill.clone()));
                match anchor {
                    Anchor::Default => {}
                    Anchor::Start => a.push(("text-anchor", "start".into())),
                    Anchor::Middle => a.push(("text-anchor", "middle".into())),
                    Anchor::End => a.push(("text-anchor", "end".into())),
                }
                if let Some(r) = rotate {
                    a.push(("transform", format!("rotate({} {} {})", num(*r), num(*x), num(*y))));
                }
                ("text", a, Some(text.clone()))
            }
            Prim::Clip { x, y, w, h } => {
                a.extend([("x", num(*x)), ("y", num(*y)), ("width", num(*w)), ("height", num(*h))]);
                ("clip", a, None)
            }
            Prim::Unclip => return None,
        })
    }
}

/// The colour a painter uses for a web colour: a CSS variable resolves to `dark` when the page is dark and
/// the variable is one the theme changes, else to its fallback (the web's light value).
pub fn resolve_color(c: &str, dark: bool) -> String {
    let c = c.trim();
    if let Some(inner) = c.strip_prefix("var(").and_then(|r| r.strip_suffix(')')) {
        let (name, fallback) = match inner.split_once(',') {
            Some((n, f)) => (n.trim(), f.trim()),
            None => (inner.trim(), "#000000"),
        };
        if dark {
            // The kubuno-dark theme's values of the variables the chart uses.
            match name {
                "--color-surface-0" => return "#1f1f1f".into(),
                "--color-text-primary" => return "#e8eaed".into(),
                "--color-text-secondary" => return "#bdc1c6".into(),
                "--color-text-tertiary" => return "#9aa0a6".into(),
                _ => {}
            }
        }
        return fallback.to_string();
    }
    c.to_string()
}
