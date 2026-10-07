//! Table-of-contents entries on the page (`computeTocPaint`, `canvas-engine.ts:2356-2397`): the page
//! number at the right edge of the entry's last line, and the leader between the text and it —
//! dots on an absolute 6 px grid (so they line up from one entry to the next), dashes, or a
//! continuous underline.

use serde_json::{Map, Value};

use super::{DocPx, LayoutParagraph, TocEntry};
use crate::marks::TextMark;
use crate::measure::Measure;

/// `TOC_NUM_GAP`: between the end of the entry's text and its number (reserved by the layout).
pub const TOC_NUM_GAP: DocPx = 16.0;
/// `TOC_DOT_STEP`, `TOC_DOT_SIZE`, `TOC_DOT_PAD`, `TOC_DOT_CLR`.
pub const TOC_DOT_STEP: DocPx = 6.0;
pub const TOC_DOT_SIZE: DocPx = 1.6;
pub const TOC_DOT_PAD: DocPx = 8.0;
pub const TOC_DOT_CLR: &str = "#80868b";

/// The entry a paragraph's attributes describe (`tocPage`, `tocPageText`, `tocLeader`,
/// `tocLeaderKind`); `None` for an ordinary paragraph.
pub fn entry_of(attrs: &Map<String, Value>) -> Option<TocEntry> {
    let page = attrs.get("tocPage").and_then(Value::as_f64).map(|p| p as i64);
    let page_text = attrs.get("tocPageText").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    let leader = attrs.get("tocLeader").and_then(Value::as_bool).unwrap_or(false);
    let leader_kind = attrs.get("tocLeaderKind").and_then(Value::as_str).map(str::to_string);
    (page.is_some() || page_text.is_some() || leader).then_some(TocEntry { page, page_text, leader, leader_kind })
}

/// What the painter draws for one entry (`TocPaint`), in the paragraph's coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct TocPaint {
    pub number: String,
    pub number_x: DocPx,
    pub baseline: DocPx,
    /// The leader's dots (or dashes): their left edges, all at `dots_y`, each `dot_w` × `dot_h`.
    pub dots_x: Vec<DocPx>,
    pub dots_y: DocPx,
    pub dot_w: DocPx,
    pub dot_h: DocPx,
    /// A continuous leader (`underline`): from x0 to x1 at the baseline.
    pub bar: Option<(DocPx, DocPx)>,
}

/// `computeTocPaint`: the number and leader of `para` (on its last line), in a column `content_w`
/// wide; `None` for an ordinary paragraph or an injected line.
pub fn toc_paint(para: &LayoutParagraph, content_w: DocPx, m: &dyn Measure) -> Option<TocPaint> {
    let toc = para.toc.as_ref()?;
    if toc.page.is_none() && !toc.leader {
        return None;
    }
    let last = para.lines.last()?;
    if last.phantom || last.image.is_some() {
        return None;
    }
    let right = match (last.cell_x, last.cell_w) {
        (Some(x), Some(w)) => x + w,
        _ => content_w,
    };
    let number = toc.number().unwrap_or_default();
    let num_w = if number.is_empty() { 0.0 } else { m.width(&number, &TextMark::default()) };
    let number_x = right - num_w;
    let text_end = last.spans.last().map(|s| s.x + s.width).or(last.caret_x).unwrap_or(0.0);
    let kind = toc.leader_kind.as_deref().filter(|k| *k != "none").unwrap_or("dots");
    let (step, dot_w, dot_h) = if kind == "dashes" { (TOC_DOT_STEP * 2.0, TOC_DOT_STEP, 1.0) } else { (TOC_DOT_STEP, TOC_DOT_SIZE, TOC_DOT_SIZE) };
    let mut dots_x = Vec::new();
    let mut bar = None;
    if toc.leader && kind == "underline" {
        let (x0, x1) = (text_end + TOC_DOT_PAD, number_x - TOC_DOT_PAD);
        if x1 > x0 {
            bar = Some((x0, x1));
        }
    } else if toc.leader {
        let x1 = number_x - TOC_DOT_PAD;
        let mut x = ((text_end + TOC_DOT_PAD) / step).ceil() * step;
        while x < x1 {
            dots_x.push(x);
            x += step;
        }
    }
    Some(TocPaint { number, number_x, baseline: last.baseline, dots_x, dots_y: last.baseline - 1.5, dot_w, dot_h, bar })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::flow::layout_paragraphs;
    use crate::layout::parse::parse_doc;
    use crate::measure::FixedMeasure;
    use crate::model::Node;

    #[test]
    fn an_entry_reserves_its_number_and_gets_dots_on_the_grid() {
        let doc: Node = serde_json::from_str(r#"{"type":"doc","content":[{"type":"paragraph","attrs":{"tocLevel":1,"tocPage":12,"tocLeader":true,"tocLeaderKind":"dots","tocKind":"toc"},"content":[{"type":"text","text":"Introduction"}]}]}"#).expect("doc");
        let (paras, _) = layout_paragraphs(&parse_doc(&doc), &|_| 600.0, &FixedMeasure);
        let p = toc_paint(&paras[0], 600.0, &FixedMeasure).expect("an entry");
        assert_eq!(p.number, "12");
        let num_w = FixedMeasure.width("12", &TextMark::default());
        assert!((p.number_x - (600.0 - num_w)).abs() < 1e-3);
        assert!(!p.dots_x.is_empty());
        assert!(p.dots_x.iter().all(|x| (x / TOC_DOT_STEP).fract().abs() < 1e-3), "on the absolute grid");
        assert!(p.dots_x.last().is_some_and(|&x| x < p.number_x - TOC_DOT_PAD));
        let text_end = paras[0].lines[0].spans.last().map(|s| s.x + s.width).unwrap_or(0.0);
        assert!(p.dots_x[0] >= text_end + TOC_DOT_PAD);
        // The text wraps before the number's reserved band.
        assert!(text_end <= 600.0 - num_w - TOC_NUM_GAP + 0.01);
    }

    #[test]
    fn an_ordinary_paragraph_has_no_entry() {
        assert!(entry_of(&Map::new()).is_none());
        let mut m = Map::new();
        m.insert("tocLevel".into(), Value::from(1));
        assert!(entry_of(&m).is_none(), "a level without a page or a leader paints nothing");
    }
}
