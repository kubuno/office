//! `word/numbering.xml`. Without this part every `w:numId` our lists reference
//! is a dangling pointer and Word shows no bullet and no number at all.
//!
//! Two abstract definitions are always written — `0` = bullets, `1` = numbers,
//! nine levels each, laid out exactly like the ones Word creates from its
//! toolbar (bullet cycle `• o ▪`, number cycle `1. a. i.`, indent 720 twips per
//! level with a 360 twip hanging marker).
//!
//! On top of those, `list.rs` allocates ONE `w:num` per list tree it renders
//! (see `alloc_list`): two consecutive lists must not share a `w:numId`, or Word
//! keeps counting (`1. 2. 3.` then `4. 5. 6.`), while every level of a SINGLE
//! tree must share one, or nested counters never restart. The instances live in
//! a thread-local registry because `numbering_xml()` takes no context and is
//! called — once, on the same thread — *after* the body has been rendered
//! (`write/mod.rs`); reading the registry also clears it, which is what makes
//! the next export start from a clean slate.

use std::cell::RefCell;
use std::collections::BTreeMap;

use super::super::xml::escape_xml_attr;

/// Abstract definition ids.
const ABSTRACT_BULLET: u32 = 0;
const ABSTRACT_ORDERED: u32 = 1;

/// Static fallback `w:numId`s, always present in the part.
pub(crate) const BULLET_NUM_ID: &str = "1";
pub(crate) const ORDERED_NUM_ID: &str = "2";

/// First `w:numId` handed out by `alloc_list` (1 and 2 are the fallbacks).
const FIRST_INSTANCE_ID: u32 = 3;

/// Word's nine numbering levels: `w:ilvl` is 0..=8, anything deeper is rejected
/// (LibreOffice `DomainMapper.cxx:1816-1836`).
pub(crate) const MAX_ILVL: u8 = 8;

/// ST_NumberFormat values we are willing to emit (ECMA-376 §17.18.59). An
/// unknown format falls back to the level default rather than producing a file
/// Word refuses to open — same spirit as LibreOffice's `decimal` fallback
/// (`docxattributeoutput.cxx:8100`).
const NUM_FMTS: &[&str] = &[
    "decimal",
    "decimalZero",
    "upperRoman",
    "lowerRoman",
    "upperLetter",
    "lowerLetter",
    "ordinal",
    "cardinalText",
    "ordinalText",
    "hex",
    "chicago",
    "bullet",
    "none",
];

/// What a list node knows about one of its levels. Everything is optional: an
/// editor list that carries no imported numbering attribute at all keeps the
/// default level of the abstract definition.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct LevelSpec<'a> {
    /// First number of the level (`w:startOverride`).
    pub(crate) start: Option<i64>,
    /// `w:numFmt/@val`.
    pub(crate) num_fmt: Option<&'a str>,
    /// `w:lvlText/@val` — the marker template (`%1.`) or the bullet glyph.
    pub(crate) lvl_text: Option<&'a str>,
    /// Font carrying the bullet glyph (`w:lvl/w:rPr/w:rFonts`).
    pub(crate) font: Option<&'a str>,
}

impl LevelSpec<'_> {
    /// True when the level says nothing the default does not already say.
    fn is_empty(&self) -> bool {
        self.start.is_none()
            && self.num_fmt.is_none()
            && self.lvl_text.is_none()
            && self.font.is_none()
    }
}

/// An owned `LevelSpec`, as stored in the registry.
#[derive(Debug, Default, Clone)]
struct LevelOverride {
    start: Option<i64>,
    num_fmt: Option<String>,
    lvl_text: Option<String>,
    font: Option<String>,
}

impl LevelOverride {
    fn apply(&mut self, spec: &LevelSpec<'_>) {
        if let Some(v) = spec.start {
            self.start = Some(v.max(0));
        }
        if let Some(v) = spec.num_fmt.filter(|f| NUM_FMTS.contains(f)) {
            self.num_fmt = Some(v.to_string());
        }
        if let Some(v) = spec.lvl_text {
            self.lvl_text = Some(v.to_string());
        }
        if let Some(v) = spec.font.filter(|f| !f.trim().is_empty()) {
            self.font = Some(v.to_string());
        }
    }

    /// True when the level needs a full `w:lvlOverride/w:lvl`, i.e. when the
    /// marker itself differs from the abstract definition. A plain restart only
    /// needs a `w:startOverride`.
    fn overrides_marker(&self) -> bool {
        self.num_fmt.is_some() || self.lvl_text.is_some() || self.font.is_some()
    }
}

/// One list tree discovered while rendering the body.
#[derive(Debug, Default)]
struct ListInstance {
    ordered: bool,
    levels: BTreeMap<u8, LevelOverride>,
}

thread_local! {
    /// List instances of the export currently being rendered on this thread.
    static INSTANCES: RefCell<Vec<ListInstance>> = const { RefCell::new(Vec::new()) };
}

/// Allocate the `w:numId` of a new list tree. Bullets and numbers get their own
/// instance so that two sibling lists never continue each other's counters.
pub(crate) fn alloc_list(ordered: bool, level0: &LevelSpec<'_>) -> String {
    let index = INSTANCES.with(|cell| {
        let mut v = cell.borrow_mut();
        let mut inst = ListInstance { ordered, levels: BTreeMap::new() };
        if !level0.is_empty() {
            inst.levels.entry(0).or_default().apply(level0);
        }
        v.push(inst);
        v.len() - 1
    });
    (FIRST_INSTANCE_ID as usize + index).to_string()
}

/// Record what a nested list says about its own level, so the exported `w:num`
/// can override the default marker or restart the counter there.
pub(crate) fn set_level(num_id: &str, ilvl: u8, spec: &LevelSpec<'_>) {
    if spec.is_empty() || ilvl > MAX_ILVL {
        return;
    }
    let Some(index) = instance_index(num_id) else {
        return;
    };
    INSTANCES.with(|cell| {
        if let Some(inst) = cell.borrow_mut().get_mut(index) {
            inst.levels.entry(ilvl).or_default().apply(spec);
        }
    });
}

/// Whether a `w:numId` numbers its items (as opposed to bulleting them). Lets
/// `list.rs` decide, without extra parameters, whether a nested list continues
/// its parent definition or needs one of its own.
pub(crate) fn is_ordered(num_id: &str) -> bool {
    match instance_index(num_id) {
        Some(i) => INSTANCES.with(|cell| cell.borrow().get(i).is_some_and(|l| l.ordered)),
        None => num_id == ORDERED_NUM_ID,
    }
}

/// Registry index behind an allocated `w:numId`, if it is one.
fn instance_index(num_id: &str) -> Option<usize> {
    let id: u32 = num_id.parse().ok()?;
    id.checked_sub(FIRST_INSTANCE_ID).map(|i| i as usize)
}

/// A single `w:lvl` of an abstract definition.
struct Lvl {
    ilvl: u8,
    start: i64,
    num_fmt: String,
    lvl_text: String,
    jc: &'static str,
    ind_left: i64,
    hanging: i64,
    /// Only bullets need a font: the glyph lives in a private-use code point
    /// that only Symbol / Wingdings map to anything.
    font: Option<String>,
}

impl Lvl {
    /// Word's own defaults for level `ilvl` of a bullet / numbered list.
    fn default_for(ordered: bool, ilvl: u8) -> Lvl {
        let ind_left = 720 * (i64::from(ilvl) + 1);
        if ordered {
            // 1. / a. / i. — the roman level is right-aligned in a narrower
            // hanging indent, exactly as Word writes it.
            let (num_fmt, jc, hanging) = match ilvl % 3 {
                0 => ("decimal", "left", 360),
                1 => ("lowerLetter", "left", 360),
                _ => ("lowerRoman", "right", 180),
            };
            Lvl {
                ilvl,
                start: 1,
                num_fmt: num_fmt.to_string(),
                // `%N` is the counter of level N-1; one digit only, `%10` does
                // not exist. Each level shows its own counter, like Word.
                lvl_text: format!("%{}.", ilvl + 1),
                jc,
                ind_left,
                hanging,
                font: None,
            }
        } else {
            // U+F0B7 in Symbol = "•", "o" in Courier New, U+F0A7 in Wingdings
            // = "▪": the three glyphs Word cycles through.
            let (glyph, font) = match ilvl % 3 {
                0 => ("\u{F0B7}", "Symbol"),
                1 => ("o", "Courier New"),
                _ => ("\u{F0A7}", "Wingdings"),
            };
            Lvl {
                ilvl,
                start: 1,
                num_fmt: "bullet".to_string(),
                lvl_text: glyph.to_string(),
                jc: "left",
                ind_left,
                hanging: 360,
                font: Some(font.to_string()),
            }
        }
    }

    fn apply(&mut self, o: &LevelOverride) {
        if let Some(v) = o.start {
            self.start = v;
        }
        if let Some(v) = &o.num_fmt {
            self.num_fmt = v.clone();
        }
        if let Some(v) = &o.lvl_text {
            self.lvl_text = v.clone();
        }
        if let Some(v) = &o.font {
            self.font = Some(v.clone());
        }
        // A number never carries a bullet font, and a bullet whose glyph came
        // from the import must not keep Symbol: the glyph would be a tofu.
        if self.num_fmt != "bullet" && o.num_fmt.is_some() {
            self.font = None;
        }
    }

    /// `<w:lvl>`. Child order is imposed by `CT_Lvl` (ECMA-376 §17.9.6):
    /// start, numFmt, lvlRestart, pStyle, isLgl, suff, lvlText, lvlPicBulletId,
    /// legacy, lvlJc, pPr, rPr. Word refuses to open a document that reorders
    /// them.
    fn render(&self) -> String {
        let mut s = format!(
            r#"<w:lvl w:ilvl="{}"><w:start w:val="{}"/><w:numFmt w:val="{}"/><w:lvlText w:val="{}"/><w:lvlJc w:val="{}"/><w:pPr><w:ind w:left="{}" w:hanging="{}"/></w:pPr>"#,
            self.ilvl,
            self.start,
            attr(&self.num_fmt),
            attr(&self.lvl_text),
            self.jc,
            self.ind_left,
            self.hanging,
        );
        if let Some(f) = self.font.as_deref().map(attr) {
            s.push_str(&format!(
                r#"<w:rPr><w:rFonts w:ascii="{f}" w:hAnsi="{f}" w:cs="{f}" w:hint="default"/></w:rPr>"#
            ));
        }
        s.push_str("</w:lvl>");
        s
    }
}

/// Attribute text: XML-escaped, then every non-ASCII character turned into a
/// numeric reference — the way Word writes its private-use bullet glyphs
/// (`&#xF0B7;`), which also keeps the part legible in any editor.
fn attr(s: &str) -> String {
    let escaped = escape_xml_attr(s);
    if escaped.is_ascii() {
        return escaped;
    }
    let mut out = String::with_capacity(escaped.len() + 8);
    for c in escaped.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            out.push_str(&format!("&#x{:04X};", c as u32));
        }
    }
    out
}

/// `<w:abstractNum>` with its nine levels.
fn abstract_num(id: u32, ordered: bool) -> String {
    let levels: String = (0..=MAX_ILVL)
        .map(|i| Lvl::default_for(ordered, i).render())
        .collect();
    // `multiLevelType` only drives Word's list gallery; LibreOffice ignores it
    // (NumberingManager.cxx:947-951).
    format!(
        r#"<w:abstractNum w:abstractNumId="{id}"><w:multiLevelType w:val="hybridMultilevel"/>{levels}</w:abstractNum>"#
    )
}

/// `<w:num>` for one allocated list tree.
///
/// Numbered trees get a `w:lvlOverride` on EVERY level: a `w:startOverride`
/// is what makes the tree restart at 1 instead of continuing the previous list,
/// and Word mis-numbers definitions whose `w:ilvl` values have holes — hence
/// the filler overrides LibreOffice emits too (`docxattributeoutput.cxx`
/// 8027-8037). Bullet trees only get one when they actually override a marker.
fn instance_num(num_id: u32, inst: &ListInstance) -> String {
    let abstract_id = if inst.ordered { ABSTRACT_ORDERED } else { ABSTRACT_BULLET };
    let mut s = format!(
        r#"<w:num w:numId="{num_id}"><w:abstractNumId w:val="{abstract_id}"/>"#
    );
    let needs_all = inst.ordered || inst.levels.values().any(LevelOverride::overrides_marker);
    if needs_all {
        for ilvl in 0..=MAX_ILVL {
            let over = inst.levels.get(&ilvl);
            // An absent `w:startOverride` is read as 0 by Word (tdf#153104),
            // so it is always written.
            let start = over.and_then(|o| o.start).unwrap_or(1);
            s.push_str(&format!(
                r#"<w:lvlOverride w:ilvl="{ilvl}"><w:startOverride w:val="{start}"/>"#
            ));
            // A `w:lvlOverride/w:lvl` REPLACES the abstract level in Word, so a
            // complete level is written, not just the changed properties.
            if let Some(o) = over.filter(|o| o.overrides_marker()) {
                let mut lvl = Lvl::default_for(inst.ordered, ilvl);
                lvl.apply(o);
                lvl.start = start;
                s.push_str(&lvl.render());
            }
            s.push_str("</w:lvlOverride>");
        }
    }
    s.push_str("</w:num>");
    s
}

/// The whole part. Consumes the instances registered by `list.rs`, so it must
/// be called once, after the body has been rendered.
pub(crate) fn numbering_xml() -> String {
    let instances = INSTANCES.with(|cell| std::mem::take(&mut *cell.borrow_mut()));

    // `CT_Numbering` order: numPicBullet*, abstractNum*, num*.
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
    );
    s.push_str(&abstract_num(ABSTRACT_BULLET, false));
    s.push_str(&abstract_num(ABSTRACT_ORDERED, true));
    // The two fallbacks: a list item rendered outside any list still needs a
    // definition to point at.
    s.push_str(&format!(
        r#"<w:num w:numId="{BULLET_NUM_ID}"><w:abstractNumId w:val="{ABSTRACT_BULLET}"/></w:num>"#
    ));
    s.push_str(&format!(
        r#"<w:num w:numId="{ORDERED_NUM_ID}"><w:abstractNumId w:val="{ABSTRACT_ORDERED}"/></w:num>"#
    ));
    for (i, inst) in instances.iter().enumerate() {
        s.push_str(&instance_num(FIRST_INSTANCE_ID + i as u32, inst));
    }
    s.push_str("</w:numbering>");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every test must start from an empty registry: they share the thread.
    fn reset() {
        let _ = numbering_xml();
    }

    #[test]
    fn part_declares_the_two_fallbacks_and_all_levels() {
        reset();
        let xml = numbering_xml();
        assert!(xml.contains(r#"<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>"#));
        assert!(xml.contains(r#"<w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>"#));
        for ilvl in 0..=8 {
            assert!(xml.contains(&format!(r#"<w:lvl w:ilvl="{ilvl}">"#)));
        }
        // Word's bullet cycle, as numeric references.
        assert!(xml.contains(r#"<w:lvlText w:val="&#xF0B7;"/>"#));
        assert!(xml.contains(r#"<w:lvlText w:val="&#xF0A7;"/>"#));
        assert!(xml.contains(r#"w:ascii="Courier New""#));
    }

    #[test]
    fn each_list_gets_its_own_num_and_restarts() {
        reset();
        let a = alloc_list(true, &LevelSpec::default());
        let b = alloc_list(true, &LevelSpec { start: Some(5), ..LevelSpec::default() });
        assert_eq!((a.as_str(), b.as_str()), ("3", "4"));
        assert!(is_ordered(&a));
        let xml = numbering_xml();
        assert!(xml.contains(r#"<w:num w:numId="3"><w:abstractNumId w:val="1"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="1"/>"#));
        assert!(xml.contains(r#"<w:num w:numId="4"><w:abstractNumId w:val="1"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="5"/>"#));
        // Reading the part clears the registry.
        assert!(!numbering_xml().contains(r#"w:numId="3""#));
    }

    #[test]
    fn imported_level_format_overrides_the_default_marker() {
        reset();
        let id = alloc_list(true, &LevelSpec::default());
        set_level(
            &id,
            1,
            &LevelSpec { num_fmt: Some("upperRoman"), lvl_text: Some("%2)"), ..LevelSpec::default() },
        );
        // Unknown formats are dropped rather than written out.
        set_level(&id, 2, &LevelSpec { num_fmt: Some("bogus"), ..LevelSpec::default() });
        let xml = numbering_xml();
        assert!(xml.contains(r#"<w:numFmt w:val="upperRoman"/><w:lvlText w:val="%2)"/>"#));
        assert!(!xml.contains("bogus"));
    }

    #[test]
    fn bullet_lists_stay_lightweight() {
        reset();
        let id = alloc_list(false, &LevelSpec::default());
        assert!(!is_ordered(&id));
        let xml = numbering_xml();
        assert!(xml.contains(r#"<w:num w:numId="3"><w:abstractNumId w:val="0"/></w:num>"#));
    }
}
