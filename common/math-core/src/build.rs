//! KaTeX's `buildHTML.ts` and the HTML builders of its functions: parse nodes to the HTML tree, with the
//! inter-atom spacing, the binary-operator cancelling and every box algorithm (fractions, scripts, radicals,
//! big operators, delimiters, accents, arrays).

use crate::data::{self, Group, Mode};
use crate::delim;
use crate::options::{Options, Style};
use crate::parse::{is_character_box, AlignSpec, ArrayNode, StyleStr, P};
use crate::tree::{self, calculate_size, em4, make_fragment, make_glue, make_line_span, make_ord, make_span, make_symbol, make_vlist, mathsym, span_dom, static_svg, try_combine_chars, wrap_fragment, Css, Dom, Span, VItem, VPos};

fn style_of(s: StyleStr) -> Style {
    match s {
        StyleStr::Display => Style::DISPLAY,
        StyleStr::Text => Style::TEXT,
        StyleStr::Script => Style::SCRIPT,
        StyleStr::ScriptScript => Style::SCRIPTSCRIPT,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RealGroup {
    No,
    Yes,
    Root,
}

const DOM_TYPES: &[&str] = &["mord", "mop", "mbin", "mrel", "mopen", "mclose", "mpunct", "minner"];

fn dom_type(node: &Dom) -> Option<&'static str> {
    let c = node.first_class()?;
    DOM_TYPES.iter().copied().find(|t| *t == c)
}

/// The children a spacing traversal descends into (`checkPartialGroup`).
fn partial_children(node: &mut Dom) -> Option<&mut Vec<Dom>> {
    match node {
        Dom::Frag(s) => Some(&mut s.children),
        Dom::Span(s) if s.classes.iter().any(|c| c == "enclosing") => Some(&mut s.children),
        _ => None,
    }
}

fn is_partial(node: &Dom) -> bool {
    matches!(node, Dom::Frag(_)) || matches!(node, Dom::Span(s) if s.classes.iter().any(|c| c == "enclosing"))
}

/// The leaves of the traversal (paths into nested partial groups), in order.
fn collect_leaves(nodes: &[Dom], prefix: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
    for (i, n) in nodes.iter().enumerate() {
        prefix.push(i);
        if is_partial(n) {
            let children = match n {
                Dom::Frag(s) | Dom::Span(s) => &s.children,
                _ => unreachable_children(),
            };
            collect_leaves(children, prefix, out);
        } else if !n.has_class("mspace") {
            out.push(prefix.clone());
        }
        prefix.pop();
    }
}

fn unreachable_children() -> &'static Vec<Dom> {
    static EMPTY: Vec<Dom> = Vec::new();
    &EMPTY
}

fn node_at<'a>(nodes: &'a mut [Dom], path: &[usize]) -> Option<&'a mut Dom> {
    let (first, rest) = path.split_first()?;
    let n = nodes.get_mut(*first)?;
    if rest.is_empty() {
        return Some(n);
    }
    let children = partial_children(n)?;
    node_at(children, rest)
}

fn vec_at<'a>(nodes: &'a mut Vec<Dom>, parent: &[usize]) -> Option<&'a mut Vec<Dom>> {
    let Some((first, rest)) = parent.split_first() else { return Some(nodes) };
    let n = nodes.get_mut(*first)?;
    let children = partial_children(n)?;
    vec_at(children, rest)
}

/// `buildExpression`.
pub fn build_expression(expression: &[P], options: &Options, real: RealGroup, surrounding: (Option<&str>, Option<&str>)) -> Vec<Dom> {
    let mut groups: Vec<Dom> = Vec::new();
    for p in expression {
        let output = build_group(p, options, None);
        match output {
            Dom::Frag(s) => groups.extend(s.children),
            other => groups.push(other),
        }
    }
    try_combine_chars(&mut groups);
    if real == RealGroup::No {
        return groups;
    }
    let mut glue_options = options.clone();
    if expression.len() == 1 {
        match &expression[0] {
            P::Sizing { size, .. } => glue_options = options.having_size(*size),
            P::Styling { style, .. } => glue_options = options.having_style(style_of(*style)),
            _ => {}
        }
    }
    let left = surrounding.0.unwrap_or("leftmost").to_string();
    let right = surrounding.1.unwrap_or("rightmost").to_string();

    // Pass 1: binary operators next to an operator or a boundary become ordinary.
    let mut leaves = Vec::new();
    collect_leaves(&groups, &mut Vec::new(), &mut leaves);
    let mut prev_class: String = left.clone();
    let mut prev_path: Option<Vec<usize>> = None;
    let class_at = |groups: &mut Vec<Dom>, p: &[usize]| node_at(groups, p).and_then(|n| n.first_class().map(str::to_string)).unwrap_or_default();
    for path in leaves.iter().map(Some).chain(std::iter::once(None)) {
        let ty = match path {
            Some(p) => class_at(&mut groups, p),
            None => right.clone(),
        };
        if prev_class == "mbin" && ["rightmost", "mrel", "mclose", "mpunct"].contains(&ty.as_str()) {
            if let Some(pp) = &prev_path {
                if let Some(n) = node_at(&mut groups, pp) {
                    if let Some(c) = n.classes_mut().first_mut() {
                        *c = "mord".into();
                    }
                }
            }
        } else if ty == "mbin" && ["leftmost", "mbin", "mopen", "mrel", "mop", "mpunct"].contains(&prev_class.as_str()) {
            if let Some(p) = path {
                if let Some(n) = node_at(&mut groups, p) {
                    if let Some(c) = n.classes_mut().first_mut() {
                        *c = "mord".into();
                    }
                }
            }
        }
        // `prev` is re-read after a possible change.
        if let Some(p) = path {
            prev_class = class_at(&mut groups, p);
            prev_path = Some(p.clone());
        }
    }

    // Pass 2: spacing glue between atom classes.
    let mut leaves = Vec::new();
    collect_leaves(&groups, &mut Vec::new(), &mut leaves);
    // (insert after path | at front of the first leaf's vector, glue)
    let mut inserts: Vec<(Option<Vec<usize>>, Dom)> = Vec::new();
    let mut prev_type: Option<String> = Some(left.clone()).filter(|c| DOM_TYPES.contains(&c.as_str()));
    let mut prev_path: Option<Vec<usize>> = None;
    for path in leaves.iter().map(Some).chain(std::iter::once(None)) {
        let (ty, tight) = match path {
            Some(p) => match node_at(&mut groups, p) {
                Some(n) => (dom_type(n).map(str::to_string), n.has_class("mtight")),
                None => (None, false),
            },
            None => (Some(right.clone()).filter(|c| DOM_TYPES.contains(&c.as_str())), options.style.is_tight()),
        };
        if let (Some(pt), Some(t)) = (&prev_type, &ty) {
            if let Some(space) = data::spacing(pt, t, tight) {
                let glue = make_glue(space, &glue_options);
                inserts.push((prev_path.clone(), glue));
            }
        }
        if let Some(p) = path {
            prev_type = ty;
            prev_path = Some(p.clone());
        }
    }
    let first_leaf = leaves.first().cloned();
    for (after, glue) in inserts.into_iter().rev() {
        match after {
            Some(p) => {
                let (last, parent) = p.split_last().map(|(l, r)| (*l, r.to_vec())).unwrap_or((0, vec![]));
                if let Some(v) = vec_at(&mut groups, &parent) {
                    v.insert(last + 1, glue);
                }
            }
            None => {
                let parent = first_leaf.as_ref().map(|p| p[..p.len() - 1].to_vec()).unwrap_or_default();
                if let Some(v) = vec_at(&mut groups, &parent) {
                    v.insert(0, glue);
                }
            }
        }
    }
    groups
}

fn sizing_wrap(node: Dom, options: &Options, base: &Options) -> Dom {
    let mut span = make_span(vec!["sizing".into()], vec![node], Some(options));
    span.css.font_scale = options.sizing_scale(base);
    let m = options.size_multiplier / base.size_multiplier;
    span.height *= m;
    span.depth *= m;
    Dom::Span(span)
}

/// `buildGroup`.
pub fn build_group(group: &P, options: &Options, base: Option<&Options>) -> Dom {
    let node = build_node(group, options);
    match base {
        Some(b) if options.size != b.size => sizing_wrap(node, options, b),
        _ => node,
    }
}

fn empty_span() -> Dom {
    Dom::Span(make_span(vec![], vec![], None))
}

/// `makeNullDelimiter`.
fn make_null_delimiter(options: &Options, classes: &[&str]) -> Dom {
    let mut c: Vec<String> = classes.iter().map(|s| s.to_string()).collect();
    c.push("nulldelimiter".into());
    let mut s = make_span(c, vec![], None);
    if options.size != crate::options::BASESIZE {
        s.css.font_scale = Some(crate::options::SIZE_MULTIPLIERS[(crate::options::BASESIZE - 1) as usize] / options.size_multiplier);
    }
    s.css.width = Some(0.12);
    Dom::Span(s)
}

/// `sizingGroup`.
fn sizing_group(body: &[P], options: &Options, base: &Options) -> Dom {
    let mut inner = build_expression(body, options, RealGroup::No, (None, None));
    let m = options.size_multiplier / base.size_multiplier;
    for n in &mut inner {
        if !n.has_class("sizing") {
            n.classes_mut().push("sizing".into());
            if let Some(s) = options.sizing_scale(base) {
                let css = n.css_mut();
                css.font_scale = Some(css.font_scale.unwrap_or(1.0) * s);
            }
        }
        let (h, d) = (n.height() * m, n.depth() * m);
        n.set_height(h);
        n.set_depth(d);
    }
    make_fragment(inner)
}

fn build_node(group: &P, options: &Options) -> Dom {
    match group {
        P::MathOrd { mode, text } => make_ord(text, *mode, options, true),
        P::TextOrd { mode, text } | P::AccentToken { mode, text } | P::OpToken { mode, text } => make_ord(text, *mode, options, false),
        P::Atom { mode, family, text } => Dom::Sym(mathsym(text, *mode, options, vec![format!("m{}", family.family())])),
        P::Spacing { mode, text } => build_spacing(text, *mode, options),
        P::OrdGroup { body, .. } => Dom::Span(make_span(vec!["mord".into()], build_expression(body, options, RealGroup::Yes, (None, None)), Some(options))),
        P::SupSub { .. } => build_supsub(group, options),
        P::GenFrac { .. } => build_genfrac(group, options),
        P::Styling { style, body, reset_font, .. } => {
            let mut new = options.having_style(style_of(*style));
            if *reset_font {
                new = new.with_font("");
            }
            sizing_group(body, &new, options)
        }
        P::Sizing { size, body, .. } => {
            let new = options.having_size(*size);
            sizing_group(body, &new, options)
        }
        P::Sqrt { body, index, .. } => build_sqrt(body, index.as_deref(), options),
        P::Op { .. } => build_op(group, None, None, options),
        P::OperatorName { .. } => build_operatorname(group, None, None, options),
        P::LeftRight { body, left, right, right_color, mode } => build_leftright(body, left, right, right_color.as_deref(), *mode, options),
        P::Middle { delim, mode } => {
            if delim == "." {
                make_null_delimiter(options, &[])
            } else {
                let mut d = delim::make_sized_delim(delim, 1, options, *mode, vec![]);
                d.classes_mut().push("@middle".into());
                d.classes_mut().push(format!("@delim:{delim}"));
                d
            }
        }
        P::DelimSizing { size, mclass, delim, mode } => {
            if delim == "." {
                span_dom(&[mclass], vec![], None)
            } else {
                delim::make_sized_delim(delim, *size, options, *mode, vec![mclass.to_string()])
            }
        }
        P::Accent { .. } => build_accent(group, None, options),
        P::Text { body, font, .. } => {
            let new = text_options(font.as_deref(), options);
            Dom::Span(make_span(vec!["mord".into(), "text".into()], build_expression(body, &new, RealGroup::Yes, (None, None)), Some(&new)))
        }
        P::Font { font, body, .. } => build_group(body, &options.with_font(font), None),
        P::Color { color, body, .. } => make_fragment(build_expression(body, &options.with_color(color), RealGroup::No, (None, None))),
        P::Kern { dim, .. } => make_glue(dim, options),
        P::Overline { body, .. } => {
            let inner = build_group(body, &options.having_cramped_style(), None);
            let line = make_line_span("overline-line", options, None);
            let t = options.font_metrics().default_rule_thickness;
            let v = make_vlist(VPos::FirstBaseline, vec![VItem::elem(inner), VItem::Kern(3.0 * t), VItem::elem(line), VItem::Kern(t)]);
            Dom::Span(make_span(vec!["mord".into(), "overline".into()], vec![v], Some(options)))
        }
        P::Underline { body, .. } => {
            let inner = build_group(body, options, None);
            let line = make_line_span("underline-line", options, None);
            let t = options.font_metrics().default_rule_thickness;
            let ih = inner.height();
            let v = make_vlist(VPos::Top(ih), vec![VItem::Kern(t), VItem::elem(line), VItem::Kern(3.0 * t), VItem::elem(inner)]);
            Dom::Span(make_span(vec!["mord".into(), "underline".into()], vec![v], Some(options)))
        }
        P::MClass { mclass, body, .. } => Dom::Span(make_span(vec![mclass.clone()], build_expression(body, options, RealGroup::Yes, (None, None)), Some(options))),
        P::Lap { alignment, body, .. } => build_lap(alignment, body, options),
        P::Phantom { body, .. } => make_fragment(build_expression(body, &options.with_phantom(), RealGroup::No, (None, None))),
        P::Smash { body, height, depth, .. } => {
            let mut node = make_span(vec![], vec![build_group(body, options, None)], None);
            if !*height && !*depth {
                return Dom::Span(node);
            }
            if *height {
                node.height = 0.0;
            }
            if *depth {
                node.depth = 0.0;
            }
            if *height && *depth {
                return Dom::Span(make_span(vec!["mord".into(), "smash".into()], vec![Dom::Span(node)], Some(options)));
            }
            for c in &mut node.children {
                if *height {
                    c.set_height(0.0);
                }
                if *depth {
                    c.set_depth(0.0);
                }
            }
            let v = make_vlist(VPos::FirstBaseline, vec![VItem::elem(Dom::Span(node))]);
            Dom::Span(make_span(vec!["mord".into()], vec![v], Some(options)))
        }
        P::HorizBrace { .. } => build_horiz_brace(group, None, options),
        P::VPhantom { body, .. } => {
            let inner = make_span(vec!["inner".into()], vec![build_group(body, &options.with_phantom(), None)], None);
            let fix = make_span(vec!["fix".into()], vec![], None);
            Dom::Span(make_span(vec!["mord".into(), "rlap".into()], vec![Dom::Span(inner), Dom::Span(fix)], Some(options)))
        }
        P::Rule { shift, width, height, .. } => {
            let mut rule = make_span(vec!["mord".into(), "rule".into()], vec![], Some(options));
            let w = calculate_size(width, options);
            let h = calculate_size(height, options);
            let s = shift.as_ref().map(|m| calculate_size(m, options)).unwrap_or(0.0);
            rule.css.border_right = Some(em4(w));
            rule.css.border_bottom = Some(em4(h));
            rule.css.vertical_align = Some(em4(s));
            rule.height = h + s;
            rule.depth = -s;
            rule.max_font_size = h * 1.125 * options.size_multiplier;
            Dom::Span(rule)
        }
        P::Enclose { label, body, background, .. } => build_enclose(label, body, background.as_deref(), options),
        P::Array(a) => build_array(a, options),
        P::HtmlMathml { body, .. } => make_fragment(build_expression(body, options, RealGroup::No, (None, None))),
        P::Cr => {
            let mut s = make_span(vec!["mspace".into()], vec![], Some(options));
            s.classes.push("newline".into());
            Dom::Span(s)
        }
        P::Infix { .. } | P::LeftRightRight { .. } | P::ColorToken { .. } | P::Size { .. } | P::Raw { .. } | P::Environment { .. } | P::Internal => empty_span(),
    }
}

fn build_spacing(text: &str, mode: Mode, options: &Options) -> Dom {
    let class = match text {
        "~" | "\\nobreakspace" => "nobreak",
        _ => "",
    };
    match text {
        " " | "\\ " | "~" | "\\space" | "\\nobreakspace" => {
            if mode == Mode::Text {
                let mut ord = make_ord(text, mode, options, false);
                ord.classes_mut().push(class.into());
                ord
            } else {
                Dom::Span(make_span(vec!["mspace".into(), class.into()], vec![Dom::Sym(mathsym(text, mode, options, vec![]))], Some(options)))
            }
        }
        "\\nobreak" | "\\allowbreak" => Dom::Span(make_span(vec!["mspace".into(), text[1..].to_string()], vec![], Some(options))),
        _ => empty_span(),
    }
}

fn text_options(font: Option<&str>, options: &Options) -> Options {
    let Some(font) = font else { return options.clone() };
    match font {
        "\\text" => options.clone(),
        "\\textrm" | "\\textnormal" => options.with_text_font_family("textrm"),
        "\\textsf" => options.with_text_font_family("textsf"),
        "\\texttt" => options.with_text_font_family("texttt"),
        "\\textbf" => options.with_text_font_weight("textbf"),
        "\\textmd" => options.with_text_font_weight("textmd"),
        "\\emph" => {
            if options.font_shape == "textit" {
                options.with_text_font_shape("textup")
            } else {
                options.with_text_font_shape("textit")
            }
        }
        "\\textit" => options.with_text_font_shape("textit"),
        "\\textup" => options.with_text_font_shape("textup"),
        _ => options.clone(),
    }
}

// ── supsub ───────────────────────────────────────────────────────────────────

fn build_supsub(group: &P, options: &Options) -> Dom {
    let P::SupSub { base, sup, sub, .. } = group else { return empty_span() };
    // Delegates.
    match base.as_deref() {
        Some(P::Op { limits, always_handle_supsub, .. }) if *limits && (options.style.size() == Style::DISPLAY.size() || *always_handle_supsub) => {
            return build_op(base.as_deref().unwrap_or(group), sup.as_deref(), sub.as_deref(), options);
        }
        Some(P::OperatorName { always_handle_supsub, limits, .. }) if *always_handle_supsub && (options.style.size() == Style::DISPLAY.size() || *limits) => {
            return build_operatorname(base.as_deref().unwrap_or(group), sup.as_deref(), sub.as_deref(), options);
        }
        Some(P::HorizBrace { is_over, .. }) if sub.is_none() == *is_over => {
            return build_horiz_brace(base.as_deref().unwrap_or(group), Some((sup.as_deref(), sub.as_deref())), options);
        }
        Some(P::Accent { base: ab, .. }) if is_character_box(ab) => {
            return build_accent(base.as_deref().unwrap_or(group), Some((sup.as_deref(), sub.as_deref())), options);
        }
        _ => {}
    }
    build_supsub_plain(base.as_deref(), sup.as_deref(), sub.as_deref(), options)
}

fn build_supsub_plain(value_base: Option<&P>, value_sup: Option<&P>, value_sub: Option<&P>, options: &Options) -> Dom {
    let base = match value_base {
        Some(b) => build_group(b, options, None),
        None => Dom::Span(make_span(vec![], vec![], None)),
    };
    let metrics = options.font_metrics();
    let mut sup_shift = 0.0;
    let mut sub_shift = 0.0;
    let is_char_box = value_base.is_some_and(is_character_box);
    let mut supm = None;
    let mut subm = None;
    if let Some(s) = value_sup {
        let new = options.having_style(options.style.sup());
        let m = build_group(s, &new, Some(options));
        if !is_char_box {
            sup_shift = base.height() - new.font_metrics().sup_drop * new.size_multiplier / options.size_multiplier;
        }
        supm = Some(m);
    }
    if let Some(s) = value_sub {
        let new = options.having_style(options.style.sub());
        let m = build_group(s, &new, Some(options));
        if !is_char_box {
            sub_shift = base.depth() + new.font_metrics().sub_drop * new.size_multiplier / options.size_multiplier;
        }
        subm = Some(m);
    }
    let min_sup_shift = if options.style == Style::DISPLAY {
        metrics.sup1
    } else if options.style.cramped() {
        metrics.sup3
    } else {
        metrics.sup2
    };
    let multiplier = options.size_multiplier;
    let margin_right = em4((0.5 / metrics.pt_per_em) / multiplier);
    let mut margin_left = None;
    if subm.is_some() {
        let is_oiint = matches!(value_base, Some(P::Op { name: Some(n), .. }) if n == "\\oiint" || n == "\\oiiint");
        if matches!(base, Dom::Sym(_)) || is_oiint {
            margin_left = Some(em4(-base.italic()));
        }
    }
    let supsub = match (supm, subm) {
        (Some(supm), Some(subm)) => {
            sup_shift = sup_shift.max(min_sup_shift).max(supm.depth() + 0.25 * metrics.x_height);
            sub_shift = sub_shift.max(metrics.sub2);
            let rule = metrics.default_rule_thickness;
            let max_width = 4.0 * rule;
            if (sup_shift - supm.depth()) - (subm.height() - sub_shift) < max_width {
                sub_shift = max_width - (sup_shift - supm.depth()) + subm.height();
                let psi = 0.8 * metrics.x_height - (sup_shift - supm.depth());
                if psi > 0.0 {
                    sup_shift += psi;
                    sub_shift -= psi;
                }
            }
            make_vlist(
                VPos::IndividualShift,
                vec![
                    VItem::Elem { elem: subm, shift: sub_shift, margin_left, margin_right: Some(margin_right), wrapper_classes: vec![], wrapper_css: Css::default() },
                    VItem::Elem { elem: supm, shift: -sup_shift, margin_left: None, margin_right: Some(margin_right), wrapper_classes: vec![], wrapper_css: Css::default() },
                ],
            )
        }
        (None, Some(subm)) => {
            sub_shift = sub_shift.max(metrics.sub1).max(subm.height() - 0.8 * metrics.x_height);
            make_vlist(VPos::Shift(sub_shift), vec![VItem::Elem { elem: subm, shift: 0.0, margin_left, margin_right: Some(margin_right), wrapper_classes: vec![], wrapper_css: Css::default() }])
        }
        (Some(supm), None) => {
            sup_shift = sup_shift.max(min_sup_shift).max(supm.depth() + 0.25 * metrics.x_height);
            make_vlist(VPos::Shift(-sup_shift), vec![VItem::Elem { elem: supm, shift: 0.0, margin_left: None, margin_right: Some(margin_right), wrapper_classes: vec![], wrapper_css: Css::default() }])
        }
        (None, None) => return base,
    };
    let mclass = outermost_type(&base, true).unwrap_or("mord").to_string();
    let msupsub = Dom::Span(make_span(vec!["msupsub".into()], vec![supsub], None));
    Dom::Span(make_span(vec![mclass], vec![base, msupsub], Some(options)))
}

/// `getTypeOfDomTree(node, side)`.
fn outermost_type(node: &Dom, right: bool) -> Option<&'static str> {
    let mut n = node;
    loop {
        match n {
            Dom::Frag(s) => {
                let next = if right { s.children.last() } else { s.children.first() };
                match next {
                    Some(c) => n = c,
                    None => break,
                }
            }
            Dom::Span(s) if s.classes.iter().any(|c| c == "enclosing") => {
                let next = if right { s.children.last() } else { s.children.first() };
                match next {
                    Some(c) => n = c,
                    None => break,
                }
            }
            _ => break,
        }
    }
    dom_type(n)
}

// ── genfrac ──────────────────────────────────────────────────────────────────

fn build_genfrac(group: &P, options: &Options) -> Dom {
    let P::GenFrac { numer, denom, has_bar, left, right, continued, bar_size, mode } = group else { return empty_span() };
    let style = options.style;
    let nstyle = style.frac_num();
    let dstyle = style.frac_den();
    let new = options.having_style(nstyle);
    let mut numerm = build_group(numer, &new, Some(options));
    if *continued {
        let h = 8.5 / options.font_metrics().pt_per_em;
        let d = 3.5 / options.font_metrics().pt_per_em;
        if numerm.height() < h {
            numerm.set_height(h);
        }
        if numerm.depth() < d {
            numerm.set_depth(d);
        }
    }
    let new = options.having_style(dstyle);
    let denomm = build_group(denom, &new, Some(options));
    let (rule, rule_width, rule_spacing) = if *has_bar {
        let r = match bar_size {
            Some(b) => make_line_span("frac-line", options, Some(calculate_size(b, options))),
            None => make_line_span("frac-line", options, None),
        };
        let h = r.height();
        (Some(r), h, h)
    } else {
        (None, 0.0, options.font_metrics().default_rule_thickness)
    };
    let fm = options.font_metrics();
    let (mut num_shift, clearance, mut denom_shift);
    if style.size() == Style::DISPLAY.size() {
        num_shift = fm.num1;
        clearance = if rule_width > 0.0 { 3.0 * rule_spacing } else { 7.0 * rule_spacing };
        denom_shift = fm.denom1;
    } else {
        if rule_width > 0.0 {
            num_shift = fm.num2;
            clearance = rule_spacing;
        } else {
            num_shift = fm.num3;
            clearance = 3.0 * rule_spacing;
        }
        denom_shift = fm.denom2;
    }
    let mut frac = match rule {
        None => {
            let cand = (num_shift - numerm.depth()) - (denomm.height() - denom_shift);
            if cand < clearance {
                num_shift += 0.5 * (clearance - cand);
                denom_shift += 0.5 * (clearance - cand);
            }
            make_vlist(VPos::IndividualShift, vec![VItem::shifted(denomm, denom_shift), VItem::shifted(numerm, -num_shift)])
        }
        Some(rule) => {
            let axis = fm.axis_height;
            if (num_shift - numerm.depth()) - (axis + 0.5 * rule_width) < clearance {
                num_shift += clearance - ((num_shift - numerm.depth()) - (axis + 0.5 * rule_width));
            }
            if (axis - 0.5 * rule_width) - (denomm.height() - denom_shift) < clearance {
                denom_shift += clearance - ((axis - 0.5 * rule_width) - (denomm.height() - denom_shift));
            }
            let mid = -(axis - 0.5 * rule_width);
            make_vlist(VPos::IndividualShift, vec![VItem::shifted(denomm, denom_shift), VItem::shifted(rule, mid), VItem::shifted(numerm, -num_shift)])
        }
    };
    let new = options.having_style(style);
    let r = new.size_multiplier / options.size_multiplier;
    let (h, d) = (frac.height() * r, frac.depth() * r);
    frac.set_height(h);
    frac.set_depth(d);
    let delim_size = if style.size() == Style::DISPLAY.size() {
        fm.delim1
    } else if style.size() == Style::SCRIPTSCRIPT.size() {
        options.having_style(Style::SCRIPT).font_metrics().delim2
    } else {
        fm.delim2
    };
    let left_delim = match left {
        None => make_null_delimiter(options, &["mopen"]),
        Some(l) => delim::make_custom_sized_delim(l, delim_size, true, &options.having_style(style), *mode, vec!["mopen".into()]),
    };
    let right_delim = if *continued {
        empty_span()
    } else {
        match right {
            None => make_null_delimiter(options, &["mclose"]),
            Some(r) => delim::make_custom_sized_delim(r, delim_size, true, &options.having_style(style), *mode, vec!["mclose".into()]),
        }
    };
    let mfrac = Dom::Span(make_span(vec!["mfrac".into()], vec![frac], None));
    let mut out = make_span(vec!["mord".into()], vec![left_delim, mfrac, right_delim], Some(options));
    out.css.font_scale = new.sizing_scale(options);
    Dom::Span(out)
}

// ── sqrt ─────────────────────────────────────────────────────────────────────

fn build_sqrt(body: &P, index: Option<&P>, options: &Options) -> Dom {
    let mut inner = build_group(body, &options.having_cramped_style(), None);
    if inner.height() == 0.0 {
        inner.set_height(options.font_metrics().x_height);
    }
    let mut inner = wrap_fragment(inner, options);
    let metrics = options.font_metrics();
    let theta = metrics.default_rule_thickness;
    let mut phi = theta;
    if options.style.id < Style::TEXT.id {
        phi = metrics.x_height;
    }
    let mut line_clearance = theta + phi / 4.0;
    let min_delim_height = inner.height() + inner.depth() + line_clearance + theta;
    let (img, rule_width, advance_width) = delim::make_sqrt_image(min_delim_height, options);
    let delim_depth = img.height() - rule_width;
    if delim_depth > inner.height() + inner.depth() + line_clearance {
        line_clearance = (line_clearance + delim_depth - inner.height() - inner.depth()) / 2.0;
    }
    let img_shift = img.height() - inner.height() - line_clearance - rule_width;
    inner.css_mut().padding_left = Some(em4(advance_width));
    let ih = inner.height();
    let body = make_vlist(
        VPos::FirstBaseline,
        vec![VItem::Elem { elem: inner, shift: 0.0, margin_left: None, margin_right: None, wrapper_classes: vec!["svg-align".into()], wrapper_css: Css::default() }, VItem::Kern(-(ih + img_shift)), VItem::elem(img), VItem::Kern(rule_width)],
    );
    match index {
        None => Dom::Span(make_span(vec!["mord".into(), "sqrt".into()], vec![body], Some(options))),
        Some(idx) => {
            let new = options.having_style(Style::SCRIPTSCRIPT);
            let rootm = build_group(idx, &new, Some(options));
            let to_shift = 0.6 * (body.height() - body.depth());
            let root_vlist = make_vlist(VPos::Shift(-to_shift), vec![VItem::elem(rootm)]);
            let mut wrap = make_span(vec!["root".into()], vec![root_vlist], None);
            wrap.css.margin_left = Some(0.2777777778);
            wrap.css.margin_right = Some(-0.5555555556);
            Dom::Span(make_span(vec!["mord".into(), "sqrt".into()], vec![Dom::Span(wrap), body], Some(options)))
        }
    }
}

// ── op, operatorname ─────────────────────────────────────────────────────────

fn assemble_sup_sub(base: Dom, sup_group: Option<&P>, sub_group: Option<&P>, options: &Options, style: Style, slant: f64, base_shift: f64) -> Dom {
    let base = Dom::Span(make_span(vec![], vec![base], None));
    let sub_single = sub_group.is_some_and(is_character_box);
    let fm = options.font_metrics();
    let sup = sup_group.map(|g| {
        let elem = build_group(g, &options.having_style(style.sup()), Some(options));
        let kern = fm.big_op_spacing1.max(fm.big_op_spacing3 - elem.depth());
        (elem, kern)
    });
    let sub = sub_group.map(|g| {
        let elem = build_group(g, &options.having_style(style.sub()), Some(options));
        let kern = fm.big_op_spacing2.max(fm.big_op_spacing4 - elem.height());
        (elem, kern)
    });
    let ml = |v: f64| VItem::Elem { elem: Dom::Span(Span::default()), shift: 0.0, margin_left: Some(em4(v)), margin_right: None, wrapper_classes: vec![], wrapper_css: Css::default() };
    let _ = ml;
    let with_margin = |elem: Dom, m: f64| VItem::Elem { elem, shift: 0.0, margin_left: Some(em4(m)), margin_right: None, wrapper_classes: vec![], wrapper_css: Css::default() };
    let has_sub = sub.is_some();
    let final_group = match (sup, sub) {
        (Some((supe, supk)), Some((sube, subk))) => {
            let bottom = fm.big_op_spacing5 + sube.height() + sube.depth() + subk + base.depth() + base_shift;
            make_vlist(VPos::Bottom(bottom), vec![VItem::Kern(fm.big_op_spacing5), with_margin(sube, -slant), VItem::Kern(subk), VItem::elem(base), VItem::Kern(supk), with_margin(supe, slant), VItem::Kern(fm.big_op_spacing5)])
        }
        (None, Some((sube, subk))) => {
            let top = base.height() - base_shift;
            make_vlist(VPos::Top(top), vec![VItem::Kern(fm.big_op_spacing5), with_margin(sube, -slant), VItem::Kern(subk), VItem::elem(base)])
        }
        (Some((supe, supk)), None) => {
            let bottom = base.depth() + base_shift;
            make_vlist(VPos::Bottom(bottom), vec![VItem::elem(base), VItem::Kern(supk), with_margin(supe, slant), VItem::Kern(fm.big_op_spacing5)])
        }
        (None, None) => return base,
    };
    let mut parts = vec![final_group];
    if has_sub && slant != 0.0 && !sub_single {
        let mut spacer = make_span(vec!["mspace".into()], vec![], Some(options));
        spacer.css.margin_right = Some(em4(slant));
        parts.insert(0, Dom::Span(spacer));
    }
    Dom::Span(make_span(vec!["mop".into(), "op-limits".into()], parts, Some(options)))
}

fn build_op(group: &P, sup: Option<&P>, sub: Option<&P>, options: &Options) -> Dom {
    let P::Op { symbol, name, body, suppress_base_shift, mode, .. } = group else { return empty_span() };
    let has_limits = sup.is_some() || sub.is_some();
    let style = options.style;
    let large = style.size() == Style::DISPLAY.size() && *symbol && name.as_deref() != Some("\\smallint");
    let mut symbol_italic = 0.0;
    let mut is_symbol_node = false;
    let mut base: Dom = if *symbol {
        let font = if large { "Size2-Regular" } else { "Size1-Regular" };
        let mut n = name.clone().unwrap_or_default();
        let mut stash = String::new();
        if n == "\\oiint" || n == "\\oiiint" {
            stash = n[1..].to_string();
            n = if stash == "oiint" { "\\iint".into() } else { "\\iiint".into() };
        }
        let s = make_symbol(&n, font, Mode::Math, Some(options), vec!["mop".into(), "op-symbol".into(), if large { "large-op".into() } else { "small-op".into() }]);
        symbol_italic = s.italic;
        if !stash.is_empty() {
            let oval = static_svg(&format!("{stash}Size{}", if large { "2" } else { "1" }), options);
            let mut v = make_vlist(VPos::IndividualShift, vec![VItem::shifted(Dom::Sym(s), 0.0), VItem::shifted(oval, if large { 0.08 } else { 0.0 })]);
            v.classes_mut().insert(0, "mop".into());
            v.set_italic(symbol_italic);
            is_symbol_node = true;
            v
        } else {
            is_symbol_node = true;
            Dom::Sym(s)
        }
    } else if let Some(b) = body {
        let inner = build_expression(b, options, RealGroup::Yes, (None, None));
        if inner.len() == 1 && matches!(inner[0], Dom::Sym(_)) {
            let mut i = inner;
            let mut s = i.remove(0);
            if let Some(c) = s.classes_mut().first_mut() {
                *c = "mop".into();
            }
            is_symbol_node = true;
            s
        } else {
            Dom::Span(make_span(vec!["mop".into()], inner, Some(options)))
        }
    } else {
        let n = name.clone().unwrap_or_default();
        let output: Vec<Dom> = n.chars().skip(1).map(|c| Dom::Sym(mathsym(&c.to_string(), *mode, options, vec![]))).collect();
        Dom::Span(make_span(vec!["mop".into()], output, Some(options)))
    };
    let mut base_shift = 0.0;
    let mut slant = 0.0;
    if is_symbol_node && !*suppress_base_shift {
        base_shift = (base.height() - base.depth()) / 2.0 - options.font_metrics().axis_height;
        slant = base.italic();
    }
    let _ = symbol_italic;
    if has_limits {
        assemble_sup_sub(base, sup, sub, options, style, slant, base_shift)
    } else {
        if base_shift != 0.0 {
            base.css_mut().top = Some(em4(base_shift));
        }
        base
    }
}

fn build_operatorname(group: &P, sup: Option<&P>, sub: Option<&P>, options: &Options) -> Dom {
    let P::OperatorName { body, .. } = group else { return empty_span() };
    let has_limits = sup.is_some() || sub.is_some();
    let base = if !body.is_empty() {
        let body: Vec<P> = body
            .iter()
            .map(|c| match c.symbol_text() {
                Some(t) => P::TextOrd { mode: c.mode(), text: t.to_string() },
                None => c.clone(),
            })
            .collect();
        let mut expression = build_expression(&body, &options.with_font("mathrm"), RealGroup::Yes, (None, None));
        for c in &mut expression {
            if let Dom::Sym(s) = c {
                s.text = s.text.replacen('\u{2212}', "-", 1).replacen('\u{2217}', "*", 1);
            }
        }
        Dom::Span(make_span(vec!["mop".into()], expression, Some(options)))
    } else {
        Dom::Span(make_span(vec!["mop".into()], vec![], Some(options)))
    };
    if has_limits {
        assemble_sup_sub(base, sup, sub, options, options.style, 0.0, 0.0)
    } else {
        base
    }
}

// ── left/right ───────────────────────────────────────────────────────────────

fn build_leftright(body: &[P], left: &str, right: &str, right_color: Option<&str>, mode: Mode, options: &Options) -> Dom {
    let mut inner = build_expression(body, options, RealGroup::Yes, (Some("mopen"), Some("mclose")));
    let mut ih = 0.0f64;
    let mut idp = 0.0f64;
    let mut had_middle = false;
    for n in &inner {
        if n.has_class("@middle") {
            had_middle = true;
        } else {
            ih = ih.max(n.height());
            idp = idp.max(n.depth());
        }
    }
    ih *= options.size_multiplier;
    idp *= options.size_multiplier;
    let left_delim = if left == "." { make_null_delimiter(options, &["mopen"]) } else { delim::make_left_right_delim(left, ih, idp, options, mode, vec!["mopen".into()]) };
    inner.insert(0, left_delim);
    if had_middle {
        // The middle delimiters are rebuilt at the inner size (their options are the leftright's here).
        for node in inner.iter_mut().skip(1) {
            if node.has_class("@middle") {
                let delim = node.classes().iter().find_map(|c| c.strip_prefix("@delim:").map(str::to_string));
                if let Some(d) = delim {
                    *node = delim::make_left_right_delim(&d, ih, idp, options, mode, vec![]);
                }
            }
        }
    }
    let right_delim = if right == "." {
        make_null_delimiter(options, &["mclose"])
    } else {
        let co = match right_color {
            Some(c) => options.with_color(c),
            None => options.clone(),
        };
        delim::make_left_right_delim(right, ih, idp, &co, mode, vec!["mclose".into()])
    };
    inner.push(right_delim);
    Dom::Span(make_span(vec!["minner".into()], inner, Some(options)))
}

// ── accent ───────────────────────────────────────────────────────────────────

fn base_symbol(d: &Dom) -> Option<&tree::Sym> {
    match d {
        Dom::Sym(s) => Some(s),
        Dom::Span(s) | Dom::Frag(s) if s.children.len() == 1 => base_symbol(&s.children[0]),
        _ => None,
    }
}

fn build_accent(group: &P, supsub: Option<(Option<&P>, Option<&P>)>, options: &Options) -> Dom {
    let P::Accent { label, is_stretchy, is_shifty, base, mode } = group else { return empty_span() };
    let supsub_group = supsub.map(|(sup, sub)| build_supsub_plain(Some(base), sup, sub, options));
    let body = build_group(base, &options.having_cramped_style(), None);
    let must_shift = *is_shifty && is_character_box(base);
    let skew = if must_shift { base_symbol(&body).map(|s| s.skew).unwrap_or(0.0) } else { 0.0 };
    let accent_below = label == "\\c";
    let mut clearance = if accent_below { body.height() + body.depth() } else { body.height().min(options.font_metrics().x_height) };
    let accent_body = if !*is_stretchy {
        let (accent, width) = if label == "\\vec" {
            (static_svg("vec", options), 0.471)
        } else {
            let mut a = make_ord(label, *mode, options, false);
            a.set_italic(0.0);
            let w = match &a {
                Dom::Sym(s) => s.width,
                _ => 0.0,
            };
            if accent_below {
                clearance += a.depth();
            }
            (a, w)
        };
        let mut ab = make_span(vec!["accent-body".into()], vec![accent], None);
        let full = label == "\\textcircled";
        if full {
            ab.classes.push("accent-full".into());
            clearance = body.height();
        }
        let mut left = skew;
        if !full {
            left -= width / 2.0;
        }
        ab.css.left = Some(em4(left));
        if full {
            ab.css.top = Some(0.2);
        }
        make_vlist(VPos::FirstBaseline, vec![VItem::elem(body), VItem::Kern(-clearance), VItem::elem(Dom::Span(ab))])
    } else {
        let svg = crate::stretchy::stretchy_svg_for(label, Some(base), options);
        let mut wcss = Css::default();
        if skew > 0.0 {
            wcss.margin_left = Some(em4(2.0 * skew));
        }
        make_vlist(VPos::FirstBaseline, vec![VItem::elem(body), VItem::Elem { elem: svg, shift: 0.0, margin_left: None, margin_right: None, wrapper_classes: vec!["svg-align".into()], wrapper_css: wcss }])
    };
    let accent_wrap = Dom::Span(make_span(vec!["mord".into(), "accent".into()], vec![accent_body], Some(options)));
    match supsub_group {
        Some(Dom::Span(mut s)) => {
            if !s.children.is_empty() {
                s.children[0] = accent_wrap.clone();
            }
            s.height = s.height.max(accent_wrap.height());
            if let Some(c) = s.classes.first_mut() {
                *c = "mord".into();
            }
            Dom::Span(s)
        }
        _ => accent_wrap,
    }
}

// ── horizBrace ───────────────────────────────────────────────────────────────

fn build_horiz_brace(group: &P, script: Option<(Option<&P>, Option<&P>)>, options: &Options) -> Dom {
    let P::HorizBrace { label, is_over, base, .. } = group else { return empty_span() };
    let style = options.style;
    let sup_sub_group = script.map(|(sup, sub)| match sup {
        Some(s) => build_group(s, &options.having_style(style.sup()), Some(options)),
        None => build_group(sub.unwrap_or(group), &options.having_style(style.sub()), Some(options)),
    });
    let body = build_group(base, &options.having_base_style(Style::DISPLAY), None);
    let brace = crate::stretchy::stretchy_svg_for(label, Some(base), options);
    let svg_align = |elem: Dom| VItem::Elem { elem, shift: 0.0, margin_left: None, margin_right: None, wrapper_classes: vec!["svg-align".into()], wrapper_css: Css::default() };
    let class = if *is_over { "mover" } else { "munder" };
    let mut vlist = if *is_over {
        make_vlist(VPos::FirstBaseline, vec![VItem::elem(body), VItem::Kern(0.1), svg_align(brace)])
    } else {
        let pos = body.depth() + 0.1 + brace.height();
        make_vlist(VPos::Bottom(pos), vec![svg_align(brace), VItem::Kern(0.1), VItem::elem(body)])
    };
    if let Some(ssg) = sup_sub_group {
        let v_span = Dom::Span(make_span(vec!["minner".into(), class.into()], vec![vlist], Some(options)));
        vlist = if *is_over {
            make_vlist(VPos::FirstBaseline, vec![VItem::elem(v_span), VItem::Kern(0.2), VItem::elem(ssg)])
        } else {
            let pos = v_span.depth() + 0.2 + ssg.height() + ssg.depth();
            make_vlist(VPos::Bottom(pos), vec![VItem::elem(ssg), VItem::Kern(0.2), VItem::elem(v_span)])
        };
    }
    Dom::Span(make_span(vec!["minner".into(), class.into()], vec![vlist], Some(options)))
}

// ── lap, enclose ─────────────────────────────────────────────────────────────

fn build_lap(alignment: &str, body: &P, options: &Options) -> Dom {
    let inner = if alignment == "clap" {
        let i = make_span(vec![], vec![build_group(body, options, None)], None);
        make_span(vec!["inner".into()], vec![Dom::Span(i)], Some(options))
    } else {
        make_span(vec!["inner".into()], vec![build_group(body, options, None)], None)
    };
    let fix = make_span(vec!["fix".into()], vec![], None);
    let node = make_span(vec![alignment.to_string()], vec![Dom::Span(inner), Dom::Span(fix)], Some(options));
    let thin = make_span(vec!["thinbox".into()], vec![Dom::Span(node)], Some(options));
    Dom::Span(make_span(vec!["mord".into(), "vbox".into()], vec![Dom::Span(thin)], Some(options)))
}

fn build_enclose(label: &str, body: &P, background: Option<&str>, options: &Options) -> Dom {
    let is_box = label == "\\fbox" || label == "\\colorbox";
    let body_options = if is_box { options.having_style(Style::DISPLAY) } else { options.clone() };
    let mut inner = wrap_fragment(build_group(body, &body_options, None), options);
    let single = is_character_box(body);
    let fm = options.font_metrics();
    if label.contains("cancel") {
        if !single {
            inner.classes_mut().push("cancel-pad".into());
        }
    } else {
        inner.classes_mut().push("boxpad".into());
    }
    let (rule, top_pad) = if is_box {
        let r = fm.fboxrule.max(options.min_rule_thickness);
        (r, fm.fboxsep + if label == "\\colorbox" { 0.0 } else { r })
    } else if label == "\\sout" {
        (fm.default_rule_thickness, 0.0)
    } else {
        (0.0, if single { 0.2 } else { 0.0 })
    };
    let mut img = make_span(vec!["stretchy".into(), label[1..].to_string()], vec![], Some(options));
    img.height = inner.height() + inner.depth() + 2.0 * top_pad;
    img.css.height = Some(em4(img.height));
    if label == "\\fbox" {
        img.css.border = Some(em4(rule));
    }
    if let Some(bg) = background {
        img.css.background = Some(bg.to_string());
    }
    let img_shift = if label == "\\sout" { -0.5 * fm.x_height } else { inner.depth() + top_pad };
    let items = if background.is_some() {
        vec![VItem::shifted(Dom::Span(img), img_shift), VItem::shifted(inner.clone(), 0.0)]
    } else {
        vec![VItem::shifted(inner.clone(), 0.0), VItem::shifted(Dom::Span(img), img_shift)]
    };
    let mut v = make_vlist(VPos::IndividualShift, items);
    if label.contains("cancel") {
        v.set_height(inner.height());
        v.set_depth(inner.depth());
    }
    Dom::Span(make_span(vec!["mord".into()], vec![v], Some(options)))
}

// ── arrays ───────────────────────────────────────────────────────────────────

fn build_array(group: &ArrayNode, options: &Options) -> Dom {
    let nr = group.body.len();
    let fm = options.font_metrics();
    let rule_thickness = fm.array_rule_width.max(options.min_rule_thickness);
    let pt = 1.0 / fm.pt_per_em;
    let mut arraycolsep = 5.0 * pt;
    if group.col_separation_type == Some("small") {
        let local = options.having_style(Style::SCRIPT).size_multiplier;
        arraycolsep = 0.2778 * (local / options.size_multiplier);
    }
    let baselineskip = 12.0 * pt;
    let jot = 3.0 * pt;
    let arrayskip = group.arraystretch * baselineskip;
    let arstrut_h = 0.7 * arrayskip;
    let arstrut_d = 0.3 * arrayskip;
    let mut total_height = 0.0;
    let mut hlines: Vec<(f64, bool)> = Vec::new();
    let set_hline_pos = |total: &mut f64, gap: &[bool], hl: &mut Vec<(f64, bool)>| {
        for (i, dashed) in gap.iter().enumerate() {
            if i > 0 {
                *total += 0.25;
            }
            hl.push((*total, *dashed));
        }
    };
    set_hline_pos(&mut total_height, group.hlines_before_row.first().map(Vec::as_slice).unwrap_or(&[]), &mut hlines);
    struct Row {
        cells: Vec<Dom>,
        height: f64,
        depth: f64,
        pos: f64,
    }
    let mut rows: Vec<Row> = Vec::new();
    let mut nc = 0;
    for (r, inrow) in group.body.iter().enumerate() {
        let mut height = arstrut_h;
        let mut depth = arstrut_d;
        nc = nc.max(inrow.len());
        let mut cells = Vec::new();
        for c in inrow {
            let elt = build_group(c, options, None);
            depth = depth.max(elt.depth());
            height = height.max(elt.height());
            cells.push(elt);
        }
        let mut gap = 0.0;
        if let Some(Some(g)) = group.row_gaps.get(r) {
            gap = calculate_size(g, options);
            if gap > 0.0 {
                gap += arstrut_d;
                if depth < gap {
                    depth = gap;
                }
                gap = 0.0;
            }
        }
        if group.add_jot && r + 1 < nr {
            depth += jot;
        }
        total_height += height;
        let pos = total_height;
        total_height += depth + gap;
        rows.push(Row { cells, height, depth, pos });
        set_hline_pos(&mut total_height, group.hlines_before_row.get(r + 1).map(Vec::as_slice).unwrap_or(&[]), &mut hlines);
    }
    let offset = total_height / 2.0 + fm.axis_height;
    let cols_desc = &group.cols;
    let mut cols: Vec<Dom> = Vec::new();
    let colsep = |w: f64| {
        let mut s = make_span(vec!["arraycolsep".into()], vec![], None);
        s.css.width = Some(em4(w));
        Dom::Span(s)
    };
    let mut c = 0usize;
    let mut d = 0usize;
    while c < nc || d < cols_desc.len() {
        let mut first_sep = true;
        while let Some(AlignSpec::Separator(sep)) = cols_desc.get(d) {
            if !first_sep {
                cols.push(colsep(fm.double_rule_sep));
            }
            let mut separator = make_span(vec!["vertical-separator".into()], vec![], Some(options));
            separator.css.height = Some(em4(total_height));
            separator.css.border_right = Some(em4(rule_thickness));
            separator.css.dashed = *sep == ':';
            separator.css.margin_left = Some(em4(-rule_thickness / 2.0));
            separator.css.margin_right = Some(em4(-rule_thickness / 2.0));
            let shift = total_height - offset;
            if shift != 0.0 {
                separator.css.vertical_align = Some(em4(-shift));
            }
            cols.push(Dom::Span(separator));
            d += 1;
            first_sep = false;
        }
        if c >= nc {
            c += 1;
            d += 1;
            continue;
        }
        let desc = cols_desc.get(d);
        let (align, pregap, postgap) = match desc {
            Some(AlignSpec::Align { align, pregap, postgap }) => (*align, *pregap, *postgap),
            _ => ('c', None, None),
        };
        if c > 0 || group.hskip_before_and_after {
            let w = pregap.unwrap_or(arraycolsep);
            if w != 0.0 {
                cols.push(colsep(w));
            }
        }
        let mut col_elems = Vec::new();
        for row in &mut rows {
            if c >= row.cells.len() {
                continue;
            }
            let mut elem = std::mem::replace(&mut row.cells[c], empty_span());
            let shift = row.pos - offset;
            elem.set_depth(row.depth);
            elem.set_height(row.height);
            col_elems.push(VItem::shifted(elem, shift));
        }
        let col_vlist = make_vlist(VPos::IndividualShift, col_elems);
        cols.push(Dom::Span(make_span(vec![format!("col-align-{align}")], vec![col_vlist], None)));
        if c + 1 < nc || group.hskip_before_and_after {
            let w = postgap.unwrap_or(arraycolsep);
            if w != 0.0 {
                cols.push(colsep(w));
            }
        }
        c += 1;
        d += 1;
    }
    let mut table_body = Dom::Span(make_span(vec!["mtable".into()], cols, None));
    if !hlines.is_empty() {
        let mut items = vec![VItem::shifted(table_body, 0.0)];
        while let Some((pos, dashed)) = hlines.pop() {
            let line = make_line_span(if dashed { "hdashline" } else { "hline" }, options, Some(rule_thickness));
            items.push(VItem::shifted(line, pos - offset));
        }
        table_body = make_vlist(VPos::IndividualShift, items);
    }
    Dom::Span(make_span(vec!["mord".into()], vec![table_body], Some(options)))
}

/// `buildHTMLUnbreakable`.
fn build_unbreakable(children: Vec<Dom>, options: &Options) -> Dom {
    let mut body = make_span(vec!["base".into()], children, Some(options));
    let mut strut = make_span(vec!["strut".into()], vec![], None);
    strut.css.height = Some(em4(body.height + body.depth));
    if body.depth != 0.0 {
        strut.css.vertical_align = Some(em4(-body.depth));
    }
    body.children.insert(0, Dom::Span(strut));
    Dom::Span(body)
}

/// `buildHTML`: the `.katex-html` span, split into unbreakable `.base` spans.
pub fn build_html(tree: &[P], options: &Options) -> Dom {
    let expression = build_expression(tree, options, RealGroup::Root, (None, None));
    let mut children = Vec::new();
    let mut parts: Vec<Dom> = Vec::new();
    let mut it = expression.into_iter().peekable();
    while let Some(e) = it.next() {
        let breakable = e.has_class("mbin") || e.has_class("mrel") || e.has_class("allowbreak");
        let newline = e.has_class("newline");
        parts.push(e);
        if breakable {
            let mut nobreak = false;
            while let Some(n) = it.peek() {
                if n.has_class("mspace") && !n.has_class("newline") {
                    if n.has_class("nobreak") {
                        nobreak = true;
                    }
                    if let Some(n) = it.next() {
                        parts.push(n);
                    }
                } else {
                    break;
                }
            }
            if !nobreak {
                children.push(build_unbreakable(std::mem::take(&mut parts), options));
            }
        } else if newline {
            let nl = parts.pop();
            if !parts.is_empty() {
                children.push(build_unbreakable(std::mem::take(&mut parts), options));
            }
            if let Some(nl) = nl {
                children.push(nl);
            }
        }
    }
    if !parts.is_empty() {
        children.push(build_unbreakable(parts, options));
    }
    Dom::Span(make_span(vec!["katex-html".into()], children, None))
}

#[allow(dead_code)]
fn group_of(p: &P) -> Option<Group> {
    if let P::Atom { family, .. } = p {
        Some(*family)
    } else {
        None
    }
}
