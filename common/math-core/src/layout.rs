//! The browser's part of KaTeX's rendering, done here: the HTML tree laid out as the CSS of `katex.css`
//! would (inline boxes side by side, vertical lists as positioned rows, sizing classes as font-size
//! ratios, `width: 100%` rules and SVGs, `text-align` of fractions, limits, accents and array columns), down
//! to a display list of glyphs, rectangles and paths in ems of the formula's font size.

use std::sync::Arc;

use crate::fonts;
use crate::svg::{parse_path, PathOp};
use crate::tree::{em4, Dom, Span, Svg, SvgFit, VList};

/// A paint operation, in ems of the formula's font size, origin at the left of the baseline, y down.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A glyph of a KaTeX font at `size` ems, its origin at (`x`, `y`).
    Glyph { font: String, id: u16, ch: char, x: f64, y: f64, size: f64, color: Option<String> },
    /// A filled rectangle.
    Rect { x: f64, y: f64, w: f64, h: f64, color: Option<String> },
    /// A filled path (nonzero), already placed; `clip` = (left, top, right, bottom).
    Path { ops: Arc<Vec<PathOp>>, clip: Option<(f64, f64, f64, f64)>, color: Option<String> },
    /// A rectangle outline (`\fbox`).
    Frame { x: f64, y: f64, w: f64, h: f64, thickness: f64, color: Option<String> },
    /// Text the KaTeX fonts do not cover: drawn with the platform's serif font.
    Text { text: String, x: f64, y: f64, size: f64, color: Option<String> },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Align {
    Left,
    Center,
    Right,
}

fn has(classes: &[String], c: &str) -> bool {
    classes.iter().any(|x| x == c)
}

/// Whether the element is `width: 100%` of its container.
fn fills(d: &Dom) -> bool {
    match d {
        Dom::Span(s) => s.css.border_bottom.is_some() && s.css.border_right.is_none() || has(&s.classes, "stretchy") || has(&s.classes, "hide-tail"),
        Dom::Svg(s) => s.svg_width.is_none() || ["hide-tail", "halfarrow-left", "halfarrow-right", "brace-left", "brace-center", "brace-right"].iter().any(|c| has(&s.classes, c)),
        _ => false,
    }
}

fn sym_advance(text: &str, font: &str) -> (f64, Vec<(char, Option<fonts::GlyphInfo>, f64)>) {
    let mut x = 0.0;
    let mut out = Vec::new();
    let mut prev: Option<u16> = None;
    for c in text.chars() {
        let g = fonts::glyph(font, c);
        let w = match g {
            Some(g) => {
                if let Some(p) = prev {
                    x += fonts::kerning(font, p, g.id);
                }
                prev = Some(g.id);
                g.advance
            }
            None => {
                prev = None;
                crate::data::char_metrics(&c.to_string(), font, crate::data::Mode::Text).map(|m| m.width).unwrap_or(0.5)
            }
        };
        out.push((c, g, x));
        x += w;
    }
    (x, out)
}

/// The natural width of `d` in its parent's ems (margins included).
pub fn natural_width(d: &Dom) -> f64 {
    match d {
        Dom::Sym(s) => {
            let (w, _) = sym_advance(&s.text, &s.font);
            s.css.font_scale.unwrap_or(1.0) * (w + if s.italic > 0.0 { em4(s.italic) } else { 0.0 })
        }
        Dom::Frag(s) => s.children.iter().map(natural_width).sum(),
        Dom::Span(s) => span_width(s),
        Dom::VList(v) => vlist_width(v),
        Dom::Svg(s) => {
            let w = if fills(d) { 0.0 } else { s.css.width.or(s.svg_width).unwrap_or(0.0) };
            w.max(s.css.min_width.unwrap_or(0.0)) + s.css.margin_left.unwrap_or(0.0) + s.css.margin_right.unwrap_or(0.0)
        }
    }
}

fn span_inner_width(s: &Span) -> f64 {
    if has(&s.classes, "thinbox") || has(&s.classes, "llap") || has(&s.classes, "rlap") || has(&s.classes, "clap") || (has(&s.classes, "accent-body") && !has(&s.classes, "accent-full")) {
        return 0.0;
    }
    if has(&s.classes, "strut") || has(&s.classes, "pstrut") {
        return 0.0;
    }
    if let Some(w) = s.css.width {
        return w;
    }
    if has(&s.classes, "stretchy") || has(&s.classes, "hide-tail") {
        return s.css.min_width.unwrap_or(0.0);
    }
    let mut w: f64 = s.children.iter().map(natural_width).sum();
    w += s.css.padding_left.unwrap_or(0.0);
    if has(&s.classes, "boxpad") {
        w += 0.6;
    }
    if has(&s.classes, "cancel-pad") {
        w += 0.4;
    }
    if let Some(br) = s.css.border_right {
        w += br;
    }
    if fills(&Dom::Span(s.clone())) && s.children.is_empty() {
        w = 0.0;
    }
    w.max(s.css.min_width.unwrap_or(0.0))
}

fn span_width(s: &Span) -> f64 {
    let k = s.css.font_scale.unwrap_or(1.0);
    k * (span_inner_width(s) + s.css.margin_left.unwrap_or(0.0) + s.css.margin_right.unwrap_or(0.0))
}

fn vlist_width(v: &VList) -> f64 {
    let mut w = 0.0f64;
    for c in &v.children {
        let cw = natural_width(&c.elem);
        w = w.max(c.margin_left.unwrap_or(0.0) + c.wrapper_css.margin_left.unwrap_or(0.0) + cw + c.margin_right.unwrap_or(0.0));
    }
    w
}

/// Lays out the formula's `.katex-html` span: (width, items).
pub fn layout(root: &Dom) -> (f64, Vec<Item>) {
    let mut out = Vec::new();
    let w = natural_width(root);
    place(root, 0.0, 0.0, 1.0, w, &None, Align::Center, &mut out);
    (w, out)
}

#[allow(clippy::too_many_arguments)]
fn place(d: &Dom, x: f64, y: f64, k: f64, avail: f64, color: &Option<String>, align: Align, out: &mut Vec<Item>) {
    match d {
        Dom::Sym(s) => {
            let col = s.css.color.clone().or_else(|| color.clone());
            let k = k * s.css.font_scale.unwrap_or(1.0);
            let (_, glyphs) = sym_advance(&s.text, &s.font);
            for (c, g, gx) in glyphs {
                match g {
                    Some(g) => out.push(Item::Glyph { font: s.font.clone(), id: g.id, ch: c, x: x + gx * k, y, size: k, color: col.clone() }),
                    None => {
                        if !c.is_whitespace() && c != '\u{a0}' && c != '\u{200b}' {
                            out.push(Item::Text { text: c.to_string(), x: x + gx * k, y, size: k, color: col.clone() });
                        }
                    }
                }
            }
        }
        Dom::Frag(s) => {
            let mut cx = x;
            for c in &s.children {
                let w = natural_width(c);
                place(c, cx, y, k, w, color, align, out);
                cx += w * k;
            }
        }
        Dom::Span(s) => place_span(s, x, y, k, avail, color, align, out),
        Dom::VList(v) => place_vlist(v, x, y, k, avail, color, align, out),
        Dom::Svg(s) => place_svg(s, x, y, k, avail, color, out),
    }
}

fn align_of(classes: &[String], inherited: Align) -> Align {
    if has(classes, "msupsub") || has(classes, "col-align-l") || has(classes, "svg-align") {
        Align::Left
    } else if has(classes, "col-align-r") {
        Align::Right
    } else if has(classes, "mfrac") || has(classes, "op-limits") || has(classes, "accent") || has(classes, "col-align-c") || has(classes, "x-arrow") || has(classes, "mover") || has(classes, "munder") {
        Align::Center
    } else {
        inherited
    }
}

#[allow(clippy::too_many_arguments)]
fn place_span(s: &Span, x: f64, y: f64, k: f64, avail: f64, color: &Option<String>, align: Align, out: &mut Vec<Item>) {
    let sk = k * s.css.font_scale.unwrap_or(1.0);
    let col = s.css.color.clone().or_else(|| color.clone());
    let align = align_of(&s.classes, align);
    let mut x0 = x + s.css.margin_left.unwrap_or(0.0) * sk;
    let mut y0 = y;
    if let Some(t) = s.css.top {
        y0 += t * sk;
    }
    if let Some(l) = s.css.left {
        x0 += l * sk;
    }
    let inner_w = if fills(&Dom::Span(s.clone())) { avail / s.css.font_scale.unwrap_or(1.0) - s.css.margin_left.unwrap_or(0.0) - s.css.margin_right.unwrap_or(0.0) } else { span_inner_width(s) };
    // Backgrounds and borders.
    if let Some(bg) = &s.css.background {
        let h = s.css.height.unwrap_or(s.height + s.depth);
        out.push(Item::Rect { x: x0, y: y0 - h * sk, w: inner_w * sk, h: h * sk, color: Some(bg.clone()) });
    }
    if let Some(b) = s.css.border {
        let h = s.css.height.unwrap_or(s.height + s.depth);
        out.push(Item::Frame { x: x0, y: y0 - h * sk, w: inner_w * sk, h: h * sk, thickness: b * sk, color: col.clone() });
    }
    if has(&s.classes, "rule") {
        let w = s.css.border_right.unwrap_or(0.0);
        let h = s.css.border_bottom.unwrap_or(0.0);
        let shift = s.css.vertical_align.unwrap_or(0.0);
        out.push(Item::Rect { x: x0, y: y0 - (h + shift) * sk, w: w * sk, h: h * sk, color: col.clone() });
        return;
    }
    if has(&s.classes, "vertical-separator") {
        let h = s.css.height.unwrap_or(0.0);
        let w = s.css.border_right.unwrap_or(0.0);
        let va = s.css.vertical_align.unwrap_or(0.0);
        let bottom = y0 - va * sk;
        if s.css.dashed {
            let mut yy = bottom - h * sk;
            while yy < bottom {
                let seg = (3.0 * w * sk).min(bottom - yy);
                out.push(Item::Rect { x: x0, y: yy, w: w * sk, h: seg, color: col.clone() });
                yy += 6.0 * w * sk;
            }
        } else {
            out.push(Item::Rect { x: x0, y: bottom - h * sk, w: w * sk, h: h * sk, color: col.clone() });
        }
        return;
    }
    if let Some(t) = s.css.border_bottom {
        // A rule (`frac-line`, `overline-line`…): its bottom edge on the baseline, the full width.
        let t = t.max(1.0 / 96.0 / 21.78 * 0.0);
        if has(&s.classes, "hdashline") {
            let mut xx = x0;
            let end = x0 + inner_w * sk;
            while xx < end {
                let seg = (3.0 * t * sk).min(end - xx);
                out.push(Item::Rect { x: xx, y: y0 - t * sk, w: seg, h: t * sk, color: col.clone() });
                xx += 6.0 * t * sk;
            }
        } else {
            out.push(Item::Rect { x: x0, y: y0 - t * sk, w: inner_w * sk, h: t * sk, color: col.clone() });
        }
        return;
    }
    if has(&s.classes, "stretchy") && (s.children.len() == 2 || s.children.len() == 3) && s.children.iter().all(|c| matches!(c, Dom::Svg(_))) {
        let w = inner_w;
        if s.children.len() == 2 {
            // halfarrow-left / halfarrow-right: 50.2 % each, at the edges.
            let hw = w * 0.502;
            place(&s.children[0], x0, y0, sk, hw, &col, align, out);
            place(&s.children[1], x0 + (w - hw) * sk, y0, sk, hw, &col, align, out);
        } else {
            // brace-left 25.1 %, brace-center at 25 % for 50 %, brace-right 25.1 % at the right edge.
            let (lw, cw) = (w * 0.251, w * 0.5);
            place(&s.children[0], x0, y0, sk, lw, &col, align, out);
            place(&s.children[1], x0 + w * 0.25 * sk, y0, sk, cw, &col, align, out);
            place(&s.children[2], x0 + (w - lw) * sk, y0, sk, lw, &col, align, out);
        }
        return;
    }
    let lap = if has(&s.classes, "llap") {
        Some(Align::Right)
    } else if has(&s.classes, "rlap") {
        Some(Align::Left)
    } else if has(&s.classes, "clap") {
        Some(Align::Center)
    } else {
        None
    };
    if let Some(l) = lap {
        // The `.inner` child is absolutely positioned at the lap's left/right edge.
        if let Some(inner) = s.children.first() {
            let w = natural_width(inner);
            let ix = match l {
                Align::Left => x0,
                Align::Right => x0 - w * sk,
                Align::Center => x0 - w * sk / 2.0,
            };
            place(inner, ix, y0, sk, w, &col, align, out);
        }
        return;
    }
    let mut cx = x0 + s.css.padding_left.unwrap_or(0.0) * sk;
    if has(&s.classes, "boxpad") {
        cx += 0.3 * sk;
    } else if has(&s.classes, "cancel-pad") {
        cx += 0.2 * sk;
    }
    let content_w: f64 = s.children.iter().map(natural_width).sum();
    let _ = content_w;
    for c in &s.children {
        let w = natural_width(c);
        let fill_w = if fills(c) { inner_w } else { w };
        place(c, cx, y0, sk, fill_w, &col, align, out);
        cx += w * sk;
    }
    if let Some(min) = s.css.min_width {
        let _ = min;
    }
}

#[allow(clippy::too_many_arguments)]
fn place_vlist(v: &VList, x: f64, y: f64, k: f64, avail: f64, color: &Option<String>, align: Align, out: &mut Vec<Item>) {
    let col = v.css.color.clone().or_else(|| color.clone());
    let align = align_of(&v.classes, align);
    let cell = vlist_width(v).max(avail.min(f64::MAX)).max(0.0);
    let cell = if avail > 0.0 { cell.max(avail) } else { cell };
    let mut y0 = y;
    if let Some(t) = v.css.top {
        y0 += t * k;
    }
    for c in &v.children {
        let baseline = y0 + (em4(c.top) + em4(c.pstrut)) * k;
        let ml = c.margin_left.unwrap_or(0.0) + c.wrapper_css.margin_left.unwrap_or(0.0);
        let mr = c.margin_right.unwrap_or(0.0);
        let child_align = align_of(&c.wrapper_classes, align);
        if fills(&c.elem) {
            let w = cell - ml - mr;
            place(&c.elem, x + ml * k, baseline, k, w, &col, child_align, out);
            continue;
        }
        let w = natural_width(&c.elem);
        let cw = ml + w + mr;
        let off = match child_align {
            Align::Left => 0.0,
            Align::Center => (cell - cw) / 2.0,
            Align::Right => cell - cw,
        };
        place(&c.elem, x + (off + ml) * k, baseline, k, w, &col, child_align, out);
    }
}

fn place_svg(s: &Svg, x: f64, y: f64, k: f64, avail: f64, color: &Option<String>, out: &mut Vec<Item>) {
    let col = s.css.color.clone().or_else(|| color.clone());
    let w_box = if s.svg_width.is_none() || s.classes.iter().any(|c| c == "hide-tail") { avail.max(s.css.min_width.unwrap_or(0.0)) } else { s.css.width.or(s.svg_width).unwrap_or(0.0) };
    let h_box = s.css.height.unwrap_or(s.svg_height);
    let x0 = x + s.css.margin_left.unwrap_or(0.0) * k;
    let top = y - h_box * k + s.css.top.unwrap_or(0.0) * k;
    let (vbw, vbh) = s.view_box;
    if vbw <= 0.0 || vbh <= 0.0 {
        return;
    }
    // The svg is `width: 100%` of its span (CSS beats the width attribute) unless an inline style sets it.
    let svg_w = w_box;
    let (sx, sy, ox, oy) = match s.fit {
        SvgFit::XMinYMinSlice | SvgFit::XMaxYMinSlice | SvgFit::XMidYMinSlice => {
            let sc = (svg_w / vbw).max(h_box / vbh);
            let ox = match s.fit {
                SvgFit::XMaxYMinSlice => svg_w - vbw * sc,
                SvgFit::XMidYMinSlice => (svg_w - vbw * sc) / 2.0,
                _ => 0.0,
            };
            (sc, sc, ox, 0.0)
        }
        SvgFit::XMinYMin => {
            let sc = (svg_w / vbw).min(h_box / vbh);
            (sc, sc, 0.0, 0.0)
        }
        SvgFit::Meet => {
            let sc = (svg_w / vbw).min(h_box / vbh);
            (sc, sc, (svg_w - vbw * sc) / 2.0, (h_box - vbh * sc) / 2.0)
        }
        SvgFit::None => (svg_w / vbw, h_box / vbh, 0.0, 0.0),
    };
    let clip = Some((x0, top, x0 + w_box * k, top + h_box * k));
    for p in &s.paths {
        let ops: Vec<PathOp> = parse_path(p)
            .into_iter()
            .map(|op| {
                let t = |px: f64, py: f64| (x0 + (ox + px * sx) * k, top + (oy + py * sy) * k);
                match op {
                    PathOp::Move(a, b) => {
                        let (a, b) = t(a, b);
                        PathOp::Move(a, b)
                    }
                    PathOp::Line(a, b) => {
                        let (a, b) = t(a, b);
                        PathOp::Line(a, b)
                    }
                    PathOp::Quad(a, b, c, d) => {
                        let (a, b) = t(a, b);
                        let (c, d) = t(c, d);
                        PathOp::Quad(a, b, c, d)
                    }
                    PathOp::Cubic(a, b, c, d, e, f) => {
                        let (a, b) = t(a, b);
                        let (c, d) = t(c, d);
                        let (e, f) = t(e, f);
                        PathOp::Cubic(a, b, c, d, e, f)
                    }
                    PathOp::Close => PathOp::Close,
                }
            })
            .collect();
        out.push(Item::Path { ops: Arc::new(ops), clip, color: col.clone() });
    }
}
