//! The KaTeX fonts (embedded, MIT - see `fonts/LICENSE-KaTeX`), read with ttf-parser: glyph lookup, advance
//! widths, kerning and outlines (as path operations in ems, y down), so a client needs no installed font.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::svg::PathOp;

macro_rules! fonts {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_bytes!(concat!("../fonts/KaTeX_", $name, ".ttf")) as &[u8])),*]
    };
}

const FONTS: &[(&str, &[u8])] = fonts!(
    "AMS-Regular",
    "Caligraphic-Bold",
    "Caligraphic-Regular",
    "Fraktur-Bold",
    "Fraktur-Regular",
    "Main-Bold",
    "Main-BoldItalic",
    "Main-Italic",
    "Main-Regular",
    "Math-BoldItalic",
    "Math-Italic",
    "SansSerif-Bold",
    "SansSerif-Italic",
    "SansSerif-Regular",
    "Script-Regular",
    "Size1-Regular",
    "Size2-Regular",
    "Size3-Regular",
    "Size4-Regular",
    "Typewriter-Regular",
);

/// The bytes of a KaTeX font by its KaTeX name (`"Main-Regular"`…).
pub fn font_data(name: &str) -> Option<&'static [u8]> {
    FONTS.iter().find(|(n, _)| *n == name).map(|(_, d)| *d)
}

/// Every embedded font: (KaTeX name, TTF bytes).
pub fn all_fonts() -> &'static [(&'static str, &'static [u8])] {
    FONTS
}

fn face(name: &str) -> Option<ttf_parser::Face<'static>> {
    ttf_parser::Face::parse(font_data(name)?, 0).ok()
}

/// A glyph of a font: its id and advance width in ems.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphInfo {
    pub id: u16,
    pub advance: f64,
}

type GlyphCache = Mutex<HashMap<(String, char), Option<GlyphInfo>>>;
type OutlineCache = Mutex<HashMap<(String, u16), std::sync::Arc<Vec<PathOp>>>>;

fn cache() -> &'static GlyphCache {
    static C: OnceLock<GlyphCache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The glyph of `c` in `font`, `None` when the font has none (the browser would fall back to another font).
pub fn glyph(font: &str, c: char) -> Option<GlyphInfo> {
    if let Ok(m) = cache().lock() {
        if let Some(v) = m.get(&(font.to_string(), c)) {
            return *v;
        }
    }
    let r = (|| {
        let f = face(font)?;
        let id = f.glyph_index(c)?;
        if id.0 == 0 {
            return None;
        }
        let upem = f.units_per_em() as f64;
        let adv = f.glyph_hor_advance(id).unwrap_or(0) as f64 / upem;
        Some(GlyphInfo { id: id.0, advance: adv })
    })();
    if let Ok(mut m) = cache().lock() {
        m.insert((font.to_string(), c), r);
    }
    r
}

/// The kerning between two glyphs in ems (the `kern` table; KaTeX's fonts have no GPOS kerning).
pub fn kerning(font: &str, left: u16, right: u16) -> f64 {
    let Some(f) = face(font) else { return 0.0 };
    let upem = f.units_per_em() as f64;
    let Some(kern) = f.tables().kern else { return 0.0 };
    for st in kern.subtables {
        if !st.horizontal || st.variable {
            continue;
        }
        if let Some(v) = st.glyphs_kerning(ttf_parser::GlyphId(left), ttf_parser::GlyphId(right)) {
            return v as f64 / upem;
        }
    }
    0.0
}

/// The font's ascent and descent (hhea, OS/2 typo and win metrics) in ems, for the CSS line box.
pub fn line_metrics(font: &str) -> Option<LineMetrics> {
    let f = face(font)?;
    let upem = f.units_per_em() as f64;
    let os2 = f.tables().os2;
    Some(LineMetrics {
        hhea_ascent: f.ascender() as f64 / upem,
        hhea_descent: -(f.descender() as f64) / upem,
        hhea_gap: f.line_gap() as f64 / upem,
        win_ascent: os2.map(|o| o.windows_ascender() as f64 / upem).unwrap_or(0.0),
        win_descent: os2.map(|o| o.windows_descender() as f64 / upem).unwrap_or(0.0),
        typo_ascent: os2.map(|o| o.typographic_ascender() as f64 / upem).unwrap_or(0.0),
        typo_descent: os2.map(|o| -(o.typographic_descender() as f64) / upem).unwrap_or(0.0),
        use_typo: os2.is_some_and(|o| o.use_typographic_metrics()),
    })
}

/// A font's vertical metrics in ems.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LineMetrics {
    pub hhea_ascent: f64,
    pub hhea_descent: f64,
    pub hhea_gap: f64,
    pub win_ascent: f64,
    pub win_descent: f64,
    pub typo_ascent: f64,
    pub typo_descent: f64,
    pub use_typo: bool,
}

struct Builder {
    ops: Vec<PathOp>,
    scale: f64,
}

impl ttf_parser::OutlineBuilder for Builder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.ops.push(PathOp::Move(x as f64 * self.scale, -(y as f64) * self.scale));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.ops.push(PathOp::Line(x as f64 * self.scale, -(y as f64) * self.scale));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let s = self.scale;
        self.ops.push(PathOp::Quad(x1 as f64 * s, -(y1 as f64) * s, x as f64 * s, -(y as f64) * s));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let s = self.scale;
        self.ops.push(PathOp::Cubic(x1 as f64 * s, -(y1 as f64) * s, x2 as f64 * s, -(y2 as f64) * s, x as f64 * s, -(y as f64) * s));
    }
    fn close(&mut self) {
        self.ops.push(PathOp::Close);
    }
}

fn outline_cache() -> &'static OutlineCache {
    static C: OnceLock<OutlineCache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The outline of glyph `id` of `font`, in ems with the origin on the baseline and y pointing down.
pub fn outline(font: &str, id: u16) -> std::sync::Arc<Vec<PathOp>> {
    if let Ok(m) = outline_cache().lock() {
        if let Some(v) = m.get(&(font.to_string(), id)) {
            return v.clone();
        }
    }
    let ops = face(font)
        .map(|f| {
            let mut b = Builder { ops: Vec::new(), scale: 1.0 / f.units_per_em() as f64 };
            f.outline_glyph(ttf_parser::GlyphId(id), &mut b);
            b.ops
        })
        .unwrap_or_default();
    let a = std::sync::Arc::new(ops);
    if let Ok(mut m) = outline_cache().lock() {
        m.insert((font.to_string(), id), a.clone());
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_load_and_match_the_metrics() {
        for (name, _) in all_fonts() {
            assert!(face(name).is_some(), "{name}");
        }
        let g = glyph("Math-Italic", 'x').unwrap_or(GlyphInfo { id: 0, advance: 0.0 });
        let m = crate::data::char_metrics("x", "Math-Italic", crate::data::Mode::Math).unwrap_or_default();
        assert!((g.advance - m.width).abs() < 0.002, "{} vs {}", g.advance, m.width);
        assert!(!outline("Main-Regular", glyph("Main-Regular", 'A').map(|g| g.id).unwrap_or(0)).is_empty());
        eprintln!("Main line metrics: {:?}", line_metrics("Main-Regular"));
        let has_kern = face("Main-Regular").and_then(|f| f.tables().kern).is_some();
        eprintln!("Main has kern: {has_kern}");
    }
}
