//! `word/settings.xml`.
//!
//! Small but load-bearing: `w:evenAndOddHeaders` lives here, so WITHOUT this
//! part Word ignores any even-page header the section references. It also holds
//! the default tab stop — the editor's rulers assume 720 twips, and a reader
//! that has to guess uses its own value instead. And `w:trackRevisions` lives
//! here too: its presence is what makes revision marking ACTIVE when the
//! document is opened.
//!
//! The children of `w:settings` are a `xsd:sequence`, NOT a choice: a reader is
//! entitled to reject the part when they are out of order. The order below is
//! the one the ECMA-376 `CT_Settings` sequence prescribes — the relevant slice
//! being `… revisionView, trackRevisions, documentProtection, …,
//! defaultTabStop, autoHyphenation, …, evenAndOddHeaders, …, compat`.

/// Document-level settings written to `word/settings.xml`.
///
/// A struct instead of a list of positional booleans on purpose: two adjacent
/// `bool` arguments read identically at the call site and get swapped sooner or
/// later, and the compiler cannot tell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SettingsOptions {
    /// Mirrors the editor's "different odd and even pages"
    /// (`w:evenAndOddHeaders`). Without it Word ignores the `even` header
    /// reference the section carries.
    pub(crate) even_odd_headers: bool,
    /// Revision marking is ON when the document is opened (`w:trackRevisions`).
    /// This is a property of the DOCUMENT, not of the current editing session.
    pub(crate) track_changes: bool,
}

/// Settings part.
pub(crate) fn settings_xml(opts: SettingsOptions) -> String {
    // Note the element is `w:trackRevisions`, not `w:trackChanges`: the latter
    // does not exist in CT_Settings and is silently ignored by every reader.
    let track = if opts.track_changes {
        "<w:trackRevisions/>\n"
    } else {
        ""
    };
    let eao = if opts.even_odd_headers {
        "<w:evenAndOddHeaders/>\n"
    } else {
        ""
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
{track}<w:defaultTabStop w:val="720"/>
{eao}<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>
</w:settings>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omits_both_flags_by_default() {
        let xml = settings_xml(SettingsOptions::default());
        assert!(!xml.contains("w:trackRevisions"));
        assert!(!xml.contains("w:evenAndOddHeaders"));
        assert!(xml.contains(r#"<w:defaultTabStop w:val="720"/>"#));
    }

    #[test]
    fn writes_track_revisions_when_active() {
        let xml = settings_xml(SettingsOptions {
            track_changes: true,
            ..Default::default()
        });
        assert!(xml.contains("<w:trackRevisions/>"));
        // `w:trackChanges` is the name everybody guesses; make sure we never
        // regress to it.
        assert!(!xml.contains("w:trackChanges"));
    }

    /// The schema sequence is what makes this test worth having: written in the
    /// wrong order the part is invalid even though every element is legal.
    #[test]
    fn children_follow_the_schema_sequence() {
        let xml = settings_xml(SettingsOptions {
            even_odd_headers: true,
            track_changes: true,
        });
        let track = xml.find("<w:trackRevisions/>").expect("trackRevisions");
        let tab = xml.find("<w:defaultTabStop").expect("defaultTabStop");
        let eao = xml.find("<w:evenAndOddHeaders/>").expect("evenAndOddHeaders");
        let compat = xml.find("<w:compat>").expect("compat");
        assert!(track < tab, "trackRevisions must precede defaultTabStop");
        assert!(tab < eao, "defaultTabStop must precede evenAndOddHeaders");
        assert!(eao < compat, "evenAndOddHeaders must precede compat");
    }
}
