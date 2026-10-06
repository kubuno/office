//! Assembling the decoded layers into a ProseMirror document.
//!
//! Every other module of this converter reads one thing and reads it well; none
//! of them knows about the others. This is where they meet: the character
//! stream gives the text, the property bins give the formatting of each
//! character position, the stylesheet supplies what the direct properties
//! inherit, and the field/bookmark plan supplies the marks that span ranges.
//!
//! Order matters in two places and only there:
//!   - properties cascade docDefaults < style < direct, so the style's `grpprl`
//!     chain must be applied BEFORE the paragraph's or run's own;
//!   - table folding happens last, on the finished paragraph list, because a
//!     `.doc` only reveals a row's geometry at the row's END.

use serde_json::{json, Map, Value};

use crate::converters::types::{PmMark, PmNode};

use super::fkp::DocProps;
use super::list::ListTable;
use super::sprm::SprmVersion;
use super::style::{FontTable, StyleSheet};
use super::text::RawParagraph;
use super::{chp, drawing, field, pap, revisions, table, text, toc};

/// Everything the assembly needs, gathered once per document.
pub(crate) struct Context<'a> {
    pub(crate) props: &'a DocProps,
    pub(crate) styles: &'a StyleSheet,
    pub(crate) fonts: Vec<String>,
    pub(crate) lists: &'a ListTable,
    pub(crate) notes: &'a field::Annotations,
    pub(crate) pics: &'a drawing::Pictures,
    /// Which paragraphs are table-of-contents entries, and which characters of
    /// theirs (the tab and the page number) the canvas repaints from `tocPage`.
    pub(crate) toc: &'a toc::TocPlan,
    /// Tracked insertions/deletions, addressed by the same `cp` space as the
    /// body — so headers and footers get them for free.
    pub(crate) revs: &'a revisions::Revisions,
    pub(crate) ver: SprmVersion,
}

impl<'a> Context<'a> {
    /// Font names indexed by `ftc`, the form `chp::to_marks` expects.
    pub(crate) fn font_names(fonts: &FontTable) -> Vec<String> {
        fonts.iter().map(|s| s.to_string()).collect()
    }
}

/// Resolved paragraph properties at a character position: the style's chain
/// applied first, then the paragraph's own `grpprl`.
fn paragraph_props(ctx: &Context<'_>, cp: u32) -> (pap::Pap, Vec<u8>) {
    let istd = ctx.props.paras.istd_at(cp).unwrap_or(0);
    let mut p = pap::Pap::with_style(istd);
    for g in ctx.styles.papx_chain(istd) {
        p.apply_grpprl_ver(g, ctx.ver);
    }
    let direct = ctx.props.paras.grpprl_at(cp).to_vec();
    p.apply_grpprl_ver(&direct, ctx.ver);
    (p, direct)
}

/// Resolved character properties at a position, on top of the paragraph style.
fn run_chp(ctx: &Context<'_>, cp: u32, para_istd: u16) -> chp::Chp {
    let mut c = chp::Chp::default();
    for g in ctx.styles.chpx_chain(para_istd) {
        c.apply_grpprl(g);
    }
    c.apply_grpprl(ctx.props.chars.grpprl_at(cp));
    c
}

/// Split one paragraph's character range into runs of constant formatting and
/// build the inline content.
fn inline_content(
    ctx: &Context<'_>,
    chars: &[char],
    cp_start: u32,
    text: &str,
    para_istd: u16,
) -> Vec<PmNode> {
    let mut out: Vec<PmNode> = Vec::new();
    // The paragraph text has already had control characters removed, so it does
    // not align with the character stream one-to-one. Walk the ORIGINAL range
    // and keep only what `text` kept, which is what makes the marks line up.
    let mut buf = String::new();
    let mut cur: Option<Vec<PmMark>> = None;
    let mut ti = text.chars();
    let mut pending = ti.next();
    // Formatting only changes at a property-run boundary, so the properties are
    // resolved once per run rather than once per character — and `PmMark` needs
    // no equality for it.
    let mut run_end = cp_start;

    let end = cp_start + text.chars().count() as u32 * 2 + 2; // generous bound
    let mut cp = cp_start;
    while cp < end && pending.is_some() {
        let Some(&c) = chars.get(cp as usize) else { break };
        cp += 1;
        let at = cp - 1;
        if Some(c) != pending {
            // A control character `split_paragraphs` dropped. Two of them still
            // carry content and must be handled HERE, before the skip: an inline
            // picture (0x01) and a field whose result the annotation plan
            // replaces with a token. Skipping them unconditionally is why inline
            // pictures never reached the body.
            if c == text::ch::PICTURE {
                let props = run_chp(ctx, at, para_istd);
                if let Some(node) = props.pic_location.and_then(|fc| ctx.pics.inline_at(fc)) {
                    flush_run(&mut out, &mut buf, &mut cur);
                    out.push(node);
                    run_end = at;
                }
            } else if let Some(tok) = ctx.notes.insert_at(at) {
                flush_run(&mut out, &mut buf, &mut cur);
                out.push(PmNode {
                    node_type: "text".into(),
                    attrs: None,
                    content: None,
                    marks: None,
                    text: Some(tok.to_string()),
                });
                run_end = at;
            }
            continue;
        }
        pending = ti.next();
        if ctx.notes.is_hidden(at) || ctx.toc.is_suppressed(at) {
            continue;
        }
        let props = run_chp(ctx, at, para_istd);
        if props.hidden {
            continue;
        }
        // An inline picture is a placeholder character, not text.
        if props.special {
            if let Some(node) = props.pic_location.and_then(|fc| ctx.pics.inline_at(fc)) {
                flush_run(&mut out, &mut buf, &mut cur);
                out.push(node);
            }
            run_end = at; // force a fresh run after the picture
            continue;
        }
        if at >= run_end || cur.is_none() {
            flush_run(&mut out, &mut buf, &mut cur);
            let mut marks = props.to_marks(&ctx.fonts);
            marks.extend(ctx.notes.marks_at(at));
            marks.extend(ctx.revs.marks_at(at));
            cur = Some(marks);
            run_end = ctx
                .props
                .chars
                .at(at)
                .map(|r| r.cp_end)
                .filter(|e| *e > at)
                .unwrap_or(at + 1);
        }
        buf.push(c);
    }
    flush_run(&mut out, &mut buf, &mut cur);
    out
}

fn flush_run(out: &mut Vec<PmNode>, buf: &mut String, cur: &mut Option<Vec<PmMark>>) {
    if buf.is_empty() {
        return;
    }
    let marks = cur.clone().unwrap_or_default();
    out.push(PmNode {
        node_type: "text".into(),
        attrs: None,
        content: None,
        marks: (!marks.is_empty()).then_some(marks),
        text: Some(std::mem::take(buf)),
    });
}

/// Build the document body from the decoded layers.
pub(crate) fn build_body(ctx: &Context<'_>, chars: &[char], paras: &[RawParagraph]) -> PmNode {
    let mut items: Vec<table::Para> = Vec::with_capacity(paras.len());
    for rp in paras {
        let (p, direct) = paragraph_props(ctx, rp.cp_start);
        let mut attrs: Map<String, Value> = p.to_attrs();
        let is_toc = ctx.toc.apply_attrs(rp.cp_start, &p, &mut attrs);
        let content = inline_content(ctx, chars, rp.cp_start, &rp.text, p.istd);

        // A heading is a distinct node type, not a paragraph attribute.
        let level = if is_toc {
            None
        } else {
            p.outline_level
                .or_else(|| ctx.styles.effective_heading_level(p.istd))
        };
        let node_type = match level {
            Some(l) if (1..=6).contains(&l) => {
                attrs.insert("level".into(), json!(l));
                "heading"
            }
            _ => "paragraph",
        };
        if let Some(id) = ctx.styles.docx_style_id(p.istd) {
            attrs.insert("styleName".into(), json!(id));
        }
        // List membership, for the folding pass below.
        if let (Some(ilfo), lvl) = (p.ilfo, p.ilvl.unwrap_or(0)) {
            if let Some(res) = ctx.lists.level(ilfo, lvl) {
                attrs.insert("listOrdered".into(), json!(res.is_ordered()));
                attrs.insert("listLevel".into(), json!(lvl));
            }
        }
        items.push(table::Para {
            node: PmNode {
                node_type: node_type.into(),
                attrs: (!attrs.is_empty()).then_some(Value::Object(attrs)),
                content: Some(content),
                marks: None,
                text: None,
            },
            cell_end: rp.in_table,
            table: table::ParaTable::parse(&direct),
        });
    }

    // Tables first: they consume whole runs of paragraphs. Lists are folded
    // afterwards, on what is left at the top level.
    let blocks = table::assemble(&items);
    let mut nodes = fold_lists(blocks);
    if nodes.is_empty() {
        nodes.push(PmNode::paragraph(vec![]));
    }
    PmNode::doc(nodes)
}

/// Turn consecutive paragraphs carrying `listOrdered`/`listLevel` into nested
/// `bulletList` / `orderedList` nodes, the shape the DOCX reader produces.
fn fold_lists(blocks: Vec<PmNode>) -> Vec<PmNode> {
    let mut out: Vec<PmNode> = Vec::new();
    // Stack of open lists, one per nesting level.
    let mut stack: Vec<(u8, bool, Vec<PmNode>)> = Vec::new();

    let close_to = |stack: &mut Vec<(u8, bool, Vec<PmNode>)>, out: &mut Vec<PmNode>, keep: usize| {
        while stack.len() > keep {
            let (_, ordered, items) = stack.pop().unwrap_or((0, false, Vec::new()));
            let list = PmNode {
                node_type: if ordered { "orderedList".into() } else { "bulletList".into() },
                attrs: None,
                content: Some(items),
                marks: None,
                text: None,
            };
            match stack.last_mut() {
                // A nested list belongs inside the last item of its parent.
                Some((_, _, parent)) => match parent.last_mut() {
                    Some(item) => item.content.get_or_insert_with(Vec::new).push(list),
                    None => parent.push(list),
                },
                None => out.push(list),
            }
        }
    };

    for mut b in blocks {
        let info = b.attrs.as_ref().and_then(|a| {
            let ordered = a.get("listOrdered")?.as_bool()?;
            let level = a.get("listLevel").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
            Some((ordered, level))
        });
        let Some((ordered, level)) = info else {
            close_to(&mut stack, &mut out, 0);
            out.push(b);
            continue;
        };
        // The list attributes were only a carrier; they are not part of the model.
        if let Some(Value::Object(a)) = b.attrs.as_mut() {
            a.remove("listOrdered");
            a.remove("listLevel");
        }
        let want = level as usize + 1;
        let keep = want.min(stack.len());
        close_to(&mut stack, &mut out, keep);
        while stack.len() < want {
            let lvl = stack.len() as u8;
            stack.push((lvl, ordered, Vec::new()));
        }
        if let Some(top) = stack.last_mut() {
            // A change of kind at the same level starts a new list.
            if top.1 != ordered {
                close_to(&mut stack, &mut out, want - 1);
                stack.push((level, ordered, Vec::new()));
            }
        }
        if let Some((_, _, items)) = stack.last_mut() {
            items.push(PmNode {
                node_type: "listItem".into(),
                attrs: None,
                content: Some(vec![b]),
                marks: None,
                text: None,
            });
        }
    }
    close_to(&mut stack, &mut out, 0);
    out
}
