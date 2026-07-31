//! Low-level XML helpers shared by the DOCX reader and writer.

use roxmltree::Node;

use crate::converters::types::PmMark;

/// Local name of an element, namespace-agnostic (handles the `w:` prefix).
pub(crate) fn local<'a>(n: &Node<'a, '_>) -> &'a str {
    n.tag_name().name()
}

/// First attribute matching `name` by local name (handles `w:val`, `r:id`, …).
pub(crate) fn attr_val(n: &Node<'_, '_>, name: &str) -> Option<String> {
    n.attributes()
        .find(|a| a.name() == name)
        .map(|a| a.value().to_string())
}

/// First direct child element with the given local name.
pub(crate) fn child<'a, 'd>(n: &Node<'a, 'd>, name: &str) -> Option<Node<'a, 'd>> {
    n.children().find(|c| c.is_element() && local(c) == name)
}

/// XML 1.0 cannot represent most C0 control characters — not even as numeric
/// references. A Rust `&str` is always valid UTF-8, so lone surrogates cannot
/// occur here; what does occur is U+000B, the character Word itself inserts for
/// a soft line break. Emitting it produces a file Word refuses to open.
fn xml_is_allowed(c: char) -> bool {
    matches!(c, '\u{9}' | '\u{A}' | '\u{D}')
        || ('\u{20}'..='\u{D7FF}').contains(&c)
        || ('\u{E000}'..='\u{FFFD}').contains(&c)
        || ('\u{10000}'..='\u{10FFFF}').contains(&c)
}

/// Escape for element content, dropping characters XML 1.0 cannot represent.
pub(crate) fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        if !xml_is_allowed(c) {
            continue;
        }
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape for an attribute value. Whitespace must become a numeric reference:
/// an XML parser normalises a literal tab or newline inside an attribute to a
/// plain space, which would silently corrupt a hyperlink target.
pub(crate) fn escape_xml_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        if !xml_is_allowed(c) {
            continue;
        }
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            _ => out.push(c),
        }
    }
    out
}

/// Split run text into `w:t` / `w:br` / `w:tab` children. A literal newline or
/// tab inside `w:t` is legal XML but illegal WordprocessingML: Word collapses
/// it to a space, losing the break.
pub(crate) fn text_to_run_children(text: &str) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::new();
    for (i, line) in normalized.split('\n').enumerate() {
        if i > 0 {
            out.push_str("<w:br/>");
        }
        for (j, seg) in line.split('\t').enumerate() {
            if j > 0 {
                out.push_str("<w:tab/>");
            }
            if !seg.is_empty() {
                out.push_str(&format!(
                    r#"<w:t xml:space="preserve">{}</w:t>"#,
                    escape_xml(seg)
                ));
            }
        }
    }
    if out.is_empty() {
        out.push_str(r#"<w:t xml:space="preserve"></w:t>"#);
    }
    out
}

pub(crate) fn simple_mark(t: &str) -> PmMark {
    PmMark { mark_type: t.to_string(), attrs: None }
}

/// An OOXML on/off property. Besides 0/1, Word uses 128 ("inherit from the
/// style, do nothing") and 129 ("the style's value, inverted") — cf. LibreOffice
/// DomainMapper.cxx:2288. `None` means: leave the current value alone.
pub(crate) fn toggle_on(n: &Node<'_, '_>, inherited: bool) -> Option<bool> {
    match attr_val(n, "val").as_deref() {
        Some("0") | Some("false") | Some("off") => Some(false),
        Some("128") => None,
        Some("129") => Some(!inherited),
        _ => Some(true),
    }
}

/// Toggle properties (b/i/strike) default ON; an explicit falsey val disables.
pub(crate) fn bool_on(n: &Node<'_, '_>) -> bool {
    toggle_on(n, false).unwrap_or(true)
}
