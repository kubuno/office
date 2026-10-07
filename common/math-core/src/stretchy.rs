//! KaTeX's `stretchy.ts` (`stretchySvg`): the wide accents and the arrows over a group, drawn as SVG paths
//! stretched or clipped to the group's width.

use crate::data;
use crate::options::Options;
use crate::parse::P;
use crate::tree::{em4, make_span, Css, Dom, Svg, SvgFit};

fn svg_span(classes: Vec<String>, path: &str, view_box: (f64, f64), height: f64, fit: SvgFit, full_width: bool, options: &Options) -> Svg {
    let mut classes = classes;
    let mut css = Css::default();
    if options.style.is_tight() {
        classes.push("mtight".into());
    }
    if let Some(c) = options.get_color() {
        css.color = Some(c);
    }
    Svg { classes, paths: vec![data::svg_path(path).unwrap_or("").to_string()], view_box, svg_width: if full_width { None } else { Some(400.0) }, svg_height: height, fit, height, depth: 0.0, max_font_size: 0.0, css }
}

/// `stretchySvg` for an accent over `body` (the built base) of the parse node `base`.
pub fn stretchy_svg(label: &str, body: &Dom, options: &Options) -> Dom {
    let _ = body;
    stretchy_svg_for(label, None, options)
}

/// `stretchySvg` with the base node (its character count picks the wide accent's image).
pub fn stretchy_svg_for(label: &str, base: Option<&P>, options: &Options) -> Dom {
    let name = label.trim_start_matches('\\');
    let (mut dom, min_width, height) = if matches!(name, "widehat" | "widecheck" | "widetilde" | "utilde") {
        let num_chars = match base {
            Some(P::OrdGroup { body, .. }) => body.len(),
            _ => 1,
        };
        let (vbw, vbh, h, path) = if num_chars > 5 {
            if name == "widehat" || name == "widecheck" {
                (2364.0, 420.0, 0.42, format!("{name}4"))
            } else {
                (2340.0, 312.0, 0.34, "tilde4".to_string())
            }
        } else {
            let img = [1, 1, 2, 2, 3, 3][num_chars.min(5)];
            if name == "widehat" || name == "widecheck" {
                ([0.0, 1062.0, 2364.0, 2364.0, 2364.0][img], [0.0, 239.0, 300.0, 360.0, 420.0][img], [0.0, 0.24, 0.3, 0.3, 0.36, 0.42][img], format!("{name}{img}"))
            } else {
                ([0.0, 600.0, 1033.0, 2339.0, 2340.0][img], [0.0, 260.0, 286.0, 306.0, 312.0][img], [0.0, 0.26, 0.286, 0.3, 0.306, 0.34][img], format!("tilde{img}"))
            }
        };
        (Dom::Svg(svg_span(vec![], &path, (vbw, vbh), h, SvgFit::None, true, options)), 0.0, h)
    } else {
        let data: (&[&str], f64, f64, Option<SvgFit>) = match name {
            "overrightarrow" | "underrightarrow" => (&["rightarrow"], 0.888, 522.0, Some(SvgFit::XMaxYMinSlice)),
            "overleftarrow" | "underleftarrow" => (&["leftarrow"], 0.888, 522.0, Some(SvgFit::XMinYMinSlice)),
            "Overrightarrow" => (&["doublerightarrow"], 0.888, 560.0, Some(SvgFit::XMaxYMinSlice)),
            "overleftharpoon" => (&["leftharpoon"], 0.888, 522.0, Some(SvgFit::XMinYMinSlice)),
            "overrightharpoon" => (&["rightharpoon"], 0.888, 522.0, Some(SvgFit::XMaxYMinSlice)),
            "overleftrightarrow" | "underleftrightarrow" => (&["leftarrow", "rightarrow"], 0.888, 522.0, None),
            "overlinesegment" | "underlinesegment" => (&["leftlinesegment", "rightlinesegment"], 0.888, 522.0, None),
            "overgroup" => (&["leftgroup", "rightgroup"], 0.888, 342.0, None),
            "overbrace" => (&["leftbrace", "midbrace", "rightbrace"], 1.6, 548.0, None),
            "underbrace" => (&["leftbraceunder", "midbraceunder", "rightbraceunder"], 1.6, 548.0, None),
            "overbracket" => (&["leftbracketover", "rightbracketover"], 1.6, 440.0, None),
            "underbracket" => (&["leftbracketunder", "rightbracketunder"], 1.6, 410.0, None),
            _ => (&["rightarrow"], 0.888, 522.0, Some(SvgFit::XMaxYMinSlice)),
        };
        let (paths, min_width, vbh, fit) = data;
        let h = vbh / 1000.0;
        if paths.len() == 1 {
            (Dom::Svg(svg_span(vec!["hide-tail".into()], paths[0], (400000.0, vbh), h, fit.unwrap_or(SvgFit::XMinYMinSlice), false, options)), min_width, h)
        } else {
            let mut spans = Vec::new();
            for (i, p) in paths.iter().enumerate() {
                let (class, fit) = match (paths.len(), i) {
                    (2, 0) => ("halfarrow-left", SvgFit::XMinYMinSlice),
                    (2, _) => ("halfarrow-right", SvgFit::XMaxYMinSlice),
                    (_, 0) => ("brace-left", SvgFit::XMinYMinSlice),
                    (_, 1) => ("brace-center", SvgFit::XMidYMinSlice),
                    _ => ("brace-right", SvgFit::XMaxYMinSlice),
                };
                let mut s = svg_span(vec![class.into()], p, (400000.0, vbh), h, fit, false, options);
                s.css.height = Some(em4(h));
                spans.push(Dom::Svg(s));
            }
            (Dom::Span(make_span(vec!["stretchy".into()], spans, Some(options))), min_width, h)
        }
    };
    dom.set_height(height);
    dom.css_mut().height = Some(em4(height));
    if min_width > 0.0 {
        dom.css_mut().min_width = Some(em4(min_width));
    }
    dom
}
