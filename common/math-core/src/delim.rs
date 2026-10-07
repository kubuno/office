//! KaTeX's `delimiter.ts`: delimiters at their natural size, at the four large sizes, stacked from pieces
//! (or drawn as tall SVG paths), sized to cover a height (`\left…\right`, fractions' delimiters) — and the
//! square-root surd.

use crate::data::{self, CharMetrics, Mode};
use crate::options::{Options, Style};
use crate::svg;
use crate::tree::{em4, make_span, make_symbol, make_vlist, Css, Dom, Span, Svg, SvgFit, VItem, VPos};

fn get_metrics(symbol: &str, font: &str, mode: Mode) -> CharMetrics {
    let replace = data::symbol(Mode::Math, symbol).and_then(|s| s.replace.clone());
    data::char_metrics(replace.as_deref().unwrap_or(symbol), font, mode).unwrap_or_default()
}

/// `styleWrap`.
fn style_wrap(delim: Dom, to_style: Style, options: &Options, classes: Vec<String>) -> Dom {
    let new_options = options.having_base_style(to_style);
    let mut span = make_span(classes, vec![delim], Some(options));
    span.css.font_scale = new_options.sizing_scale(options);
    let m = new_options.size_multiplier / options.size_multiplier;
    span.height *= m;
    span.depth *= m;
    span.max_font_size = new_options.size_multiplier;
    Dom::Span(span)
}

/// `centerSpan`.
fn center_span(span: &mut Dom, options: &Options, style: Style) {
    let new_options = options.having_base_style(style);
    let shift = (1.0 - options.size_multiplier / new_options.size_multiplier) * options.font_metrics().axis_height;
    span.classes_mut().push("delimcenter".into());
    span.css_mut().top = Some(em4(shift));
    let (h, d) = (span.height() - shift, span.depth() + shift);
    span.set_height(h);
    span.set_depth(d);
}

fn make_small_delim(delim: &str, style: Style, center: bool, options: &Options, mode: Mode, classes: Vec<String>) -> Dom {
    let text = make_symbol(delim, "Main-Regular", mode, Some(options), vec![]);
    let mut span = style_wrap(Dom::Sym(text), style, options, classes);
    if center {
        center_span(&mut span, options, style);
    }
    span
}

fn make_large_delim(delim: &str, size: u8, center: bool, options: &Options, mode: Mode, classes: Vec<String>) -> Dom {
    let inner = make_symbol(delim, &format!("Size{size}-Regular"), mode, Some(options), vec![]);
    let wrapped = Dom::Span(make_span(vec!["delimsizing".into(), format!("size{size}")], vec![Dom::Sym(inner)], Some(options)));
    let mut span = style_wrap(wrapped, Style::TEXT, options, classes);
    if center {
        center_span(&mut span, options, Style::TEXT);
    }
    span
}

fn make_glyph_span(symbol: &str, font: &str, mode: Mode) -> VItem {
    let size_class = if font == "Size1-Regular" { "delim-size1" } else { "delim-size4" };
    let s = make_symbol(symbol, font, mode, None, vec![]);
    let inner = Dom::Span(make_span(vec![], vec![Dom::Sym(s)], None));
    VItem::elem(Dom::Span(make_span(vec!["delimsizinginner".into(), size_class.into()], vec![inner], None)))
}

fn make_inner(ch: char, height: f64, options: &Options) -> VItem {
    let width = data::metrics_entry("Size4-Regular", ch as u32).or_else(|| data::metrics_entry("Size1-Regular", ch as u32)).map(|m| m[4]).unwrap_or(0.0);
    let vbh = (1000.0 * height).round();
    let path = svg::inner_path(ch, vbh);
    let mut classes = Vec::new();
    let mut css = Css::default();
    if options.style.is_tight() {
        classes.push("mtight".into());
    }
    if let Some(c) = options.get_color() {
        css.color = Some(c);
    }
    css.height = Some(em4(height));
    css.width = Some(em4(width));
    VItem::elem(Dom::Svg(Svg { classes, paths: vec![path], view_box: (1000.0 * width, vbh), svg_width: Some(width), svg_height: height, fit: SvgFit::XMinYMin, height, depth: 0.0, max_font_size: 0.0, css }))
}

const LAP_IN_EMS: f64 = 0.008;

fn make_stacked_delim(delim: &str, height_total: f64, center: bool, options: &Options, mode: Mode, classes: Vec<String>) -> Dom {
    let (mut top, mut middle, mut repeat, mut bottom) = (delim.to_string(), None::<String>, delim.to_string(), delim.to_string());
    let mut svg_label = "";
    let mut view_box_width = 0.0;
    let mut font = "Size1-Regular";
    let set = |t: &str, r: &str, b: &str| (t.to_string(), r.to_string(), b.to_string());
    match delim {
        "\\uparrow" => {
            repeat = "\u{23d0}".into();
            bottom = "\u{23d0}".into();
        }
        "\\Uparrow" => {
            repeat = "\u{2016}".into();
            bottom = "\u{2016}".into();
        }
        "\\downarrow" => {
            top = "\u{23d0}".into();
            repeat = "\u{23d0}".into();
        }
        "\\Downarrow" => {
            top = "\u{2016}".into();
            repeat = "\u{2016}".into();
        }
        "\\updownarrow" => (top, repeat, bottom) = set("\\uparrow", "\u{23d0}", "\\downarrow"),
        "\\Updownarrow" => (top, repeat, bottom) = set("\\Uparrow", "\u{2016}", "\\Downarrow"),
        "|" | "\\lvert" | "\\rvert" | "\\vert" => {
            repeat = "\u{2223}".into();
            svg_label = "vert";
            view_box_width = 333.0;
        }
        "\\|" | "\\lVert" | "\\rVert" | "\\Vert" => {
            repeat = "\u{2225}".into();
            svg_label = "doublevert";
            view_box_width = 556.0;
        }
        "[" | "\\lbrack" => {
            (top, repeat, bottom) = set("\u{23a1}", "\u{23a2}", "\u{23a3}");
            font = "Size4-Regular";
            svg_label = "lbrack";
            view_box_width = 667.0;
        }
        "]" | "\\rbrack" => {
            (top, repeat, bottom) = set("\u{23a4}", "\u{23a5}", "\u{23a6}");
            font = "Size4-Regular";
            svg_label = "rbrack";
            view_box_width = 667.0;
        }
        "\\lfloor" | "\u{230a}" => {
            (top, repeat, bottom) = set("\u{23a2}", "\u{23a2}", "\u{23a3}");
            font = "Size4-Regular";
            svg_label = "lfloor";
            view_box_width = 667.0;
        }
        "\\lceil" | "\u{2308}" => {
            (top, repeat, bottom) = set("\u{23a1}", "\u{23a2}", "\u{23a2}");
            font = "Size4-Regular";
            svg_label = "lceil";
            view_box_width = 667.0;
        }
        "\\rfloor" | "\u{230b}" => {
            (top, repeat, bottom) = set("\u{23a5}", "\u{23a5}", "\u{23a6}");
            font = "Size4-Regular";
            svg_label = "rfloor";
            view_box_width = 667.0;
        }
        "\\rceil" | "\u{2309}" => {
            (top, repeat, bottom) = set("\u{23a4}", "\u{23a5}", "\u{23a5}");
            font = "Size4-Regular";
            svg_label = "rceil";
            view_box_width = 667.0;
        }
        "(" | "\\lparen" => {
            (top, repeat, bottom) = set("\u{239b}", "\u{239c}", "\u{239d}");
            font = "Size4-Regular";
            svg_label = "lparen";
            view_box_width = 875.0;
        }
        ")" | "\\rparen" => {
            (top, repeat, bottom) = set("\u{239e}", "\u{239f}", "\u{23a0}");
            font = "Size4-Regular";
            svg_label = "rparen";
            view_box_width = 875.0;
        }
        "\\{" | "\\lbrace" => {
            (top, repeat, bottom) = set("\u{23a7}", "\u{23aa}", "\u{23a9}");
            middle = Some("\u{23a8}".into());
            font = "Size4-Regular";
        }
        "\\}" | "\\rbrace" => {
            (top, repeat, bottom) = set("\u{23ab}", "\u{23aa}", "\u{23ad}");
            middle = Some("\u{23ac}".into());
            font = "Size4-Regular";
        }
        "\\lgroup" | "\u{27ee}" => {
            (top, repeat, bottom) = set("\u{23a7}", "\u{23aa}", "\u{23a9}");
            font = "Size4-Regular";
        }
        "\\rgroup" | "\u{27ef}" => {
            (top, repeat, bottom) = set("\u{23ab}", "\u{23aa}", "\u{23ad}");
            font = "Size4-Regular";
        }
        "\\lmoustache" | "\u{23b0}" => {
            (top, repeat, bottom) = set("\u{23a7}", "\u{23aa}", "\u{23ad}");
            font = "Size4-Regular";
        }
        "\\rmoustache" | "\u{23b1}" => {
            (top, repeat, bottom) = set("\u{23ab}", "\u{23aa}", "\u{23a9}");
            font = "Size4-Regular";
        }
        _ => {}
    }
    let tm = get_metrics(&top, font, mode);
    let top_ht = tm.height + tm.depth;
    let rm = get_metrics(&repeat, font, mode);
    let repeat_ht = rm.height + rm.depth;
    let bm = get_metrics(&bottom, font, mode);
    let bottom_ht = bm.height + bm.depth;
    let mut middle_ht = 0.0;
    let mut middle_factor = 1.0;
    if let Some(m) = &middle {
        let mm = get_metrics(m, font, mode);
        middle_ht = mm.height + mm.depth;
        middle_factor = 2.0;
    }
    let min_height = top_ht + bottom_ht + middle_ht;
    let repeat_count = if repeat_ht > 0.0 { ((height_total - min_height) / (middle_factor * repeat_ht)).ceil().max(0.0) } else { 0.0 };
    let real_ht = min_height + repeat_count * middle_factor * repeat_ht;
    let mut axis = options.font_metrics().axis_height;
    if center {
        axis *= options.size_multiplier;
    }
    let depth = real_ht / 2.0 - axis;
    let mut stack = Vec::new();
    if !svg_label.is_empty() {
        let mid = real_ht - top_ht - bottom_ht;
        let vbh = (real_ht * 1000.0).round();
        let path = svg::tall_delim(svg_label, (mid * 1000.0).round());
        let w = view_box_width / 1000.0;
        let mut css = Css::default();
        let mut classes = Vec::new();
        if options.style.is_tight() {
            classes.push("mtight".into());
        }
        if let Some(c) = options.get_color() {
            css.color = Some(c);
        }
        css.width = Some(em4(w));
        css.height = Some(em4(vbh / 1000.0));
        stack.push(VItem::elem(Dom::Svg(Svg { classes, paths: vec![path], view_box: (view_box_width, vbh), svg_width: Some(w), svg_height: vbh / 1000.0, fit: SvgFit::Meet, height: vbh / 1000.0, depth: 0.0, max_font_size: 0.0, css })));
    } else {
        stack.push(make_glyph_span(&bottom, font, mode));
        stack.push(VItem::Kern(-LAP_IN_EMS));
        let rch = repeat.chars().next().unwrap_or(' ');
        match &middle {
            None => {
                let inner = real_ht - top_ht - bottom_ht + 2.0 * LAP_IN_EMS;
                stack.push(make_inner(rch, inner, options));
            }
            Some(m) => {
                let inner = (real_ht - top_ht - bottom_ht - middle_ht) / 2.0 + 2.0 * LAP_IN_EMS;
                stack.push(make_inner(rch, inner, options));
                stack.push(VItem::Kern(-LAP_IN_EMS));
                stack.push(make_glyph_span(m, font, mode));
                stack.push(VItem::Kern(-LAP_IN_EMS));
                stack.push(make_inner(rch, inner, options));
            }
        }
        stack.push(VItem::Kern(-LAP_IN_EMS));
        stack.push(make_glyph_span(&top, font, mode));
    }
    let new_options = options.having_base_style(Style::TEXT);
    let inner = make_vlist(VPos::Bottom(depth), stack);
    let wrapped = Dom::Span(make_span(vec!["delimsizing".into(), "mult".into()], vec![inner], Some(&new_options)));
    style_wrap(wrapped, Style::TEXT, options, classes)
}

const VB_PAD: f64 = 80.0;
const EM_PAD: f64 = 0.08;
pub const SIZE_TO_MAX_HEIGHT: [f64; 5] = [0.0, 1.2, 1.8, 2.4, 3.0];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Delimiter {
    Small(Style),
    Large(u8),
    Stack,
}

const STACK_LARGE: &[&str] = &["(", "\\lparen", ")", "\\rparen", "[", "\\lbrack", "]", "\\rbrack", "\\{", "\\lbrace", "\\}", "\\rbrace", "\\lfloor", "\\rfloor", "\u{230a}", "\u{230b}", "\\lceil", "\\rceil", "\u{2308}", "\u{2309}", "\\surd"];
#[allow(dead_code)]
const STACK_ALWAYS: &[&str] = &["\\uparrow", "\\downarrow", "\\updownarrow", "\\Uparrow", "\\Downarrow", "\\Updownarrow", "|", "\\|", "\\vert", "\\Vert", "\\lvert", "\\rvert", "\\lVert", "\\rVert", "\\lgroup", "\\rgroup", "\u{27ee}", "\u{27ef}", "\\lmoustache", "\\rmoustache", "\u{23b0}", "\u{23b1}"];
const STACK_NEVER: &[&str] = &["<", ">", "\\langle", "\\rangle", "/", "\\backslash", "\\lt", "\\gt"];

fn seq_never() -> Vec<Delimiter> {
    vec![Delimiter::Small(Style::SCRIPTSCRIPT), Delimiter::Small(Style::SCRIPT), Delimiter::Small(Style::TEXT), Delimiter::Large(1), Delimiter::Large(2), Delimiter::Large(3), Delimiter::Large(4)]
}
fn seq_always() -> Vec<Delimiter> {
    vec![Delimiter::Small(Style::SCRIPTSCRIPT), Delimiter::Small(Style::SCRIPT), Delimiter::Small(Style::TEXT), Delimiter::Stack]
}
fn seq_large() -> Vec<Delimiter> {
    vec![Delimiter::Small(Style::SCRIPTSCRIPT), Delimiter::Small(Style::SCRIPT), Delimiter::Small(Style::TEXT), Delimiter::Large(1), Delimiter::Large(2), Delimiter::Large(3), Delimiter::Large(4), Delimiter::Stack]
}

fn delim_font(d: Delimiter) -> String {
    match d {
        Delimiter::Small(_) => "Main-Regular".into(),
        Delimiter::Large(s) => format!("Size{s}-Regular"),
        Delimiter::Stack => "Size4-Regular".into(),
    }
}

fn traverse_sequence(delim: &str, height: f64, seq: &[Delimiter], options: &Options) -> Delimiter {
    let start = 2usize.min((3 - options.style.size() as i32).max(0) as usize);
    for &d in seq.iter().skip(start) {
        if d == Delimiter::Stack {
            break;
        }
        let m = get_metrics(delim, &delim_font(d), Mode::Math);
        let mut hd = m.height + m.depth;
        if let Delimiter::Small(st) = d {
            hd *= options.having_base_style(st).size_multiplier;
        }
        if hd > height {
            return d;
        }
    }
    *seq.last().unwrap_or(&Delimiter::Stack)
}

fn normalize(delim: &str) -> &str {
    match delim {
        "<" | "\\lt" | "\u{27e8}" => "\\langle",
        ">" | "\\gt" | "\u{27e9}" => "\\rangle",
        d => d,
    }
}

/// `makeSizedDelim`.
pub fn make_sized_delim(delim: &str, size: u8, options: &Options, mode: Mode, classes: Vec<String>) -> Dom {
    let delim = normalize(delim);
    if STACK_LARGE.contains(&delim) || STACK_NEVER.contains(&delim) {
        make_large_delim(delim, size, false, options, mode, classes)
    } else {
        make_stacked_delim(delim, SIZE_TO_MAX_HEIGHT[size as usize], false, options, mode, classes)
    }
}

/// `makeCustomSizedDelim`.
pub fn make_custom_sized_delim(delim: &str, height: f64, center: bool, options: &Options, mode: Mode, classes: Vec<String>) -> Dom {
    let delim = normalize(delim);
    let seq = if STACK_NEVER.contains(&delim) {
        seq_never()
    } else if STACK_LARGE.contains(&delim) {
        seq_large()
    } else {
        seq_always()
    };
    match traverse_sequence(delim, height, &seq, options) {
        Delimiter::Small(st) => make_small_delim(delim, st, center, options, mode, classes),
        Delimiter::Large(s) => make_large_delim(delim, s, center, options, mode, classes),
        Delimiter::Stack => make_stacked_delim(delim, height, center, options, mode, classes),
    }
}

/// `makeLeftRightDelim`.
pub fn make_left_right_delim(delim: &str, height: f64, depth: f64, options: &Options, mode: Mode, classes: Vec<String>) -> Dom {
    let axis = options.font_metrics().axis_height * options.size_multiplier;
    let factor = 901.0;
    let extend = 5.0 / options.font_metrics().pt_per_em;
    let max_dist = (height - axis).max(depth + axis);
    let total = (max_dist / 500.0 * factor).max(2.0 * max_dist - extend);
    make_custom_sized_delim(delim, total, true, options, mode, classes)
}

/// `makeSqrtImage`: the surd span, its rule width and its advance width.
pub fn make_sqrt_image(height: f64, options: &Options) -> (Dom, f64, f64) {
    let new_options = options.having_base_sizing();
    let delim = traverse_sequence("\\surd", height * new_options.size_multiplier, &seq_large(), &new_options);
    let mut size_multiplier = new_options.size_multiplier;
    let extra_vinculum = (options.min_rule_thickness - options.font_metrics().sqrt_rule_thickness).max(0.0);
    let (span_height, tex_height, view_box_height, advance_width, name, min_width);
    match delim {
        Delimiter::Small(_) => {
            view_box_height = 1000.0 + 1000.0 * extra_vinculum + VB_PAD;
            if height < 1.0 {
                size_multiplier = 1.0;
            } else if height < 1.4 {
                size_multiplier = 0.7;
            }
            span_height = (1.0 + extra_vinculum + EM_PAD) / size_multiplier;
            tex_height = (1.0 + extra_vinculum) / size_multiplier;
            name = "sqrtMain".to_string();
            min_width = 0.853;
            advance_width = 0.833 / size_multiplier;
        }
        Delimiter::Large(s) => {
            view_box_height = (1000.0 + VB_PAD) * SIZE_TO_MAX_HEIGHT[s as usize];
            tex_height = (SIZE_TO_MAX_HEIGHT[s as usize] + extra_vinculum) / size_multiplier;
            span_height = (SIZE_TO_MAX_HEIGHT[s as usize] + extra_vinculum + EM_PAD) / size_multiplier;
            name = format!("sqrtSize{s}");
            min_width = 1.02;
            advance_width = 1.0 / size_multiplier;
        }
        Delimiter::Stack => {
            span_height = height + extra_vinculum + EM_PAD;
            tex_height = height + extra_vinculum;
            view_box_height = (1000.0 * height + extra_vinculum).floor() + VB_PAD;
            name = "sqrtTall".to_string();
            min_width = 0.742;
            advance_width = 1.056;
        }
    }
    let path = svg::sqrt_path(&name, extra_vinculum, view_box_height);
    let mut classes = vec!["hide-tail".to_string()];
    let mut css = Css::default();
    if options.style.is_tight() {
        classes.push("mtight".into());
    }
    if let Some(c) = options.get_color() {
        css.color = Some(c);
    }
    css.min_width = Some(min_width);
    css.height = Some(em4(span_height));
    let span = Dom::Svg(Svg { classes, paths: vec![path], view_box: (400000.0, view_box_height), svg_width: Some(400.0), svg_height: span_height, fit: SvgFit::XMinYMinSlice, height: tex_height, depth: 0.0, max_font_size: 0.0, css });
    let rule_width = options.font_metrics().sqrt_rule_thickness + extra_vinculum;
    (span, rule_width, advance_width)
}

/// An empty span of the given classes (a `.` delimiter).
pub fn empty_span(classes: Vec<String>) -> Dom {
    Dom::Span(Span { classes, ..Span::default() })
}
