//! Package-level parts: `[Content_Types].xml` and the root relationships.

use super::ctx::{image_content_type, ExportCtx};

const WP: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml";

/// `[Content_Types].xml`, built from what the body actually produced: an image
/// extension with no `<Default>` entry makes the package invalid.
pub(crate) fn build_content_types(ctx: &ExportCtx) -> String {
    // Parts written conditionally must be declared exactly when they exist: an
    // undeclared part makes the package invalid, a declared-but-absent one too.
    let mut conditional = String::new();
    if !ctx.footnotes.is_empty() {
        conditional.push_str(&format!(
            r#"<Override PartName="/word/footnotes.xml" ContentType="{WP}.footnotes+xml"/>"#
        ));
    }
    if !ctx.endnotes.is_empty() {
        conditional.push_str(&format!(
            r#"<Override PartName="/word/endnotes.xml" ContentType="{WP}.endnotes+xml"/>"#
        ));
    }
    if !ctx.comments.is_empty() {
        conditional.push_str(&format!(
            r#"<Override PartName="/word/comments.xml" ContentType="{WP}.comments+xml"/>"#
        ));
        if ctx
            .comments
            .iter()
            .any(|c| c.resolved || c.parent_para_id.is_some())
        {
            conditional.push_str(
                r#"<Override PartName="/word/commentsExtended.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml"/>"#,
            );
        }
    }
    let mut defaults = String::new();
    for ext in &ctx.media_exts {
        if let Some(mime) = image_content_type(ext) {
            defaults.push_str(&format!(
                r#"<Default Extension="{ext}" ContentType="{mime}"/>"#
            ));
        }
    }
    let mut overrides = String::new();
    for (path, content_type, _) in &ctx.parts {
        overrides.push_str(&format!(
            r#"<Override PartName="/{path}" ContentType="{content_type}"/>"#
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>{defaults}
<Override PartName="/word/document.xml" ContentType="{WP}.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="{WP}.styles+xml"/>
<Override PartName="/word/numbering.xml" ContentType="{WP}.numbering+xml"/>
<Override PartName="/word/settings.xml" ContentType="{WP}.settings+xml"/>{conditional}
<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
<Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>{overrides}
</Types>"#
    )
}

pub(crate) const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
  <Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/>
</Relationships>"#;
