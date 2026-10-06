//! `word/comments.xml` and `word/commentsExtended.xml`.
//!
//! A comment is a RANGE plus a reference: `commentRangeStart` … the commented
//! text … `commentRangeEnd`, then — in a run of its own — `commentReference`.
//! Putting the reference anywhere else makes Word offer to repair the file
//! (LibreOffice: docxattributeoutput.cxx:2544).
//!
//! Replies and the resolved flag are NOT in `comments.xml`: they live in
//! `commentsExtended.xml`, keyed by the `w14:paraId` of the comment's LAST
//! paragraph (`:8925`). Keying on the first silently breaks replies on
//! multi-paragraph comments.

use crate::converters::docx::xml::{escape_xml, escape_xml_attr, text_to_run_children};

use super::ctx::{CommentOut, ExportCtx};

/// Initials from a display name ("Jean Dupont" → "JD").
fn initials_of(author: &str) -> String {
    author
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(3)
        .collect::<String>()
        .to_uppercase()
}

/// Epoch milliseconds → the ISO-8601 form Word writes in `w:date`.
fn docx_date(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_else(chrono::Utc::now)
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

/// Register a thread the first time one of its ranges opens, and return the
/// DOCX id of its root. `None` when the caller supplied no thread for that id:
/// an orphan `commentRangeStart` makes Word repair the file, so a comment mark
/// without a thread is ignored rather than half-written.
pub(crate) fn register_thread(ctx: &mut ExportCtx, comment_id: &str) -> Option<Vec<u32>> {
    if let Some(ids) = ctx.comment_ids.get(comment_id) {
        return Some(ids.clone());
    }
    let th = ctx.threads.get(comment_id)?.clone();
    let root_id = ctx.next_mark_id();
    let root_para = ctx.next_para_id();
    let has_replies = !th.replies.is_empty();
    let mut ids = vec![root_id];
    ctx.comments.push(CommentOut {
        id: root_id,
        initials: initials_of(&th.author),
        author: th.author.clone(),
        date: docx_date(th.created_at),
        text: th.text.clone(),
        para_id: root_para,
        parent_para_id: None,
        // Word marks a thread resolved through its LAST entry.
        resolved: th.resolved && !has_replies,
    });
    for (i, r) in th.replies.iter().enumerate() {
        let id = ctx.next_mark_id();
        ids.push(id);
        let para = ctx.next_para_id();
        ctx.comments.push(CommentOut {
            id,
            initials: initials_of(&r.author),
            author: r.author.clone(),
            date: docx_date(r.created_at),
            text: r.text.clone(),
            para_id: para,
            parent_para_id: Some(root_para),
            resolved: th.resolved && i + 1 == th.replies.len(),
        });
    }
    ctx.comment_ids.insert(comment_id.to_string(), ids.clone());
    Some(ids)
}

/// `commentRangeEnd` followed by the reference run — in that order.
pub(crate) fn render_range_end(id: u32) -> String {
    format!(
        r#"<w:commentRangeEnd w:id="{id}"/><w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="{id}"/></w:r>"#
    )
}

pub(crate) fn comments_xml(ctx: &ExportCtx) -> Option<String> {
    if ctx.comments.is_empty() {
        return None;
    }
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
 xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"
 xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" mc:Ignorable="w14">
"#,
    );
    for c in &ctx.comments {
        out.push_str(&format!(
            r#"<w:comment w:id="{id}" w:author="{author}" w:date="{date}" w:initials="{initials}"><w:p w14:paraId="{para:08X}"><w:pPr><w:pStyle w:val="CommentText"/></w:pPr><w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:annotationRef/></w:r><w:r>{text}</w:r></w:p></w:comment>"#,
            id = c.id,
            author = escape_xml_attr(&c.author),
            date = c.date,
            initials = escape_xml_attr(&c.initials),
            para = c.para_id,
            text = text_to_run_children(&c.text),
        ));
    }
    out.push_str("</w:comments>");
    let _ = escape_xml;
    Some(out)
}

/// Only written when a thread has a reply or is resolved: LibreOffice keeps the
/// part out of the package otherwise (docxexport.cxx:965).
pub(crate) fn comments_extended_xml(ctx: &ExportCtx) -> Option<String> {
    if !ctx
        .comments
        .iter()
        .any(|c| c.resolved || c.parent_para_id.is_some())
    {
        return None;
    }
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w15:commentsEx xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
 xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml" mc:Ignorable="w15">
"#,
    );
    for c in &ctx.comments {
        let parent = c
            .parent_para_id
            .map(|p| format!(r#" w15:paraIdParent="{p:08X}""#))
            .unwrap_or_default();
        let done = if c.resolved { "1" } else { "0" };
        out.push_str(&format!(
            r#"<w15:commentEx w15:paraId="{:08X}"{parent} w15:done="{done}"/>"#,
            c.para_id
        ));
    }
    out.push_str("</w15:commentsEx>");
    Some(out)
}
