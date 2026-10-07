//! CSS colours, as the web code writes them: `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb(…)`, `rgba(…)`, the
//! few named colours the office code uses, `transparent` and `none`.

/// A colour with straight (non-premultiplied) alpha, every channel in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const WHITE: Rgba = Rgba { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const TRANSPARENT: Rgba = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

    pub fn rgb8(r: u8, g: u8, b: u8) -> Rgba {
        Rgba { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }
    }

    pub fn with_alpha(self, a: f32) -> Rgba {
        Rgba { a: a.clamp(0.0, 1.0), ..self }
    }

    /// `#rrggbb` (alpha dropped).
    pub fn to_hex(self) -> String {
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}", c(self.r), c(self.g), c(self.b))
    }
}

fn hex_digit(c: u8) -> Option<u8> {
    (c as char).to_digit(16).map(|d| d as u8)
}

/// Parses a CSS colour. `none` and `transparent` are fully transparent. `None` when it is not a
/// colour the office code writes (the canvas then keeps its previous style, as a browser does).
pub fn parse_color(css: &str) -> Option<Rgba> {
    let s = css.trim();
    let lower = s.to_ascii_lowercase();
    if let Some(hex) = lower.strip_prefix('#') {
        let b = hex.as_bytes();
        let d: Option<Vec<u8>> = b.iter().map(|c| hex_digit(*c)).collect();
        let d = d?;
        return match d.len() {
            3 | 4 => {
                let v = |i: usize| d[i] * 17;
                let a = if d.len() == 4 { v(3) } else { 255 };
                Some(Rgba::rgb8(v(0), v(1), v(2)).with_alpha(a as f32 / 255.0))
            }
            6 | 8 => {
                let v = |i: usize| d[i] * 16 + d[i + 1];
                let a = if d.len() == 8 { v(6) } else { 255 };
                Some(Rgba::rgb8(v(0), v(2), v(4)).with_alpha(a as f32 / 255.0))
            }
            _ => None,
        };
    }
    if let Some(args) = lower.strip_prefix("rgba(").or_else(|| lower.strip_prefix("rgb(")) {
        let args = args.strip_suffix(')')?;
        let parts: Vec<&str> = args.split([',', ' ', '/']).map(str::trim).filter(|p| !p.is_empty()).collect();
        if parts.len() < 3 {
            return None;
        }
        let channel = |p: &str| -> Option<f32> {
            match p.strip_suffix('%') {
                Some(n) => n.parse::<f32>().ok().map(|v| (v / 100.0).clamp(0.0, 1.0)),
                None => p.parse::<f32>().ok().map(|v| (v / 255.0).clamp(0.0, 1.0)),
            }
        };
        let alpha = match parts.get(3) {
            Some(p) => match p.strip_suffix('%') {
                Some(n) => n.parse::<f32>().ok()? / 100.0,
                None => p.parse::<f32>().ok()?,
            },
            None => 1.0,
        };
        return Some(Rgba { r: channel(parts[0])?, g: channel(parts[1])?, b: channel(parts[2])?, a: alpha.clamp(0.0, 1.0) });
    }
    Some(match lower.as_str() {
        // A browser ignores `fillStyle = "none"` (not a colour): the previous style stays.
        "transparent" => Rgba::TRANSPARENT,
        "black" => Rgba::BLACK,
        "white" => Rgba::WHITE,
        "red" => Rgba::rgb8(255, 0, 0),
        "green" => Rgba::rgb8(0, 128, 0),
        "blue" => Rgba::rgb8(0, 0, 255),
        "gray" | "grey" => Rgba::rgb8(128, 128, 128),
        "yellow" => Rgba::rgb8(255, 255, 0),
        "orange" => Rgba::rgb8(255, 165, 0),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_forms_the_office_code_writes_parse() {
        assert_eq!(parse_color("#1a73e8"), Some(Rgba::rgb8(0x1a, 0x73, 0xe8)));
        assert_eq!(parse_color("#fff"), Some(Rgba::WHITE));
        let c = parse_color("rgba(26, 115, 232, 0.12)").expect("rgba");
        assert!((c.a - 0.12).abs() < 1e-6 && (c.r - 26.0 / 255.0).abs() < 1e-6);
        assert_eq!(parse_color("rgba(0,0,0,0.05)").map(|c| (c.a * 100.0).round()), Some(5.0));
        assert_eq!(parse_color("none"), None, "not a colour: a canvas keeps its previous style");
        assert_eq!(parse_color("not a colour"), None);
        assert_eq!(Rgba::rgb8(0x6c, 0x8e, 0xbf).to_hex(), "#6c8ebf");
    }
}
