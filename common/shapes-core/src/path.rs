//! Path commands — the renderer-neutral form of a geometry.
//!
//! The web builds SVG path strings (`M x,y L x,y C … A … Z`) and hands them to `Path2D`; a platform
//! here gets the same commands as data and hands them to its own API (a Direct2D geometry sink, a
//! Core Graphics path…). [`Path::to_svg`] writes them back as the web's exact string, which is what
//! the parity tests compare.

/// One path command, in the shape's own box (origin at its top-left corner).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cmd {
    MoveTo(f64, f64),
    LineTo(f64, f64),
    /// Cubic Bézier: two control points, then the end point.
    CubicTo(f64, f64, f64, f64, f64, f64),
    /// Quadratic Bézier: one control point, then the end point.
    QuadTo(f64, f64, f64, f64),
    /// SVG elliptical arc (`A rx,ry 0 large sweep x,y`), x-axis rotation always 0. `sweep` true is
    /// SVG's sweep-flag 1: the positive-angle direction, clockwise on a y-down screen.
    ArcTo { rx: f64, ry: f64, large: bool, sweep: bool, x: f64, y: f64 },
    Close,
}

/// A sequence of commands.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Path {
    pub cmds: Vec<Cmd>,
}

/// The web's number formatting in path strings: rounded to two decimals (`Math.round(v * 100) / 100`)
/// and printed like JavaScript prints a number.
pub fn round2(v: f64) -> f64 {
    js_round(v * 100.0) / 100.0
}

/// JavaScript's `Math.round`: ties go towards +∞ (Rust's `round` goes away from zero).
pub fn js_round(v: f64) -> f64 {
    let f = v.floor();
    if v - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}

/// A number as JavaScript's `String(n)` prints it, for the values a path holds (`-0` is `0`).
pub fn js_num(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    if !v.is_finite() {
        return if v.is_nan() { "NaN".into() } else if v > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    format!("{v}")
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        self.cmds.push(Cmd::MoveTo(x, y));
    }

    pub fn line_to(&mut self, x: f64, y: f64) {
        self.cmds.push(Cmd::LineTo(x, y));
    }

    pub fn close(&mut self) {
        self.cmds.push(Cmd::Close);
    }

    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }

    /// The SVG path data, formatted like the web (`M x,y L x,y …`, two decimals, one space after
    /// each command). Trailing space trimmed.
    pub fn to_svg(&self) -> String {
        let n = |v: f64| js_num(round2(v));
        let mut d = String::new();
        for c in &self.cmds {
            match *c {
                Cmd::MoveTo(x, y) => d.push_str(&format!("M {},{} ", n(x), n(y))),
                Cmd::LineTo(x, y) => d.push_str(&format!("L {},{} ", n(x), n(y))),
                Cmd::CubicTo(a, b, c2, d2, e, f) => d.push_str(&format!("C {},{} {},{} {},{} ", n(a), n(b), n(c2), n(d2), n(e), n(f))),
                Cmd::QuadTo(a, b, e, f) => d.push_str(&format!("Q {},{} {},{} ", n(a), n(b), n(e), n(f))),
                Cmd::ArcTo { rx, ry, large, sweep, x, y } => {
                    d.push_str(&format!("A {},{} 0 {} {} {},{} ", n(rx), n(ry), large as u8, sweep as u8, n(x), n(y)))
                }
                Cmd::Close => d.push_str("Z "),
            }
        }
        d.trim_end().to_string()
    }

    /// Every coordinate rounded to two decimals, as the web's `Path2D` receives them.
    pub fn rounded(&self) -> Path {
        let r = round2;
        Path {
            cmds: self
                .cmds
                .iter()
                .map(|c| match *c {
                    Cmd::MoveTo(x, y) => Cmd::MoveTo(r(x), r(y)),
                    Cmd::LineTo(x, y) => Cmd::LineTo(r(x), r(y)),
                    Cmd::CubicTo(a, b, c2, d, e, f) => Cmd::CubicTo(r(a), r(b), r(c2), r(d), r(e), r(f)),
                    Cmd::QuadTo(a, b, e, f) => Cmd::QuadTo(r(a), r(b), r(e), r(f)),
                    Cmd::ArcTo { rx, ry, large, sweep, x, y } => Cmd::ArcTo { rx: r(rx), ry: r(ry), large, sweep, x: r(x), y: r(y) },
                    Cmd::Close => Cmd::Close,
                })
                .collect(),
        }
    }

    /// The same path moved by `(dx, dy)`.
    pub fn translated(&self, dx: f64, dy: f64) -> Path {
        Path {
            cmds: self
                .cmds
                .iter()
                .map(|c| match *c {
                    Cmd::MoveTo(x, y) => Cmd::MoveTo(x + dx, y + dy),
                    Cmd::LineTo(x, y) => Cmd::LineTo(x + dx, y + dy),
                    Cmd::CubicTo(a, b, c2, d, e, f) => Cmd::CubicTo(a + dx, b + dy, c2 + dx, d + dy, e + dx, f + dy),
                    Cmd::QuadTo(a, b, e, f) => Cmd::QuadTo(a + dx, b + dy, e + dx, f + dy),
                    Cmd::ArcTo { rx, ry, large, sweep, x, y } => Cmd::ArcTo { rx, ry, large, sweep, x: x + dx, y: y + dy },
                    Cmd::Close => Cmd::Close,
                })
                .collect(),
        }
    }
}

/// One sub-path of a geometry and how it is painted.
#[derive(Debug, Clone, PartialEq)]
pub struct SubPath {
    pub path: Path,
    pub fill: bool,
    pub stroke: bool,
    /// Luminance factor of this sub-path's fill (LibreOffice's DARKEN/LIGHTEN commands). 1 = untouched.
    pub shade: f64,
}

/// A rectangle in the shape's own box.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// `shadeColour`: a solid `#rrggbb` colour with a sub-path's shade applied (others unchanged).
pub fn shade_colour(hex: &str, k: f64) -> String {
    let valid = hex.len() == 7 && hex.starts_with('#') && hex[1..].chars().all(|c| c.is_ascii_hexdigit());
    if k == 1.0 || !valid {
        return hex.to_string();
    }
    let c = |o: usize| -> u8 {
        let v = u8::from_str_radix(&hex[o..o + 2], 16).unwrap_or(0) as f64;
        js_round(v * k).clamp(0.0, 255.0) as u8
    };
    format!("#{:02x}{:02x}{:02x}", c(1), c(3), c(5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_like_javascript() {
        assert_eq!(js_num(round2(12.345)), "12.35");
        assert_eq!(js_num(round2(-0.001)), "0");
        assert_eq!(js_num(100.0), "100");
        assert_eq!(js_num(round2(0.1 + 0.2)), "0.3");
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(2.5), 3.0);
    }

    #[test]
    fn svg_is_written_like_the_web() {
        let mut p = Path::new();
        p.move_to(0.0, 1.006);
        p.line_to(10.0, 0.0);
        p.cmds.push(Cmd::ArcTo { rx: 5.0, ry: 5.0, large: false, sweep: true, x: 0.0, y: 0.0 });
        p.close();
        assert_eq!(p.to_svg(), "M 0,1.01 L 10,0 A 5,5 0 0 1 0,0 Z");
    }

    #[test]
    fn shade_scales_each_channel() {
        assert_eq!(shade_colour("#808080", 0.5), "#404040");
        assert_eq!(shade_colour("#ffffff", 1.3), "#ffffff");
        assert_eq!(shade_colour("red", 0.5), "red");
    }
}
