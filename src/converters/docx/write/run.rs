//! `<w:r>` and `<w:rPr>` — runs, character formatting, inline content.

use serde_json::Value;

use crate::converters::docx::xml::{escape_xml_attr, text_to_run_children};
use crate::converters::types::{PmMark, PmNode};

use super::ctx::ExportCtx;
use super::drawing::render_image;
use super::revisions::{self, Revision};
use super::{comments, notes};

/// Inline content of a paragraph.
///
/// Consecutive nodes carrying the SAME `link` mark are wrapped in a single
/// `<w:hyperlink>`: the hyperlink is the element, not the run, so emitting one
/// per run would split a single link into as many links as it has formatting
/// changes (cf. LibreOffice docxattributeoutput.cxx:4267-4295). Tracked changes
/// are grouped the same way, one level FURTHER IN — see
/// `render_revision_groups`.
pub(crate) fn render_inline_content(nodes: &[PmNode], ctx: &mut ExportCtx) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < nodes.len() {
        // Bookmarks and comments are RANGES: their markers are children of the
        // paragraph, never of `w:hyperlink`, so they are emitted before the
        // group is wrapped.
        let marks: &[PmMark] = nodes[i].marks.as_deref().unwrap_or(&[]);
        let (bms, cmts) = ranges_of(marks);
        out.push_str(&close_ranges(&bms, &cmts, ctx));
        out.push_str(&open_ranges(&bms, &cmts, ctx));
        match node_href(&nodes[i]) {
            Some(href) => {
                // Extend the group over every following node with the same target.
                let mut j = i + 1;
                while j < nodes.len() && node_href(&nodes[j]).as_deref() == Some(href.as_str()) {
                    j += 1;
                }
                let inner = render_revision_groups(&nodes[i..j], ctx);
                // An empty `<w:hyperlink>` is pointless and, with a dangling
                // r:id, is exactly what makes Word offer to repair the file.
                if !inner.is_empty() {
                    out.push_str(&render_hyperlink(&href, inner, ctx));
                }
                i = j;
            }
            None => {
                // Extend over the link-less neighbours so that a tracked change
                // spanning several runs still yields ONE `w:ins` / `w:del`. The
                // group stops as soon as the open ranges would differ, since
                // their markers are emitted once per group, above.
                let mut j = i + 1;
                while j < nodes.len() && node_href(&nodes[j]).is_none() {
                    let (b, c) = ranges_of(nodes[j].marks.as_deref().unwrap_or(&[]));
                    if b != bms || c != cmts {
                        break;
                    }
                    j += 1;
                }
                out.push_str(&render_revision_groups(&nodes[i..j], ctx));
                i = j;
            }
        }
    }
    out
}

/// Consecutive nodes belonging to the SAME tracked change wrapped in a single
/// `<w:ins>` / `<w:del>`, exactly as `render_inline_content` groups a hyperlink:
/// the revision is the element, not the run, so one element per run would turn a
/// single insertion into as many separate changes as it has formatting splits.
///
/// NESTING: `w:hyperlink` stays OUTSIDE. `CT_RunTrackChange` (the type of
/// `w:ins`/`w:del`) accepts `EG_ContentRunContent`, which has no `hyperlink`,
/// whereas `CT_Hyperlink` accepts `EG_PContent` — which does reach `w:ins`
/// through `EG_RunLevelElts`. So `<w:hyperlink><w:ins><w:r/></w:ins></w:hyperlink>`
/// is the only legal order of the two.
///
/// A group with no revision at all goes through untouched: `revisions::wrap`
/// returns its input for an empty slice, so one loop covers both cases.
fn render_revision_groups(nodes: &[PmNode], ctx: &mut ExportCtx) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < nodes.len() {
        // `max(1)` only guards against a zero-length group: an empty one would
        // spin here forever.
        let len = revisions::group_len(nodes, i).max(1);
        let revs = revisions::node_revisions(&nodes[i]);
        let mut inner = String::new();
        for node in &nodes[i..(i + len).min(nodes.len())] {
            inner.push_str(&render_inline_node(node, ctx));
        }
        // An empty `w:ins` is a change over nothing, which readers report as a
        // damaged file; `wrap` drops it for us.
        out.push_str(&revisions::wrap(&revs, &inner, ctx));
        i += len;
    }
    out
}

/// One inline node, without any hyperlink wrapping.
fn render_inline_node(node: &PmNode, ctx: &mut ExportCtx) -> String {
    let marks = node.marks.as_deref().unwrap_or(&[]);
    match node.node_type.as_str() {
        "text" => match &node.text {
            Some(text) => render_run(text, marks),
            None => String::new(),
        },
        // A break keeps the character formatting of its neighbours, otherwise
        // the empty line collapses to the default size.
        "hardBreak" => format!("<w:r>{}<w:br/></w:r>", render_rpr(marks)),
        "inlineImage" | "image" => render_image(node, ctx),
        // An inline atom carrying its own text: the text goes to
        // `footnotes.xml`, only the reference stays in the body.
        "footnote" => {
            let text = node
                .attrs
                .as_ref()
                .and_then(|a| a.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            notes::render_footnote_ref(text, ctx)
        }
        // Same shape, its own numbering sequence and its own part.
        "endnote" => {
            let text = node
                .attrs
                .as_ref()
                .and_then(|a| a.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            notes::render_endnote_ref(text, ctx)
        }
        _ => String::new(),
    }
}

/// Bookmark names and comment ids carried by an inline node.
fn ranges_of(marks: &[PmMark]) -> (Vec<String>, Vec<String>) {
    let mut bms = Vec::new();
    let mut cmts = Vec::new();
    for m in marks {
        let attr = |k: &str| {
            m.attrs
                .as_ref()
                .and_then(|a| a.get(k))
                .and_then(|v| v.as_str())
        };
        match m.mark_type.as_str() {
            // `bookmark_ref` is REUSED here on purpose: the anchor of an
            // internal link and the bookmark itself must be sanitised the same
            // way, or the link points nowhere.
            "bookmark" => {
                if let Some(n) = attr("name") {
                    bms.push(bookmark_ref(n));
                }
            }
            "comment" => {
                if let Some(id) = attr("commentId") {
                    cmts.push(id.to_string());
                }
            }
            _ => {}
        }
    }
    (bms, cmts)
}

/// Close the ranges this node leaves. A bookmark or a comment range may span
/// paragraphs, so only what the node no longer carries is closed.
fn close_ranges(bms: &[String], cmts: &[String], ctx: &mut ExportCtx) -> String {
    let mut out = String::new();
    let stale: Vec<(String, u32)> = ctx
        .open_bookmarks
        .iter()
        .filter(|(n, _)| !bms.contains(n))
        .cloned()
        .collect();
    for (name, id) in stale {
        out.push_str(&format!(r#"<w:bookmarkEnd w:id="{id}"/>"#));
        ctx.open_bookmarks.retain(|(n, _)| n != &name);
    }
    let stale_c: Vec<String> = ctx
        .open_comments
        .iter()
        .filter(|c| !cmts.contains(c))
        .cloned()
        .collect();
    for cid in stale_c {
        // Every id of the thread — root and replies — closes on the same range.
        for id in ctx.comment_ids.get(&cid).cloned().unwrap_or_default() {
            out.push_str(&comments::render_range_end(id));
        }
        ctx.open_comments.retain(|c| c != &cid);
    }
    out
}

fn open_ranges(bms: &[String], cmts: &[String], ctx: &mut ExportCtx) -> String {
    let mut out = String::new();
    for name in bms {
        if !ctx.open_bookmarks.iter().any(|(n, _)| n == name) {
            let id = ctx.next_mark_id();
            out.push_str(&format!(
                r#"<w:bookmarkStart w:id="{id}" w:name="{}"/>"#,
                escape_xml_attr(name)
            ));
            ctx.open_bookmarks.push((name.clone(), id));
        }
    }
    for cid in cmts {
        if !ctx.open_comments.contains(cid) {
            if let Some(ids) = comments::register_thread(ctx, cid) {
                for id in ids {
                    out.push_str(&format!(r#"<w:commentRangeStart w:id="{id}"/>"#));
                }
                ctx.open_comments.push(cid.clone());
            }
        }
    }
    out
}

/// Force every range still open closed: an unclosed `bookmarkStart` makes Word
/// repair the file, so the body must never end with one.
pub(crate) fn close_open_ranges(ctx: &mut ExportCtx) -> String {
    let mut out = String::new();
    for (_, id) in std::mem::take(&mut ctx.open_bookmarks) {
        out.push_str(&format!(r#"<w:bookmarkEnd w:id="{id}"/>"#));
    }
    for cid in std::mem::take(&mut ctx.open_comments) {
        for id in ctx.comment_ids.get(&cid).cloned().unwrap_or_default() {
            out.push_str(&comments::render_range_end(id));
        }
    }
    out
}

/// `<w:hyperlink>` around already-serialised runs. An `#anchor` href targets a
/// bookmark inside the document (`w:anchor`), anything else is an external
/// relationship (`r:id`), created once per URL by `ExportCtx::hyperlink_rid`.
fn render_hyperlink(href: &str, inner: String, ctx: &mut ExportCtx) -> String {
    match href.strip_prefix('#') {
        Some(anchor) => {
            let name = bookmark_ref(anchor);
            if name.is_empty() {
                return inner;
            }
            format!(
                r#"<w:hyperlink w:anchor="{}">{inner}</w:hyperlink>"#,
                escape_xml_attr(&name)
            )
        }
        None => {
            let rid = ctx.hyperlink_rid(href);
            format!(r#"<w:hyperlink r:id="{rid}" w:history="1">{inner}</w:hyperlink>"#)
        }
    }
}

/// Target of the `link` mark of a node, if any.
fn node_href(node: &PmNode) -> Option<String> {
    link_href(node.marks.as_deref().unwrap_or(&[]))
}

/// Target of a `link` mark, ignoring empty hrefs.
fn link_href(marks: &[PmMark]) -> Option<String> {
    let href = marks
        .iter()
        .find(|m| m.mark_type == "link")?
        .attrs
        .as_ref()?
        .get("href")?
        .as_str()?
        .trim();
    (!href.is_empty()).then(|| href.to_string())
}

/// Word forbids whitespace in bookmark names and truncates them at 40
/// characters (sw/source/filter/ww8/wrtww8.cxx:4624-4646). An anchor pointing
/// at a name Word cannot hold is a dead link, so apply the same rules here.
fn bookmark_ref(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| if c.is_whitespace() { '_' } else { c })
        .collect();
    cleaned.chars().take(40).collect()
}

/// One `<w:r>`. Inside a deletion the character data MUST go to `w:delText`,
/// not `w:t`: a `w:t` under `w:del` is read back as ordinary text, so accepting
/// the change would leave the deleted words in the document (ECMA-376
/// §17.3.3.7, and LibreOffice docxattributeoutput.cxx:1521 which switches the
/// tag on `m_bInRedlineDelete`). The deletion mark travels with the run's own
/// marks, so this needs no extra parameter.
pub(crate) fn render_run(text: &str, marks: &[PmMark]) -> String {
    let rpr = render_rpr(marks);
    if revisions::revisions_of(marks).iter().any(Revision::is_deletion) {
        return revisions::render_del_run(&rpr, text);
    }
    format!("<w:r>{rpr}{}</w:r>", text_to_run_children(text))
}

/// Character formatting gathered from the editor marks, before serialisation.
#[derive(Default)]
struct RunStyle {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    double_strike: bool,
    code: bool,
    link: bool,
    small_caps: bool,
    all_caps: bool,
    hidden: bool,
    /// `w:vertAlign` value.
    script: Option<&'static str>,
    /// "RRGGBB", no leading `#`.
    color: Option<String>,
    highlight: Option<String>,
    font_family: Option<String>,
    /// Font size in points, needed on its own to resolve `w:position`.
    font_pt: Option<f64>,
    /// Character spacing in points (positive = expanded).
    spacing_pt: Option<f64>,
    /// Horizontal scaling in percent (`w:w`).
    scale_pct: Option<i64>,
    /// Vertical offset as a percentage of the font size (`w:position`).
    position_pct: Option<f64>,
    underline_style: Option<String>,
    underline_color: Option<String>,
    underline_words: bool,
}

fn collect_style(marks: &[PmMark]) -> RunStyle {
    let mut s = RunStyle::default();
    for mark in marks {
        match mark.mark_type.as_str() {
            "bold" => s.bold = true,
            "italic" => s.italic = true,
            "underline" => s.underline = true,
            "strike" => s.strike = true,
            "code" => s.code = true,
            "link" => s.link = true,
            "superscript" => s.script = Some("superscript"),
            "subscript" => s.script = Some("subscript"),
            "highlight" => {
                s.highlight = mark
                    .attrs
                    .as_ref()
                    .and_then(|a| a.get("color"))
                    .and_then(|c| c.as_str())
                    .and_then(hex6);
            }
            "textStyle" => {
                let Some(a) = mark.attrs.as_ref() else { continue };
                if let Some(c) = a.get("color").and_then(|v| v.as_str()) {
                    s.color = hex6(c);
                }
                if let Some(f) = a.get("fontFamily").and_then(|v| v.as_str()) {
                    let f = f.trim().trim_matches(['"', '\'']).trim();
                    if !f.is_empty() {
                        s.font_family = Some(f.to_string());
                    }
                }
                // `fontSize` is a NUMBER on import but a CSS string ("9pt") in
                // the frontend templates: both forms must be accepted.
                if let Some(pt) = a.get("fontSize").and_then(css_pt) {
                    if pt > 0.0 {
                        s.font_pt = Some(pt);
                    }
                }
                if let Some(pt) = a.get("letterSpacing").and_then(css_pt) {
                    s.spacing_pt = Some(pt);
                }
                if let Some(p) = a.get("charScale").and_then(css_pt) {
                    s.scale_pct = Some(p.round() as i64);
                }
                if let Some(p) = a.get("charPosition").and_then(css_pt) {
                    s.position_pct = Some(p);
                }
                if flag(a, "smallCaps") {
                    s.small_caps = true;
                }
                if flag(a, "allCaps") {
                    s.all_caps = true;
                }
                if flag(a, "hidden") {
                    s.hidden = true;
                }
                if flag(a, "doubleStrike") {
                    s.double_strike = true;
                }
                if flag(a, "underlineWords") {
                    s.underline_words = true;
                }
                if let Some(u) = a.get("underlineStyle").and_then(|v| v.as_str()) {
                    s.underline_style = Some(underline_val(u).to_string());
                }
                if let Some(c) = a.get("underlineColor").and_then(|v| v.as_str()) {
                    s.underline_color = hex6(c);
                }
            }
            _ => {}
        }
    }
    s
}

/// `<w:rPr>` for a set of marks. The child ORDER is imposed by the `CT_RPr`
/// sequence (EG_RPrBase, ooxml/model.xml:13712-13830): Word reports
/// "unreadable content" on a document whose `rPr` children are out of order,
/// which is why LibreOffice re-sorts them at serialisation time
/// (docxattributeoutput.cxx:3426-3486). Do not reorder the blocks below.
pub(crate) fn render_rpr(marks: &[PmMark]) -> String {
    let s = collect_style(marks);
    let mut rpr = String::new();

    // 1. rStyle
    if s.code {
        rpr.push_str(r#"<w:rStyle w:val="CodeChar"/>"#);
    } else if s.link {
        rpr.push_str(r#"<w:rStyle w:val="Hyperlink"/>"#);
    }
    // 2. rFonts — Word applies one family per script; use it for all of them.
    if let Some(f) = &s.font_family {
        let f = escape_xml_attr(f);
        rpr.push_str(&format!(
            r#"<w:rFonts w:ascii="{f}" w:hAnsi="{f}" w:eastAsia="{f}" w:cs="{f}"/>"#
        ));
    }
    // 3-4. b/bCs, i/iCs — the Cs twins carry the complex-script variant.
    if s.bold {
        rpr.push_str("<w:b/><w:bCs/>");
    }
    if s.italic {
        rpr.push_str("<w:i/><w:iCs/>");
    }
    // 5-6. caps, smallCaps
    if s.all_caps {
        rpr.push_str("<w:caps/>");
    }
    if s.small_caps {
        rpr.push_str("<w:smallCaps/>");
    }
    // 7-8. strike, dstrike — mutually exclusive in Word.
    if s.double_strike {
        rpr.push_str("<w:dstrike/>");
    } else if s.strike {
        rpr.push_str("<w:strike/>");
    }
    // 9. vanish
    if s.hidden {
        rpr.push_str("<w:vanish/>");
    }
    // 10. color — resolved RGB only: we write no theme1.xml, so a w:themeColor
    // would be a dangling reference.
    if let Some(c) = &s.color {
        rpr.push_str(&format!(r#"<w:color w:val="{c}"/>"#));
    }
    // 11. spacing — ST_SignedTwipsMeasure (1 pt = 20 twips), NOT half-points.
    // Word ignores values from 0x8000 up (legacy 16-bit handling,
    // ConversionHelper.cxx:423-430), so drop them rather than emit garbage.
    if let Some(tw) = s.spacing_pt.map(|pt| (pt * 20.0).round() as i64) {
        if tw != 0 && tw.unsigned_abs() < 0x8000 {
            rpr.push_str(&format!(r#"<w:spacing w:val="{tw}"/>"#));
        }
    }
    // 12. w — horizontal scaling in percent, valid range 1..600.
    if let Some(p) = s.scale_pct {
        if (1..=600).contains(&p) && p != 100 {
            rpr.push_str(&format!(r#"<w:w w:val="{p}"/>"#));
        }
    }
    // 13. position — signed half-points relative to the baseline, whereas the
    // model stores a percentage of the font size (DomainMapper.cxx:3997-4021).
    if let Some(pct) = s.position_pct {
        let hp = (pct / 100.0 * s.font_pt.unwrap_or(DEFAULT_PT) * 2.0).round() as i64;
        if hp != 0 {
            rpr.push_str(&format!(r#"<w:position w:val="{hp}"/>"#));
        }
    }
    // 14. sz / szCs — half-points (DomainMapper.cxx:2407). Word always writes
    // both; szCs is the separate complex-script size.
    if let Some(hp) = s.font_pt.map(|pt| (pt * 2.0).round() as i64) {
        if hp > 0 {
            rpr.push_str(&format!(r#"<w:sz w:val="{hp}"/><w:szCs w:val="{hp}"/>"#));
        }
    }
    // 15. highlight — only for an EXACT match against one of the 16 named
    // colours; anything else falls through to w:shd below, as LibreOffice does
    // (docxattributeoutput.cxx:8758).
    let named = s.highlight.as_deref().and_then(highlight_name);
    if let Some(n) = named {
        rpr.push_str(&format!(r#"<w:highlight w:val="{n}"/>"#));
    }
    // 16. u
    if s.underline {
        let style = match (s.underline_style.as_deref(), s.underline_words) {
            (Some(st), _) if st != "single" => st,
            (_, true) => "words",
            _ => "single",
        };
        match &s.underline_color {
            Some(c) => rpr.push_str(&format!(r#"<w:u w:val="{style}" w:color="{c}"/>"#)),
            None => rpr.push_str(&format!(r#"<w:u w:val="{style}"/>"#)),
        }
    }
    // 17. shd — arbitrary character shading, the fallback for a highlight
    // colour w:highlight cannot express. `w:color="auto"` means black here,
    // `w:fill="auto"` would mean white (CellColorHandler.cxx:110-125).
    if named.is_none() {
        if let Some(h) = &s.highlight {
            rpr.push_str(&format!(
                r#"<w:shd w:val="clear" w:color="auto" w:fill="{h}"/>"#
            ));
        }
    }
    // 18. vertAlign
    if let Some(v) = s.script {
        rpr.push_str(&format!(r#"<w:vertAlign w:val="{v}"/>"#));
    }

    if rpr.is_empty() {
        String::new()
    } else {
        format!("<w:rPr>{rpr}</w:rPr>")
    }
}

/// Fallback font size, used only to turn a relative `w:position` into the
/// half-points Word expects when the run carries no explicit size.
const DEFAULT_PT: f64 = 11.0;

/// A boolean textStyle attribute, tolerating the `true` / `1` / `"true"` forms
/// JSON round-trips can produce.
fn flag(attrs: &Value, key: &str) -> bool {
    match attrs.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|v| v != 0.0),
        Some(Value::String(s)) => !s.is_empty() && !s.eq_ignore_ascii_case("false"),
        _ => false,
    }
}

/// A length from the model — a raw number or a CSS string such as "11pt" — in
/// points. The unit suffix is dropped rather than converted, matching the
/// frontend which reads these attributes with `parseFloat`.
fn css_pt(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s
            .trim()
            .trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '%')
            .trim()
            .parse::<f64>()
            .ok(),
        _ => None,
    }
}

/// A CSS colour from the editor -> "RRGGBB" (upper case, no `#`), `None` when
/// it cannot be represented (`transparent`, `inherit`, a named colour we do not
/// know): callers then simply omit the property instead of writing a wrong one.
fn hex6(css: &str) -> Option<String> {
    let s = css.trim();
    if let Some(h) = s.strip_prefix('#') {
        let digits: String = h.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        return match digits.len() {
            // #RGB and #RGBA expand each digit; the alpha channel is dropped,
            // DOCX character colours have none.
            3 | 4 => Some(
                digits
                    .chars()
                    .take(3)
                    .flat_map(|c| [c, c])
                    .collect::<String>()
                    .to_ascii_uppercase(),
            ),
            6 | 8 => Some(digits[..6].to_ascii_uppercase()),
            _ => None,
        };
    }
    if let Some(inner) = s
        .strip_prefix("rgba(")
        .or_else(|| s.strip_prefix("rgb("))
        .and_then(|v| v.strip_suffix(')'))
    {
        let mut comps = inner
            .split([',', '/', ' '])
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.parse::<f64>().ok())
            .map(|v| v.clamp(0.0, 255.0).round() as u8);
        let r = comps.next()?;
        let g = comps.next()?;
        let b = comps.next()?;
        return Some(format!("{r:02X}{g:02X}{b:02X}"));
    }
    named_css_color(&s.to_ascii_lowercase()).map(str::to_string)
}

/// The handful of CSS colour names the editor actually produces.
fn named_css_color(name: &str) -> Option<&'static str> {
    Some(match name {
        "black" => "000000",
        "white" => "FFFFFF",
        "red" => "FF0000",
        "lime" => "00FF00",
        "green" => "008000",
        "blue" => "0000FF",
        "yellow" => "FFFF00",
        "cyan" | "aqua" => "00FFFF",
        "magenta" | "fuchsia" => "FF00FF",
        "gray" | "grey" => "808080",
        "silver" => "C0C0C0",
        "navy" => "000080",
        "teal" => "008080",
        "olive" => "808000",
        "purple" => "800080",
        "maroon" => "800000",
        "orange" => "FFA500",
        _ => return None,
    })
}

/// "RRGGBB" -> `w:highlight` named colour, or `None` when no exact match
/// exists. These are Word's 16 SATURATED colours, not pastels — same table as
/// LibreOffice `DomainMapper::getColorFromId` (DomainMapper.cxx:5196-5212).
fn highlight_name(hex: &str) -> Option<&'static str> {
    Some(match hex.to_ascii_uppercase().as_str() {
        "000000" => "black",
        "0000FF" => "blue",
        "00FFFF" => "cyan",
        "00FF00" => "green",
        "FF00FF" => "magenta",
        "FF0000" => "red",
        "FFFF00" => "yellow",
        "FFFFFF" => "white",
        "000080" => "darkBlue",
        "008080" => "darkCyan",
        "008000" => "darkGreen",
        "800080" => "darkMagenta",
        "800000" => "darkRed",
        "808000" => "darkYellow",
        "808080" => "darkGray",
        "C0C0C0" => "lightGray",
        _ => return None,
    })
}

/// Editor underline style -> `w:u/@w:val` (ST_Underline). Do NOT copy the
/// LibreOffice bug that maps a long dash to "dashLongHeavy"
/// (docxattributeoutput.cxx:8483).
fn underline_val(style: &str) -> &str {
    match style {
        "double" | "thick" | "dotted" | "dottedHeavy" | "dash" | "dashedHeavy" | "dashLong"
        | "dashLongHeavy" | "dotDash" | "dashDotHeavy" | "dotDotDash" | "dashDotDotHeavy"
        | "wave" | "wavyHeavy" | "wavyDouble" | "words" => style,
        _ => "single",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// `EG_RPrBase` order (ooxml/model.xml:13712-13830), restricted to what we
    /// emit. Word offers to repair a file whose `rPr` children deviate from it.
    const RPR_ORDER: &[&str] = &[
        "w:rStyle", "w:rFonts", "w:b", "w:bCs", "w:i", "w:iCs", "w:caps", "w:smallCaps",
        "w:strike", "w:dstrike", "w:vanish", "w:color", "w:spacing", "w:w", "w:position",
        "w:sz", "w:szCs", "w:highlight", "w:u", "w:shd", "w:vertAlign",
    ];

    /// Element names of an XML fragment, in document order.
    fn tags(xml: &str) -> Vec<String> {
        xml.split('<')
            .skip(1)
            .filter(|s| !s.starts_with('/'))
            .map(|s| {
                s.split([' ', '/', '>'])
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect()
    }

    fn mark(t: &str, attrs: Option<Value>) -> PmMark {
        PmMark { mark_type: t.into(), attrs }
    }

    fn rich_marks() -> Vec<PmMark> {
        vec![
            mark("bold", None),
            mark("italic", None),
            mark("strike", None),
            mark("underline", None),
            mark("superscript", None),
            mark("highlight", Some(json!({ "color": "#FFFF00" }))),
            mark(
                "textStyle",
                Some(json!({
                    "color": "#1155CC",
                    "fontFamily": "Georgia",
                    "fontSize": "14pt",
                    "letterSpacing": 1.5,
                    "smallCaps": true,
                    "allCaps": true,
                    "hidden": true,
                    "charScale": 150,
                    "charPosition": 33,
                    "underlineStyle": "wave",
                    "underlineColor": "#FF0000"
                })),
            ),
        ]
    }

    #[test]
    fn rpr_children_follow_the_schema_order() {
        let xml = render_rpr(&rich_marks());
        let mut idx = 0usize;
        for tag in tags(&xml).into_iter().filter(|t| t != "w:rPr") {
            let pos = RPR_ORDER
                .iter()
                .position(|p| *p == tag)
                .unwrap_or_else(|| panic!("balise inattendue dans rPr: {tag}"));
            assert!(pos >= idx, "ordre CT_RPr violé sur {tag} dans {xml}");
            idx = pos;
        }
    }

    #[test]
    fn rpr_carries_every_character_property() {
        let xml = render_rpr(&rich_marks());
        for expected in [
            r#"<w:rFonts w:ascii="Georgia""#,
            "<w:b/><w:bCs/>",
            "<w:i/><w:iCs/>",
            "<w:caps/>",
            "<w:smallCaps/>",
            "<w:strike/>",
            "<w:vanish/>",
            r#"<w:color w:val="1155CC"/>"#,
            r#"<w:spacing w:val="30"/>"#,      // 1.5 pt = 30 twips
            r#"<w:w w:val="150"/>"#,
            r#"<w:position w:val="9"/>"#,      // 33 % of 14 pt, in half-points
            r#"<w:sz w:val="28"/><w:szCs w:val="28"/>"#,
            r#"<w:highlight w:val="yellow"/>"#,
            r#"<w:u w:val="wave" w:color="FF0000"/>"#,
            r#"<w:vertAlign w:val="superscript"/>"#,
        ] {
            assert!(xml.contains(expected), "manque {expected} dans {xml}");
        }
    }

    /// A colour outside Word's 16 named ones must degrade to `w:shd`, never to
    /// a wrong `w:highlight`.
    #[test]
    fn unnamed_highlight_falls_back_to_shading() {
        let xml = render_rpr(&[mark("highlight", Some(json!({ "color": "rgb(18, 52, 86)" })))]);
        assert!(!xml.contains("w:highlight"), "{xml}");
        assert!(xml.contains(r#"<w:shd w:val="clear" w:color="auto" w:fill="123456"/>"#), "{xml}");
    }

    #[test]
    fn consecutive_runs_share_one_hyperlink() {
        let link = mark("link", Some(json!({ "href": "https://kubuno.com/a b" })));
        let nodes = vec![
            PmNode::text("Kub", vec![link.clone()]),
            PmNode::text("uno", vec![link.clone(), mark("bold", None)]),
            PmNode::text(" hors lien", vec![]),
        ];
        let mut ctx = ExportCtx::new();
        let xml = render_inline_content(&nodes, &mut ctx);
        assert_eq!(xml.matches("<w:hyperlink").count(), 1, "{xml}");
        assert_eq!(xml.matches("<w:r>").count(), 3, "{xml}");
        assert_eq!(ctx.rels.len(), 1, "la relation doit être créée une seule fois");
        // The target is escaped by the rels writer, not mangled here.
        assert!(ctx.doc_rels_xml().contains(r#"Target="https://kubuno.com/a b" TargetMode="External""#));
    }

    #[test]
    fn anchor_links_use_w_anchor_and_a_legal_bookmark_name() {
        let nodes = vec![PmNode::text(
            "voir",
            vec![mark("link", Some(json!({ "href": "#chapitre 1" })))],
        )];
        let mut ctx = ExportCtx::new();
        let xml = render_inline_content(&nodes, &mut ctx);
        assert!(xml.contains(r#"<w:hyperlink w:anchor="chapitre_1">"#), "{xml}");
        assert!(ctx.rels.is_empty(), "un lien interne ne crée pas de relation");
    }

    fn rev_mark(kind: &str, id: &str) -> PmMark {
        mark(
            kind,
            Some(json!({
                "author": "Jean Dupont",
                "authorId": "u1",
                "date": "2026-07-28T09:12:00Z",
                "id": id
            })),
        )
    }

    /// A change split by formatting is ONE change, exactly like a hyperlink.
    #[test]
    fn consecutive_runs_share_one_insertion() {
        let ins = rev_mark("insertion", "r1");
        let nodes = vec![
            PmNode::text("ajou", vec![ins.clone()]),
            PmNode::text("té", vec![ins.clone(), mark("bold", None)]),
            PmNode::text(" intact", vec![]),
            PmNode::text("autre", vec![rev_mark("insertion", "r2")]),
        ];
        let mut ctx = ExportCtx::new();
        let xml = render_inline_content(&nodes, &mut ctx);
        assert_eq!(xml.matches("<w:ins ").count(), 2, "{xml}");
        assert_eq!(xml.matches("<w:r>").count(), 4, "{xml}");
        assert!(xml.contains(r#"w:author="Jean Dupont""#), "{xml}");
    }

    /// The whole point of `w:del`: the text must NOT come back as normal text.
    #[test]
    fn deleted_text_uses_del_text() {
        let del = rev_mark("deletion", "r3");
        let nodes = vec![PmNode::text("supprimé\tsuite", vec![del])];
        let mut ctx = ExportCtx::new();
        let xml = render_inline_content(&nodes, &mut ctx);
        assert!(xml.contains("<w:del "), "{xml}");
        assert!(xml.contains("<w:delText"), "{xml}");
        assert!(!xml.contains("<w:t "), "un w:t dans un w:del: {xml}");
        assert!(xml.contains("<w:tab/>"), "{xml}");
    }

    /// `CT_RunTrackChange` cannot hold a `w:hyperlink`, the reverse is legal.
    #[test]
    fn hyperlink_wraps_the_revision_not_the_other_way_round() {
        let marks = vec![
            mark("link", Some(json!({ "href": "https://kubuno.com/" }))),
            rev_mark("insertion", "r4"),
        ];
        let nodes = vec![PmNode::text("lien ajouté", marks)];
        let mut ctx = ExportCtx::new();
        let xml = render_inline_content(&nodes, &mut ctx);
        let i = |s: &str| xml.find(s).unwrap_or_else(|| panic!("manque {s} dans {xml}"));
        assert!(i("<w:hyperlink") < i("<w:ins ") && i("<w:ins ") < i("<w:r>"), "{xml}");
        assert!(xml.contains("</w:ins></w:hyperlink>"), "{xml}");
    }

    /// Exports a REAL `.docx` through the whole pipeline — marks on the
    /// ProseMirror tree, not hand-written XML — so that an actual reader can be
    /// pointed at it:
    ///
    /// ```text
    /// cargo test -p kubuno-office write::run::tests::export -- --ignored --nocapture
    /// soffice --headless --norestore -env:UserInstallation=file:///tmp/lo-rev \
    ///         --convert-to fodt --outdir /tmp /tmp/kubuno_revisions_e2e.docx
    /// ```
    ///
    /// Expected in the FODT: `text:tracked-changes` with one `text:insertion`
    /// and one `text:deletion`, and `SUPPRIME-PAR-MARIE` present ONLY inside the
    /// `text:deletion` record — never as body text.
    #[test]
    #[ignore]
    fn export_sample_docx_with_revisions() {
        let ins = rev_mark("insertion", "c1");
        let del = rev_mark("deletion", "d1");
        let doc = PmNode::doc(vec![
            PmNode::paragraph(vec![PmNode::text("Texte normal avant. ", vec![])]),
            PmNode::paragraph(vec![
                PmNode::text("Debut. ", vec![]),
                PmNode::text("INSERE-PAR", vec![ins.clone()]),
                PmNode::text("-JEAN", vec![ins, mark("bold", None)]),
                PmNode::text(" SUPPRIME-PAR-MARIE", vec![del.clone()]),
                PmNode::text(" fin.", vec![]),
            ]),
            PmNode::paragraph(vec![PmNode::text(
                "Lien ajoute",
                vec![
                    mark("link", Some(json!({ "href": "https://kubuno.com/" }))),
                    rev_mark("insertion", "c2"),
                ],
            )]),
            // Paragraph MARK revision: the split made while tracking.
            PmNode {
                node_type: "paragraph".into(),
                attrs: Some(json!({
                    "insertion": { "author": "Jean Dupont", "date": "2026-07-28T09:12:00Z", "id": "c3" }
                })),
                content: Some(vec![PmNode::text("Paragraphe scinde.", vec![])]),
                marks: None,
                text: None,
            },
            PmNode::paragraph(vec![PmNode::text("Texte normal apres.", vec![])]),
        ]);
        let bytes = super::super::export_docx(&doc, "Revisions").expect("export");
        let path = std::env::var("KUBUNO_DOCX_OUT")
            .unwrap_or_else(|_| "/tmp/kubuno_revisions_e2e.docx".to_string());
        std::fs::write(&path, bytes).expect("write file");
        println!("docx: {path}");
    }

    /// U+000B (Word's soft break) and a tab must not reach `w:t` verbatim.
    #[test]
    fn text_children_are_sanitised() {
        let xml = render_run("a\tb\u{b}c\nd", &[]);
        assert!(xml.contains("<w:tab/>"), "{xml}");
        assert!(xml.contains("<w:br/>"), "{xml}");
        assert!(!xml.contains('\u{b}'), "{xml}");
    }
}

