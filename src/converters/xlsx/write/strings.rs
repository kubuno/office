//! Shared string table builder (xl/sharedStrings.xml).
use std::collections::HashMap;

use super::super::util::esc_xml;

/// Deduplicating shared-string table. Cells of type `t="s"` store an index
/// into this table instead of inline text.
#[derive(Default)]
pub struct SharedStrings {
    list:   Vec<String>,
    lookup: HashMap<String, usize>,
}

impl SharedStrings {
    pub fn new() -> Self { Self::default() }

    /// Intern a string and return its table index.
    pub fn index(&mut self, s: &str) -> usize {
        if let Some(&i) = self.lookup.get(s) { return i; }
        let i = self.list.len();
        self.list.push(s.to_string());
        self.lookup.insert(s.to_string(), i);
        i
    }

    pub fn is_empty(&self) -> bool { self.list.is_empty() }

    /// Serialize the table to the sharedStrings part XML.
    pub fn build_xml(&self) -> String {
        let mut out = String::with_capacity(64 + self.list.len() * 24);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
        out.push_str(&format!(
            "<sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"{n}\" uniqueCount=\"{n}\">",
            n = self.list.len()
        ));
        for s in &self.list {
            // Leading/trailing whitespace must be preserved explicitly.
            let space = if s.starts_with(char::is_whitespace) || s.ends_with(char::is_whitespace) {
                " xml:space=\"preserve\""
            } else { "" };
            out.push_str(&format!("<si><t{space}>{}</t></si>", esc_xml(s)));
        }
        out.push_str("</sst>");
        out
    }
}
