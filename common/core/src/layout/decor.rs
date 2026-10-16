//! What the web paints on a page besides its body (`drawPageDecorations`, `paintWatermark`,
//! `paintPageBorder` in `DocumentEditorPage.tsx`): the page colour, the header and the footer (rich
//! documents of their own, with the `{page}`, `{pages}`, `{date}`, `{titre}` fields), the simple page
//! number, the footnotes at the foot of the page, the line numbers, the page border and the
//! watermark. Computed here, in page coordinates (CSS px, origin at the sheet's top-left corner),
//! so that every platform paints the same thing; the platform only draws.
//!
//! The envelope keys are the web's (`DocMeta`): `pageNumbers`, `pageNumFormat`, `pageNumStart`,
//! `header`, `footer`, `hfFirstPage`, `pageColor`, `watermark`, `pageBorder`, `lineNumbers`; a
//! `sectionBreak` carries the `hfLinked`, `header`, `footer` and `pageColor` of the section after it.

use serde_json::Value;

use super::flow::layout_paragraphs;
use super::parse::parse_doc;
use super::{DocPx, LayoutParagraph, PageLayout, SpanKind};
use crate::marks::TextMark;
use crate::measure::Measure;
use crate::model::Node;

/// A run of text the platform paints with one font: `x` is its left edge (already aligned),
/// `baseline` its baseline.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub text: String,
    pub x: DocPx,
    pub baseline: DocPx,
    pub marks: TextMark,
    /// CSS colour.
    pub color: String,
}

/// A laid-out header or footer: paragraphs in band coordinates, painted at (`x`, `y`) of the page
/// with `alpha` (0.45 on screen: « estompé comme dans Word »).
#[derive(Clone, Debug)]
pub struct Band {
    pub paragraphs: Vec<LayoutParagraph>,
    pub x: DocPx,
    pub y: DocPx,
    pub alpha: f32,
}

/// `PageBorderDef`.
#[derive(Clone, Debug, PartialEq)]
pub struct PageBorder {
    pub color: String,
    pub width: f32,
    /// `solid`, `dashed`, `dotted` or `double`.
    pub style: String,
    pub margin: f32,
}

impl PageBorder {
    /// The rectangles `paintPageBorder` strokes (`double`: a second one, `width × 2` further in).
    pub fn rects(&self, page_w: DocPx, page_h: DocPx) -> Vec<(DocPx, DocPx, DocPx, DocPx)> {
        if self.width <= 0.0 {
            return Vec::new();
        }
        let m = self.margin.max(2.0);
        let rect = |inset: DocPx| (inset, inset, page_w - 2.0 * inset, page_h - 2.0 * inset);
        let mut out = vec![rect(m)];
        if self.style == "double" {
            out.push(rect(m + self.width * 2.0));
        }
        out
    }
}

/// `paintWatermark`: a faded text drawn under the content, centred on the page, turned along
/// the diagonal (`-atan2(pageH, pageW)`) when `diagonal`.
#[derive(Clone, Debug, PartialEq)]
pub struct Watermark {
    pub text: String,
    pub color: String,
    pub opacity: f32,
    /// Radians (0 horizontal).
    pub angle: f32,
    /// Bold, at this size in points, in this family.
    pub marks: TextMark,
    /// The centre of the text.
    pub cx: DocPx,
    pub cy: DocPx,
}

/// Everything around one page's body.
#[derive(Clone, Debug, Default)]
pub struct PageDecor {
    /// The page's ground (`sectionBreak.pageColor` ?? `pageColor`), `None` for white.
    pub background: Option<String>,
    pub header: Option<Band>,
    pub footer: Option<Band>,
    pub page_number: Option<Label>,
    /// The footnote block: the short separator (x0, y, x1) and the notes' lines.
    pub footnote_rule: Option<(DocPx, DocPx, DocPx)>,
    pub footnotes: Vec<Label>,
    pub line_numbers: Vec<Label>,
    pub border: Option<PageBorder>,
    pub watermark: Option<Watermark>,
    /// Paragraph shading (painted under the text) and paragraph borders (over it), page coordinates:
    /// (x, y, width, height) and the colour or the border (`shading`, `paraBorder`).
    pub para_fills: Vec<((DocPx, DocPx, DocPx, DocPx), String)>,
    pub para_borders: Vec<((DocPx, DocPx, DocPx, DocPx), PageBorder)>,
}

/// A paragraph's `shading` and `paraBorder` (the border's `margin` unused), for [`page_decor`].
pub type ParaStyle = (Option<String>, Option<PageBorder>);

/// `paraBorder` read from a block's attributes.
pub fn para_style_of(attrs: &serde_json::Map<String, Value>) -> ParaStyle {
    let shading = attrs.get("shading").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    let border = attrs.get("paraBorder").and_then(Value::as_object).map(|b| PageBorder {
        color: b.get("color").and_then(Value::as_str).unwrap_or("#9aa0a6").to_string(),
        width: num(b.get("width"), 1.0) as f32,
        style: b.get("style").and_then(Value::as_str).unwrap_or("solid").to_string(),
        margin: 0.0,
    });
    (shading, border)
}

/// The page's geometry, as `PageGeometry` gives it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sheet {
    pub page_w: DocPx,
    pub page_h: DocPx,
    /// `marginH`: the LEFT margin (the web uses it on both sides for the bands - a quirk kept).
    pub margin_h: DocPx,
    /// `marginV`: the top margin.
    pub margin_v: DocPx,
    pub margin_bottom: DocPx,
    /// The right margin (for the content width).
    pub margin_right: DocPx,
}

/// Where the page is in the document.
#[derive(Clone, Copy, Debug)]
pub struct PagePos {
    /// 0-based.
    pub index: usize,
    pub count: usize,
    pub section: usize,
}

/// What a page's decorations depend on outside the page: the envelope's keys, the section
/// metadata of the breaks, the title and today's date (the platform's clock and locale: the core
/// reads neither).
#[derive(Clone, Debug, Default)]
pub struct DecorContext {
    /// The envelope's root keys this module reads (absent ones are `None`).
    pub root: std::collections::BTreeMap<String, Value>,
    /// Per section (0 = base): `(hfLinked, header, footer, pageColor)` of its `sectionBreak`.
    pub sections: Vec<SectionMeta>,
    pub title: String,
    /// `new Date().toLocaleDateString(lang)`.
    pub date: String,
    /// The pages before each page, for continuous line numbering: body lines per page.
    pub body_lines_before: Vec<usize>,
}

/// `SectionHFMeta`.
#[derive(Clone, Debug, Default)]
pub struct SectionMeta {
    pub hf_linked: bool,
    pub header: Option<Node>,
    pub footer: Option<Node>,
    pub page_color: Option<String>,
}

/// The root keys [`page_decor`] reads.
pub const ROOT_KEYS: [&str; 10] =
    ["pageNumbers", "pageNumFormat", "pageNumStart", "header", "footer", "hfFirstPage", "pageColor", "watermark", "pageBorder", "lineNumbers"];

/// `headerBandTop` / `footerBandTop`.
pub fn header_band_top(s: &Sheet) -> DocPx {
    s.margin_v * 0.28
}

pub fn footer_band_top(s: &Sheet) -> DocPx {
    s.page_h - s.margin_bottom * 0.72
}

/// `toRoman` (upper case).
pub fn to_roman(mut n: usize) -> String {
    const MAP: [(usize, &str); 13] =
        [(1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")];
    let mut s = String::new();
    for (v, sym) in MAP {
        while n >= v {
            s.push_str(sym);
            n -= v;
        }
    }
    s
}

/// `toAlpha`: 1 → a, 26 → z, 27 → aa.
pub fn to_alpha(mut n: usize) -> String {
    let mut s = Vec::new();
    while n > 0 {
        n -= 1;
        s.push((b'a' + (n % 26) as u8) as char);
        n /= 26;
    }
    s.iter().rev().collect()
}

/// `formatPageNumber`.
pub fn format_page_number(n: i64, fmt: &str) -> String {
    let n = n.max(1) as usize;
    match fmt {
        "roman-lower" => to_roman(n).to_lowercase(),
        "roman-upper" => to_roman(n),
        "alpha-lower" => to_alpha(n),
        "alpha-upper" => to_alpha(n).to_uppercase(),
        _ => n.to_string(),
    }
}

/// `toHFContent`: a stored header/footer as a document - a `{type:"doc"}` as is, the former three
/// zones `{l,c,r}` as aligned paragraphs, a plain string as one paragraph; `None` for nothing.
pub fn hf_content(v: &Value) -> Option<Node> {
    if let Some(o) = v.as_object() {
        if o.get("type").and_then(Value::as_str) == Some("doc") {
            return serde_json::from_value(v.clone()).ok();
        }
        if o.contains_key("l") || o.contains_key("c") || o.contains_key("r") {
            let mut paras = Vec::new();
            for (k, align) in [("l", "left"), ("c", "center"), ("r", "right")] {
                let s = o.get(k).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).unwrap_or_default();
                if !s.is_empty() && s != "null" {
                    paras.push(serde_json::json!({ "type": "paragraph", "attrs": { "textAlign": align }, "content": [{ "type": "text", "text": s }] }));
                }
            }
            return if paras.is_empty() { None } else { serde_json::from_value(serde_json::json!({ "type": "doc", "content": paras })).ok() };
        }
    }
    match v.as_str() {
        Some(s) if !s.is_empty() => serde_json::from_value(serde_json::json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": s }] }] })).ok(),
        _ => None,
    }
}

/// `isHFEmpty`: no content, or only empty paragraphs.
pub fn hf_is_empty(doc: &Node) -> bool {
    doc.children().iter().all(|n| n.node_type() == Some("paragraph") && n.children().is_empty())
}

/// `expandHFDoc`: the fields replaced in every text node (case-insensitive, `{titre}` or
/// `{title}`), on a copy.
pub fn expand_hf(doc: &Node, page: i64, pages: i64, title: &str, date: &str, fmt: &str) -> Node {
    fn sub(s: &str, page: &str, pages: &str, title: &str, date: &str) -> String {
        // Case-insensitive replacement of each `{field}`, longest name first so `{pages}` is not
        // read as `{page}` followed by `s}`.
        let mut out = String::with_capacity(s.len());
        let mut i = 0;
        let fields: [(&str, &str); 5] = [("{pages}", pages), ("{page}", page), ("{date}", date), ("{titre}", title), ("{title}", title)];
        'outer: while i < s.len() {
            for (name, value) in fields {
                // The names are ASCII: an ASCII case-insensitive match on the original bytes.
                if s.get(i..i + name.len()).is_some_and(|w| w.eq_ignore_ascii_case(name)) {
                    out.push_str(value);
                    i += name.len();
                    continue 'outer;
                }
            }
            let ch = s[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
        out
    }
    fn walk(n: &Node, f: &dyn Fn(&str) -> String) -> Node {
        let mut n = n.clone();
        if n.node_type() == Some("text") {
            if let Some(t) = n.text() {
                n.set_text(&f(&t));
            }
            return n;
        }
        if n.has_child_array() {
            let kids: Vec<Node> = n.children().iter().map(|c| walk(c, f)).collect();
            n.set_children(kids);
        }
        n
    }
    let (page, pages) = (format_page_number(page, fmt), format_page_number(pages, fmt));
    walk(doc, &|s| sub(s, &page, &pages, title, date))
}

fn num(v: Option<&Value>, default: f64) -> f64 {
    v.and_then(Value::as_f64).unwrap_or(default)
}

fn string(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

/// `effectiveHF`: the header or footer of `section`, following « Lier au précédent » back to the
/// first unlinked section, else the document's.
fn effective_hf(ctx: &DecorContext, section: usize, kind: &str) -> Option<Node> {
    for s in (1..=section.min(ctx.sections.len().saturating_sub(1))).rev() {
        let m = &ctx.sections[s];
        if !m.hf_linked {
            return if kind == "header" { m.header.clone() } else { m.footer.clone() };
        }
    }
    ctx.root.get(kind).and_then(hf_content)
}

fn arial(pt: f32) -> TextMark {
    TextMark { font_size: Some(pt), font_family: Some("Arial".into()), ..TextMark::default() }
}

/// `countBodyLines`: the lines of a page that line numbering counts (no image lines, no table
/// cell lines, no injected fragments, no paragraph with `suppressLineNumbers`).
pub fn body_lines(page: &PageLayout, suppressed: &dyn Fn(usize) -> bool) -> usize {
    page.paragraphs
        .iter()
        .filter(|p| !suppressed(p.pm_start))
        .flat_map(|p| p.lines.iter())
        .filter(|l| l.image.is_none() && l.cell_x.is_none() && !l.phantom)
        .count()
}

/// `pageFootnotes`: the footnote references of a page, in order: (number, text, position).
pub fn page_footnotes(page: &PageLayout) -> Vec<(usize, String, usize)> {
    let mut out = Vec::new();
    for p in &page.paragraphs {
        for l in &p.lines {
            if l.phantom {
                continue;
            }
            for s in &l.spans {
                if let SpanKind::Footnote { n, text } = &s.kind {
                    out.push((*n, text.clone(), s.pm_pos));
                }
            }
        }
    }
    out
}

/// The decorations of one page (`drawPageDecorations` on screen: `dimHF` true).
pub fn page_decor(
    page: &PageLayout,
    pos: PagePos,
    sheet: &Sheet,
    ctx: &DecorContext,
    suppressed: &dyn Fn(usize) -> bool,
    para_style: &dyn Fn(usize) -> ParaStyle,
    m: &dyn Measure,
) -> PageDecor {
    let root = |k: &str| ctx.root.get(k);
    let mut d = PageDecor { background: ctx.sections.get(pos.section).and_then(|s| s.page_color.clone()).or_else(|| string(root("pageColor"))), ..Default::default() };

    let first_page_blank = root("hfFirstPage").and_then(Value::as_bool).unwrap_or(false) && pos.index == 0;
    let start = num(root("pageNumStart"), 1.0) as i64;
    let fmt = root("pageNumFormat").and_then(Value::as_str).unwrap_or("arabic").to_string();
    let content_w = (sheet.page_w - 2.0 * sheet.margin_h).max(1.0);

    if !first_page_blank {
        let band = |kind: &str, top: DocPx| -> Option<Band> {
            let doc = effective_hf(ctx, pos.section, kind)?;
            if hf_is_empty(&doc) {
                return None;
            }
            let expanded = expand_hf(&doc, pos.index as i64 + start, pos.count as i64 + start - 1, &ctx.title, &ctx.date, &fmt);
            let (paragraphs, _) = layout_paragraphs(&parse_doc(&expanded), &|_| content_w, m);
            Some(Band { paragraphs, x: sheet.margin_h, y: top, alpha: 0.45 })
        };
        d.header = band("header", header_band_top(sheet));
        d.footer = band("footer", footer_band_top(sheet));

        let pn = root("pageNumbers").and_then(Value::as_str).unwrap_or("none");
        if pn != "none" {
            let marks = arial(10.0);
            let text = format_page_number(pos.index as i64 + start, &fmt);
            let tw = m.width(&text, &marks);
            let baseline = if pn.starts_with("header") { sheet.margin_v * 0.55 } else { sheet.page_h - sheet.margin_bottom * 0.5 };
            let x = if pn.ends_with("center") { (sheet.page_w - tw) / 2.0 } else { sheet.page_w - sheet.margin_h - tw };
            d.page_number = Some(Label { text, x, baseline, marks, color: "#5f6368".into() });
        }
    }

    // Footnotes: a short rule, then « n. text » in 9 px Arial, wrapped on spaces with a 4-space
    // indent, stopping at the page's bottom edge.
    let notes = page_footnotes(page);
    if !notes.is_empty() {
        let max_w = sheet.page_w - 2.0 * sheet.margin_h;
        let y_top = sheet.page_h - sheet.margin_bottom + 4.0;
        d.footnote_rule = Some((sheet.margin_h, y_top + 0.5, sheet.margin_h + 150f32.min(max_w / 3.0)));
        let marks = arial(9.0 * 72.0 / 96.0);
        let mut y = y_top + 13.0;
        'notes: for (n, text, _) in notes {
            if y > sheet.page_h - 6.0 {
                break;
            }
            let body = if text.is_empty() { "…".to_string() } else { text };
            let mut line = format!("{n}. ");
            for w in body.split_whitespace() {
                let test = format!("{line}{w} ");
                if m.width(&test, &marks) > max_w && !line.trim().is_empty() {
                    d.footnotes.push(Label { text: line.clone(), x: sheet.margin_h, baseline: y, marks: marks.clone(), color: "#3c4043".into() });
                    y += 11.0;
                    line = format!("    {w} ");
                    if y > sheet.page_h - 6.0 {
                        break 'notes;
                    }
                } else {
                    line = test;
                }
            }
            if !line.trim().is_empty() && y <= sheet.page_h - 6.0 {
                d.footnotes.push(Label { text: line, x: sheet.margin_h, baseline: y, marks: marks.clone(), color: "#3c4043".into() });
                y += 11.0;
            }
        }
    }

    // Line numbers, right-aligned in the left margin, every `interval` lines.
    if let Some(ln) = root("lineNumbers").and_then(Value::as_object) {
        if !first_page_blank {
            let step = num(ln.get("interval"), 1.0).round().max(1.0) as usize;
            let mut n = if ln.get("mode").and_then(Value::as_str) == Some("continuous") { ctx.body_lines_before.get(pos.index).copied().unwrap_or(0) } else { 0 };
            let marks = arial(9.0);
            let right = (sheet.margin_h - 10.0).max(8.0);
            for p in &page.paragraphs {
                if suppressed(p.pm_start) {
                    continue;
                }
                for line in &p.lines {
                    if line.image.is_some() || line.cell_x.is_some() || line.phantom {
                        continue;
                    }
                    n += 1;
                    if n % step == 0 {
                        let text = n.to_string();
                        let tw = m.width(&text, &marks);
                        d.line_numbers.push(Label { text, x: right - tw, baseline: sheet.margin_v + line.baseline, marks: marks.clone(), color: "#9aa0a6".into() });
                    }
                }
            }
        }
    }

    // Shading and paragraph borders (« Bordures et trame »): the content width, 4 px out on each side,
    // 2 px above the first line and below the last; tables excluded.
    let content_w = sheet.page_w - sheet.margin_h - sheet.margin_right;
    for p in &page.paragraphs {
        if p.table.is_some() {
            continue;
        }
        let (Some(first), Some(last)) = (p.lines.first(), p.lines.last()) else { continue };
        let (shading, border) = para_style(p.pm_start);
        if shading.is_none() && border.is_none() {
            continue;
        }
        let (pad_x, pad_y) = (4.0, 2.0);
        let top = sheet.margin_v + first.y - pad_y;
        let height = (last.y + last.height + pad_y) - first.y + pad_y;
        let rect = (sheet.margin_h - pad_x, top, content_w + pad_x * 2.0, height);
        if let Some(s) = shading {
            d.para_fills.push((rect, s));
        }
        if let Some(b) = border.filter(|b| b.width > 0.0) {
            d.para_borders.push((rect, b));
        }
    }

    if let Some(pb) = root("pageBorder").and_then(Value::as_object) {
        d.border = Some(PageBorder {
            color: pb.get("color").and_then(Value::as_str).unwrap_or("#1a73e8").to_string(),
            width: num(pb.get("width"), 2.0) as f32,
            style: pb.get("style").and_then(Value::as_str).unwrap_or("solid").to_string(),
            margin: num(pb.get("margin"), 24.0) as f32,
        });
    }

    if let Some(wm) = root("watermark").and_then(Value::as_object) {
        let text = wm.get("text").and_then(Value::as_str).unwrap_or("").trim().to_string();
        if !text.is_empty() {
            let diagonal = wm.get("diagonal").and_then(Value::as_bool).unwrap_or(true);
            let family = wm.get("font").and_then(Value::as_str).filter(|f| !f.is_empty()).unwrap_or("Arial").to_string();
            let size_pt = num(wm.get("size"), 0.0) as f32;
            let px = if size_pt > 0.0 {
                size_pt * 96.0 / 72.0
            } else {
                let target = if diagonal { sheet.page_w.hypot(sheet.page_h) * 0.85 } else { sheet.page_w - 2.0 * sheet.margin_h };
                let probe = TextMark { bold: true, font_size: Some(100.0 * 72.0 / 96.0), font_family: Some(family.clone()), ..TextMark::default() };
                let w100 = m.width(&text, &probe).max(1.0);
                (target / w100 * 100.0).max(24.0)
            };
            d.watermark = Some(Watermark {
                text,
                color: wm.get("color").and_then(Value::as_str).unwrap_or("#bdbdbd").to_string(),
                opacity: (num(wm.get("opacity"), 0.45) as f32).clamp(0.0, 1.0),
                angle: if diagonal { -sheet.page_h.atan2(sheet.page_w) } else { 0.0 },
                marks: TextMark { bold: true, font_size: Some(px * 72.0 / 96.0), font_family: Some(family), ..TextMark::default() },
                cx: sheet.page_w / 2.0,
                cy: sheet.page_h / 2.0,
            });
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FixedMeasure;

    fn sheet() -> Sheet {
        Sheet { page_w: 794.0, page_h: 1123.0, margin_h: 96.0, margin_v: 96.0, margin_bottom: 96.0, margin_right: 96.0 }
    }

    #[test]
    fn page_numbers_are_formatted_like_the_web() {
        assert_eq!(format_page_number(4, "arabic"), "4");
        assert_eq!(format_page_number(14, "roman-lower"), "xiv");
        assert_eq!(format_page_number(1994, "roman-upper"), "MCMXCIV");
        assert_eq!(format_page_number(28, "alpha-lower"), "ab");
        assert_eq!(format_page_number(26, "alpha-upper"), "Z");
        assert_eq!(format_page_number(0, "arabic"), "1", "`if (n < 1) n = 1`");
    }

    #[test]
    fn header_fields_are_expanded_case_insensitively() {
        let doc: Node = serde_json::from_str(r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"{Titre} - page {PAGE} / {pages} - {date}"}]}]}"#).expect("doc");
        let out = expand_hf(&doc, 3, 12, "Rapport", "07/10/2026", "roman-lower");
        assert_eq!(out.children()[0].children()[0].text().as_deref(), Some("Rapport - page iii / xii - 07/10/2026"));
    }

    #[test]
    fn old_three_zone_headers_become_aligned_paragraphs() {
        let doc = hf_content(&serde_json::json!({ "l": "Gauche", "c": "", "r": "Droite" })).expect("doc");
        let aligns: Vec<_> = doc.children().iter().map(|p| p.attrs().get("textAlign").cloned()).collect();
        assert_eq!(aligns, [Some(serde_json::json!("left")), Some(serde_json::json!("right"))]);
        assert!(hf_content(&serde_json::json!("")).is_none());
        assert!(hf_is_empty(&hf_content(&serde_json::json!({"type":"doc","content":[{"type":"paragraph"}]})).expect("doc")));
    }

    #[test]
    fn the_page_number_sits_where_draw_page_decorations_puts_it() {
        let mut ctx = DecorContext::default();
        ctx.root.insert("pageNumbers".into(), serde_json::json!("footer-right"));
        ctx.root.insert("pageNumStart".into(), serde_json::json!(5));
        let pos = PagePos { index: 1, count: 3, section: 0 };
        let d = page_decor(&PageLayout::default(), pos, &sheet(), &ctx, &|_| false, &|_| (None, None), &FixedMeasure);
        let label = d.page_number.expect("a number");
        assert_eq!(label.text, "6");
        assert_eq!(label.baseline, 1123.0 - 48.0);
        let tw = FixedMeasure.width("6", &label.marks);
        assert!((label.x - (794.0 - 96.0 - tw)).abs() < 1e-3);
        // « Première page différente »: nothing on the first page.
        ctx.root.insert("hfFirstPage".into(), serde_json::json!(true));
        let d = page_decor(&PageLayout::default(), PagePos { index: 0, ..pos }, &sheet(), &ctx, &|_| false, &|_| (None, None), &FixedMeasure);
        assert!(d.page_number.is_none());
    }

    #[test]
    fn a_header_is_laid_out_in_its_band_and_an_unlinked_section_has_its_own() {
        let mut ctx = DecorContext::default();
        ctx.root.insert("header".into(), serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Doc {page}"}]}]}));
        ctx.sections = vec![SectionMeta { hf_linked: true, ..Default::default() }, SectionMeta { hf_linked: false, header: hf_content(&serde_json::json!("Annexe")), ..Default::default() }];
        let d = page_decor(&PageLayout::default(), PagePos { index: 0, count: 2, section: 0 }, &sheet(), &ctx, &|_| false, &|_| (None, None), &FixedMeasure);
        let band = d.header.expect("header");
        assert_eq!((band.x, band.y, band.alpha), (96.0, 96.0 * 0.28, 0.45));
        let text: String = band.paragraphs[0].lines[0].spans.iter().map(|s| &*s.text).collect();
        assert_eq!(text, "Doc 1");
        let d = page_decor(&PageLayout::default(), PagePos { index: 1, count: 2, section: 1 }, &sheet(), &ctx, &|_| false, &|_| (None, None), &FixedMeasure);
        let text: String = d.header.expect("own header").paragraphs[0].lines[0].spans.iter().map(|s| &*s.text).collect();
        assert_eq!(text, "Annexe");
        assert!(d.footer.is_none());
    }

    #[test]
    fn border_and_watermark_follow_paint_page_border_and_paint_watermark() {
        let mut ctx = DecorContext::default();
        ctx.root.insert("pageBorder".into(), serde_json::json!({"color":"#1a73e8","width":2,"style":"double","margin":24}));
        ctx.root.insert("watermark".into(), serde_json::json!({"text":"CONFIDENTIEL","color":"#bdbdbd","opacity":0.45,"diagonal":true,"size":0}));
        ctx.root.insert("pageColor".into(), serde_json::json!("#fff8e1"));
        let d = page_decor(&PageLayout::default(), PagePos { index: 0, count: 1, section: 0 }, &sheet(), &ctx, &|_| false, &|_| (None, None), &FixedMeasure);
        let b = d.border.expect("border");
        assert_eq!(b.rects(794.0, 1123.0), vec![(24.0, 24.0, 746.0, 1075.0), (28.0, 28.0, 738.0, 1067.0)]);
        let w = d.watermark.expect("watermark");
        assert!((w.angle + 1123f32.atan2(794.0)).abs() < 1e-6);
        assert_eq!((w.cx, w.cy), (397.0, 561.5));
        // Auto size: 85 % of the diagonal for the text's width.
        let width = FixedMeasure.width("CONFIDENTIEL", &w.marks);
        assert!((width - 794f32.hypot(1123.0) * 0.85).abs() < 1.0, "{width}");
        assert_eq!(d.background.as_deref(), Some("#fff8e1"));
    }

    #[test]
    fn a_shaded_and_bordered_paragraph_gets_its_box() {
        let doc: Node = serde_json::from_str(r##"{"type":"doc","content":[{"type":"paragraph","attrs":{"shading":"#fff2cc","paraBorder":{"color":"#9aa0a6","width":1,"style":"solid"}},"content":[{"type":"text","text":"Encadre"}]}]}"##).expect("doc");
        let (paragraphs, _) = layout_paragraphs(&parse_doc(&doc), &|_| 602.0, &FixedMeasure);
        let first = paragraphs[0].lines[0].clone();
        let page = PageLayout { paragraphs, ..Default::default() };
        let attrs = doc.children()[0].attrs();
        let d = page_decor(&page, PagePos { index: 0, count: 1, section: 0 }, &sheet(), &DecorContext::default(), &|_| false, &|_| para_style_of(&attrs), &FixedMeasure);
        let ((x, y, w, h), color) = d.para_fills[0].clone();
        assert_eq!(color, "#fff2cc");
        assert_eq!((x, w), (92.0, 602.0 + 8.0));
        assert!((y - (96.0 + first.y - 2.0)).abs() < 1e-3);
        assert!((h - (first.height + 4.0)).abs() < 1e-3);
        assert_eq!(d.para_borders[0].1.style, "solid");
    }

    #[test]
    fn footnotes_are_wrapped_under_a_short_rule() {
        let mut ctx = DecorContext::default();
        ctx.root.insert("x".into(), Value::Null);
        let doc: Node = serde_json::from_str(r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Texte"},{"type":"footnote","attrs":{"text":"Une note assez longue pour tenir sur deux lignes de la marge du bas de cette page de test"}}]}]}"#).expect("doc");
        let (paragraphs, _) = layout_paragraphs(&parse_doc(&doc), &|_| 602.0, &FixedMeasure);
        let page = PageLayout { paragraphs, ..Default::default() };
        let narrow = Sheet { page_w: 400.0, ..sheet() };
        let d = page_decor(&page, PagePos { index: 0, count: 1, section: 0 }, &narrow, &ctx, &|_| false, &|_| (None, None), &FixedMeasure);
        let (x0, y, x1) = d.footnote_rule.expect("rule");
        assert_eq!((x0, y), (96.0, 1123.0 - 96.0 + 4.5));
        assert!((x1 - (96.0 + (400.0f32 - 192.0) / 3.0)).abs() < 1e-3);
        assert!(d.footnotes.len() >= 2, "{:?}", d.footnotes);
        assert!(d.footnotes[0].text.starts_with("1. Une"));
        assert!(d.footnotes[1].text.starts_with("    "));
        assert_eq!(d.footnotes[1].baseline - d.footnotes[0].baseline, 11.0);
    }
}
