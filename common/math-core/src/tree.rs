//! KaTeX's HTML tree (`domTree.ts`) and its construction helpers (`buildCommon.ts`): symbols, spans, vertical
//! lists, SVG spans, with the heights and depths KaTeX computes and the CSS it would emit.

use crate::data::{self, CharMetrics, Measurement, Mode};
use crate::options::Options;

/// The CSS properties KaTeX sets inline (in ems of the element's font size).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Css {
    pub color: Option<String>,
    pub margin_left: Option<f64>,
    pub margin_right: Option<f64>,
    /// `position: relative; top`.
    pub top: Option<f64>,
    /// `position: relative; left`.
    pub left: Option<f64>,
    pub padding_left: Option<f64>,
    pub width: Option<f64>,
    pub min_width: Option<f64>,
    pub height: Option<f64>,
    pub border_bottom: Option<f64>,
    pub border_right: Option<f64>,
    pub dashed: bool,
    pub vertical_align: Option<f64>,
    /// The font-size ratio of KaTeX's sizing classes.
    pub font_scale: Option<f64>,
    /// `background-color`.
    pub background: Option<String>,
    /// `border` (all sides) width and colour (`\fbox`).
    pub border: Option<f64>,
    pub border_color: Option<String>,
    pub padding: Option<f64>,
}

/// A symbol (`SymbolNode`): text in one font.
#[derive(Clone, Debug, PartialEq)]
pub struct Sym {
    pub text: String,
    pub font: String,
    pub height: f64,
    pub depth: f64,
    pub italic: f64,
    pub skew: f64,
    pub width: f64,
    pub max_font_size: f64,
    pub classes: Vec<String>,
    pub css: Css,
}

/// A span (`Span`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub classes: Vec<String>,
    pub children: Vec<Dom>,
    pub height: f64,
    pub depth: f64,
    pub max_font_size: f64,
    pub italic: f64,
    pub css: Css,
}

/// A child of a vertical list.
#[derive(Clone, Debug, PartialEq)]
pub struct VChild {
    pub elem: Dom,
    /// The wrapper's `top` (unrounded, ems) and the pstrut height.
    pub top: f64,
    pub pstrut: f64,
    pub margin_left: Option<f64>,
    pub margin_right: Option<f64>,
    pub wrapper_classes: Vec<String>,
    pub wrapper_css: Css,
}

/// A vertical list (`makeVList`'s `vlist-t`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VList {
    pub classes: Vec<String>,
    pub children: Vec<VChild>,
    pub height: f64,
    pub depth: f64,
    pub max_font_size: f64,
    pub italic: f64,
    pub css: Css,
}

/// How an SVG fills its span (`preserveAspectRatio`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SvgFit {
    /// `xMinYMin slice` (the sqrt surd, clipped to the span).
    XMinYMinSlice,
    /// `xMaxYMin slice` (right-aligned arrow heads).
    XMaxYMinSlice,
    /// `xMidYMin slice` (the middle of a brace).
    XMidYMinSlice,
    /// `xMinYMin` (meet).
    XMinYMin,
    /// No attribute: `xMidYMid meet`.
    Meet,
    /// `none`: stretched.
    None,
}

/// An SVG span (`SvgSpan` holding one `SvgNode`).
#[derive(Clone, Debug, PartialEq)]
pub struct Svg {
    pub classes: Vec<String>,
    pub paths: Vec<String>,
    pub view_box: (f64, f64),
    /// The SVG width in ems (`None` = 100% of the span).
    pub svg_width: Option<f64>,
    pub svg_height: f64,
    pub fit: SvgFit,
    pub height: f64,
    pub depth: f64,
    pub max_font_size: f64,
    pub css: Css,
}

/// A node of the HTML tree.
#[derive(Clone, Debug, PartialEq)]
pub enum Dom {
    Sym(Sym),
    Span(Span),
    /// `DocumentFragment`.
    Frag(Span),
    VList(VList),
    Svg(Svg),
}

impl Dom {
    pub fn height(&self) -> f64 {
        match self {
            Dom::Sym(s) => s.height,
            Dom::Span(s) | Dom::Frag(s) => s.height,
            Dom::VList(v) => v.height,
            Dom::Svg(s) => s.height,
        }
    }
    pub fn depth(&self) -> f64 {
        match self {
            Dom::Sym(s) => s.depth,
            Dom::Span(s) | Dom::Frag(s) => s.depth,
            Dom::VList(v) => v.depth,
            Dom::Svg(s) => s.depth,
        }
    }
    pub fn max_font_size(&self) -> f64 {
        match self {
            Dom::Sym(s) => s.max_font_size,
            Dom::Span(s) | Dom::Frag(s) => s.max_font_size,
            Dom::VList(v) => v.max_font_size,
            Dom::Svg(s) => s.max_font_size,
        }
    }
    pub fn italic(&self) -> f64 {
        match self {
            Dom::Sym(s) => s.italic,
            Dom::Span(s) | Dom::Frag(s) => s.italic,
            Dom::VList(v) => v.italic,
            Dom::Svg(_) => 0.0,
        }
    }
    pub fn set_height(&mut self, h: f64) {
        match self {
            Dom::Sym(s) => s.height = h,
            Dom::Span(s) | Dom::Frag(s) => s.height = h,
            Dom::VList(v) => v.height = h,
            Dom::Svg(s) => s.height = h,
        }
    }
    pub fn set_depth(&mut self, d: f64) {
        match self {
            Dom::Sym(s) => s.depth = d,
            Dom::Span(s) | Dom::Frag(s) => s.depth = d,
            Dom::VList(v) => v.depth = d,
            Dom::Svg(s) => s.depth = d,
        }
    }
    pub fn set_max_font_size(&mut self, m: f64) {
        match self {
            Dom::Sym(s) => s.max_font_size = m,
            Dom::Span(s) | Dom::Frag(s) => s.max_font_size = m,
            Dom::VList(v) => v.max_font_size = m,
            Dom::Svg(s) => s.max_font_size = m,
        }
    }
    pub fn set_italic(&mut self, i: f64) {
        match self {
            Dom::Sym(s) => s.italic = i,
            Dom::Span(s) | Dom::Frag(s) => s.italic = i,
            Dom::VList(v) => v.italic = i,
            Dom::Svg(_) => {}
        }
    }
    pub fn classes(&self) -> &[String] {
        match self {
            Dom::Sym(s) => &s.classes,
            Dom::Span(s) | Dom::Frag(s) => &s.classes,
            Dom::VList(v) => &v.classes,
            Dom::Svg(s) => &s.classes,
        }
    }
    pub fn classes_mut(&mut self) -> &mut Vec<String> {
        match self {
            Dom::Sym(s) => &mut s.classes,
            Dom::Span(s) | Dom::Frag(s) => &mut s.classes,
            Dom::VList(v) => &mut v.classes,
            Dom::Svg(s) => &mut s.classes,
        }
    }
    pub fn css_mut(&mut self) -> &mut Css {
        match self {
            Dom::Sym(s) => &mut s.css,
            Dom::Span(s) | Dom::Frag(s) => &mut s.css,
            Dom::VList(v) => &mut v.css,
            Dom::Svg(s) => &mut s.css,
        }
    }
    pub fn css(&self) -> &Css {
        match self {
            Dom::Sym(s) => &s.css,
            Dom::Span(s) | Dom::Frag(s) => &s.css,
            Dom::VList(v) => &v.css,
            Dom::Svg(s) => &s.css,
        }
    }
    pub fn has_class(&self, c: &str) -> bool {
        self.classes().iter().any(|x| x == c)
    }
    /// `classes[0]`.
    pub fn first_class(&self) -> Option<&str> {
        self.classes().first().map(String::as_str)
    }
    pub fn is_frag(&self) -> bool {
        matches!(self, Dom::Frag(_))
    }
}

/// `makeEm`: `+n.toFixed(4)`.
pub fn em4(n: f64) -> f64 {
    (n * 10000.0).round() / 10000.0
}

fn lookup_symbol(value: &str, font: &str, mode: Mode) -> (String, Option<CharMetrics>) {
    let mut v = value.to_string();
    if let Some(info) = data::symbol(mode, value) {
        if let Some(r) = &info.replace {
            v = r.clone();
        }
    }
    let m = data::char_metrics(&v, font, mode);
    (v, m)
}

/// `makeSymbol`.
pub fn make_symbol(value: &str, font: &str, mode: Mode, options: Option<&Options>, classes: Vec<String>) -> Sym {
    let (text, metrics) = lookup_symbol(value, font, mode);
    let mut s = match metrics {
        Some(m) => {
            let mut italic = m.italic;
            if mode == Mode::Text || options.is_some_and(|o| o.font == "mathit") {
                italic = 0.0;
            }
            Sym { text, font: font.to_string(), height: m.height, depth: m.depth, italic, skew: m.skew, width: m.width, max_font_size: 0.0, classes, css: Css::default() }
        }
        None => Sym { text, font: font.to_string(), height: 0.0, depth: 0.0, italic: 0.0, skew: 0.0, width: 0.0, max_font_size: 0.0, classes, css: Css::default() },
    };
    if let Some(o) = options {
        s.max_font_size = o.size_multiplier;
        if o.style.is_tight() {
            s.classes.push("mtight".into());
        }
        if let Some(c) = o.get_color() {
            s.css.color = Some(c);
        }
    }
    s
}

/// `mathsym`.
pub fn mathsym(value: &str, mode: Mode, options: &Options, classes: Vec<String>) -> Sym {
    if options.font == "boldsymbol" && lookup_symbol(value, "Main-Bold", mode).1.is_some() {
        let mut c = classes;
        c.push("mathbf".into());
        return make_symbol(value, "Main-Bold", mode, Some(options), c);
    }
    let main = value == "\\" || data::symbol(mode, value).is_some_and(|s| s.font == "main");
    if main {
        make_symbol(value, "Main-Regular", mode, Some(options), classes)
    } else {
        let mut c = classes;
        c.push("amsrm".into());
        make_symbol(value, "AMS-Regular", mode, Some(options), c)
    }
}

/// `fontMap`: a math font command's font name.
pub fn font_map(font: &str) -> Option<&'static str> {
    Some(match font {
        "mathbf" => "Main-Bold",
        "mathrm" => "Main-Regular",
        "textit" => "Main-Italic",
        "mathit" => "Main-Italic",
        "mathnormal" => "Math-Italic",
        "mathsfit" => "SansSerif-Italic",
        "mathbb" => "AMS-Regular",
        "mathcal" => "Caligraphic-Regular",
        "mathfrak" => "Fraktur-Regular",
        "mathscr" => "Script-Regular",
        "mathsf" => "SansSerif-Regular",
        "mathtt" => "Typewriter-Regular",
        _ => return None,
    })
}

/// `retrieveTextFontName`.
pub fn text_font_name(family: &str, weight: &str, shape: &str) -> String {
    let base = match family {
        "amsrm" => "AMS",
        "textrm" => "Main",
        "textsf" => "SansSerif",
        "texttt" => "Typewriter",
        other => other,
    };
    let st = if weight == "textbf" && shape == "textit" {
        "BoldItalic"
    } else if weight == "textbf" {
        "Bold"
    } else if shape == "textit" {
        "Italic"
    } else {
        "Regular"
    };
    format!("{base}-{st}")
}

/// `makeOrd`.
pub fn make_ord(text: &str, mode: Mode, options: &Options, mathord: bool) -> Dom {
    let classes = vec!["mord".to_string()];
    let use_font = mode == Mode::Math || (mode == Mode::Text && !options.font.is_empty());
    let font_or_family = if use_font { options.font.clone() } else { options.font_family.clone() };
    if !font_or_family.is_empty() {
        let (font_name, font_classes): (String, Vec<String>) = if font_or_family == "boldsymbol" {
            if mathord && lookup_symbol(text, "Math-BoldItalic", mode).1.is_some() {
                ("Math-BoldItalic".into(), vec!["boldsymbol".into()])
            } else {
                ("Main-Bold".into(), vec!["mathbf".into()])
            }
        } else if use_font {
            (font_map(&options.font).unwrap_or("Main-Regular").to_string(), vec![options.font.clone()])
        } else {
            (text_font_name(&options.font_family, &options.font_weight, &options.font_shape), vec![options.font_family.clone(), options.font_weight.clone(), options.font_shape.clone()])
        };
        if lookup_symbol(text, &font_name, mode).1.is_some() {
            let mut c = classes;
            c.extend(font_classes);
            return Dom::Sym(make_symbol(text, &font_name, mode, Some(options), c));
        }
    }
    if mathord {
        let mut c = classes;
        c.push("mathnormal".into());
        Dom::Sym(make_symbol(text, "Math-Italic", mode, Some(options), c))
    } else {
        let font = data::symbol(mode, text).map(|s| s.font.clone());
        match font.as_deref() {
            Some("ams") => {
                let f = text_font_name("amsrm", &options.font_weight, &options.font_shape);
                let mut c = classes;
                c.extend(["amsrm".to_string(), options.font_weight.clone(), options.font_shape.clone()]);
                Dom::Sym(make_symbol(text, &f, mode, Some(options), c))
            }
            _ => {
                let f = text_font_name("textrm", &options.font_weight, &options.font_shape);
                let mut c = classes;
                c.extend([options.font_weight.clone(), options.font_shape.clone()]);
                Dom::Sym(make_symbol(text, &f, mode, Some(options), c))
            }
        }
    }
}

fn class_string(c: &[String]) -> String {
    c.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" ")
}

fn can_combine(prev: &Sym, next: &Sym) -> bool {
    if class_string(&prev.classes) != class_string(&next.classes) || prev.skew != next.skew || prev.max_font_size != next.max_font_size || (prev.italic != 0.0 && prev.classes.iter().any(|c| c == "mathnormal")) {
        return false;
    }
    if prev.classes.len() == 1 && (prev.classes[0] == "mbin" || prev.classes[0] == "mord") {
        return false;
    }
    prev.css == next.css
}

/// `tryCombineChars`.
pub fn try_combine_chars(chars: &mut Vec<Dom>) {
    let mut i = 0;
    while i + 1 < chars.len() {
        let combine = matches!((&chars[i], &chars[i + 1]), (Dom::Sym(p), Dom::Sym(n)) if can_combine(p, n));
        if combine {
            let next = chars.remove(i + 1);
            if let (Dom::Sym(p), Dom::Sym(n)) = (&mut chars[i], next) {
                p.text.push_str(&n.text);
                p.height = p.height.max(n.height);
                p.depth = p.depth.max(n.depth);
                p.italic = n.italic;
                p.width += n.width;
            }
        } else {
            i += 1;
        }
    }
}

fn size_from_children(children: &[Dom]) -> (f64, f64, f64) {
    let (mut h, mut d, mut m) = (0.0f64, 0.0f64, 0.0f64);
    for c in children {
        if c.height() > h {
            h = c.height();
        }
        if c.depth() > d {
            d = c.depth();
        }
        if c.max_font_size() > m {
            m = c.max_font_size();
        }
    }
    (h, d, m)
}

fn init_css(classes: &mut Vec<String>, options: Option<&Options>) -> Css {
    let mut css = Css::default();
    if let Some(o) = options {
        if o.style.is_tight() {
            classes.push("mtight".into());
        }
        if let Some(c) = o.get_color() {
            css.color = Some(c);
        }
    }
    css
}

/// `makeSpan`.
pub fn make_span(classes: Vec<String>, children: Vec<Dom>, options: Option<&Options>) -> Span {
    let mut classes = classes;
    let css = init_css(&mut classes, options);
    let (height, depth, max_font_size) = size_from_children(&children);
    Span { classes, children, height, depth, max_font_size, italic: 0.0, css }
}

pub fn span_dom(classes: &[&str], children: Vec<Dom>, options: Option<&Options>) -> Dom {
    Dom::Span(make_span(classes.iter().map(|s| s.to_string()).collect(), children, options))
}

/// `makeLineSpan`: a rule drawn as a bottom border.
pub fn make_line_span(class: &str, options: &Options, thickness: Option<f64>) -> Dom {
    let mut line = make_span(vec![class.to_string()], vec![], Some(options));
    line.height = thickness.unwrap_or(options.font_metrics().default_rule_thickness).max(options.min_rule_thickness);
    line.css.border_bottom = Some(em4(line.height));
    line.max_font_size = 1.0;
    Dom::Span(line)
}

/// `makeFragment`.
pub fn make_fragment(children: Vec<Dom>) -> Dom {
    let (height, depth, max_font_size) = size_from_children(&children);
    Dom::Frag(Span { classes: vec![], children, height, depth, max_font_size, italic: 0.0, css: Css::default() })
}

/// `wrapFragment`.
pub fn wrap_fragment(group: Dom, options: &Options) -> Dom {
    if group.is_frag() {
        Dom::Span(make_span(vec![], vec![group], Some(options)))
    } else {
        group
    }
}

/// A child of `makeVList`.
#[allow(clippy::large_enum_variant)]
pub enum VItem {
    Elem { elem: Dom, shift: f64, margin_left: Option<f64>, margin_right: Option<f64>, wrapper_classes: Vec<String>, wrapper_css: Css },
    Kern(f64),
}

impl VItem {
    pub fn elem(elem: Dom) -> VItem {
        VItem::Elem { elem, shift: 0.0, margin_left: None, margin_right: None, wrapper_classes: vec![], wrapper_css: Css::default() }
    }
    pub fn shifted(elem: Dom, shift: f64) -> VItem {
        VItem::Elem { elem, shift, margin_left: None, margin_right: None, wrapper_classes: vec![], wrapper_css: Css::default() }
    }
}

/// The position type of `makeVList`.
pub enum VPos {
    IndividualShift,
    Top(f64),
    Bottom(f64),
    Shift(f64),
    FirstBaseline,
}

/// `makeVList`.
pub fn make_vlist(pos: VPos, items: Vec<VItem>) -> Dom {
    // getVListChildrenAndDepth
    let (children, depth): (Vec<VItem>, f64) = match pos {
        VPos::IndividualShift => {
            let mut out = Vec::new();
            let mut it = items.into_iter();
            let mut depth = 0.0;
            if let Some(first) = it.next() {
                let (fshift, fh, fd) = match &first {
                    VItem::Elem { elem, shift, .. } => (*shift, elem.height(), elem.depth()),
                    VItem::Kern(_) => (0.0, 0.0, 0.0),
                };
                depth = -fshift - fd;
                let mut curr = depth;
                let (mut prev_h, mut prev_d) = (fh, fd);
                out.push(first);
                for c in it {
                    if let VItem::Elem { elem, shift, .. } = &c {
                        let diff = -shift - curr - elem.depth();
                        let size = diff - (prev_h + prev_d);
                        curr += diff;
                        prev_h = elem.height();
                        prev_d = elem.depth();
                        out.push(VItem::Kern(size));
                    }
                    out.push(c);
                }
            }
            (out, depth)
        }
        VPos::Top(top) => {
            let mut bottom = top;
            for c in &items {
                bottom -= match c {
                    VItem::Kern(k) => *k,
                    VItem::Elem { elem, .. } => elem.height() + elem.depth(),
                };
            }
            (items, bottom)
        }
        VPos::Bottom(b) => (items, -b),
        VPos::Shift(s) => {
            let fd = match items.first() {
                Some(VItem::Elem { elem, .. }) => elem.depth(),
                _ => 0.0,
            };
            (items, -fd - s)
        }
        VPos::FirstBaseline => {
            let fd = match items.first() {
                Some(VItem::Elem { elem, .. }) => elem.depth(),
                _ => 0.0,
            };
            (items, -fd)
        }
    };
    let mut pstrut = 0.0f64;
    for c in &children {
        if let VItem::Elem { elem, .. } = c {
            pstrut = pstrut.max(elem.max_font_size()).max(elem.height());
        }
    }
    pstrut += 2.0;
    let mut out = Vec::new();
    let (mut min_pos, mut max_pos, mut curr) = (depth, depth, depth);
    for c in children {
        match c {
            VItem::Kern(k) => curr += k,
            VItem::Elem { elem, margin_left, margin_right, wrapper_classes, wrapper_css, .. } => {
                let top = -pstrut - curr - elem.depth();
                curr += elem.height() + elem.depth();
                out.push(VChild { elem, top, pstrut, margin_left, margin_right, wrapper_classes, wrapper_css });
            }
        }
        min_pos = min_pos.min(curr);
        max_pos = max_pos.max(curr);
    }
    Dom::VList(VList { classes: vec!["vlist-t".into()], children: out, height: max_pos, depth: -min_pos, max_font_size: 0.0, italic: 0.0, css: Css::default() })
}

/// `calculateSize`.
pub fn calculate_size(m: &Measurement, options: &Options) -> f64 {
    let pt_per_unit = |u: &str| -> Option<f64> {
        Some(match u {
            "pt" => 1.0,
            "mm" => 7227.0 / 2540.0,
            "cm" => 7227.0 / 254.0,
            "in" => 72.27,
            "bp" => 803.0 / 800.0,
            "pc" => 12.0,
            "dd" => 1238.0 / 1157.0,
            "cc" => 14856.0 / 1157.0,
            "nd" => 685.0 / 642.0,
            "nc" => 1370.0 / 107.0,
            "sp" => 1.0 / 65536.0,
            "px" => 803.0 / 800.0,
            _ => return None,
        })
    };
    let scale = if let Some(p) = pt_per_unit(&m.unit) {
        p / options.font_metrics().pt_per_em / options.size_multiplier
    } else if m.unit == "mu" {
        options.font_metrics().css_em_per_mu
    } else {
        let unit_options = if options.style.is_tight() { options.having_style(options.style.text()) } else { options.clone() };
        let mut s = if m.unit == "ex" { unit_options.font_metrics().x_height } else { unit_options.font_metrics().quad };
        if unit_options != *options {
            s *= unit_options.size_multiplier / options.size_multiplier;
        }
        s
    };
    (m.number * scale).min(options.max_size)
}

/// `makeGlue`.
pub fn make_glue(m: &Measurement, options: &Options) -> Dom {
    let mut rule = make_span(vec!["mspace".into()], vec![], Some(options));
    rule.css.margin_right = Some(em4(calculate_size(m, options)));
    Dom::Span(rule)
}

/// `staticSvg` (`\vec`, the oval of `\oiint`).
pub fn static_svg(name: &str, options: &Options) -> Dom {
    let (w, h) = match name {
        "vec" => (0.471, 0.714),
        "oiintSize1" => (0.957, 0.499),
        "oiintSize2" => (1.472, 0.659),
        "oiiintSize1" => (1.304, 0.499),
        "oiiintSize2" => (1.98, 0.659),
        _ => (0.0, 0.0),
    };
    let path = data::svg_path(name).unwrap_or("").to_string();
    let mut classes = vec!["overlay".to_string()];
    let mut css = init_css(&mut classes, Some(options));
    css.width = Some(em4(w));
    css.height = Some(em4(h));
    Dom::Svg(Svg { classes, paths: vec![path], view_box: (1000.0 * w, 1000.0 * h), svg_width: Some(w), svg_height: h, fit: SvgFit::XMinYMin, height: h, depth: 0.0, max_font_size: 0.0, css })
}
