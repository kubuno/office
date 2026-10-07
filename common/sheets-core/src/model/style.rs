//! Cell formatting (`CellStyle` in `api.ts`), and the effective style of a cell: its column's default,
//! then its row's, then its own (`styleAt`: `{ ...colStyle, ...rowStyle, ...cellStyle }`).

use serde_json::{Map, Value as Json};

/// One stop of a gradient fill.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientStop {
    /// 0–100.
    pub position: f64,
    pub color: String,
    /// 0–1 (absent = 1).
    pub opacity: Option<f64>,
}

/// A gradient fill (`bgGradient`): linear at an angle (0 = up, clockwise) or radial.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub radial: bool,
    pub angle: f64,
    pub stops: Vec<GradientStop>,
}

impl Gradient {
    fn from_json(v: &Json) -> Option<Gradient> {
        let o = v.as_object()?;
        let stops = o.get("stops")?.as_array()?.iter().filter_map(|s| {
            let s = s.as_object()?;
            Some(GradientStop {
                position: s.get("position").and_then(Json::as_f64).unwrap_or(0.0),
                color: s.get("color").and_then(Json::as_str).unwrap_or("#000000").to_string(),
                opacity: s.get("opacity").and_then(Json::as_f64),
            })
        });
        Some(Gradient {
            radial: o.get("type").and_then(Json::as_str) == Some("radial"),
            angle: o.get("angle").and_then(Json::as_f64).unwrap_or(0.0),
            stops: stops.collect(),
        })
    }
}

/// Horizontal alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Vertical alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Center,
    Bottom,
}

/// The typed view of a style object. `None` = the key is absent (or not of the expected type).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    /// In CSS pixels (the web draws `${fontSize}px`).
    pub font_size: Option<f64>,
    pub font_family: Option<String>,
    pub color: Option<String>,
    pub bg: Option<String>,
    pub align: Option<Align>,
    pub valign: Option<VAlign>,
    pub wrap: Option<bool>,
    /// A raw number-format code (`"0.00%"`, `"dd/mm/yyyy"`).
    pub num_fmt_code: Option<String>,
    /// `number`, `currency`, `percent`, `scientific`.
    pub num_fmt: Option<String>,
    pub decimals: Option<f64>,
    pub thousands: Option<bool>,
    /// Border colours per edge: top, right, bottom, left.
    pub bt: Option<String>,
    pub br: Option<String>,
    pub bb: Option<String>,
    pub bl: Option<String>,
    /// Border widths per edge (absent = 1).
    pub btw: Option<f64>,
    pub brw: Option<f64>,
    pub bbw: Option<f64>,
    pub blw: Option<f64>,
    pub bg_gradient: Option<Gradient>,
}

fn s(o: &Map<String, Json>, k: &str) -> Option<String> {
    o.get(k).and_then(Json::as_str).map(str::to_string)
}
fn b(o: &Map<String, Json>, k: &str) -> Option<bool> {
    o.get(k).and_then(Json::as_bool)
}
fn n(o: &Map<String, Json>, k: &str) -> Option<f64> {
    o.get(k).and_then(Json::as_f64)
}

impl Style {
    /// Reads a style object.
    pub fn from_json(v: &Json) -> Style {
        let Some(o) = v.as_object() else { return Style::default() };
        Style::from_map(o)
    }

    pub fn from_map(o: &Map<String, Json>) -> Style {
        Style {
            bold: b(o, "bold"),
            italic: b(o, "italic"),
            underline: b(o, "underline"),
            strike: b(o, "strike"),
            font_size: n(o, "fontSize"),
            font_family: s(o, "fontFamily"),
            color: s(o, "color"),
            bg: s(o, "bg"),
            align: match o.get("align").and_then(Json::as_str) {
                Some("left") => Some(Align::Left),
                Some("center") => Some(Align::Center),
                Some("right") => Some(Align::Right),
                _ => None,
            },
            valign: match o.get("valign").and_then(Json::as_str) {
                Some("top") => Some(VAlign::Top),
                Some("center") => Some(VAlign::Center),
                Some("bottom") => Some(VAlign::Bottom),
                _ => None,
            },
            wrap: b(o, "wrap"),
            num_fmt_code: s(o, "numFmtCode"),
            num_fmt: s(o, "numFmt"),
            decimals: n(o, "decimals"),
            thousands: b(o, "thousands"),
            bt: s(o, "bt"),
            br: s(o, "br"),
            bb: s(o, "bb"),
            bl: s(o, "bl"),
            btw: n(o, "btw"),
            brw: n(o, "brw"),
            bbw: n(o, "bbw"),
            blw: n(o, "blw"),
            bg_gradient: o.get("bgGradient").and_then(Gradient::from_json),
        }
    }

    /// `!!style.bold` etc.
    pub fn is_bold(&self) -> bool {
        self.bold == Some(true)
    }
    pub fn is_italic(&self) -> bool {
        self.italic == Some(true)
    }
    pub fn is_underline(&self) -> bool {
        self.underline == Some(true)
    }
    pub fn is_strike(&self) -> bool {
        self.strike == Some(true)
    }
    pub fn is_wrap(&self) -> bool {
        self.wrap == Some(true)
    }
}

/// `{ ...a, ...b }` on style objects: `b`'s keys overwrite `a`'s.
pub fn merge(a: Option<&Json>, b: Option<&Json>) -> Map<String, Json> {
    let mut out = a.and_then(Json::as_object).cloned().unwrap_or_default();
    if let Some(b) = b.and_then(Json::as_object) {
        for (k, v) in b {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

/// `styleAt(col, row, cellS)`: column default < row default < cell.
pub fn effective(col_style: Option<&Json>, row_style: Option<&Json>, cell_style: Option<&Json>) -> Style {
    if col_style.is_none() && row_style.is_none() {
        return cell_style.map(Style::from_json).unwrap_or_default();
    }
    let m = merge(Some(&Json::Object(merge(col_style, row_style))), cell_style);
    Style::from_map(&m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_cell_wins_over_its_row_and_column() {
        let col = json!({"bold": true, "color": "#111111"});
        let row = json!({"color": "#222222", "italic": true});
        let cell = json!({"italic": false});
        let s = effective(Some(&col), Some(&row), Some(&cell));
        assert_eq!((s.is_bold(), s.color.as_deref(), s.is_italic()), (true, Some("#222222"), false));
    }

    #[test]
    fn gradients_parse() {
        let s = Style::from_json(&json!({"bgGradient": {"type": "linear", "angle": 90, "stops": [{"position": 0, "color": "#fff"}, {"position": 100, "color": "#000", "opacity": 0.5}]}}));
        let g = s.bg_gradient.unwrap();
        assert!(!g.radial);
        assert_eq!(g.stops.len(), 2);
        assert_eq!(g.stops[1].opacity, Some(0.5));
    }
}
