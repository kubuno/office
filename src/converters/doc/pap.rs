//! Paragraph properties — the `PAP` of a `.doc` and its translation to editor
//! attributes.
//!
//! A paragraph's formatting is never stored as a record: it is a *sequence of
//! modifications*. Word starts from the style's PAP and replays the paragraph's
//! `grpprl` on top of it, one sprm at a time. [`Pap`] is that accumulator; a
//! caller builds one per paragraph (style first, then the direct PAPX) and
//! finally asks for [`Pap::to_attrs`].
//!
//! Dispatch goes through [`SprmKind`], so the same code reads Word 6/95 and
//! Word 97+ documents even though their opcodes share no numbers at all; only
//! the handful of properties Word 97 added are matched by raw opcode.
//!
//! Units are the trap of this format. Everything horizontal or vertical is in
//! **twips** (1440 per inch ⇒ 15 per CSS pixel), line spacing is a *signed*
//! twip count whose sign chooses the rule, border widths are eighths of a
//! point and border spacing whole points, and shading is a pair of palette
//! indices blended through a percentage table. Reference: LibreOffice
//! `sw/source/filter/ww8/ww8par6.cxx` (`Read_LR`, `Read_UL`, `Read_LineSpace`,
//! `Read_ParaAutoBefore`, `Read_Border`, `Read_Shade`, `SwWW8Shade::SetShade`),
//! `ww8par.cxx` (`Read_Tab`) and `ww8scan.cxx`.
//!
//! The attribute names produced here are **exactly** those the DOCX importer
//! emits (`converters::docx::read::paragraph::parse_ppr_attrs`) so that the
//! same paragraph renders identically whichever format it came from.

// `table.rs`, `style.rs` and `list.rs` read the raw fields of `Pap` rather than
// its attribute map; they are being written alongside this module, so parts of
// the surface look unused for now.
#![allow(dead_code)]

use serde_json::{json, Map, Value};

use super::sprm::{iter_sprms_ver, ww6, ww8, Sprm, SprmKind as K, SprmVersion};

/// Twips per CSS pixel — 1440 twips/inch ÷ 96 px/inch.
const TWIP_PER_PX: f64 = 15.0;

/// Sides of a paragraph frame, in Word's order (`BRC_Sides` of `ww8struc.hxx`).
const TOP: usize = 0;
const LEFT: usize = 1;
const BOTTOM: usize = 2;
const RIGHT: usize = 3;
const BETWEEN: usize = 4;
const BAR: usize = 5;

// ---------------------------------------------------------------------------
// Sub-structures
// ---------------------------------------------------------------------------

/// How Word measures the distance between baselines (`LSPD`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineRule {
    /// `fMultLinespace = 1`: a multiplier, 240 = single.
    Multiple,
    /// Positive `dyaLine`: the line is at least that tall.
    AtLeast,
    /// Negative `dyaLine`: the line is exactly `|dyaLine|` tall.
    Exactly,
}

/// The decoded `LSPD` of `sprmPDyaLine`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LineSpacing {
    pub(crate) rule: LineRule,
    /// Twips for `AtLeast`/`Exactly` (always positive), 240ths for `Multiple`.
    pub(crate) value: i32,
}

/// One tab stop, as stored (position in twips).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TabStop {
    pub(crate) pos: i32,
    /// `left` · `center` · `right` · `decimal` · `bar`.
    pub(crate) kind: &'static str,
    /// Fill character: `dot` · `hyphen` · `underscore`, `None` for spaces.
    pub(crate) leader: Option<&'static str>,
}

/// A border code, normalised to the WW9 shape whatever version wrote it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Brc {
    /// Colour as `0xRRGGBB`, `None` when automatic.
    pub(crate) color: Option<u32>,
    /// Line width in eighths of a point.
    pub(crate) width_8pt: u8,
    /// Line pattern ([MS-DOC] `brcType`): 0 = none, 1 = single, 3 = double…
    pub(crate) brc_type: u8,
    /// Distance between the border and the text, in whole points.
    pub(crate) space_pt: u8,
    pub(crate) shadow: bool,
    /// Came from a `sprmPBrc*` (Word 2000) rather than a `sprmPBrc*80`.
    /// Word replays both and lets the newer one win, whatever the order.
    from_v9: bool,
}

impl Brc {
    /// `brcType` 0 draws nothing; `0xFF` is the "nil" code Word writes to
    /// *cancel* a border inherited from the style.
    fn is_visible(&self) -> bool {
        self.brc_type != 0 && self.brc_type != 0xFF
    }

    /// Same appearance, whichever sprm carried it.
    fn same_look(&self, other: &Brc) -> bool {
        (self.color, self.width_8pt, self.brc_type, self.space_pt)
            == (other.color, other.width_8pt, other.brc_type, other.space_pt)
    }

    /// The four line patterns the editor can draw, from the ~27 Word has
    /// (`editeng/source/items/borderline.cxx`, `ConvertBorderStyleFromWord`).
    fn style(&self) -> &'static str {
        match self.brc_type {
            6 => "dotted",
            7 | 8 | 9 | 22 => "dashed",
            // Double, triple, double wave, shading beams and every
            // thin/thick combination all draw as two visible lines.
            3 | 10..=19 | 21 | 23 => "double",
            _ => "solid",
        }
    }
}

/// A shading descriptor: two colours blended by a pattern index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Shd {
    pub(crate) fore: Option<u32>,
    pub(crate) back: Option<u32>,
    pub(crate) ipat: u16,
    /// Came from `sprmPShd` rather than the legacy `sprmPShd80`.
    from_v9: bool,
}

// ---------------------------------------------------------------------------
// Pap
// ---------------------------------------------------------------------------

/// The accumulated paragraph properties.
///
/// Every field Word can leave unset is an `Option`, so a caller can tell "the
/// style said nothing" from "the style said zero" — which matters, since a
/// direct sprm setting an indent back to 0 must beat an inherited 720.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Pap {
    /// Index of the paragraph style this PAP started from.
    pub(crate) istd: u16,

    /// Justification code and whether it was written bidi-relative (`sprmPJc`)
    /// rather than absolute (`sprmPJc80`).
    pub(crate) jc: Option<(u8, bool)>,
    /// Right-to-left paragraph (`sprmPFBiDi`) — flips a relative `jc`.
    pub(crate) bidi: bool,

    /// Indents, in twips. `dxa_left1` is the first line, negative = hanging.
    pub(crate) dxa_left: Option<i32>,
    pub(crate) dxa_right: Option<i32>,
    pub(crate) dxa_left1: Option<i32>,

    /// Space above and below, in twips.
    pub(crate) dya_before: Option<i32>,
    pub(crate) dya_after: Option<i32>,
    /// "Auto" spacing replaces the explicit value with a fixed one.
    pub(crate) auto_before: bool,
    pub(crate) auto_after: bool,

    pub(crate) line_spacing: Option<LineSpacing>,

    /// Pagination.
    pub(crate) keep_next: Option<bool>,
    pub(crate) keep_lines: Option<bool>,
    pub(crate) page_break_before: Option<bool>,
    pub(crate) widow_control: Option<bool>,
    pub(crate) suppress_line_numbers: Option<bool>,
    pub(crate) dont_hyphenate: Option<bool>,
    pub(crate) contextual_spacing: Option<bool>,
    pub(crate) mirror_indents: Option<bool>,

    /// Outline level, 0-8 for levels 1-9, 9 for body text.
    pub(crate) outline_level: Option<u8>,

    /// List membership, for `list.rs`: level and list-format override index.
    pub(crate) ilvl: Option<u8>,
    pub(crate) ilfo: Option<u16>,

    /// Absolute set of tab stops, sorted by position.
    pub(crate) tabs: Vec<TabStop>,

    /// Top, left, bottom, right, between, bar.
    pub(crate) borders: [Option<Brc>; 6],
    pub(crate) shd: Option<Shd>,

    /// Drop cap: (position — 1 in text, 2 in margin — and dropped line count).
    pub(crate) drop_cap: Option<(u8, u8)>,

    /// Table membership — consumed by `table.rs`, never rendered directly.
    pub(crate) f_in_table: bool,
    /// This paragraph mark is a table row terminator.
    pub(crate) f_ttp: bool,
    /// Nesting depth: 0 outside any table, 1 in a top-level table, 2 nested…
    pub(crate) itap: u32,
}

impl Pap {
    /// A PAP inheriting from style `istd` with nothing set yet.
    pub(crate) fn with_style(istd: u16) -> Pap {
        Pap { istd, ..Pap::default() }
    }

    /// Replay a Word 97+ `grpprl` on top of the current state.
    pub(crate) fn apply_grpprl(&mut self, grpprl: &[u8]) {
        self.apply_grpprl_ver(grpprl, SprmVersion::Ww8);
    }

    /// Replay a `grpprl` written in a given sprm encoding.
    ///
    /// Unknown and non-paragraph sprms are skipped, not ignored: the iterator
    /// knows each operand's length from the opcode, so one property we do not
    /// implement cannot shift the ones after it.
    pub(crate) fn apply_grpprl_ver(&mut self, grpprl: &[u8], ver: SprmVersion) {
        for s in iter_sprms_ver(grpprl, ver) {
            self.apply_sprm(&s, ver);
        }
    }

    /// Replay a single sprm.
    pub(crate) fn apply_sprm(&mut self, s: &Sprm<'_>, ver: SprmVersion) {
        let o = s.operand;
        match s.kind(ver) {
            K::ParaStyle => {
                if let Some(v) = s.u16() {
                    self.istd = v;
                }
            }
            // `sprmPJc` is written relative to the reading direction;
            // `sprmPJc80` and the Word 6/95 opcode are absolute.
            K::ParaJustify => {
                let relative = ver == SprmVersion::Ww8 && s.opcode == ww8::PJC;
                self.jc = s.u8().map(|v| (v, relative));
            }
            K::ParaBidi => self.bidi = s.u8().is_some_and(|v| v != 0),

            // Indents, a signed twip count (`Read_LR`).
            K::ParaIndentLeft => self.dxa_left = s.i16().map(i32::from),
            K::ParaIndentRight => self.dxa_right = s.i16().map(i32::from),
            K::ParaIndentFirstLine => self.dxa_left1 = s.i16().map(i32::from),

            // Spacing. Word writes these as words but reads them signed and
            // takes the absolute value (`Read_UL`).
            K::ParaSpaceBefore => self.dya_before = s.i16().map(|v| i32::from(v).abs()),
            K::ParaSpaceAfter => self.dya_after = s.i16().map(|v| i32::from(v).abs()),
            K::ParaSpaceBeforeAuto => self.auto_before = s.u8().is_some_and(|v| v != 0),
            K::ParaSpaceAfterAuto => self.auto_after = s.u8().is_some_and(|v| v != 0),
            K::ParaLineSpacing => self.line_spacing = decode_lspd(o),

            // Pagination.
            K::ParaKeepLines => self.keep_lines = s.u8().map(|v| v != 0),
            K::ParaKeepNext => self.keep_next = s.u8().map(|v| v != 0),
            K::ParaPageBreakBefore => self.page_break_before = s.u8().map(|v| v != 0),
            K::ParaWidowControl => self.widow_control = s.u8().map(|v| v != 0),
            K::ParaContextualSpacing => self.contextual_spacing = s.u8().map(|v| v != 0),

            K::ParaOutlineLevel => self.outline_level = s.u8(),
            K::ParaListLevel => self.ilvl = s.u8(),
            K::ParaListIndex => self.ilfo = s.u16(),

            // `sprmPChgTabs` inserts a second deletion array the PAPX form does
            // not have, so the stride differs ([MS-DOC] 2.6.2, PChgTabsOperand
            // vs PChgTabsPapxOperand).
            K::ParaTabs => {
                let complex = s.opcode
                    == if ver == SprmVersion::Ww8 {
                        ww8::PCHGTABS
                    } else {
                        u16::from(ww6::PCHGTABS)
                    };
                self.apply_tabs(o, if complex { 4 } else { 2 });
            }

            K::ParaBorderTop => self.set_border(TOP, brc(o)),
            K::ParaBorderLeft => self.set_border(LEFT, brc(o)),
            K::ParaBorderBottom => self.set_border(BOTTOM, brc(o)),
            K::ParaBorderRight => self.set_border(RIGHT, brc(o)),
            K::ParaBorderBetween => self.set_border(BETWEEN, brc(o)),

            // The 10-byte WW9 shading wins over the 2-byte legacy one whatever
            // the order — `Read_Shade` bails out as soon as `sprmPShd` exists.
            K::ParaShading => {
                let new = if o.len() >= 10 { shd9(o) } else { s.u16().map(|v| shd80(v, ver)) };
                if let Some(v) = new {
                    if v.from_v9 || !self.shd.is_some_and(|old| old.from_v9) {
                        self.shd = Some(v);
                    }
                }
            }

            // Table membership. `itap` is authoritative when present; the older
            // boolean only says "depth ≥ 1", and the inner-cell / inner-row
            // sprms are how Word 2000+ marks nesting.
            K::ParaInTable => {
                self.f_in_table = s.u8().is_some_and(|v| v != 0);
                self.itap = if self.f_in_table { self.itap.max(1) } else { 0 };
            }
            K::ParaTableRowEnd => self.f_ttp = s.u8().is_some_and(|v| v != 0),
            K::ParaTableDepth => {
                if let Some(v) = s.u32() {
                    self.itap = v;
                    self.f_in_table = v > 0;
                }
            }
            K::ParaInnerTableCell => {
                if s.u8().is_some_and(|v| v != 0) {
                    self.f_in_table = true;
                    self.itap = self.itap.max(2);
                }
            }
            K::ParaInnerTableRowEnd => {
                if s.u8().is_some_and(|v| v != 0) {
                    self.f_ttp = true;
                    self.itap = self.itap.max(2);
                }
            }

            // Everything else is either a character/table/section sprm or one
            // of the few paragraph properties with no `SprmKind` of its own.
            _ => self.apply_unmapped(s, ver),
        }
    }

    /// Paragraph sprms `SprmKind` does not name, matched by raw Word 97 opcode.
    /// Word 6/95 opcodes are single bytes and can never collide with these.
    fn apply_unmapped(&mut self, s: &Sprm<'_>, ver: SprmVersion) {
        if ver != SprmVersion::Ww8 {
            return;
        }
        match s.opcode {
            ww8::PBRCBAR | ww8::PBRCBAR80 => self.set_border(BAR, brc(s.operand)),
            ww8::PFNOLINENUMB => self.suppress_line_numbers = s.u8().map(|v| v != 0),
            // sic: `fNoAutoHyph` set means hyphenation OFF (`Read_Hyphenation`).
            ww8::PFNOAUTOHYPH => self.dont_hyphenate = s.u8().map(|v| v != 0),
            ww8::PFMIRRORINDENTS => self.mirror_indents = s.u8().map(|v| v != 0),
            // DCS: `fdct` in bits 0-2, dropped line count in bits 3-7.
            ww8::PDCS => {
                if let Some(v) = s.u16() {
                    let (kind, lines) = ((v & 0x07) as u8, ((v >> 3) & 0x1F) as u8);
                    self.drop_cap = (kind != 0).then_some((kind, lines));
                }
            }
            _ => {}
        }
    }

    /// A WW9 border code overrides a WW8 one for the same side, never the
    /// other way round (`lcl_ReadBorders`: "Version 9 BRCs if present will
    /// override version 8").
    fn set_border(&mut self, side: usize, new: Option<Brc>) {
        let Some(b) = new else { return };
        if b.from_v9 || !self.borders[side].is_some_and(|old| old.from_v9) {
            self.borders[side] = Some(b);
        }
    }

    /// `sprmPChgTabsPapx` / `sprmPChgTabs` — the one variable-length paragraph
    /// operand whose layout is not self-describing:
    ///
    /// ```text
    /// cTabsDel : u8 | rgdxaDel : i16 × cTabsDel [ | rgdxaClose : i16 × cTabsDel ]
    /// cTabsAdd : u8 | rgdxaAdd : i16 × cTabsAdd | rgtbdAdd : u8 × cTabsAdd
    /// ```
    ///
    /// Deletions come first and are matched by position; the type bytes sit
    /// *after* the whole add-position array, not interleaved with it. Reading
    /// them interleaved is the classic way to lose the rest of a grpprl —
    /// hence the length check up front, as `SwWW8ImplReader::Read_Tab` does.
    /// `del_stride` is 2 for the PAPX form and 4 for the complex one, which
    /// carries a second array of "closing" positions.
    fn apply_tabs(&mut self, data: &[u8], del_stride: usize) {
        let n = data.len();
        let Some(&c_del) = data.first() else { return };
        let c_del = c_del as usize;
        let add_count_at = 1 + del_stride * c_del;
        if n <= add_count_at {
            return;
        }
        let c_add = data[add_count_at] as usize;
        let add_at = add_count_at + 1;
        let typ_at = add_at + 2 * c_add;
        if n < typ_at + c_add {
            // Would need more bytes than the operand has: the record is
            // corrupt, and guessing would produce phantom stops.
            return;
        }

        for i in 0..c_del {
            let at = 1 + del_stride * i;
            let pos = i32::from(i16::from_le_bytes([data[at], data[at + 1]]));
            self.tabs.retain(|t| t.pos != pos);
        }
        for i in 0..c_add {
            let at = add_at + 2 * i;
            let pos = i32::from(i16::from_le_bytes([data[at], data[at + 1]]));
            let tbd = data[typ_at + i];
            let kind = match tbd & 0x07 {
                1 => "center",
                2 => "right",
                3 => "decimal",
                4 => "bar",
                _ => "left",
            };
            let leader = match (tbd >> 3) & 0x07 {
                1 => Some("dot"),
                2 => Some("hyphen"),
                3 | 4 => Some("underscore"),
                _ => None,
            };
            // An added stop replaces any stop already at that position.
            self.tabs.retain(|t| t.pos != pos);
            self.tabs.push(TabStop { pos, kind, leader });
        }
        self.tabs.sort_by_key(|t| t.pos);
    }

    /// The editor's paragraph attributes.
    ///
    /// Names and units are those of `parse_ppr_attrs` on the DOCX side: twips
    /// divided by 15 give pixels, `lineHeight` is a bare multiplier, and
    /// `lineSpacingPt` — despite its name — is a pixel value there too.
    pub(crate) fn to_attrs(&self) -> Map<String, Value> {
        self.to_attrs_with(AUTO_SPACE_TWIPS)
    }

    /// Same, with an explicit value for Word's "automatic" paragraph spacing —
    /// the one paragraph property that cannot be decided without the `Dop`.
    ///
    /// Two more document-level rules live above this layer and cancel that
    /// spacing entirely: Word drops it on the very first paragraph, and
    /// between two items of the same list (`ww8par.cxx`, `m_bFirstPara` /
    /// `m_pPrevNumRule`).
    pub(crate) fn to_attrs_with(&self, auto_space: i32) -> Map<String, Value> {
        let mut m = Map::new();

        if let Some(a) = self.text_align() {
            m.insert("textAlign".into(), json!(a));
        }

        // Indents: zero is the default, so only a real offset is worth storing.
        if let Some(v) = self.dxa_left.map(px).filter(|v| *v != 0.0) {
            m.insert("indentLeft".into(), json!(v));
        }
        if let Some(v) = self.dxa_right.map(px).filter(|v| *v != 0.0) {
            m.insert("indentRight".into(), json!(v));
        }
        if let Some(v) = self.dxa_left1.map(px).filter(|v| *v != 0.0) {
            m.insert("indentFirstLine".into(), json!(v));
        }

        // Spacing. "Auto" is Word's HTML-ish spacing; it replaces the stored
        // value rather than adding to it (`Read_ParaAutoBefore`).
        let before = if self.auto_before { Some(auto_space) } else { self.dya_before };
        let after = if self.auto_after { Some(auto_space) } else { self.dya_after };
        if let Some(v) = before {
            m.insert("spaceBefore".into(), json!(px(v)));
        }
        if let Some(v) = after {
            m.insert("spaceAfter".into(), json!(px(v)));
        }

        if let Some(ls) = self.line_spacing {
            match ls.rule {
                LineRule::Multiple => {
                    // 240 = single, as in OOXML's `lineRule="auto"`.
                    let mult = (f64::from(ls.value) / 240.0 * 100.0).round() / 100.0;
                    if mult > 0.0 {
                        m.insert("lineHeight".into(), json!(mult));
                    }
                }
                LineRule::Exactly | LineRule::AtLeast => {
                    let mode = if ls.rule == LineRule::Exactly { "exactly" } else { "atLeast" };
                    m.insert("lineSpacingMode".into(), json!(mode));
                    m.insert("lineSpacingPt".into(), json!(px(ls.value)));
                }
            }
        }

        // Booleans the editor treats as off by default: store only the "on".
        for (name, v) in [
            ("keepNext", self.keep_next),
            ("keepLines", self.keep_lines),
            ("pageBreakBefore", self.page_break_before),
            ("suppressLineNumbers", self.suppress_line_numbers),
            ("dontHyphenate", self.dont_hyphenate),
            ("contextualSpacing", self.contextual_spacing),
            ("mirrorIndents", self.mirror_indents),
        ] {
            if v == Some(true) {
                m.insert(name.into(), json!(true));
            }
        }
        // widowControl is on by default: only the opt-out is meaningful.
        if self.widow_control == Some(false) {
            m.insert("widowControl".into(), json!(false));
        }

        // Word numbers outline levels 0-8 and uses 9 for "body text"; the
        // editor numbers them 1-9 with nothing for body text.
        if let Some(l) = self.outline_level.filter(|l| *l < 9) {
            m.insert("outlineLevel".into(), json!(i64::from(l) + 1));
        }

        if !self.tabs.is_empty() {
            let stops: Vec<Value> = self
                .tabs
                .iter()
                .map(|t| {
                    let mut o = Map::new();
                    o.insert("pos".into(), json!(px(t.pos)));
                    o.insert("type".into(), json!(t.kind));
                    if let Some(l) = t.leader {
                        o.insert("leader".into(), json!(l));
                    }
                    Value::Object(o)
                })
                .collect();
            m.insert("tabStops".into(), Value::Array(stops));
        }

        if let Some(b) = self.para_border() {
            m.insert("paraBorder".into(), b);
        }
        if let Some(c) = self.shading() {
            m.insert("shading".into(), json!(c));
        }
        m
    }

    /// Justification. In an RTL paragraph a relative `jc` names the reading
    /// start and end rather than the left and right edges (`Read_RTLJustify`);
    /// `sprmPJc80` stays absolute even there.
    fn text_align(&self) -> Option<&'static str> {
        let (v, relative) = self.jc?;
        let rtl = relative && self.bidi;
        Some(match v {
            1 => "center",
            2 if rtl => "left",
            2 => "right",
            3 | 4 => "justify",
            _ if rtl => "right",
            _ => "left",
        })
    }

    /// `paraBorder`, in the uniform `{color,width,style}` shape when the four
    /// sides agree — the only shape the canvas renderer draws — and in the
    /// per-side shape otherwise, which the DOCX writer still round-trips.
    fn para_border(&self) -> Option<Value> {
        let visible: Vec<(usize, Brc)> = (0..6)
            .filter_map(|i| self.borders[i].filter(Brc::is_visible).map(|b| (i, b)))
            .collect();
        if visible.is_empty() {
            return None;
        }
        let box_sides = [TOP, LEFT, BOTTOM, RIGHT];
        let uniform = visible.len() == 4
            && visible.iter().all(|(i, _)| box_sides.contains(i))
            && visible.windows(2).all(|w| w[0].1.same_look(&w[1].1));
        if uniform {
            return Some(brc_json(&visible[0].1, false));
        }
        let mut o = Map::new();
        for (i, b) in visible {
            let name = match i {
                TOP => "top",
                LEFT => "left",
                BOTTOM => "bottom",
                RIGHT => "right",
                BETWEEN => "between",
                _ => "bar",
            };
            o.insert(name.into(), brc_json(&b, true));
        }
        Some(Value::Object(o))
    }

    /// The flat colour Word would paint behind the paragraph: the foreground
    /// and background palette entries mixed by the pattern's percentage
    /// (`SwWW8Shade::SetShade`). An automatic result means "no fill".
    fn shading(&self) -> Option<String> {
        let shd = self.shd?;
        let pct = GRAY_SCALE.get(shd.ipat as usize).copied().unwrap_or(0);
        if pct == 0 {
            // Clear pattern: whatever the background says, and nothing at all
            // when that is automatic.
            return shd.back.map(hex);
        }
        // No "automatic" inside a fill: Word treats it as black on white.
        let fore = shd.fore.unwrap_or(0x00_0000);
        let back = shd.back.unwrap_or(0xFF_FFFF);
        let mix = |shift: u32| -> u32 {
            let (f, b) = ((fore >> shift) & 0xFF, (back >> shift) & 0xFF);
            (f * pct + b * (1000 - pct)) / 1000
        };
        Some(hex((mix(16) << 16) | (mix(8) << 8) | mix(0)))
    }
}

/// Default value of Word's "automatic" paragraph spacing, in twips.
///
/// It depends on the document's `fDontUseHTMLAutoSpacing`
/// (`GetParagraphAutoSpace`): 100 twips (5 pt) when the compatibility option
/// is on, 280 (14 pt) when it is off — which is Word's own default and what
/// the LibreOffice corpus overwhelmingly shows. A caller holding the `Dop` can
/// pass the other value to [`Pap::to_attrs_with`].
pub(crate) const AUTO_SPACE_TWIPS: i32 = 280;

/// Twips → CSS pixels, rounded like the DOCX importer.
fn px(twips: i32) -> f64 {
    (f64::from(twips) / TWIP_PER_PX).round()
}

/// `0xRRGGBB` → `#rrggbb`.
fn hex(rgb: u32) -> String {
    format!("#{:06x}", rgb & 0x00FF_FFFF)
}

/// `sprmPDyaLine` — a 4-byte `LSPD`: a **signed** twip count then a flag.
///
/// The sign is the rule: negative means "exactly |v|", positive "at least v".
/// When the flag word is 1 the count is not a length at all but 240ths of a
/// line (`Read_LineSpace`).
fn decode_lspd(operand: &[u8]) -> Option<LineSpacing> {
    let o = operand.get(..4)?;
    let dya = i32::from(i16::from_le_bytes([o[0], o[1]]));
    let multi = u16::from_le_bytes([o[2], o[3]]);
    Some(if multi == 1 {
        LineSpacing { rule: LineRule::Multiple, value: dya.max(0) }
    } else if dya < 0 {
        LineSpacing { rule: LineRule::Exactly, value: -dya }
    } else {
        LineSpacing { rule: LineRule::AtLeast, value: dya }
    })
}

/// A border code, in whichever of the three encodings its length says:
/// 8 bytes = Word 2000 (`BRCVer9`, a real colour), 4 = Word 97 (`BRC`, a
/// palette index), 2 = Word 6/95 (`BRCVer6`, a bitfield).
fn brc(o: &[u8]) -> Option<Brc> {
    match o.len() {
        0..=1 => None,
        2..=3 => Some(brc6(u16::from_le_bytes([o[0], o[1]]))),
        4..=7 => Some(Brc {
            color: ico_color(o[2]),
            width_8pt: o[0],
            brc_type: o[1],
            space_pt: o[3] & 0x1F,
            shadow: o[3] & 0x20 != 0,
            from_v9: false,
        }),
        _ => Some(Brc {
            color: colorref(&o[0..4]),
            width_8pt: o[4],
            brc_type: o[5],
            space_pt: o[6] & 0x1F,
            shadow: o[6] & 0x20 != 0,
            from_v9: true,
        }),
    }
}

/// Word 6/95's packed `BRCVer6`: width 0-2, type 3-4, shadow 5, colour 6-10,
/// spacing 11-15. A width above 5 is not a width at all but a dotted/dashed
/// code, and the width unit is 0.75 pt rather than an eighth of a point
/// (`WW8_BRC::WW8_BRC(const WW8_BRCVer6&)`).
fn brc6(v: u16) -> Brc {
    let mut width = (v & 0x07) as u8;
    let mut brc_type = ((v >> 3) & 0x03) as u8;
    if width > 5 {
        brc_type = width;
        width = 1;
    }
    Brc {
        color: ico_color(((v >> 6) & 0x1F) as u8),
        width_8pt: width.saturating_mul(6),
        brc_type,
        space_pt: ((v >> 11) & 0x1F) as u8,
        shadow: v & 0x20 != 0,
        from_v9: false,
    }
}

/// One border side as the editor's `{color,width,style[,space]}`.
///
/// Widths follow the DOCX writer's inverse: it emits `w:sz` (eighths of a
/// point) as `px × 6` and `w:space` (points) as `px × 0.75`.
fn brc_json(b: &Brc, with_space: bool) -> Value {
    let mut o = Map::new();
    o.insert("color".into(), json!(hex(b.color.unwrap_or(0x00_0000))));
    let w = (f64::from(b.width_8pt) / 6.0 * 100.0).round() / 100.0;
    o.insert("width".into(), json!(if w > 0.0 { w } else { 1.0 }));
    o.insert("style".into(), json!(b.style()));
    if with_space && b.space_pt > 0 {
        o.insert("space".into(), json!((f64::from(b.space_pt) / 0.75).round()));
    }
    Value::Object(o)
}

/// `sprmPShd80` — five bits of foreground, five of background, then the
/// pattern: six bits from Word 97 on, five in Word 6/95 (`WW8_SHD::GetStyle`).
fn shd80(v: u16, ver: SprmVersion) -> Shd {
    let mask = if ver == SprmVersion::Ww8 { 0x3F } else { 0x1F };
    Shd {
        fore: ico_color((v & 0x1F) as u8),
        back: ico_color(((v >> 5) & 0x1F) as u8),
        ipat: (v >> 10) & mask,
        from_v9: false,
    }
}

/// `sprmPShd` — two `COLORREF`s and a pattern index, ten bytes in all.
fn shd9(o: &[u8]) -> Option<Shd> {
    let o = o.get(..10)?;
    Some(Shd {
        fore: colorref(&o[0..4]),
        back: colorref(&o[4..8]),
        ipat: u16::from_le_bytes([o[8], o[9]]),
        from_v9: true,
    })
}

/// A Word `COLORREF`: R, G, B, then a byte that is `0xFF` for "automatic".
/// A transparent background does not show the page through — Word paints it
/// like automatic, which is why the same test covers both (`ExtractColour`).
fn colorref(b: &[u8]) -> Option<u32> {
    let b = b.get(..4)?;
    if b[3] == 0xFF {
        return None;
    }
    Some((u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]))
}

/// Word's 17-entry colour palette (`SwWW8ImplReader::GetCol`); index 0 is
/// "automatic", which has no colour of its own.
fn ico_color(ico: u8) -> Option<u32> {
    const PALETTE: [u32; 16] = [
        0x00_0000, 0x00_00FF, 0x00_FFFF, 0x00_FF00, 0xFF_00FF, 0xFF_0000, 0xFF_FF00, 0xFF_FFFF,
        0x00_0080, 0x00_8080, 0x00_8000, 0x80_0080, 0x80_0000, 0x80_8000, 0x80_8080, 0xC0_C0C0,
    ];
    (ico >= 1).then(|| PALETTE.get(ico as usize - 1).copied()).flatten()
}

/// Per-mille of foreground in each of Word's shading patterns, from
/// `SwWW8Shade::SetShade`. Index 0 is "clear", 1 "solid", 2-13 the round
/// percentages, 14-25 the hatch patterns (all rendered as one third), and the
/// tail the finer percentages Word 2000 added.
const GRAY_SCALE: [u32; 62] = [
    0, 1000, 50, 100, 200, 250, 300, 400, 500, 600, 700, 750, 800, 900, 333, 333, 333, 333, 333,
    333, 333, 333, 333, 333, 333, 333, 500, 500, 500, 500, 500, 500, 500, 500, 500, 25, 75, 125,
    150, 175, 225, 275, 325, 350, 375, 425, 450, 475, 525, 550, 575, 625, 650, 675, 725, 775, 825,
    850, 875, 925, 950, 975,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a Word 97 grpprl from (opcode, operand) pairs, inserting the
    /// length byte the variable-length encoding requires.
    fn grpprl(items: &[(u16, &[u8])]) -> Vec<u8> {
        let mut v = Vec::new();
        for (op, operand) in items {
            v.extend_from_slice(&op.to_le_bytes());
            if (op >> 13) & 0x07 == 6 {
                v.push(operand.len() as u8);
            }
            v.extend_from_slice(operand);
        }
        v
    }

    fn attrs(items: &[(u16, &[u8])]) -> Map<String, Value> {
        let mut p = Pap::default();
        p.apply_grpprl(&grpprl(items));
        p.to_attrs()
    }

    #[test]
    fn indents_and_spacing_are_twips_over_fifteen() {
        let a = attrs(&[
            (ww8::PDXALEFT, &720i16.to_le_bytes()),
            (ww8::PDXALEFT1, &(-360i16).to_le_bytes()),
            (ww8::PDXARIGHT, &180i16.to_le_bytes()),
            (ww8::PDYABEFORE, &120u16.to_le_bytes()),
            (ww8::PDYAAFTER, &0u16.to_le_bytes()),
        ]);
        assert_eq!(a["indentLeft"], json!(48.0));
        assert_eq!(a["indentFirstLine"], json!(-24.0));
        assert_eq!(a["indentRight"], json!(12.0));
        assert_eq!(a["spaceBefore"], json!(8.0));
        assert_eq!(a["spaceAfter"], json!(0.0));
    }

    /// The sign of `dyaLine` chooses the rule; the flag word overrides both.
    #[test]
    fn line_spacing_sign_selects_the_rule() {
        let exact = attrs(&[(ww8::PDYALINE, &[0xA0, 0xFF, 0x00, 0x00])]); // -96
        assert_eq!(exact["lineSpacingMode"], json!("exactly"));
        assert_eq!(exact["lineSpacingPt"], json!(6.0));

        let at_least = attrs(&[(ww8::PDYALINE, &[0x60, 0x00, 0x00, 0x00])]); // +96
        assert_eq!(at_least["lineSpacingMode"], json!("atLeast"));
        assert_eq!(at_least["lineSpacingPt"], json!(6.0));

        let mult = attrs(&[(ww8::PDYALINE, &[0x68, 0x01, 0x01, 0x00])]); // 360, fMult
        assert_eq!(mult["lineHeight"], json!(1.5));
        assert!(!mult.contains_key("lineSpacingMode"));
    }

    #[test]
    fn justification_and_bidi() {
        assert_eq!(attrs(&[(ww8::PJC80, &[3])])["textAlign"], json!("justify"));
        assert_eq!(attrs(&[(ww8::PJC80, &[2])])["textAlign"], json!("right"));
        // sprmPJc is relative: in an RTL paragraph "end" is the left edge.
        let rtl = attrs(&[(ww8::PFBIDI, &[1]), (ww8::PJC, &[2])]);
        assert_eq!(rtl["textAlign"], json!("left"));
        // …but sprmPJc80 stays absolute even then.
        let rtl80 = attrs(&[(ww8::PFBIDI, &[1]), (ww8::PJC80, &[2])]);
        assert_eq!(rtl80["textAlign"], json!("right"));
    }

    #[test]
    fn outline_level_nine_is_body_text() {
        assert_eq!(attrs(&[(ww8::POUTLVL, &[1])])["outlineLevel"], json!(2));
        assert!(!attrs(&[(ww8::POUTLVL, &[9])]).contains_key("outlineLevel"));
    }

    #[test]
    fn widow_control_stores_only_the_opt_out() {
        assert_eq!(attrs(&[(ww8::PFWIDOWCONTROL, &[0])])["widowControl"], json!(false));
        assert!(!attrs(&[(ww8::PFWIDOWCONTROL, &[1])]).contains_key("widowControl"));
    }

    /// Keep-with-next and keep-lines are two different sprms, easy to swap.
    #[test]
    fn keep_flags_are_not_swapped() {
        assert_eq!(attrs(&[(ww8::PFKEEPFOLLOW, &[1])])["keepNext"], json!(true));
        assert_eq!(attrs(&[(ww8::PFKEEP, &[1])])["keepLines"], json!(true));
    }

    /// The tab operand must be consumed exactly, or the sprms after it move.
    #[test]
    fn tab_stops_delete_then_add_without_desync() {
        let mut p = Pap::default();
        // Two stops: 1440 decimal + dot leader, 720 plain left.
        let add = [
            0x00u8, // cTabsDel
            0x02, // cTabsAdd
            0xA0, 0x05, // 1440
            0xD0, 0x02, // 720
            0x0B, // decimal (3) + dot leader (1 << 3)
            0x00, // left, no leader
        ];
        p.apply_grpprl(&grpprl(&[
            (ww8::PCHGTABSPAPX, &add),
            (ww8::PJC80, &[1]), // must still be seen
        ]));
        assert_eq!(
            p.tabs,
            vec![
                TabStop { pos: 720, kind: "left", leader: None },
                TabStop { pos: 1440, kind: "decimal", leader: Some("dot") },
            ]
        );
        assert_eq!(p.to_attrs()["textAlign"], json!("center"));

        // A later sprm deletes the 720 one.
        p.apply_grpprl(&grpprl(&[(ww8::PCHGTABSPAPX, &[0x01, 0xD0, 0x02, 0x00])]));
        assert_eq!(p.tabs.len(), 1);
        assert_eq!(p.tabs[0].pos, 1440);

        let a = p.to_attrs();
        assert_eq!(a["tabStops"], json!([{"pos": 96.0, "type": "decimal", "leader": "dot"}]));
    }

    /// `sprmPChgTabs` carries a second deletion array the PAPX form does not:
    /// using the wrong stride reads the add count out of the closing positions.
    #[test]
    fn complex_tab_sprm_has_four_byte_deletions() {
        let mut p = Pap::default();
        p.tabs.push(TabStop { pos: 720, kind: "left", leader: None });
        // One deletion (720, closing 1000), then one addition at 2160.
        let data = [0x01u8, 0xD0, 0x02, 0xE8, 0x03, 0x01, 0x70, 0x08, 0x00];
        p.apply_grpprl(&grpprl(&[(ww8::PCHGTABS, &data)]));
        assert_eq!(p.tabs, vec![TabStop { pos: 2160, kind: "left", leader: None }]);
    }

    /// A truncated tab record must be dropped whole rather than half-read.
    #[test]
    fn truncated_tab_record_is_ignored() {
        let mut p = Pap::default();
        p.apply_grpprl(&grpprl(&[(ww8::PCHGTABSPAPX, &[0x00, 0x02, 0xA0, 0x05])]));
        assert!(p.tabs.is_empty());
    }

    #[test]
    fn borders_uniform_and_per_side() {
        // Four identical single 1/2 pt black borders → the uniform shape.
        let b = [0x00u8, 0x00, 0x00, 0x00, 4, 1, 0, 0];
        let a = attrs(&[
            (ww8::PBRCTOP, &b),
            (ww8::PBRCLEFT, &b),
            (ww8::PBRCBOTTOM, &b),
            (ww8::PBRCRIGHT, &b),
        ]);
        assert_eq!(a["paraBorder"], json!({"color": "#000000", "width": 0.67, "style": "solid"}));

        // A lone bottom border keeps its side name.
        let dashed = [0xFFu8, 0x00, 0x00, 0x00, 6, 7, 0, 0]; // red, dashed
        let c = attrs(&[(ww8::PBRCBOTTOM, &dashed)]);
        assert_eq!(
            c["paraBorder"],
            json!({"bottom": {"color": "#ff0000", "width": 1.0, "style": "dashed"}})
        );
    }

    /// The Word 2000 border code wins over the Word 97 one for the same side,
    /// whatever order they appear in.
    #[test]
    fn new_border_overrides_the_legacy_one() {
        let old = [8u8, 1, 6, 0]; // 1 pt, single, red (palette 6)
        let new = [0x00u8, 0x00, 0xFF, 0x00, 8, 1, 0, 0]; // same, but true blue
        let a = attrs(&[(ww8::PBRCTOP, &new), (ww8::PBRCTOP80, &old)]);
        assert_eq!(
            a["paraBorder"],
            json!({"top": {"color": "#0000ff", "width": 1.33, "style": "solid"}})
        );
    }

    /// A `brcType` of zero cancels a border rather than drawing a hairline.
    #[test]
    fn zero_brc_type_draws_nothing() {
        let a = attrs(&[(ww8::PBRCTOP, &[0, 0, 0, 0, 4, 0, 0, 0])]);
        assert!(!a.contains_key("paraBorder"));
    }

    #[test]
    fn shading_blends_palette_entries() {
        // WW8 form: fore = black (1), back = white (8), pattern 8 = 50 %.
        let v: u16 = 1 | (8 << 5) | (8 << 10);
        assert_eq!(attrs(&[(ww8::PSHD80, &v.to_le_bytes())])["shading"], json!("#7f7f7f"));

        // WW9 form: solid red foreground.
        let shd = [0xFFu8, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x01, 0x00];
        assert_eq!(attrs(&[(ww8::PSHD, &shd)])["shading"], json!("#ff0000"));

        // Automatic on both sides with a clear pattern → no fill at all.
        let auto = [0x00u8, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00];
        assert!(!attrs(&[(ww8::PSHD, &auto)]).contains_key("shading"));
    }

    /// The WW9 shading sprm wins over the legacy one whatever the order.
    #[test]
    fn new_shading_overrides_the_legacy_one() {
        let legacy: u16 = 1 | (8 << 5) | (1 << 10); // solid black
        let modern = [0x00u8, 0x00, 0xFF, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x01, 0x00];
        let a = attrs(&[(ww8::PSHD, &modern), (ww8::PSHD80, &legacy.to_le_bytes())]);
        assert_eq!(a["shading"], json!("#0000ff"));
    }

    #[test]
    fn table_flags_track_nesting() {
        let mut p = Pap::default();
        p.apply_grpprl(&grpprl(&[(ww8::PFINTABLE, &[1]), (ww8::PFTTP, &[1])]));
        assert!(p.f_in_table && p.f_ttp && p.itap == 1);

        let mut q = Pap::default();
        q.apply_grpprl(&grpprl(&[(ww8::PITAP, &2u32.to_le_bytes())]));
        assert!(q.f_in_table && q.itap == 2);

        // Leaving the table clears the depth.
        q.apply_grpprl(&grpprl(&[(ww8::PFINTABLE, &[0])]));
        assert!(!q.f_in_table && q.itap == 0);
    }

    /// A direct sprm must be able to reset an inherited indent to zero.
    #[test]
    fn direct_sprm_overrides_the_style() {
        let mut p = Pap::with_style(1);
        p.apply_grpprl(&grpprl(&[(ww8::PDXALEFT, &720i16.to_le_bytes())]));
        p.apply_grpprl(&grpprl(&[(ww8::PDXALEFT, &0i16.to_le_bytes())]));
        assert_eq!(p.dxa_left, Some(0));
        assert!(!p.to_attrs().contains_key("indentLeft"));
    }

    /// Character and table sprms share the stream but never touch a PAP.
    #[test]
    fn foreign_sprms_are_left_alone() {
        let mut p = Pap::default();
        p.apply_grpprl(&grpprl(&[
            (ww8::CFBOLD, &[1]),
            (ww8::PJC, &[1]), // this one is ours
            (ww8::TDXALEFT, &[0x00, 0x00]),
        ]));
        assert_eq!(p.jc, Some((1, true)));
        assert_eq!(p.to_attrs().len(), 1);
    }

    /// Auto spacing replaces the stored value, and the caller can override the
    /// value Word would have used.
    #[test]
    fn auto_spacing_replaces_the_explicit_value() {
        let mut p = Pap::default();
        p.apply_grpprl(&grpprl(&[
            (ww8::PDYABEFORE, &240u16.to_le_bytes()),
            (ww8::PFDYABEFOREAUTO, &[1]),
        ]));
        // 280 twips = 14 pt, Word's default when fDontUseHTMLAutoSpacing is off.
        assert_eq!(p.to_attrs()["spaceBefore"], json!(19.0));
        assert_eq!(p.to_attrs_with(100)["spaceBefore"], json!(7.0));
    }

    /// Word 6/95 uses one-byte opcodes and its own numbers, but the same
    /// meanings — and its border code is a packed word, not four bytes.
    #[test]
    fn word_6_grpprl_reads_the_same_properties() {
        // sprmPJc(5)=center, sprmPDxaLeft(17)=720, sprmPDyaLine(20)=-240 exact,
        // sprmPBrcTop(38): width 2 (=1.5 pt), type 1, ico 6 (red).
        let g: Vec<u8> = vec![
            5, 1, //
            17, 0xD0, 0x02, //
            20, 0x10, 0xFF, 0x00, 0x00, //
            38, 0x8A, 0x01,
        ];
        let mut p = Pap::default();
        p.apply_grpprl_ver(&g, SprmVersion::Ww6);
        let a = p.to_attrs();
        assert_eq!(a["textAlign"], json!("center"));
        assert_eq!(a["indentLeft"], json!(48.0));
        assert_eq!(a["lineSpacingMode"], json!("exactly"));
        assert_eq!(a["lineSpacingPt"], json!(16.0));
        assert_eq!(
            a["paraBorder"],
            json!({"top": {"color": "#ff0000", "width": 2.0, "style": "solid"}})
        );
    }
}
