//! CSS colours as the web's canvas reads them (`fillStyle = '#1a73e8'`, `'rgba(0,0,0,0.35)'`, `'transparent'`…).

/// A straight (not premultiplied) RGBA colour, channels 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Rgba {
    pub const TRANSPARENT: Rgba = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
    pub const BLACK: Rgba = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const WHITE: Rgba = Rgba { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };

    pub fn rgb8(r: u8, g: u8, b: u8) -> Rgba {
        Rgba { r: r as f64 / 255.0, g: g as f64 / 255.0, b: b as f64 / 255.0, a: 1.0 }
    }

    pub fn with_alpha(self, a: f64) -> Rgba {
        Rgba { a, ..self }
    }

    /// Parses a CSS colour; `None` for what a canvas would ignore (the previous style then stays — callers
    /// pick their fallback).
    pub fn parse(css: &str) -> Option<Rgba> {
        let s = css.trim().to_ascii_lowercase();
        if let Some(hex) = s.strip_prefix('#') {
            let v: Vec<u8> = hex.chars().map(|c| c.to_digit(16).map(|d| d as u8)).collect::<Option<Vec<_>>>()?;
            return match v.len() {
                3 => Some(Rgba::rgb8(v[0] * 17, v[1] * 17, v[2] * 17)),
                4 => Some(Rgba::rgb8(v[0] * 17, v[1] * 17, v[2] * 17).with_alpha((v[3] * 17) as f64 / 255.0)),
                6 => Some(Rgba::rgb8(v[0] * 16 + v[1], v[2] * 16 + v[3], v[4] * 16 + v[5])),
                8 => Some(Rgba::rgb8(v[0] * 16 + v[1], v[2] * 16 + v[3], v[4] * 16 + v[5]).with_alpha((v[6] * 16 + v[7]) as f64 / 255.0)),
                _ => None,
            };
        }
        if let Some(rest) = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb(")) {
            let inner = rest.strip_suffix(')')?;
            let parts: Vec<&str> = inner.split([',', '/', ' ']).map(str::trim).filter(|p| !p.is_empty()).collect();
            if parts.len() < 3 {
                return None;
            }
            let ch = |p: &str| -> Option<f64> {
                if let Some(pc) = p.strip_suffix('%') {
                    Some((pc.parse::<f64>().ok()? / 100.0).clamp(0.0, 1.0))
                } else {
                    Some((p.parse::<f64>().ok()? / 255.0).clamp(0.0, 1.0))
                }
            };
            let a = match parts.get(3) {
                Some(p) => match p.strip_suffix('%') {
                    Some(pc) => pc.parse::<f64>().ok()? / 100.0,
                    None => p.parse::<f64>().ok()?,
                },
                None => 1.0,
            };
            return Some(Rgba { r: ch(parts[0])?, g: ch(parts[1])?, b: ch(parts[2])?, a: a.clamp(0.0, 1.0) });
        }
        named(&s)
    }

    /// `rgbaFromHex(hex, opacity 0..100)`.
    pub fn from_hex_opacity(hex: &str, opacity: f64) -> Rgba {
        let c = Rgba::parse(hex).unwrap_or(Rgba::BLACK);
        c.with_alpha(opacity.clamp(0.0, 100.0) / 100.0)
    }
}

fn named(s: &str) -> Option<Rgba> {
    Some(match s {
        "transparent" => Rgba::TRANSPARENT,
        "black" => Rgba::BLACK,
        "white" => Rgba::WHITE,
        "red" => Rgba::rgb8(255, 0, 0),
        "green" => Rgba::rgb8(0, 128, 0),
        "blue" => Rgba::rgb8(0, 0, 255),
        "yellow" => Rgba::rgb8(255, 255, 0),
        "orange" => Rgba::rgb8(255, 165, 0),
        "purple" => Rgba::rgb8(128, 0, 128),
        "gray" | "grey" => Rgba::rgb8(128, 128, 128),
        "silver" => Rgba::rgb8(192, 192, 192),
        "navy" => Rgba::rgb8(0, 0, 128),
        "teal" => Rgba::rgb8(0, 128, 128),
        "maroon" => Rgba::rgb8(128, 0, 0),
        "lime" => Rgba::rgb8(0, 255, 0),
        "aqua" | "cyan" => Rgba::rgb8(0, 255, 255),
        "fuchsia" | "magenta" => Rgba::rgb8(255, 0, 255),
        "olive" => Rgba::rgb8(128, 128, 0),
        "pink" => Rgba::rgb8(255, 192, 203),
        "brown" => Rgba::rgb8(165, 42, 42),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_forms_parse() {
        assert_eq!(Rgba::parse("#fff"), Some(Rgba::WHITE));
        assert_eq!(Rgba::parse("#1A73E8"), Some(Rgba::rgb8(0x1a, 0x73, 0xe8)));
        let c = Rgba::parse("rgba(0,0,0,0.35)").expect("rgba");
        assert!((c.a - 0.35).abs() < 1e-12);
        assert_eq!(Rgba::parse("rgb(255, 0, 0)"), Some(Rgba::rgb8(255, 0, 0)));
        assert_eq!(Rgba::parse("transparent"), Some(Rgba::TRANSPARENT));
        assert_eq!(Rgba::parse("nonsense"), None);
        assert!((Rgba::from_hex_opacity("#000000", 50.0).a - 0.5).abs() < 1e-12);
    }
}
