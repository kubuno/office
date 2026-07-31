//! `docProps/app.xml` — the "extended" document properties.
//!
//! Not required to open a file, but Word's Properties panel reads its statistics
//! from here: without the part, page and word counts show as empty. The child
//! ORDER is imposed by `CT_Properties`; LibreOffice writes the same sequence
//! (oox/source/core/xmlfilterbase.cxx:783).

use crate::converters::docx::xml::escape_xml;

/// Application name reported to readers. Kept stable so a round-trip through our
/// converter is identifiable in the file itself.
const APPLICATION: &str = "Kubuno Office";

pub(crate) fn app_xml(pages: usize, words: usize, chars: usize) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"
 xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">
<Application>{app}</Application>
<Pages>{pages}</Pages>
<Words>{words}</Words>
<Characters>{chars}</Characters>
<Paragraphs>{pages}</Paragraphs>
<DocSecurity>0</DocSecurity>
<ScaleCrop>false</ScaleCrop>
<LinksUpToDate>false</LinksUpToDate>
<CharactersWithSpaces>{chars}</CharactersWithSpaces>
<SharedDoc>false</SharedDoc>
<HyperlinksChanged>false</HyperlinksChanged>
<AppVersion>16.0000</AppVersion>
</Properties>"#,
        app = escape_xml(APPLICATION),
    )
}

/// Rough statistics of a body, for the part above. Counting exactly what Word
/// counts is not worth a second text walk: these are informational.
pub(crate) fn body_stats(body_xml: &str) -> (usize, usize, usize) {
    let paragraphs = body_xml.matches("<w:p>").count().max(1);
    let text: String = body_xml
        .split("<w:t")
        .skip(1)
        .filter_map(|seg| seg.split_once('>').and_then(|(_, r)| r.split_once("</w:t>")))
        .map(|(t, _)| t)
        .collect::<Vec<_>>()
        .join(" ");
    let words = text.split_whitespace().count();
    (paragraphs, words, text.chars().count())
}
