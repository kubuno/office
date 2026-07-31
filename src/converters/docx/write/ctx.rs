//! Export context: everything the body discovers while rendering that the
//! package must declare afterwards (relationships, media, extra parts).
//!
//! The body MUST be rendered before `[Content_Types].xml` and
//! `document.xml.rels` are written — a hyperlink or an image only becomes known
//! when the run carrying it is serialised. Mirrors `XmlFilterBase::addRelation`
//! (oox/source/core/xmlfilterbase.cxx:552).

use std::collections::{BTreeSet, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};

const REL_BASE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// Relationship type suffixes, appended to `REL_BASE`.
pub(crate) const REL_HYPERLINK: &str = "hyperlink";
pub(crate) const REL_IMAGE: &str = "image";

pub(crate) const REL_FOOTNOTES: &str = "footnotes";
pub(crate) const REL_ENDNOTES: &str = "endnotes";
pub(crate) const REL_COMMENTS: &str = "comments";
pub(crate) const REL_SETTINGS: &str = "settings";
/// Microsoft extension (oox/source/token/relationship.inc): replies and the
/// resolved flag of a comment live in this part, not in `comments.xml`.
pub(crate) const REL_COMMENTS_EXT: &str =
    "http://schemas.microsoft.com/office/2011/relationships/commentsExtended";

/// A comment thread as the editor holds it. Supplied by the caller: threads live
/// in the collaborative document, not in the ProseMirror tree, so the exporter
/// cannot discover them on its own.
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct CommentThreadIn {
    pub id: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub text: String,
    /// Epoch milliseconds.
    #[serde(default, alias = "createdAt")]
    pub created_at: i64,
    #[serde(default)]
    pub resolved: bool,
    #[serde(default)]
    pub replies: Vec<CommentReplyIn>,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct CommentReplyIn {
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub text: String,
    #[serde(default, alias = "createdAt")]
    pub created_at: i64,
}

/// One `w:comment` to emit — a thread root or one of its replies.
pub(crate) struct CommentOut {
    pub(crate) id: u32,
    pub(crate) author: String,
    pub(crate) initials: String,
    pub(crate) date: String,
    pub(crate) text: String,
    /// `w14:paraId` of this entry (hex, non-zero).
    pub(crate) para_id: u32,
    /// `w15:paraIdParent` — set on a reply.
    pub(crate) parent_para_id: Option<u32>,
    pub(crate) resolved: bool,
}

/// rId1 and rId2 are pre-assigned to the two parts we always write.
pub(crate) const RID_STYLES: &str = "rId1";
pub(crate) const RID_NUMBERING: &str = "rId2";

pub(crate) struct Rel {
    pub(crate) id: String,
    pub(crate) rel_type: &'static str,
    pub(crate) target: String,
    pub(crate) external: bool,
}

#[derive(Default)]
pub(crate) struct ExportCtx {
    pub(crate) rels: Vec<Rel>,
    /// Deduplicates external targets: the same URL used twice gets one relation.
    href_rid: HashMap<String, String>,
    /// (zip path, bytes) for `word/media/*`.
    pub(crate) media: Vec<(String, Vec<u8>)>,
    /// Image extensions needing a `<Default>` entry in `[Content_Types].xml`.
    pub(crate) media_exts: BTreeSet<String>,
    /// Deduplicates media by content: same bytes → same relationship.
    media_by_hash: HashMap<u64, String>,
    /// Relationship types that are already full URIs (see `add_rel_absolute`).
    pub(crate) absolute_rels: Vec<&'static str>,
    /// Extra parts to write verbatim: (zip path, content-type override, xml).
    pub(crate) parts: Vec<(String, &'static str, String)>,
    next_rid: u32,
    /// `wp:docPr@id` must be unique document-wide and non-zero.
    next_docpr: u32,
    /// Footnote texts in document order. The DOCX id is `index + 2`: ids 0 and 1
    /// are reserved for the separator notes.
    pub(crate) footnotes: Vec<String>,
    /// Endnote texts, numbered the same way in their own part.
    pub(crate) endnotes: Vec<String>,
    /// Threads supplied by the caller, keyed by the editor's comment id.
    pub(crate) threads: HashMap<String, CommentThreadIn>,
    /// `w:comment` entries actually emitted — only threads that are anchored.
    pub(crate) comments: Vec<CommentOut>,
    /// Editor comment id → DOCX ids of the thread: the root FIRST, then its
    /// replies. A reply is a comment in its own right and must be anchored on
    /// the same range as its root, otherwise Word and Writer show only the root.
    pub(crate) comment_ids: HashMap<String, Vec<u32>>,
    /// Bookmark ranges still open, as (sanitised name, id).
    pub(crate) open_bookmarks: Vec<(String, u32)>,
    /// Comment ranges still open, by editor id.
    pub(crate) open_comments: Vec<String>,
    /// Shared counter for bookmark ids and comment ids.
    next_mark_id: u32,
    next_para_id: u32,
}

impl ExportCtx {
    /// A context with no comment thread. Only the tests need it: production
    /// always goes through `with_comments`, since the threads come from the
    /// stored envelope.
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        ExportCtx::with_comments(&[])
    }

    pub(crate) fn with_comments(threads: &[CommentThreadIn]) -> Self {
        ExportCtx {
            next_rid: 3, // rId1 = styles.xml, rId2 = numbering.xml
            next_docpr: 1,
            next_mark_id: 1,
            // Word requires 0 < paraId < 0x80000000.
            next_para_id: 0x1000_0001,
            threads: threads.iter().map(|t| (t.id.clone(), t.clone())).collect(),
            ..Default::default()
        }
    }

    pub(crate) fn next_mark_id(&mut self) -> u32 {
        let id = self.next_mark_id;
        self.next_mark_id += 1;
        id
    }

    pub(crate) fn next_para_id(&mut self) -> u32 {
        let id = self.next_para_id;
        self.next_para_id = self.next_para_id.wrapping_add(1) | 0x1000_0000;
        id & 0x7FFF_FFFF
    }

    pub(crate) fn add_rel(&mut self, rel_type: &'static str, target: String, external: bool) -> String {
        let id = format!("rId{}", self.next_rid);
        self.next_rid += 1;
        self.rels.push(Rel { id: id.clone(), rel_type, target, external });
        id
    }

    /// A relationship whose type is a COMPLETE URI rather than a suffix of the
    /// OOXML namespace — the Microsoft extensions are of that kind.
    pub(crate) fn add_rel_absolute(&mut self, rel_type: &'static str, target: String) -> String {
        let id = format!("rId{}", self.next_rid);
        self.next_rid += 1;
        self.rels.push(Rel { id: id.clone(), rel_type, target, external: false });
        self.absolute_rels.push(rel_type);
        id
    }

    /// Relationship for an external hyperlink target, created once per URL.
    pub(crate) fn hyperlink_rid(&mut self, href: &str) -> String {
        if let Some(id) = self.href_rid.get(href) {
            return id.clone();
        }
        let id = self.add_rel(REL_HYPERLINK, href.to_string(), true);
        self.href_rid.insert(href.to_string(), id.clone());
        id
    }

    /// Store an image under `word/media/` and return its relationship id.
    ///
    /// Identical bytes are stored once: the editor repeats the same data URI for
    /// a logo used on every page, and one copy per use would bloat the package.
    pub(crate) fn add_image(&mut self, ext: &str, bytes: Vec<u8>) -> String {
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        ext.hash(&mut hasher);
        let key = hasher.finish();
        if let Some(id) = self.media_by_hash.get(&key) {
            return id.clone();
        }
        let n = self.media.len() + 1;
        let ext = ext.to_ascii_lowercase();
        let name = format!("image{n}.{ext}");
        self.media.push((format!("word/media/{name}"), bytes));
        self.media_exts.insert(ext);
        let id = self.add_rel(REL_IMAGE, format!("media/{name}"), false);
        self.media_by_hash.insert(key, id.clone());
        id
    }

    pub(crate) fn next_docpr_id(&mut self) -> u32 {
        let id = self.next_docpr;
        self.next_docpr += 1;
        id
    }

    /// `word/_rels/document.xml.rels`, including the two fixed entries.
    pub(crate) fn doc_rels_xml(&self) -> String {
        let mut s = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="{RID_STYLES}" Type="{REL_BASE}/styles" Target="styles.xml"/>
<Relationship Id="{RID_NUMBERING}" Type="{REL_BASE}/numbering" Target="numbering.xml"/>"#
        );
        for r in &self.rels {
            let mode = if r.external { r#" TargetMode="External""# } else { "" };
            // An absolute type is written as-is; a suffix is joined to REL_BASE.
            if self.absolute_rels.contains(&r.rel_type) {
                s.push_str(&format!(
                    r#"<Relationship Id="{}" Type="{}" Target="{}"/>"#,
                    r.id,
                    r.rel_type,
                    super::super::xml::escape_xml_attr(&r.target)
                ));
                continue;
            }
            s.push_str(&format!(
                r#"<Relationship Id="{}" Type="{REL_BASE}/{}" Target="{}"{mode}/>"#,
                r.id,
                r.rel_type,
                super::super::xml::escape_xml_attr(&r.target)
            ));
        }
        s.push_str("</Relationships>");
        s
    }
}

/// MIME type for an image extension, or `None` when we cannot declare it.
pub(crate) fn image_content_type(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "emf" => "image/x-emf",
        "wmf" => "image/x-wmf",
        _ => return None,
    })
}
