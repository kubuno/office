//! Draws the core's chart primitives ([`Prim`], the elements the web's chart SVG holds) with the framework's
//! `Graphics`: an SVG path parser for `d`, CSS colours with their opacity, text anchored like SVG's
//! `text-anchor` with its baseline at `y`.

use kubuno_desktop::ui::graphics::{Brush, Color, FillMode, Font, FontStyle, Graphics, GraphicsPath, LineCap, LineJoin, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::Rect;
use kubuno_office_sheets_core::chart::{resolve_color, Anchor, Prim};

/// Where and how the primitives are drawn.
pub struct PrimTarget<'a> {
    pub g: &'a Graphics<'a>,
    /// Top-left of the drawing in the surface.
    pub x: f32,
    pub y: f32,
    /// Scale from the primitives' units to DIP.
    pub scale: f32,
    pub dark: bool,
    /// The text family (`--font-family-sans`, or the chart's `font`).
    pub family: &'a str,
}

/// A CSS colour (`#rgb`, `#rrggbb`, `rgba(…)`, `transparent`, `none`, a `var(--x, fallback)`) and an opacity.
pub fn css_color(c: &str, opacity: f64, dark: bool) -> Option<Color> {
    let c = resolve_color(c, dark);
    let c = c.trim();
    if c.is_empty() || c == "none" || c == "transparent" {
        return None;
    }
    let base = if let Some(inner) = c.strip_prefix("rgba(").or_else(|| c.strip_prefix("rgb(")).and_then(|r| r.strip_suffix(')')) {
        let p: Vec<f32> = inner.split(',').filter_map(|v| v.trim().parse::<f32>().ok()).collect();
        if p.len() < 3 {
            return None;
        }
        Color::rgba_f(p[0] / 255.0, p[1] / 255.0, p[2] / 255.0, p.get(3).copied().unwrap_or(1.0))
    } else if c.len() == 4 && c.starts_with('#') {
        let e: String = c[1..].chars().flat_map(|ch| [ch, ch]).collect();
        Color::from_hex(&format!("#{e}"))?
    } else {
        Color::from_hex(c)?
    };
    Some(if opacity < 1.0 { base.with_alpha(base.a * opacity as f32) } else { base })
}

/// Parses an SVG path (`M L H V C Q A Z`, absolute and relative) into a path, mapped by `map`.
pub fn svg_path(d: &str, map: &dyn Fn(f64, f64) -> PointF, scale: f32, evenodd: bool) -> GraphicsPath {
    let mut path = GraphicsPath::new().with_fill_mode(if evenodd { FillMode::Alternate } else { FillMode::Winding });
    let mut tokens: Vec<Tok> = Vec::new();
    let mut num = String::new();
    let flush = |num: &mut String, tokens: &mut Vec<Tok>| {
        if !num.is_empty() {
            if let Ok(v) = num.parse::<f64>() {
                tokens.push(Tok::Num(v));
            }
            num.clear();
        }
    };
    let mut prev = ' ';
    for ch in d.chars() {
        if ch.is_ascii_alphabetic() && ch != 'e' && ch != 'E' {
            flush(&mut num, &mut tokens);
            tokens.push(Tok::Cmd(ch));
        } else if ch == ',' || ch.is_whitespace() {
            flush(&mut num, &mut tokens);
        } else if ch == '-' && !num.is_empty() && prev != 'e' && prev != 'E' {
            flush(&mut num, &mut tokens);
            num.push(ch);
        } else if ch == '.' && num.contains('.') && !num.contains('e') {
            flush(&mut num, &mut tokens);
            num.push(ch);
        } else {
            num.push(ch);
        }
        prev = ch;
    }
    flush(&mut num, &mut tokens);
    let (mut cx, mut cy, mut sx, mut sy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut i = 0;
    let mut cmd = 'M';
    let nums = |i: &mut usize, n: usize, tokens: &[Tok]| -> Option<Vec<f64>> {
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            match tokens.get(*i) {
                Some(Tok::Num(v)) => out.push(*v),
                _ => return None,
            }
            *i += 1;
        }
        Some(out)
    };
    while i < tokens.len() {
        if let Tok::Cmd(c) = tokens[i] {
            cmd = c;
            i += 1;
            if c == 'Z' || c == 'z' {
                path.close_figure();
                cx = sx;
                cy = sy;
                continue;
            }
        }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
        match cmd.to_ascii_uppercase() {
            'M' => {
                let Some(v) = nums(&mut i, 2, &tokens) else { break };
                cx = ox + v[0];
                cy = oy + v[1];
                sx = cx;
                sy = cy;
                path.move_to(map(cx, cy));
                // Further pairs after a moveto are linetos.
                cmd = if rel { 'l' } else { 'L' };
            }
            'L' => {
                let Some(v) = nums(&mut i, 2, &tokens) else { break };
                cx = ox + v[0];
                cy = oy + v[1];
                path.line_to(map(cx, cy));
            }
            'H' => {
                let Some(v) = nums(&mut i, 1, &tokens) else { break };
                cx = if rel { cx + v[0] } else { v[0] };
                path.line_to(map(cx, cy));
            }
            'V' => {
                let Some(v) = nums(&mut i, 1, &tokens) else { break };
                cy = if rel { cy + v[0] } else { v[0] };
                path.line_to(map(cx, cy));
            }
            'C' => {
                let Some(v) = nums(&mut i, 6, &tokens) else { break };
                let (c1, c2) = (map(ox + v[0], oy + v[1]), map(ox + v[2], oy + v[3]));
                cx = ox + v[4];
                cy = oy + v[5];
                path.bezier_to(c1, c2, map(cx, cy));
            }
            'Q' => {
                let Some(v) = nums(&mut i, 4, &tokens) else { break };
                let c1 = map(ox + v[0], oy + v[1]);
                cx = ox + v[2];
                cy = oy + v[3];
                path.quad_to(c1, map(cx, cy));
            }
            'A' => {
                let Some(v) = nums(&mut i, 7, &tokens) else { break };
                cx = ox + v[5];
                cy = oy + v[6];
                path.arc_to(map(cx, cy), (v[0] as f32) * scale, (v[1] as f32) * scale, v[2] as f32, v[3] != 0.0, v[4] != 0.0);
            }
            _ => {
                i += 1;
            }
        }
    }
    path
}

enum Tok {
    Cmd(char),
    Num(f64),
}

impl PrimTarget<'_> {
    fn pt(&self, x: f64, y: f64) -> PointF {
        PointF::new(self.x + x as f32 * self.scale, self.y + y as f32 * self.scale)
    }

    fn pen(&self, color: Color, width: f64, round: u8) -> Pen {
        let mut p = Pen::new(color, (width as f32 * self.scale).max(0.0));
        if round >= 1 {
            p = p.with_join(LineJoin::Round);
        }
        if round >= 2 {
            p = p.with_caps(LineCap::Round);
        }
        p
    }

    /// Draws the primitives in order (clip regions nest with `Clip`/`Unclip`).
    pub fn draw(&self, prims: &[Prim]) {
        let g = self.g;
        let mut clips = Vec::new();
        for p in prims {
            match p {
                Prim::Rect { x, y, w, h, rx, fill, fill_opacity, stroke, stroke_width, .. } => {
                    let r = Rect::new(self.x + *x as f32 * self.scale, self.y + *y as f32 * self.scale, self.x + (*x + *w) as f32 * self.scale, self.y + (*y + *h) as f32 * self.scale);
                    if let Some(c) = css_color(fill, fill_opacity.unwrap_or(1.0), self.dark) {
                        match rx {
                            Some(r0) if *r0 > 0.0 => g.fill_rounded_rectangle(Brush::solid(c), r, *r0 as f32 * self.scale),
                            _ => g.fill_rectangle(Brush::solid(c), r),
                        }
                    }
                    let sw = stroke_width.unwrap_or(1.0);
                    if let Some(c) = stroke.as_deref().and_then(|s| css_color(s, 1.0, self.dark)).filter(|_| sw > 0.0) {
                        g.draw_rectangle(&self.pen(c, sw, 0), r);
                    }
                }
                Prim::Line { x1, y1, x2, y2, stroke, stroke_opacity, stroke_width } => {
                    if let Some(c) = css_color(stroke, stroke_opacity.unwrap_or(1.0), self.dark) {
                        g.draw_line(&self.pen(c, stroke_width.unwrap_or(1.0), 0), self.pt(*x1, *y1), self.pt(*x2, *y2));
                    }
                }
                Prim::Path { d, fill, fill_opacity, evenodd, stroke, stroke_width, round, .. } => {
                    let path = svg_path(d, &|x, y| self.pt(x, y), self.scale, *evenodd);
                    if let Some(c) = css_color(fill, fill_opacity.unwrap_or(1.0), self.dark) {
                        g.fill_path(Brush::solid(c), &path);
                    }
                    let sw = stroke_width.unwrap_or(1.0);
                    if let Some(c) = stroke.as_deref().and_then(|s| css_color(s, 1.0, self.dark)).filter(|_| sw > 0.0) {
                        g.draw_path(&self.pen(c, sw, *round), &path);
                    }
                }
                Prim::Polygon { points, fill, fill_opacity, stroke, stroke_opacity } => {
                    let pts: Vec<PointF> = points.iter().map(|(x, y)| self.pt(*x, *y)).collect();
                    if pts.len() >= 2 {
                        if let Some(c) = css_color(fill, fill_opacity.unwrap_or(1.0), self.dark) {
                            g.fill_polygon(Brush::solid(c), &pts);
                        }
                        if let Some(c) = stroke.as_deref().and_then(|s| css_color(s, stroke_opacity.unwrap_or(1.0), self.dark)) {
                            g.draw_polygon(&self.pen(c, 1.0, 0), &pts);
                        }
                    }
                }
                Prim::Polyline { points, stroke } => {
                    let pts: Vec<PointF> = points.iter().map(|(x, y)| self.pt(*x, *y)).collect();
                    if let Some(c) = css_color(stroke, 1.0, self.dark) {
                        g.draw_lines(&self.pen(c, 1.0, 0), &pts);
                    }
                }
                Prim::Circle { cx, cy, r, fill, stroke, stroke_opacity, .. } => {
                    let rr = Rect::new(self.x + (*cx - *r) as f32 * self.scale, self.y + (*cy - *r) as f32 * self.scale, self.x + (*cx + *r) as f32 * self.scale, self.y + (*cy + *r) as f32 * self.scale);
                    if let Some(c) = css_color(fill, 1.0, self.dark) {
                        g.fill_ellipse(Brush::solid(c), rr);
                    }
                    if let Some(c) = stroke.as_deref().and_then(|s| css_color(s, stroke_opacity.unwrap_or(1.0), self.dark)) {
                        g.draw_ellipse(&self.pen(c, 1.0, 0), rr);
                    }
                }
                Prim::Text { x, y, text, size, bold, anchor, fill, rotate } => {
                    let Some(c) = css_color(fill, 1.0, self.dark) else { continue };
                    let font = Font::with_dip(self.family, (*size as f32 * self.scale).max(1.0), if *bold { FontStyle::BOLD } else { FontStyle::REGULAR });
                    let fmt = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Near, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP | StringFormatFlags::NO_CLIP };
                    let w = g.measure_string(text, &font, None, &fmt).width;
                    let fs = *size as f32 * self.scale;
                    let p = self.pt(*x, *y);
                    // SVG's y is the baseline: the ascent of the system font is about 0.9 em above it.
                    let left = match anchor {
                        Anchor::Middle => p.x - w / 2.0,
                        Anchor::End => p.x - w,
                        _ => p.x,
                    };
                    let top = p.y - fs * 0.93;
                    let saved = rotate.map(|deg| {
                        let s = g.save();
                        g.rotate_transform_at(deg as f32, p);
                        s
                    });
                    g.draw_string(text, &font, Brush::solid(c), Rect::new(left, top, left + w + 2.0, top + fs * 1.4), &fmt);
                    if let Some(s) = saved {
                        g.restore(s);
                    }
                }
                Prim::Clip { x, y, w, h } => {
                    clips.push(g.save());
                    g.intersect_clip(Rect::new(self.x + *x as f32 * self.scale, self.y + *y as f32 * self.scale, self.x + (*x + *w) as f32 * self.scale, self.y + (*y + *h) as f32 * self.scale));
                }
                Prim::Unclip => {
                    if let Some(s) = clips.pop() {
                        g.restore(s);
                    }
                }
            }
        }
        while let Some(s) = clips.pop() {
            g.restore(s);
        }
    }
}
