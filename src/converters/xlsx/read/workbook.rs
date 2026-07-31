//! Workbook part (xl/workbook.xml) and relationship (.rels) parsing.
use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::Reader;

use super::super::util::attr;

/// (name, formula) of a defined name.
pub type NamePair = (String, String);

/// One `<sheet>` entry of the workbook part.
#[derive(Debug, Default)]
pub struct WbSheetRef {
    pub name:   String,
    pub rid:    String,
    pub hidden: bool, // state="hidden" | "veryHidden"
}

/// A defined name scoped to one sheet (`localSheetId`), including the
/// `_xlnm.Print_Area` / `_xlnm.Print_Titles` builtins the caller routes.
#[derive(Debug)]
pub struct LocalName {
    pub sheet: usize,  // localSheetId = index into the workbook sheet list
    pub name:  String,
    pub def:   String, // definition WITHOUT the leading '='
}

/// → (sheets, global defined_names [(name, "=formula")], sheet-local names)
pub fn parse_workbook(xml: &str) -> (Vec<WbSheetRef>, Vec<NamePair>, Vec<LocalName>) {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut sheets: Vec<WbSheetRef> = Vec::new();
    let mut names: Vec<NamePair> = Vec::new();
    let mut locals: Vec<LocalName> = Vec::new();
    // Pending definedName: (name, localSheetId?) — resolved at the End event.
    let mut cur_name: Option<(String, Option<usize>)> = None;
    let mut cur_def = String::new();
    let mut in_def = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"sheet" => {
                    let name = attr(&e, b"name").unwrap_or_default();
                    let rid = e.attributes().flatten()
                        .find(|a| a.key.as_ref().ends_with(b":id") || a.key.local_name().as_ref() == b"id")
                        .map(|a| String::from_utf8_lossy(&a.value).into_owned())
                        .unwrap_or_default();
                    let hidden = matches!(attr(&e, b"state").as_deref(), Some("hidden") | Some("veryHidden"));
                    sheets.push(WbSheetRef { name, rid, hidden });
                }
                b"definedName" => {
                    let n = attr(&e, b"name").unwrap_or_default();
                    let local = attr(&e, b"localSheetId").and_then(|v| v.parse::<usize>().ok());
                    // Excel built-ins: only the sheet-scoped print ranges are kept;
                    // the other _xlnm names (_FilterDatabase, …) are skipped.
                    let keep = if n.starts_with("_xlnm") {
                        local.is_some() && (n == "_xlnm.Print_Area" || n == "_xlnm.Print_Titles")
                    } else { true };
                    cur_name = if keep { Some((n, local)) } else { None };
                    cur_def.clear();
                    in_def = true;
                }
                _ => {}
            },
            Ok(Event::Text(e)) if in_def => cur_def.push_str(&e.unescape().unwrap_or_default()),
            Ok(Event::End(e)) if e.local_name().as_ref() == b"definedName" => {
                if let Some((n, local)) = cur_name.take() {
                    let def = cur_def.trim().to_string();
                    if !def.is_empty() {
                        match local {
                            Some(idx) => locals.push(LocalName { sheet: idx, name: n, def }),
                            None => names.push((n, format!("={def}"))),
                        }
                    }
                }
                in_def = false;
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    (sheets, names, locals)
}

/// Parse a .rels part keeping the relationship type and mode:
/// Id → (Target, Type, external). `external` = TargetMode="External".
pub fn parse_typed_rels(xml: &str) -> HashMap<String, (String, String, bool)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut map = HashMap::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref() == b"Relationship" => {
                // Targets are URLs — unescape entities (&amp; …) properly.
                let unescaped = |name: &[u8]| e.attributes().flatten()
                    .find(|a| a.key.local_name().as_ref() == name)
                    .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()));
                if let (Some(id), Some(target)) = (unescaped(b"Id"), unescaped(b"Target")) {
                    let ty = attr(&e, b"Type").unwrap_or_default();
                    let external = attr(&e, b"TargetMode").as_deref() == Some("External");
                    map.insert(id, (target, ty, external));
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    map
}

/// Parse a .rels part into an Id → Target map.
pub fn parse_rels(xml: &str) -> HashMap<String, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut map = HashMap::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref() == b"Relationship" => {
                if let (Some(id), Some(target)) = (attr(&e, b"Id"), attr(&e, b"Target")) { map.insert(id, target); }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    map
}
