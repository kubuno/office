//! Character properties (`CHP`) — the formatting of a run of text.
//!
//! A `.doc` never stores an effective character format: it stores *deltas*.
//! Word starts from a hard-coded default CHP, applies the document defaults,
//! then the character style pointed at by `istd`, then the direct formatting
//! found in the CHPX of the property bin covering the run. Each layer is a
//! `grpprl` — a packed list of sprms — and applying them in that order is the
//! whole of `Chp::apply_grpprl`.
//!
//! Reference implementation: LibreOffice `sw/source/filter/ww8/ww8par6.cxx`
//! (`Read_BoldUsw`, `Read_Underline`, `Read_TextColor`, `Read_FontSize`,
//! `Read_Kern`, `Read_ScaleWidth`, `Read_SubSuper`, `Read_SubSuperProp`,
//! `Read_CharHighlight`) plus the sprm opcodes of `sprmids.hxx` and [MS-DOC]
//! 2.6.1.
//!
//! The ProseMirror marks produced here use EXACTLY the attribute names of the
//! DOCX importer (`converters::docx::read::run`). The same paragraph must look
//! the same whether it came from a `.doc` or a `.docx`; a second vocabulary
//! would silently split the two formats apart.

use serde_json::{json, Value};

use crate::converters::types::PmMark;

use super::sprm::{iter_sprms, Sgc, Sprm};

// ── sprm opcodes, character category (sgc = 2) ────────────────────────────────
//
// Names and values from `sprmids.hxx`; the comment is the [MS-DOC] field.

const SPRM_C_PIC_LOCATION: u16 = 0x6A03; // chp.fcPic
const SPRM_C_HIGHLIGHT: u16 = 0x2A0C; // chp.icoHighlight
const SPRM_C_F_WEB_HIDDEN: u16 = 0x0811; // chp.fWebHidden
const SPRM_C_ISTD: u16 = 0x4A30; // chp.istd — the character style
const SPRM_C_PLAIN: u16 = 0x2A33; // reset the whole CHP
const SPRM_C_F_BOLD: u16 = 0x0835;
const SPRM_C_F_ITALIC: u16 = 0x0836;
const SPRM_C_F_STRIKE: u16 = 0x0837;
const SPRM_C_F_OUTLINE: u16 = 0x0838;
const SPRM_C_F_SHADOW: u16 = 0x0839;
const SPRM_C_F_SMALL_CAPS: u16 = 0x083A;
const SPRM_C_F_CAPS: u16 = 0x083B;
const SPRM_C_F_VANISH: u16 = 0x083C;
const SPRM_C_KUL: u16 = 0x2A3E; // chp.kul — underline style
const SPRM_C_DXA_SPACE: u16 = 0x8840; // chp.dxaSpace — letter spacing, twips
const SPRM_C_ICO: u16 = 0x2A42; // chp.ico — PALETTE INDEX, not RGB
const SPRM_C_HPS: u16 = 0x4A43; // chp.hps — size in half-points
const SPRM_C_HPS_POS: u16 = 0x4845; // chp.hpsPos — raise/lower, half-points
const SPRM_C_ISS: u16 = 0x2A48; // chp.iss — super/subscript
const SPRM_C_RG_FTC0: u16 = 0x4A4F; // ASCII font index into SttbfFfn
const SPRM_C_RG_FTC1: u16 = 0x4A50; // East-Asian font index
const SPRM_C_RG_FTC2: u16 = 0x4A51; // complex-script font index
const SPRM_C_CHAR_SCALE: u16 = 0x4852; // chp.charScale — horizontal scaling, %
const SPRM_C_F_DSTRIKE: u16 = 0x2A53; // chp.fDStrike
const SPRM_C_F_SPEC: u16 = 0x0855; // chp.fSpec — the character is a placeholder
const SPRM_C_FTC_BI: u16 = 0x4A5E; // bidi font index
const SPRM_C_CV: u16 = 0x6870; // Word 2000+: COLORREF, real RGB
const SPRM_C_CV_UL: u16 = 0x6877; // underline COLORREF

/// Vertical alignment of a run (`chp.iss`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Script {
    #[default]
    Baseline,
    Superscript,
    Subscript,
}

/// The effective character formatting of a run.
///
/// Fields mirror the CHP of [MS-DOC] 2.6.1, restricted to what the editor can
/// actually render. Anything absent from a `grpprl` keeps the value inherited
/// from the layer below, which is why this is a plain mutable struct rather
/// than a bag of `Option`s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Chp {
    /// Index of the character style in the stylesheet. 10 is Word's "Default
    /// Paragraph Font"; resolving it into a `grpprl` is the stylesheet's job.
    pub(crate) istd: u16,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) strike: bool,
    pub(crate) double_strike: bool,
    pub(crate) outline: bool,
    pub(crate) shadow: bool,
    pub(crate) small_caps: bool,
    pub(crate) caps: bool,
    /// Hidden text (`fVanish` / `fWebHidden`).
    pub(crate) hidden: bool,
    /// Underline style, raw `kul` code — see `underline_style`.
    pub(crate) kul: u8,
    /// Explicit underline colour, `None` meaning "same as the text".
    pub(crate) underline_color: Option<[u8; 3]>,
    pub(crate) script: Script,
    /// Raise (positive) or lower (negative) in half-points.
    pub(crate) hps_pos: i16,
    /// Font size in HALF-points, Word's unit. 20 = 10 pt.
    pub(crate) hps: u16,
    /// Indices into the font table (`SttbfFfn`), NOT names.
    pub(crate) ftc_ascii: u16,
    pub(crate) ftc_east_asian: u16,
    pub(crate) ftc_complex: u16,
    /// Text colour, `None` for Word's "automatic".
    pub(crate) color: Option<[u8; 3]>,
    /// Highlighter colour, `None` when there is none.
    pub(crate) highlight: Option<[u8; 3]>,
    /// Letter spacing in twips (1 pt = 20 twips), signed.
    pub(crate) dxa_space: i16,
    /// Horizontal scaling in percent.
    pub(crate) char_scale: u16,
    /// `fSpec`: the character is a placeholder (picture, field, footnote…) and
    /// its meaning is given by `pic_location`, not by its code point.
    pub(crate) special: bool,
    /// Offset of the picture/object data, only meaningful when `special`.
    pub(crate) pic_location: Option<u32>,
}

impl Default for Chp {
    /// Word's built-in CHP defaults ([MS-DOC] 2.6.1): 10 pt, automatic colour,
    /// no underline, 100 % scaling, "Default Paragraph Font".
    fn default() -> Self {
        Chp {
            istd: 10,
            bold: false,
            italic: false,
            strike: false,
            double_strike: false,
            outline: false,
            shadow: false,
            small_caps: false,
            caps: false,
            hidden: false,
            kul: 0,
            underline_color: None,
            script: Script::Baseline,
            hps_pos: 0,
            hps: 20,
            ftc_ascii: 0,
            ftc_east_asian: 0,
            ftc_complex: 0,
            color: None,
            highlight: None,
            dxa_space: 0,
            char_scale: 100,
            special: false,
            pic_location: None,
        }
    }
}

impl Chp {
    /// Apply one property layer on top of this CHP.
    ///
    /// Call order IS the cascade: document defaults, then the character style,
    /// then the direct formatting of the run. The 0/1/128/129 toggles are
    /// resolved against the value already in `self`, i.e. against the layer
    /// below — 129 means "the opposite of what the style says", which is only
    /// correct if the style was applied first.
    pub(crate) fn apply_grpprl(&mut self, grpprl: &[u8]) {
        // Word 2000 added `sprmCCv` (real RGB) next to the legacy `sprmCIco`
        // (palette index) and writes BOTH for backwards compatibility. The old
        // one must lose, whatever the order inside the grpprl
        // (ww8par6.cxx:3684, "Has newer colour variant, ignore this old variant").
        let has_cv = iter_sprms(grpprl)
            .iter()
            .any(|s| s.opcode == SPRM_C_CV && s.u32().is_some());
        for s in iter_sprms(grpprl) {
            if s.sgc() != Sgc::Character {
                continue;
            }
            self.apply_sprm(&s, has_cv);
        }
    }

    /// Apply a single character sprm. `has_cv` says whether the same layer also
    /// carries a `sprmCCv`, which supersedes `sprmCIco`.
    fn apply_sprm(&mut self, s: &Sprm<'_>, has_cv: bool) {
        match s.opcode {
            // Toggles: 0 off, 1 on, 128 inherit, 129 invert the inherited value.
            SPRM_C_F_BOLD => self.bold = s.toggle(self.bold).unwrap_or(self.bold),
            SPRM_C_F_ITALIC => self.italic = s.toggle(self.italic).unwrap_or(self.italic),
            SPRM_C_F_STRIKE => self.strike = s.toggle(self.strike).unwrap_or(self.strike),
            SPRM_C_F_DSTRIKE => {
                self.double_strike = s.toggle(self.double_strike).unwrap_or(self.double_strike)
            }
            SPRM_C_F_OUTLINE => self.outline = s.toggle(self.outline).unwrap_or(self.outline),
            SPRM_C_F_SHADOW => self.shadow = s.toggle(self.shadow).unwrap_or(self.shadow),
            SPRM_C_F_SMALL_CAPS => {
                self.small_caps = s.toggle(self.small_caps).unwrap_or(self.small_caps)
            }
            SPRM_C_F_CAPS => self.caps = s.toggle(self.caps).unwrap_or(self.caps),
            SPRM_C_F_VANISH | SPRM_C_F_WEB_HIDDEN => {
                self.hidden = s.toggle(self.hidden).unwrap_or(self.hidden)
            }
            SPRM_C_F_SPEC => self.special = s.toggle(self.special).unwrap_or(self.special),

            SPRM_C_ISTD => {
                if let Some(v) = s.u16() {
                    self.istd = v;
                }
            }
            SPRM_C_KUL => {
                if let Some(v) = s.u8() {
                    self.kul = v;
                }
            }
            SPRM_C_ISS => {
                self.script = match s.u8() {
                    Some(1) => Script::Superscript,
                    Some(2) => Script::Subscript,
                    Some(_) => Script::Baseline,
                    None => self.script,
                }
            }
            SPRM_C_HPS => {
                if let Some(v) = s.u16().filter(|v| *v > 0) {
                    self.hps = v;
                }
            }
            SPRM_C_HPS_POS => {
                if let Some(v) = s.i16() {
                    self.hps_pos = v;
                }
            }
            SPRM_C_DXA_SPACE => {
                if let Some(v) = s.i16() {
                    self.dxa_space = v;
                }
            }
            // Out of the 1..600 range Word falls back to 100 %, it does NOT
            // clamp (ww8par6.cxx:4990).
            SPRM_C_CHAR_SCALE => {
                if let Some(v) = s.u16() {
                    self.char_scale = if (1..=600).contains(&v) { v } else { 100 };
                }
            }
            SPRM_C_RG_FTC0 => {
                if let Some(v) = s.u16() {
                    self.ftc_ascii = v;
                }
            }
            SPRM_C_RG_FTC1 => {
                if let Some(v) = s.u16() {
                    self.ftc_east_asian = v;
                }
            }
            // The "other" font and the bidi font share a slot in Word's model.
            SPRM_C_RG_FTC2 | SPRM_C_FTC_BI => {
                if let Some(v) = s.u16() {
                    self.ftc_complex = v;
                }
            }
            // Palette index, 0 = automatic (ww8par6.cxx:3690).
            SPRM_C_ICO if !has_cv => {
                if let Some(v) = s.u8() {
                    self.color = ico_color(v);
                }
            }
            SPRM_C_CV => {
                if let Some(v) = s.u32() {
                    self.color = colorref(v);
                }
            }
            SPRM_C_CV_UL => {
                if let Some(v) = s.u32() {
                    self.underline_color = colorref(v);
                }
            }
            SPRM_C_HIGHLIGHT => {
                if let Some(v) = s.u8() {
                    self.highlight = ico_color(v);
                }
            }
            SPRM_C_PIC_LOCATION => self.pic_location = s.u32(),
            // `sprmCPlain` resets the CHP to the paragraph style's character
            // properties — a layer we do not have here. LibreOffice ignores it
            // too (ww8par6.cxx:6062, handler `nullptr`); resetting to the
            // hard-coded defaults would be worse than doing nothing.
            SPRM_C_PLAIN => {}
            _ => {}
        }
    }

    /// The marks the editor needs for this run.
    ///
    /// `fonts` is the document's font table (`SttbfFfn`), indexed by `ftc`;
    /// pass an empty slice when it is not available and the font is simply
    /// omitted. Attribute names are those of the DOCX importer — see the module
    /// header.
    pub(crate) fn to_marks(&self, fonts: &[String]) -> Vec<PmMark> {
        let mut marks: Vec<PmMark> = Vec::new();
        let mut style = serde_json::Map::new();

        if let Some(name) = font_name(fonts, self.ftc_ascii) {
            style.insert("fontFamily".into(), json!(name));
        }
        // Word always has an effective size; the DOCX importer likewise writes
        // `fontSize` on every run.
        style.insert("fontSize".into(), json!(f64::from(self.hps) / 2.0));

        if let Some(c) = self.color {
            style.insert("color".into(), json!(hex(c)));
        }

        if let Some((line, words)) = underline_style(self.kul) {
            // "single" stays implicit, exactly like the DOCX importer.
            if line != "single" {
                style.insert("underlineStyle".into(), json!(line));
            }
            if words {
                style.insert("underlineWords".into(), json!(true));
            }
            if let Some(c) = self.underline_color {
                style.insert("underlineColor".into(), json!(hex(c)));
            }
        }
        if self.double_strike {
            style.insert("doubleStrike".into(), json!(true));
        }
        if self.caps {
            style.insert("allCaps".into(), json!(true));
        }
        if self.small_caps {
            style.insert("smallCaps".into(), json!(true));
        }
        if self.hidden {
            style.insert("hidden".into(), json!(true));
        }
        // Signed twips → points. Word's legacy 16-bit guard drops anything at or
        // above 0x8000 (ConversionHelper.cxx:423-430).
        if self.dxa_space != 0 && self.dxa_space.unsigned_abs() < 0x8000 {
            style.insert(
                "letterSpacing".into(),
                json!(f64::from(self.dxa_space) / 20.0),
            );
        }
        if self.char_scale != 100 {
            style.insert("charScale".into(), json!(self.char_scale));
        }
        if let Some(pct) = self.char_position() {
            style.insert("charPosition".into(), json!(pct));
        }

        if self.bold {
            marks.push(simple("bold"));
        }
        if self.italic {
            marks.push(simple("italic"));
        }
        if underline_style(self.kul).is_some() {
            marks.push(simple("underline"));
        }
        // A double strike-through is still a strike-through for the editor, the
        // doubling being carried by the `doubleStrike` attribute.
        if self.strike || self.double_strike {
            marks.push(simple("strike"));
        }
        match self.script {
            Script::Superscript => marks.push(simple("superscript")),
            Script::Subscript => marks.push(simple("subscript")),
            Script::Baseline => {}
        }
        if let Some(c) = self.highlight {
            marks.push(PmMark {
                mark_type: "highlight".into(),
                attrs: Some(json!({ "color": hex(c) })),
            });
        }
        if !style.is_empty() {
            marks.push(PmMark {
                mark_type: "textStyle".into(),
                attrs: Some(Value::Object(style)),
            });
        }
        marks
    }

    /// `hpsPos` as a percentage of the font size, which is how the editor stores
    /// a raised/lowered run. Same computation as the DOCX importer and as
    /// `Read_SubSuperProp` (ww8par6.cxx:3576-3585), capped at ±13999.
    fn char_position(&self) -> Option<f64> {
        if self.hps_pos == 0 {
            return None;
        }
        let pct = if self.hps > 0 {
            (f64::from(self.hps_pos) * 100.0 / f64::from(self.hps)).round()
        } else if self.hps_pos > 0 {
            33.0 // DFLT_ESC_SUPER
        } else {
            -8.0 // DFLT_ESC_SUB
        };
        Some(pct.clamp(-13999.0, 13999.0))
    }
}

/// Resolve a `ftc` into a font family name.
///
/// The font table is the `SttbfFfn` of the table stream; a `.doc` run only ever
/// names a font by its index there. Kept as a free function taking the table so
/// that the stylesheet reader can own it.
pub(crate) fn font_name(fonts: &[String], ftc: u16) -> Option<&str> {
    fonts
        .get(ftc as usize)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
}

/// Word's 17-entry colour palette (`ico`). Index 0 is "automatic", which the
/// editor expresses by the absence of a colour. Values are LibreOffice's
/// `SwWW8ImplReader::GetCol` table (ww8par6.cxx:122-133) — note that Word's
/// "blue" is the DARK blue and "light blue" the pure one.
pub(crate) fn ico_color(ico: u8) -> Option<[u8; 3]> {
    const PALETTE: [[u8; 3]; 16] = [
        [0x00, 0x00, 0x00], // 1  black
        [0x00, 0x00, 0xFF], // 2  light blue
        [0x00, 0xFF, 0xFF], // 3  light cyan
        [0x00, 0xFF, 0x00], // 4  light green
        [0xFF, 0x00, 0xFF], // 5  light magenta
        [0xFF, 0x00, 0x00], // 6  light red
        [0xFF, 0xFF, 0x00], // 7  yellow
        [0xFF, 0xFF, 0xFF], // 8  white
        [0x00, 0x00, 0x80], // 9  blue
        [0x00, 0x80, 0x80], // 10 cyan
        [0x00, 0x80, 0x00], // 11 green
        [0x80, 0x00, 0x80], // 12 magenta
        [0x80, 0x00, 0x00], // 13 red
        [0x80, 0x80, 0x00], // 14 brown
        [0x80, 0x80, 0x80], // 15 gray
        [0xC0, 0xC0, 0xC0], // 16 light gray
    ];
    match ico {
        // 0 = automatic, anything past the palette is "unknown" and Word paints
        // it automatic as well (ww8par6.cxx:3692).
        0 => None,
        n if (n as usize) <= PALETTE.len() => Some(PALETTE[n as usize - 1]),
        _ => None,
    }
}

/// A Word 2000 `COLORREF` operand (`sprmCCv`, `sprmCCvUl`): the dword is
/// `0xTTBBGGRR`, so the operand bytes are already R, G, B — the swap is in the
/// dword, not in the stream (`msfilter::util::BGRToRGB`). A transparency byte of
/// 0xFF means "automatic".
fn colorref(v: u32) -> Option<[u8; 3]> {
    if (v >> 24) & 0xFF == 0xFF {
        return None;
    }
    Some([
        (v & 0xFF) as u8,
        ((v >> 8) & 0xFF) as u8,
        ((v >> 16) & 0xFF) as u8,
    ])
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

fn simple(name: &str) -> PmMark {
    PmMark { mark_type: name.into(), attrs: None }
}

/// `chp.kul` → (DOCX underline style, "words only"), or `None` for no line.
///
/// The codes are Word's, the names are the OOXML ones the editor already
/// understands, so a `.doc` and a `.docx` underline reach the same attribute.
/// Mapping taken from `Read_Underline` (ww8par6.cxx:3590-3625): code 5 is
/// "hidden" and code 8 is unused, both meaning no visible line, and any code
/// LibreOffice does not know draws nothing either.
pub(crate) fn underline_style(kul: u8) -> Option<(&'static str, bool)> {
    Some(match kul {
        1 => ("single", false),
        2 => ("single", true), // "by word"
        3 => ("double", false),
        4 => ("dotted", false),
        6 => ("thick", false),
        7 => ("dash", false),
        9 => ("dotDash", false),
        10 => ("dotDotDash", false),
        11 => ("wave", false),
        20 => ("dottedHeavy", false),
        23 => ("dashedHeavy", false),
        25 => ("dashDotHeavy", false),
        26 => ("dashDotDotHeavy", false),
        27 => ("wavyHeavy", false),
        39 => ("dashLong", false),
        43 => ("wavyDouble", false),
        55 => ("dashLongHeavy", false),
        _ => return None, // 0 none, 5 hidden, 8 unused, everything unknown
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style_attr(marks: &[PmMark], key: &str) -> Option<Value> {
        marks
            .iter()
            .find(|m| m.mark_type == "textStyle")
            .and_then(|m| m.attrs.as_ref())
            .and_then(|a| a.get(key).cloned())
    }

    fn has(marks: &[PmMark], t: &str) -> bool {
        marks.iter().any(|m| m.mark_type == t)
    }

    /// Build a CHP from one grpprl on top of the Word defaults.
    fn chp(grpprl: &[u8]) -> Chp {
        let mut c = Chp::default();
        c.apply_grpprl(grpprl);
        c
    }

    /// A sprm with a one-byte operand.
    fn b(op: u16, v: u8) -> Vec<u8> {
        vec![op as u8, (op >> 8) as u8, v]
    }
    /// A sprm with a two-byte operand.
    fn w(op: u16, v: u16) -> Vec<u8> {
        vec![op as u8, (op >> 8) as u8, v as u8, (v >> 8) as u8]
    }
    /// A sprm with a four-byte operand.
    fn d(op: u16, v: u32) -> Vec<u8> {
        vec![
            op as u8,
            (op >> 8) as u8,
            v as u8,
            (v >> 8) as u8,
            (v >> 16) as u8,
            (v >> 24) as u8,
        ]
    }

    #[test]
    fn toggles_resolve_against_the_layer_below() {
        assert!(chp(&b(SPRM_C_F_BOLD, 1)).bold);
        assert!(!chp(&b(SPRM_C_F_BOLD, 0)).bold);
        // 128 = keep what the style said, 129 = take its opposite.
        let mut style_bold = Chp::default();
        style_bold.apply_grpprl(&b(SPRM_C_F_BOLD, 1));
        let mut inherit = style_bold.clone();
        inherit.apply_grpprl(&b(SPRM_C_F_BOLD, 128));
        assert!(inherit.bold, "128 doit conserver la valeur du style");
        let mut invert = style_bold;
        invert.apply_grpprl(&b(SPRM_C_F_BOLD, 129));
        assert!(!invert.bold, "129 doit inverser la valeur du style");
        // …and 129 on a non-bold style turns it on.
        assert!(chp(&b(SPRM_C_F_BOLD, 129)).bold);
    }

    #[test]
    fn ico_palette_is_not_rgb() {
        // ico 6 is "light red" = pure red, ico 13 the dark one.
        assert_eq!(ico_color(6), Some([0xFF, 0x00, 0x00]));
        assert_eq!(ico_color(13), Some([0x80, 0x00, 0x00]));
        assert_eq!(ico_color(0), None); // automatic
        assert_eq!(ico_color(200), None);
        let m = chp(&b(SPRM_C_ICO, 6)).to_marks(&[]);
        assert_eq!(style_attr(&m, "color"), Some(json!("#FF0000")));
    }

    #[test]
    fn cv_is_real_rgb_and_beats_ico() {
        // COLORREF 0x00BBGGRR: 0x00C08040 is R=0x40 G=0x80 B=0xC0.
        assert_eq!(colorref(0x00C0_8040), Some([0x40, 0x80, 0xC0]));
        assert_eq!(colorref(0xFF00_0000), None); // automatic
        let mut g = b(SPRM_C_ICO, 6); // Word writes both for compatibility
        g.extend(d(SPRM_C_CV, 0x00C0_8040));
        assert_eq!(chp(&g).color, Some([0x40, 0x80, 0xC0]));
        // …whatever the order inside the grpprl.
        let mut g = d(SPRM_C_CV, 0x00C0_8040);
        g.extend(b(SPRM_C_ICO, 6));
        assert_eq!(chp(&g).color, Some([0x40, 0x80, 0xC0]));
    }

    #[test]
    fn underline_uses_the_docx_names() {
        assert_eq!(underline_style(0), None);
        assert_eq!(underline_style(5), None); // "hidden"
        assert_eq!(underline_style(2), Some(("single", true)));
        assert_eq!(underline_style(39), Some(("dashLong", false)));
        let mut g = b(SPRM_C_KUL, 4);
        g.extend(d(SPRM_C_CV_UL, 0x0000_00FF));
        let m = chp(&g).to_marks(&[]);
        assert!(has(&m, "underline"));
        assert_eq!(style_attr(&m, "underlineStyle"), Some(json!("dotted")));
        assert_eq!(style_attr(&m, "underlineColor"), Some(json!("#FF0000")));
        // A plain single underline leaves the style implicit, like the DOCX side.
        let m = chp(&b(SPRM_C_KUL, 1)).to_marks(&[]);
        assert!(has(&m, "underline"));
        assert_eq!(style_attr(&m, "underlineStyle"), None);
        assert_eq!(style_attr(&m, "underlineWords"), None);
        let m = chp(&b(SPRM_C_KUL, 2)).to_marks(&[]);
        assert_eq!(style_attr(&m, "underlineWords"), Some(json!(true)));
    }

    #[test]
    fn size_spacing_scale_and_position() {
        let mut g = w(SPRM_C_HPS, 24); // 12 pt
        g.extend(w(SPRM_C_DXA_SPACE, 40u16)); // 40 twips = 2 pt
        g.extend(w(SPRM_C_CHAR_SCALE, 150));
        g.extend(w(SPRM_C_HPS_POS, 6u16)); // 3 pt over 12 pt = 25 %
        let m = chp(&g).to_marks(&[]);
        assert_eq!(style_attr(&m, "fontSize"), Some(json!(12.0)));
        assert_eq!(style_attr(&m, "letterSpacing"), Some(json!(2.0)));
        assert_eq!(style_attr(&m, "charScale"), Some(json!(150)));
        assert_eq!(style_attr(&m, "charPosition"), Some(json!(25.0)));
        // Negative spacing and position (condensed, lowered).
        let mut g = w(SPRM_C_HPS, 20);
        g.extend(w(SPRM_C_DXA_SPACE, (-10i16) as u16));
        g.extend(w(SPRM_C_HPS_POS, (-4i16) as u16));
        let m = chp(&g).to_marks(&[]);
        assert_eq!(style_attr(&m, "letterSpacing"), Some(json!(-0.5)));
        assert_eq!(style_attr(&m, "charPosition"), Some(json!(-20.0)));
        // Out-of-range scaling falls back to 100 %, i.e. no attribute.
        assert_eq!(
            style_attr(&chp(&w(SPRM_C_CHAR_SCALE, 900)).to_marks(&[]), "charScale"),
            None
        );
    }

    #[test]
    fn caps_hidden_strike_and_script() {
        let mut g = b(SPRM_C_F_CAPS, 1);
        g.extend(b(SPRM_C_F_SMALL_CAPS, 1));
        g.extend(b(SPRM_C_F_VANISH, 1));
        g.extend(b(SPRM_C_F_DSTRIKE, 1));
        g.extend(b(SPRM_C_ISS, 1));
        g.extend(b(SPRM_C_HIGHLIGHT, 7));
        let m = chp(&g).to_marks(&[]);
        assert_eq!(style_attr(&m, "allCaps"), Some(json!(true)));
        assert_eq!(style_attr(&m, "smallCaps"), Some(json!(true)));
        assert_eq!(style_attr(&m, "hidden"), Some(json!(true)));
        assert_eq!(style_attr(&m, "doubleStrike"), Some(json!(true)));
        assert!(has(&m, "strike"), "un barré double reste un barré");
        assert!(has(&m, "superscript"));
        let hl = m.iter().find(|k| k.mark_type == "highlight").expect("highlight");
        assert_eq!(
            hl.attrs.as_ref().and_then(|a| a.get("color")),
            Some(&json!("#FFFF00"))
        );
        assert!(has(&chp(&b(SPRM_C_ISS, 2)).to_marks(&[]), "subscript"));
    }

    #[test]
    fn font_comes_from_the_table_not_the_run() {
        let fonts = vec!["Times New Roman".to_string(), "Arial".to_string()];
        let m = chp(&w(SPRM_C_RG_FTC0, 1)).to_marks(&fonts);
        assert_eq!(style_attr(&m, "fontFamily"), Some(json!("Arial")));
        // Unknown index or no table at all: the attribute is simply absent.
        assert_eq!(
            style_attr(&chp(&w(SPRM_C_RG_FTC0, 9)).to_marks(&fonts), "fontFamily"),
            None
        );
        assert_eq!(
            style_attr(&chp(&w(SPRM_C_RG_FTC0, 1)).to_marks(&[]), "fontFamily"),
            None
        );
    }

    #[test]
    fn cascade_defaults_style_direct() {
        let mut c = Chp::default();
        c.apply_grpprl(&w(SPRM_C_HPS, 20)); // document defaults: 10 pt
        c.apply_grpprl(&{
            let mut g = b(SPRM_C_F_BOLD, 1); // character style: bold, 14 pt
            g.extend(w(SPRM_C_HPS, 28));
            g
        });
        c.apply_grpprl(&b(SPRM_C_F_ITALIC, 1)); // direct: italic
        assert!(c.bold && c.italic);
        assert_eq!(c.hps, 28);
        // A sprm of another category must not leak in (paragraph justification).
        let before = c.clone();
        c.apply_grpprl(&b(0x2403, 1));
        assert_eq!(c, before);
    }

    // ── Real documents ────────────────────────────────────────────────────────
    //
    // The corpus tests below need real `.doc` files, so they are opt-in:
    //   KUBUNO_DOC_CORPUS=<dir>   walk every .doc, apply every CHPX
    //   KUBUNO_DOC_LO_DIR=<dir>   compare with LibreOffice's own output,
    //                             <dir>/src/x.doc next to <dir>/lo/x.fodt
    // The comparison is a PRECISION check: everything we call bold/italic must
    // be bold/italic for LibreOffice too. The converse cannot hold yet — a CHPX
    // only carries DIRECT formatting and the stylesheet layer (`style.rs`) is
    // not wired in.

    use std::collections::{HashMap, HashSet};

    use super::super::fib::{u32_at, Fib};
    use super::super::fkp::bin_table_pages;
    use super::super::piece::{charset_for, parse_pieces, read_text, Piece};

    /// A CHPX and the byte range of the WordDocument stream it covers.
    struct Chpx {
        fc_start: u32,
        fc_end: u32,
        grpprl: Vec<u8>,
    }

    /// Decode one 512-byte CHPX FKP.
    ///
    /// Layout: `crun+1` byte positions (4 bytes each), then `crun` one-byte
    /// offsets — in WORDS, and 0 meaning "no property" — then, at byte 511, the
    /// run count. Each CHPX is a length byte followed by the grpprl ([MS-DOC]
    /// 2.9.66). Local to the tests: decoding FKP pages belongs to `fkp.rs`,
    /// which is another file.
    fn chpx_page(page: &[u8]) -> Vec<Chpx> {
        if page.len() < 512 {
            return Vec::new();
        }
        let crun = page[511] as usize;
        if crun == 0 || 4 * (crun + 1) + crun > 511 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for i in 0..crun {
            let off = page[4 * (crun + 1) + i] as usize * 2;
            if off == 0 || off >= page.len() {
                continue;
            }
            let cb = page[off] as usize;
            if off + 1 + cb > page.len() {
                continue;
            }
            out.push(Chpx {
                fc_start: u32_at(page, i * 4),
                fc_end: u32_at(page, (i + 1) * 4),
                grpprl: page[off + 1..off + 1 + cb].to_vec(),
            });
        }
        out
    }

    /// Every CHPX of a document, in stream order.
    fn all_chpx(fib: &Fib, doc: &[u8], table: &[u8]) -> Vec<Chpx> {
        let mut out = Vec::new();
        for page in bin_table_pages(table, fib.fc_plcf_bte_chpx, fib.lcb_plcf_bte_chpx) {
            let start = page as usize * 512;
            let end = start.saturating_add(512);
            if end <= doc.len() {
                out.extend(chpx_page(&doc[start..end]));
            }
        }
        out
    }

    /// Character-position range of a byte range, walking the piece table.
    fn cp_range(pieces: &[Piece], fc_start: u32, fc_end: u32) -> Option<(usize, usize)> {
        for p in pieces {
            let width = if p.compressed { 1u32 } else { 2 };
            let bytes = (p.cp_end - p.cp_start) * width;
            let (b0, b1) = (p.fc, p.fc + bytes);
            let (s, e) = (fc_start.max(b0), fc_end.min(b1));
            if s < e {
                let cp0 = p.cp_start + (s - b0) / width;
                let cp1 = p.cp_start + (e - b0) / width;
                return Some((cp0 as usize, cp1 as usize));
            }
        }
        None
    }

    /// Words long enough to be discriminating, lower-cased.
    fn words_of(s: &str) -> Vec<String> {
        s.split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.chars().count() >= 4)
            .map(|w| w.to_lowercase())
            .collect()
    }

    /// What a CHPX says about a toggle INDEPENDENTLY of the stylesheet.
    ///
    /// 128 means "whatever the style says" and 129 "the opposite of it", so
    /// those two cannot be checked without the style layer — a `.doc` written by
    /// Word is full of them. Only an explicit 0 or 1 is comparable.
    fn explicit_toggle(grpprl: &[u8], opcode: u16) -> Option<bool> {
        iter_sprms(grpprl)
            .iter()
            .rfind(|s| s.opcode == opcode)
            .and_then(|s| match s.u8() {
                Some(0) => Some(false),
                Some(1) => Some(true),
                _ => None,
            })
    }

    /// What we extract from a document's DIRECT formatting: the words whose
    /// weight/posture the CHPX states absolutely, plus every font size and text
    /// colour it mentions.
    struct Extract {
        bold: HashSet<String>,
        italic: HashSet<String>,
        sizes: HashSet<String>,
        colors: HashSet<String>,
    }

    fn our_words(bytes: &[u8]) -> Option<Extract> {
        use std::io::{Cursor, Read};
        let mut comp = cfb::CompoundFile::open(Cursor::new(bytes)).ok()?;
        let mut read = |name: &str| -> Option<Vec<u8>> {
            let mut s = comp.open_stream(name).ok()?;
            let mut buf = Vec::new();
            s.read_to_end(&mut buf).ok()?;
            Some(buf)
        };
        let doc = read("WordDocument")?;
        let fib = Fib::parse(&doc).ok()?;
        if fib.encrypted || fib.nfib < 193 {
            return None; // Word 6/95 uses 8-bit sprm opcodes: another decoder.
        }
        let table = read(fib.table_stream_name()).unwrap_or_default();
        let pieces = parse_pieces(&fib, &table);
        let text = read_text(&fib, &doc, &pieces, charset_for(&fib));

        let mut out = Extract {
            bold: HashSet::new(),
            italic: HashSet::new(),
            sizes: HashSet::new(),
            colors: HashSet::new(),
        };
        for x in all_chpx(&fib, &doc, &table) {
            let mut c = Chp::default();
            c.apply_grpprl(&x.grpprl);
            // A size or a colour is absolute: no toggle, no inheritance.
            if iter_sprms(&x.grpprl).iter().any(|s| s.opcode == SPRM_C_HPS) {
                out.sizes.insert(format!("{}pt", f64::from(c.hps) / 2.0));
            }
            if let Some(col) = c.color {
                out.colors.insert(hex(col));
            }
            let bold = explicit_toggle(&x.grpprl, SPRM_C_F_BOLD);
            let italic = explicit_toggle(&x.grpprl, SPRM_C_F_ITALIC);
            if bold != Some(true) && italic != Some(true) {
                continue;
            }
            let Some((cp0, cp1)) = cp_range(&pieces, x.fc_start, x.fc_end) else { continue };
            let n = text.chars.len();
            let slice: String = text.chars[cp0.min(n)..cp1.min(n)].iter().collect();
            if std::env::var("KUBUNO_DOC_DEBUG").is_ok() {
                eprintln!(
                    "  [{}..{}) gras={bold:?} ital={italic:?} sprms={:02X?} → {:?}",
                    cp0,
                    cp1,
                    x.grpprl,
                    slice.chars().take(60).collect::<String>()
                );
            }
            for word in words_of(&slice) {
                if bold == Some(true) {
                    out.bold.insert(word.clone());
                }
                if italic == Some(true) {
                    out.italic.insert(word);
                }
            }
        }
        Some(out)
    }

    /// An ODF style: parent name, then weight and posture when it states them.
    type OdfStyle = (Option<String>, Option<bool>, Option<bool>);
    /// Weight and posture inherited at some point of the inline tree.
    type Emphasis = (Option<bool>, Option<bool>);

    /// The same four sets, as LibreOffice sees them in its flat ODF output.
    fn lo_words(fodt: &str) -> Option<Extract> {
        const STYLE_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:style:1.0";
        const FO_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0";
        const TEXT_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";

        let doc = roxmltree::Document::parse(fodt).ok()?;
        // style name → (parent, bold, italic), any of which may be unset.
        let mut styles: HashMap<String, OdfStyle> = HashMap::new();
        let (mut sizes, mut colors) = (HashSet::new(), HashSet::new());
        for st in doc
            .descendants()
            .filter(|n| n.has_tag_name("style") || n.has_tag_name("default-style"))
        {
            let name = st.attribute((STYLE_NS, "name")).unwrap_or("");
            let parent = st
                .attribute((STYLE_NS, "parent-style-name"))
                .map(str::to_string);
            let (mut b, mut i) = (None, None);
            if let Some(tp) = st.children().find(|n| n.has_tag_name("text-properties")) {
                b = tp.attribute((FO_NS, "font-weight")).map(|v| v != "normal");
                i = tp.attribute((FO_NS, "font-style")).map(|v| v != "normal");
                // "11pt" → "11pt", "11.5pt" → "11.5pt"; percentages are relative
                // and not comparable to a half-point size.
                if let Some(s) = tp.attribute((FO_NS, "font-size")).filter(|s| s.ends_with("pt")) {
                    let v: f64 = s.trim_end_matches("pt").parse().unwrap_or(0.0);
                    sizes.insert(format!("{v}pt"));
                }
                if let Some(c) = tp.attribute((FO_NS, "color")).filter(|c| c.len() == 7) {
                    colors.insert(c.to_ascii_uppercase());
                }
            }
            styles.insert(name.to_string(), (parent, b, i));
        }
        // Unset stays unset: a span that says nothing about weight inherits it
        // from the paragraph, so the two levels cannot be collapsed to booleans
        // before they are merged.
        let resolve = |name: &str| -> Emphasis {
            let (mut b, mut i) = (None, None);
            let mut cur = Some(name.to_string());
            let mut guard = 0;
            while let Some(n) = cur {
                guard += 1;
                if guard > 16 {
                    break;
                }
                let Some((parent, sb, si)) = styles.get(&n) else { break };
                b = b.or(*sb);
                i = i.or(*si);
                cur = parent.clone();
            }
            (b, i)
        };

        /// Walk a paragraph's inline content, carrying the inherited weight and
        /// posture down through NESTED spans (a hyperlink wraps its span, and
        /// Word's runs often nest two levels deep).
        fn walk(
            node: roxmltree::Node<'_, '_>,
            props: Emphasis,
            resolve: &dyn Fn(&str) -> Emphasis,
            text_ns: &str,
            bold: &mut HashSet<String>,
            italic: &mut HashSet<String>,
        ) {
            for child in node.children() {
                if child.is_text() {
                    for word in words_of(child.text().unwrap_or("")) {
                        if props.0 == Some(true) {
                            bold.insert(word.clone());
                        }
                        if props.1 == Some(true) {
                            italic.insert(word);
                        }
                    }
                } else if child.is_element() {
                    let mut next = props;
                    if child.has_tag_name("span") {
                        if let Some(n) = child.attribute((text_ns, "style-name")) {
                            let (b, i) = resolve(n);
                            next = (b.or(props.0), i.or(props.1));
                        }
                    }
                    walk(child, next, resolve, text_ns, bold, italic);
                }
            }
        }

        let (mut bold, mut italic) = (HashSet::new(), HashSet::new());
        for para in doc
            .descendants()
            .filter(|n| n.has_tag_name("p") || n.has_tag_name("h"))
        {
            let base = para
                .attribute((TEXT_NS, "style-name"))
                .map(&resolve)
                .unwrap_or((None, None));
            walk(para, base, &resolve, TEXT_NS, &mut bold, &mut italic);
        }
        Some(Extract { bold, italic, sizes, colors })
    }

    #[test]
    fn character_formatting_matches_libreoffice() {
        let Ok(root) = std::env::var("KUBUNO_DOC_LO_DIR") else {
            eprintln!("KUBUNO_DOC_LO_DIR absent — test ignoré");
            return;
        };
        let root = std::path::PathBuf::from(root);
        let mut files = 0usize;
        // (matched, total) per property.
        let mut score = [(0usize, 0usize); 4];
        let labels = ["gras", "italique", "taille", "couleur"];
        let entries = std::fs::read_dir(root.join("src"))
            .expect("KUBUNO_DOC_LO_DIR doit contenir src/ et lo/");
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "doc") {
                continue;
            }
            let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else { continue };
            let Ok(fodt) = std::fs::read_to_string(root.join("lo").join(format!("{stem}.fodt")))
            else {
                continue;
            };
            let Ok(bytes) = std::fs::read(&p) else { continue };
            let Some(ours) = our_words(&bytes) else { continue };
            let Some(theirs) = lo_words(&fodt) else { continue };
            files += 1;
            let pairs = [
                (&ours.bold, &theirs.bold),
                (&ours.italic, &theirs.italic),
                (&ours.sizes, &theirs.sizes),
                (&ours.colors, &theirs.colors),
            ];
            for (k, (mine, ref_)) in pairs.iter().enumerate() {
                let ok = mine.iter().filter(|v| ref_.contains(*v)).count();
                score[k].0 += ok;
                score[k].1 += mine.len();
                let missed = mine.len() - ok;
                if missed > 3 && missed * 2 > ok {
                    eprintln!("  divergence {stem} / {}: {ok} ok, {missed} de trop", labels[k]);
                }
            }
        }
        eprintln!(
            "comparaison LibreOffice sur {files} fichiers — {}",
            labels
                .iter()
                .zip(score.iter())
                .map(|(l, (ok, n))| format!("{l} {ok}/{n}"))
                .collect::<Vec<_>>()
                .join(" · ")
        );
        assert!(files > 0, "aucune paire .doc/.fodt exploitable");
        let total: usize = score.iter().map(|(ok, _)| ok).sum();
        assert!(total > 200, "trop peu de mise en forme extraite pour conclure");
        for (k, (ok, n)) in score.iter().enumerate() {
            if *n < 20 {
                continue; // not enough samples to mean anything
            }
            let precision = *ok as f64 / *n as f64;
            assert!(
                precision > 0.9,
                "trop de faux positifs sur {}: {ok}/{n}",
                labels[k]
            );
        }
    }

    #[test]
    fn every_chpx_of_the_corpus_decodes() {
        let Ok(root) = std::env::var("KUBUNO_DOC_CORPUS") else {
            eprintln!("KUBUNO_DOC_CORPUS absent — test ignoré");
            return;
        };
        let (mut seen, mut runs, mut formatted) = (0usize, 0usize, 0usize);
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "doc") {
                    continue;
                }
                let Ok(bytes) = std::fs::read(&p) else { continue };
                use std::io::{Cursor, Read};
                let Ok(mut comp) = cfb::CompoundFile::open(Cursor::new(&bytes[..])) else {
                    continue;
                };
                let mut buf = Vec::new();
                match comp.open_stream("WordDocument") {
                    Ok(mut s) => {
                        if s.read_to_end(&mut buf).is_err() {
                            continue;
                        }
                    }
                    Err(_) => continue,
                }
                let Ok(fib) = Fib::parse(&buf) else { continue };
                if fib.encrypted {
                    continue;
                }
                let mut table = Vec::new();
                if let Ok(mut t) = comp.open_stream(fib.table_stream_name()) {
                    let _ = t.read_to_end(&mut table);
                }
                seen += 1;
                for x in all_chpx(&fib, &buf, &table) {
                    let mut c = Chp::default();
                    c.apply_grpprl(&x.grpprl);
                    // Exercising the mark builder is the point: it must never
                    // panic on a hostile grpprl either.
                    let marks = c.to_marks(&[]);
                    runs += 1;
                    if !marks.is_empty() {
                        formatted += 1;
                    }
                }
            }
        }
        eprintln!("corpus CHPX: {seen} documents, {runs} runs, {formatted} formatés");
        assert!(seen > 0, "aucun .doc lisible dans KUBUNO_DOC_CORPUS");
    }
}
