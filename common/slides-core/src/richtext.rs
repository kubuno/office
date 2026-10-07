//! Rich text of the text boxes — a port of the web's `presentationRichText.ts`.
//!
//! The stored model is a ProseMirror-like doc (`{type:'doc', content:[{type:'paragraph', attrs?, content:[{type:
//! 'text', text, marks?}]}]}`) read into paragraphs of runs; a run is a maximal span sharing the same marks, and a
//! mark left unset falls back to the element's own style when drawn. [`layout_rich`] is `layoutRich`: the word
//! wrap across styled runs, list markers, indents and justification, measured through [`Measure`].
//!
//! Every laid-out segment also carries where its text comes from (paragraph and character offsets), which the
//! web did not need (the browser's `contentEditable` placed its caret) and the desktop's own editor does.

use serde_json::{json, Map, Value};

use crate::model::{as_f64, num};

/// One run of text and its marks (`RichRun`). A mark is present or absent: the stored doc cannot say « not
/// bold » in a bold box (QUIRK of the web model, kept).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Run {
    pub text: String,
    pub b: bool,
    pub i: bool,
    pub u: bool,
    pub s: bool,
    pub sup: bool,
    pub sub: bool,
    pub color: Option<String>,
    /// Highlight colour.
    pub hl: Option<String>,
    /// Font size in slide px.
    pub size: Option<f64>,
}

impl Run {
    pub fn plain(text: &str) -> Run {
        Run { text: text.to_string(), ..Run::default() }
    }

    /// Same marks (`sameStyle`).
    pub fn same_style(&self, o: &Run) -> bool {
        self.b == o.b && self.i == o.i && self.u == o.u && self.s == o.s && self.sup == o.sup && self.sub == o.sub && self.hl == o.hl && self.color == o.color && self.size.unwrap_or(0.0) == o.size.unwrap_or(0.0)
    }

    /// The run's marks with another text.
    pub fn with_text(&self, text: String) -> Run {
        Run { text, ..self.clone() }
    }
}

/// A paragraph's attributes (`ParaAttrs`): kept as the stored object (unknown keys included), read through
/// the typed getters.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParaAttrs(pub Map<String, Value>);

impl ParaAttrs {
    fn truthy(&self, k: &str) -> bool {
        crate::model::truthy(self.0.get(k))
    }
    /// `left`, `center`, `right`, `justify`.
    pub fn align(&self) -> Option<&str> {
        self.0.get("align").and_then(Value::as_str).filter(|s| !s.is_empty())
    }
    /// `bullet` or `number`.
    pub fn list(&self) -> Option<&str> {
        self.0.get("list").and_then(Value::as_str).filter(|s| !s.is_empty())
    }
    /// Indent in slide px.
    pub fn indent(&self) -> f64 {
        as_f64(self.0.get("indent")).unwrap_or(0.0)
    }
    /// Line-height multiplier (default 1.3).
    pub fn line_height(&self) -> Option<f64> {
        as_f64(self.0.get("lineHeight")).filter(|v| *v != 0.0)
    }
    /// True when the web keeps the attrs (`a.align || a.list || a.indent || a.lineHeight`).
    pub fn significant(&self) -> bool {
        self.truthy("align") || self.truthy("list") || self.truthy("indent") || self.truthy("lineHeight")
    }
    pub fn set(&mut self, k: &str, v: Option<Value>) {
        match v {
            Some(v) => {
                self.0.insert(k.to_string(), v);
            }
            None => {
                self.0.shift_remove(k);
            }
        }
    }
}

/// One paragraph (`RichPara`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Para {
    pub runs: Vec<Run>,
    pub attrs: ParaAttrs,
}

impl Para {
    /// The paragraph's text.
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// Its length in characters.
    pub fn len(&self) -> usize {
        self.runs.iter().map(|r| r.text.chars().count()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.runs.iter().all(|r| r.text.is_empty())
    }
}

// ── doc ⇄ paragraphs ─────────────────────────────────────────────────────────

fn marks_to_run(text: &str, marks: Option<&Value>) -> Run {
    let mut run = Run::plain(text);
    for m in marks.and_then(Value::as_array).into_iter().flatten() {
        let attrs = m.get("attrs");
        match m.get("type").and_then(Value::as_str) {
            Some("bold") => run.b = true,
            Some("italic") => run.i = true,
            Some("underline") => run.u = true,
            Some("strike") => run.s = true,
            Some("superscript") => run.sup = true,
            Some("subscript") => run.sub = true,
            Some("highlight") => {
                run.hl = Some(match attrs.and_then(|a| a.get("color")).filter(|c| crate::model::truthy(Some(c))) {
                    Some(Value::String(c)) => c.clone(),
                    Some(other) => js_string(other),
                    None => "#fff176".to_string(),
                })
            }
            Some("textStyle") => {
                if let Some(a) = attrs.filter(|a| a.is_object()) {
                    if let Some(c) = a.get("color").filter(|c| crate::model::truthy(Some(c))) {
                        run.color = Some(match c {
                            Value::String(s) => s.clone(),
                            other => js_string(other),
                        });
                    }
                    if let Some(fs) = a.get("fontSize").filter(|v| !v.is_null()) {
                        if let Some(n) = parse_float(&js_string(fs)) {
                            run.size = Some(n);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    run
}

/// `String(v)` for the JSON values marks hold.
fn js_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.as_f64().map(crate::path_num).unwrap_or_default(),
        other => other.to_string(),
    }
}

/// JavaScript's `parseFloat`: the longest numeric prefix, else `None` (NaN).
pub fn parse_float(s: &str) -> Option<f64> {
    let t = s.trim_start();
    let b = t.as_bytes();
    let mut end = 0;
    let mut seen_digit = false;
    if end < b.len() && (b[end] == b'+' || b[end] == b'-') {
        end += 1;
    }
    while end < b.len() && b[end].is_ascii_digit() {
        end += 1;
        seen_digit = true;
    }
    if end < b.len() && b[end] == b'.' {
        end += 1;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
            seen_digit = true;
        }
    }
    if !seen_digit {
        return None;
    }
    if end < b.len() && (b[end] == b'e' || b[end] == b'E') {
        let mut k = end + 1;
        if k < b.len() && (b[k] == b'+' || b[k] == b'-') {
            k += 1;
        }
        let digits = k;
        while k < b.len() && b[k].is_ascii_digit() {
            k += 1;
        }
        if k > digits {
            end = k;
        }
    }
    t[..end].parse().ok()
}

/// `docToParas`: anything but a doc gives one empty paragraph; text nodes with no text are dropped.
pub fn doc_to_paras(doc: Option<&Value>) -> Vec<Para> {
    let Some(d) = doc.filter(|d| d.get("type").and_then(Value::as_str) == Some("doc")) else { return vec![Para::default()] };
    let Some(content) = d.get("content").and_then(Value::as_array) else { return vec![Para::default()] };
    content
        .iter()
        .map(|p| {
            let runs = p
                .get("content")
                .and_then(Value::as_array)
                .map(|nodes| {
                    nodes
                        .iter()
                        .filter(|n| n.get("type").and_then(Value::as_str) == Some("text"))
                        .filter_map(|n| n.get("text").and_then(Value::as_str).filter(|t| !t.is_empty()).map(|t| marks_to_run(t, n.get("marks"))))
                        .collect()
                })
                .unwrap_or_default();
            let attrs = match p.get("attrs") {
                Some(Value::Object(a)) => {
                    let pa = ParaAttrs(a.clone());
                    if pa.significant() {
                        pa
                    } else {
                        ParaAttrs::default()
                    }
                }
                _ => ParaAttrs::default(),
            };
            Para { runs, attrs }
        })
        .collect()
}

fn run_marks(run: &Run) -> Vec<Value> {
    let mut marks = Vec::new();
    if run.b {
        marks.push(json!({ "type": "bold" }));
    }
    if run.i {
        marks.push(json!({ "type": "italic" }));
    }
    if run.u {
        marks.push(json!({ "type": "underline" }));
    }
    if run.s {
        marks.push(json!({ "type": "strike" }));
    }
    if run.sup {
        marks.push(json!({ "type": "superscript" }));
    }
    if run.sub {
        marks.push(json!({ "type": "subscript" }));
    }
    if let Some(hl) = run.hl.as_ref().filter(|h| !h.is_empty()) {
        marks.push(json!({ "type": "highlight", "attrs": { "color": hl } }));
    }
    let mut attrs = Map::new();
    if let Some(c) = run.color.as_ref().filter(|c| !c.is_empty()) {
        attrs.insert("color".into(), Value::String(c.clone()));
    }
    if let Some(s) = run.size {
        attrs.insert("fontSize".into(), num(s));
    }
    if !attrs.is_empty() {
        marks.push(json!({ "type": "textStyle", "attrs": Value::Object(attrs) }));
    }
    marks
}

/// `mergeRuns`: adjacent runs with the same marks merged, empty ones dropped.
pub fn merge_runs(runs: &[Run]) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for r in runs {
        if r.text.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.same_style(r) => last.text.push_str(&r.text),
            _ => out.push(r.clone()),
        }
    }
    out
}

/// `parasToDoc`.
pub fn paras_to_doc(paras: &[Para]) -> Value {
    let content: Vec<Value> = paras
        .iter()
        .map(|p| {
            let runs = merge_runs(&p.runs);
            let nodes: Vec<Value> = runs
                .iter()
                .map(|r| {
                    let marks = run_marks(r);
                    if marks.is_empty() {
                        json!({ "type": "text", "text": r.text })
                    } else {
                        json!({ "type": "text", "text": r.text, "marks": marks })
                    }
                })
                .collect();
            let mut node = Map::new();
            node.insert("type".into(), Value::String("paragraph".into()));
            node.insert("content".into(), Value::Array(nodes));
            if p.attrs.significant() {
                node.insert("attrs".into(), Value::Object(p.attrs.0.clone()));
            }
            Value::Object(node)
        })
        .collect();
    json!({ "type": "doc", "content": content })
}

/// `parasToPlain`: one line per paragraph.
pub fn paras_to_plain(paras: &[Para]) -> String {
    paras.iter().map(Para::text).collect::<Vec<_>>().join("\n")
}

/// `textDocFromString`: one paragraph per line.
pub fn doc_from_string(s: &str) -> Value {
    let content: Vec<Value> = s.split('\n').map(|line| if line.is_empty() { json!({ "type": "paragraph", "content": [] }) } else { json!({ "type": "paragraph", "content": [{ "type": "text", "text": line }] }) }).collect();
    json!({ "type": "doc", "content": content })
}

// ── Layout ───────────────────────────────────────────────────────────────────

/// Text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

impl Align {
    pub fn parse(s: Option<&str>) -> Option<Align> {
        match s? {
            "left" => Some(Align::Left),
            "center" => Some(Align::Center),
            "right" => Some(Align::Right),
            "justify" => Some(Align::Justify),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
            Align::Justify => "justify",
        }
    }
}

/// The element-level style a run falls back to (`RichDefaults`).
#[derive(Debug, Clone, PartialEq)]
pub struct Defaults {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: String,
    pub size: f64,
    pub family: String,
    pub align: Align,
}

/// A run's style once resolved against the defaults (`ResolvedStyle`). `size` is in canvas px once laid out.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: String,
    pub size: f64,
    pub family: String,
    pub hl: Option<String>,
    /// Baseline shift as a fraction of the size (negative = up).
    pub rise: f64,
}

impl Style {
    /// The CSS font shorthand the web measures and draws with (`${italic} ${bold} ${size}px ${family}`).
    pub fn font_string(&self) -> String {
        format!("{} {} {}px {}", if self.italic { "italic" } else { "normal" }, if self.bold { "bold" } else { "normal" }, crate::path_num(self.size), self.family)
    }
}

/// `resolveStyle`.
pub fn resolve_style(run: &Run, d: &Defaults) -> Style {
    let base = run.size.unwrap_or(d.size);
    Style {
        bold: run.b || d.bold,
        italic: run.i || d.italic,
        underline: run.u || d.underline,
        strike: run.s,
        color: run.color.clone().unwrap_or_else(|| d.color.clone()),
        size: if run.sup || run.sub { base * 0.66 } else { base },
        family: d.family.clone(),
        hl: run.hl.clone(),
        rise: if run.sup {
            -0.42
        } else if run.sub {
            0.16
        } else {
            0.0
        },
    }
}

/// The platform's text measurer: the advance width of `text` in `style` (canvas px), with the canvas's
/// `letterSpacing` (added after every character, the last included).
pub trait Measure {
    fn width(&self, text: &str, style: &Style, letter_spacing: f64) -> f64;
}

/// A fixed-metrics measurer (every character `size × 0.5` wide) for tests and the text interface.
#[derive(Debug, Default, Clone, Copy)]
pub struct FixedMeasure;

impl Measure for FixedMeasure {
    fn width(&self, text: &str, style: &Style, ls: f64) -> f64 {
        let n = text.chars().count() as f64;
        n * (style.size * 0.5 + ls)
    }
}

/// A laid-out segment (`LaidSeg`) and where its text comes from: paragraph `para`, characters `start..end`.
#[derive(Debug, Clone, PartialEq)]
pub struct Seg {
    pub text: String,
    pub style: Style,
    pub width: f64,
    pub para: usize,
    pub start: usize,
    pub end: usize,
}

/// A list marker (`• ` / `n. `) at `x` from the column's left.
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub text: String,
    pub style: Style,
    pub width: f64,
    pub x: f64,
}

/// A laid-out line (`LaidLine`), plus its paragraph and character range.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub segs: Vec<Seg>,
    pub height: f64,
    pub ascent: f64,
    pub align: Align,
    pub indent: f64,
    pub justify: bool,
    pub marker: Option<Marker>,
    pub para: usize,
    /// First and one-past-last character of the line in its paragraph (skipped leading spaces included).
    pub start: usize,
    pub end: usize,
}

/// True for JavaScript's `\s` (close enough: Unicode whitespace).
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

/// `text.split(/(\s+)/)` without the empty pieces: alternating words and whitespace groups, with their
/// character offsets.
fn tokens(text: &str) -> Vec<(String, usize, bool)> {
    let mut out: Vec<(String, usize, bool)> = Vec::new();
    for (i, c) in text.chars().enumerate() {
        let sp = is_space(c);
        match out.last_mut() {
            Some((t, _, s)) if *s == sp => t.push(c),
            _ => out.push((c.to_string(), i, sp)),
        }
    }
    out
}

fn same_resolved(a: &Style, b: &Style) -> bool {
    a == b
}

/// `layoutRich`: paragraphs into lines fitting `max_w`; `size_scale` multiplies every font size (the canvas
/// scale, and the shrink-to-fit factor).
pub fn layout_rich(paras: &[Para], d: &Defaults, max_w: f64, m: &dyn Measure, ls: f64, size_scale: f64) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    let scale_style = |mut s: Style| {
        s.size *= size_scale;
        s
    };
    let mut number_counter = 0u32;
    for (pi, para) in paras.iter().enumerate() {
        let attrs = &para.attrs;
        let p_align = Align::parse(attrs.align()).unwrap_or(d.align);
        let indent_px = attrs.indent() * size_scale;
        let lh_mul = attrs.line_height().unwrap_or(1.3);
        let marker = match attrs.list() {
            Some("bullet") => {
                number_counter = 0;
                let style = scale_style(resolve_style(&Run::default(), d));
                let w = m.width("•  ", &style, ls);
                Some(Marker { text: "•  ".into(), style, width: w, x: 0.0 })
            }
            Some("number") => {
                number_counter += 1;
                let style = scale_style(resolve_style(&Run::default(), d));
                let text = format!("{number_counter}.  ");
                let w = m.width(&text, &style, ls);
                Some(Marker { text, style, width: w, x: 0.0 })
            }
            _ => {
                number_counter = 0;
                None
            }
        };
        let left_pad = indent_px + marker.as_ref().map(|mk| mk.width).unwrap_or(0.0);
        let line_max_w = (max_w - left_pad).max(1.0);

        // Tokens with their style and source offsets.
        let mut toks: Vec<(String, Style, usize, bool)> = Vec::new();
        let mut offset = 0usize;
        for run in &para.runs {
            let style = scale_style(resolve_style(run, d));
            for (t, at, sp) in tokens(&run.text) {
                toks.push((t, style.clone(), offset + at, sp));
            }
            offset += run.text.chars().count();
        }
        let para_line_start = lines.len();
        let base_size = d.size * size_scale;
        let mut cur: Vec<Seg> = Vec::new();
        let mut cur_w = 0.0f64;
        let mut max_size = base_size;
        let mut line_start = 0usize;
        let mut last_end = 0usize;
        let flush = |lines: &mut Vec<Line>, cur: &mut Vec<Seg>, cur_w: &mut f64, max_size: &mut f64, start: usize, end: usize| {
            lines.push(Line { segs: std::mem::take(cur), height: *max_size * lh_mul, ascent: *max_size, align: p_align, indent: left_pad, justify: false, marker: None, para: pi, start, end });
            *cur_w = 0.0;
            *max_size = base_size;
        };
        for (text, style, at, sp) in toks {
            let n = text.chars().count();
            let w = m.width(&text, &style, ls);
            if !sp && cur_w > 0.0 && cur_w + w > line_max_w {
                flush(&mut lines, &mut cur, &mut cur_w, &mut max_size, line_start, at);
                line_start = at;
            }
            if sp && cur_w == 0.0 {
                // Leading whitespace of a line is not drawn.
                if cur.is_empty() {
                    line_start = line_start.min(at);
                }
                last_end = at + n;
                continue;
            }
            match cur.last_mut() {
                Some(last) if same_resolved(&last.style, &style) => {
                    last.text.push_str(&text);
                    last.width += w;
                    last.end = at + n;
                }
                _ => cur.push(Seg { text, style: style.clone(), width: w, para: pi, start: at, end: at + n }),
            }
            cur_w += w;
            if style.size > max_size {
                max_size = style.size;
            }
            last_end = at + n;
        }
        let end = last_end.max(offset);
        flush(&mut lines, &mut cur, &mut cur_w, &mut max_size, line_start, end);
        if let (Some(mk), Some(first)) = (marker, lines.get_mut(para_line_start)) {
            first.marker = Some(Marker { x: indent_px, ..mk });
        }
        if p_align == Align::Justify {
            let n = lines.len();
            for l in lines.iter_mut().take(n.saturating_sub(1)).skip(para_line_start) {
                l.justify = true;
            }
        }
    }
    lines
}

/// The display transform of `textTransform` (`upper`, `lower`, `capitalize`), character for character so the
/// caret offsets still hold (a character whose case mapping is longer than one character is kept as it is).
pub fn transform_text(s: &str, tf: Option<&str>) -> String {
    let one = |it: &mut dyn Iterator<Item = char>, c: char| -> char {
        let first = it.next();
        match (first, it.next()) {
            (Some(x), None) => x,
            _ => c,
        }
    };
    match tf {
        Some("upper") => s.chars().map(|c| one(&mut c.to_uppercase(), c)).collect(),
        Some("lower") => s.chars().map(|c| one(&mut c.to_lowercase(), c)).collect(),
        Some("capitalize") => {
            // `/\b\w/g` with JavaScript's ASCII `\w`.
            let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
            let mut prev_word = false;
            s.chars()
                .map(|c| {
                    let out = if word(c) && !prev_word { c.to_ascii_uppercase() } else { c };
                    prev_word = word(c);
                    out
                })
                .collect()
        }
        _ => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Defaults {
        Defaults { bold: false, italic: false, underline: false, color: "#202124".into(), size: 20.0, family: "Arial".into(), align: Align::Left }
    }

    #[test]
    fn the_doc_round_trips_through_paragraphs() {
        let doc = json!({ "type": "doc", "content": [
            { "type": "paragraph", "attrs": { "align": "center", "x": 1 }, "content": [
                { "type": "text", "text": "Hello " },
                { "type": "text", "text": "world", "marks": [{ "type": "bold" }, { "type": "textStyle", "attrs": { "color": "#f00", "fontSize": 30 } }] } ] },
            { "type": "paragraph", "content": [] } ] });
        let paras = doc_to_paras(Some(&doc));
        assert_eq!(paras.len(), 2);
        assert_eq!(paras[0].runs[1], Run { text: "world".into(), b: true, color: Some("#f00".into()), size: Some(30.0), ..Run::default() });
        assert_eq!(paras_to_doc(&paras), doc);
        assert_eq!(paras_to_plain(&paras), "Hello world\n");
    }

    #[test]
    fn marks_follow_the_web_reader() {
        let paras = doc_to_paras(Some(&json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [
            { "type": "text", "text": "a", "marks": [{ "type": "highlight" }, { "type": "textStyle", "attrs": { "fontSize": "18px" } }] },
            { "type": "text", "text": "" }, { "type": "image" } ] }] })));
        assert_eq!(paras[0].runs, vec![Run { text: "a".into(), hl: Some("#fff176".into()), size: Some(18.0), ..Run::default() }]);
        assert_eq!(doc_to_paras(None), vec![Para::default()]);
        assert_eq!(parse_float(" 12.5px"), Some(12.5));
        assert_eq!(parse_float("px"), None);
    }

    #[test]
    fn insignificant_attrs_are_dropped_like_the_web() {
        let paras = doc_to_paras(Some(&json!({ "type": "doc", "content": [{ "type": "paragraph", "attrs": { "indent": 0 }, "content": [] }] })));
        assert!(paras[0].attrs.0.is_empty());
    }

    #[test]
    fn lines_break_on_spaces_and_skip_leading_ones() {
        // 10 px per character at size 20.
        let paras = vec![Para { runs: vec![Run::plain("aaa bbb ccc")], attrs: ParaAttrs::default() }];
        let lines = layout_rich(&paras, &defaults(), 75.0, &FixedMeasure, 0.0, 1.0);
        let texts: Vec<String> = lines.iter().map(|l| l.segs.iter().map(|s| s.text.as_str()).collect()).collect();
        assert_eq!(texts, ["aaa bbb ", "ccc"]);
        assert_eq!((lines[1].start, lines[1].end), (8, 11));
        assert_eq!(lines[0].height, 20.0 * 1.3);
    }

    #[test]
    fn lists_number_consecutive_paragraphs() {
        let mut attrs = ParaAttrs::default();
        attrs.set("list", Some(json!("number")));
        let p = |t: &str| Para { runs: vec![Run::plain(t)], attrs: attrs.clone() };
        let lines = layout_rich(&[p("a"), p("b")], &defaults(), 500.0, &FixedMeasure, 0.0, 1.0);
        assert_eq!(lines[1].marker.as_ref().map(|m| m.text.as_str()), Some("2.  "));
        assert_eq!(lines[1].indent, 40.0);
    }

    #[test]
    fn superscript_is_smaller_and_raised() {
        let s = resolve_style(&Run { sup: true, ..Run::default() }, &defaults());
        assert!((s.size - 13.2).abs() < 1e-9 && s.rise == -0.42);
        // 20 × 0.66 in floating point, printed like JavaScript prints it.
        assert_eq!(s.font_string(), "normal normal 13.200000000000001px Arial");
    }

    #[test]
    fn capitalize_uppercases_ascii_word_starts() {
        assert_eq!(transform_text("hello wörld_x 2nd", Some("capitalize")), "Hello WöRld_x 2nd");
        assert_eq!(transform_text("ß", Some("upper")), "ß");
    }
}
