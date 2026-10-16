//! Port of `office/web/src/mathPages.ts` - the stored Maths document. A document's whole content is
//! the single `latex` string of the `MathFormula` row (`.kbmath` Drive file); this module reads it
//! into pages-of-blocks ([`parse_doc`], accepting every historical form) and writes it back
//! ([`serialize_doc`]), **byte-identical to what the web writes** (field order and number formatting
//! match `JSON.stringify` / `Number.toString`, so a file saved by either client round-trips).

use super::graph::{normalize_graph_spec, spec_to_stored_json, GraphSpec};
use super::num_fmt::{json_number, json_string};
use serde_json::Value;

const HEADER: &str = "%%KBMATH-MULTIPAGE%%";
const DOC_HEADER: &str = "%%KBMATH-DOC%%";

/// Margin used for default block placement on the sheet.
pub const SHEET_PAD: f64 = 24.0;
pub const DEFAULT_FORMAT: &str = "a4";

/// Paper format in mm, portrait (`mathPages.ts` `PaperFormat`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaperFormat {
    pub id: &'static str,
    pub name: &'static str,
    pub w: f64,
    pub h: f64,
}

/// `mathPages.ts` `PAGE_FORMATS`.
pub const PAGE_FORMATS: &[PaperFormat] = &[
    PaperFormat { id: "a3", name: "A3", w: 297.0, h: 420.0 },
    PaperFormat { id: "a4", name: "A4", w: 210.0, h: 297.0 },
    PaperFormat { id: "a5", name: "A5", w: 148.0, h: 210.0 },
    PaperFormat { id: "b5", name: "B5", w: 176.0, h: 250.0 },
    PaperFormat { id: "letter", name: "Letter", w: 216.0, h: 279.0 },
    PaperFormat { id: "legal", name: "Legal", w: 216.0, h: 356.0 },
];

/// Page orientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}
impl Orientation {
    fn as_str(self) -> &'static str {
        match self {
            Orientation::Portrait => "portrait",
            Orientation::Landscape => "landscape",
        }
    }
}

/// Pixel size of a sheet at ~96 dpi (3.78 px/mm), honouring orientation (`mathPages.ts` `paperPx`).
pub fn paper_px(format: &str, orientation: Orientation, px_per_mm: f64) -> (f64, f64) {
    let f = PAGE_FORMATS.iter().find(|p| p.id == format).unwrap_or(&PAGE_FORMATS[1]);
    let (w, h) = match orientation {
        Orientation::Landscape => (f.h, f.w),
        Orientation::Portrait => (f.w, f.h),
    };
    ((w * px_per_mm).round(), (h * px_per_mm).round())
}

/// Horizontal alignment of a formula block (only `left`/`right` are stored; `center` is the
/// default/absence).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockAlign {
    Left,
    Right,
}

/// Text block style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextStyle {
    H1,
    H2,
    P,
}

/// A block's position/size on the sheet, in px (`mathPages.ts` `BlockFrame`). `r` = rotation (deg).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockFrame {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: Option<f64>,
    pub r: Option<f64>,
}

/// A block freely placed on a page (`mathPages.ts` `MathBlock`).
#[derive(Clone, Debug, PartialEq)]
pub enum MathBlock {
    Formula { latex: String, align: Option<BlockAlign>, numbered: Option<bool>, frame: Option<BlockFrame>, scale: Option<f64> },
    Graph { spec: GraphSpec, frame: Option<BlockFrame> },
    Text { text: String, style: TextStyle, frame: Option<BlockFrame>, scale: Option<f64> },
}

/// A page of blocks (`mathPages.ts` `DocPage`).
#[derive(Clone, Debug, PartialEq)]
pub struct DocPage {
    pub name: String,
    pub blocks: Vec<MathBlock>,
    pub format: Option<String>,
    pub orientation: Option<Orientation>,
}

// ── Reading ─────────────────────────────────────────────────────────────────────

fn finite(v: Option<&Value>) -> Option<f64> {
    match v.and_then(Value::as_f64) {
        Some(x) if x.is_finite() => Some(x),
        _ => None,
    }
}

fn normalize_frame(f: Option<&Value>) -> Option<BlockFrame> {
    let o = f?.as_object()?;
    let x = finite(o.get("x"))?;
    let y = finite(o.get("y"))?;
    let w = finite(o.get("w"))?;
    let h = finite(o.get("h"));
    let r = finite(o.get("r")).filter(|&r| r != 0.0);
    Some(BlockFrame { x, y, w, h, r })
}

fn normalize_block(b: &Value) -> Option<MathBlock> {
    let o = b.as_object()?;
    let frame = normalize_frame(o.get("frame"));
    // scale: a finite number > 0 and != 1, clamped to [0.2, 8].
    let scale = o
        .get("scale")
        .and_then(Value::as_f64)
        .filter(|&s| s.is_finite() && s > 0.0 && s != 1.0)
        .map(|s| s.clamp(0.2, 8.0));

    match o.get("type").and_then(Value::as_str) {
        Some("graph") => Some(MathBlock::Graph {
            spec: normalize_graph_spec(o.get("spec").unwrap_or(&Value::Null)),
            frame,
        }),
        Some("text") => Some(MathBlock::Text {
            text: o.get("text").and_then(Value::as_str).unwrap_or("").to_string(),
            style: match o.get("style").and_then(Value::as_str) {
                Some("h1") => TextStyle::H1,
                Some("h2") => TextStyle::H2,
                _ => TextStyle::P,
            },
            frame,
            scale,
        }),
        _ => Some(MathBlock::Formula {
            latex: o.get("latex").and_then(Value::as_str).unwrap_or("").to_string(),
            align: match o.get("align").and_then(Value::as_str) {
                Some("left") => Some(BlockAlign::Left),
                Some("right") => Some(BlockAlign::Right),
                _ => None,
            },
            numbered: if o.get("numbered") == Some(&Value::Bool(true)) { Some(true) } else { None },
            frame,
            scale,
        }),
    }
}

/// Read a document into pages-of-blocks, accepting every historical format (`mathPages.ts`
/// `parseDoc`): plain LaTeX → one formula block; `%%KBMATH-MULTIPAGE%%` → one formula block per
/// page; `%%KBMATH-DOC%%` → the block format.
pub fn parse_doc(src: Option<&str>) -> Vec<DocPage> {
    let s = src.unwrap_or("");
    if let Some(rest) = s.strip_prefix(DOC_HEADER) {
        if let Ok(j) = serde_json::from_str::<Value>(rest) {
            if let Some(pages) = j.get("pages").and_then(Value::as_array) {
                if !pages.is_empty() {
                    return pages
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            let name = p
                                .get("name")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                                .unwrap_or_else(|| format!("Page {}", i + 1));
                            let mut blocks: Vec<MathBlock> = p
                                .get("blocks")
                                .and_then(Value::as_array)
                                .map(|arr| arr.iter().filter_map(normalize_block).collect())
                                .unwrap_or_default();
                            if blocks.is_empty() {
                                blocks.push(MathBlock::Formula {
                                    latex: String::new(),
                                    align: None,
                                    numbered: None,
                                    frame: None,
                                    scale: None,
                                });
                            }
                            let format = p.get("format").and_then(Value::as_str).map(str::to_string);
                            let orientation = match p.get("orientation").and_then(Value::as_str) {
                                Some("landscape") => Some(Orientation::Landscape),
                                Some("portrait") => Some(Orientation::Portrait),
                                _ => None,
                            };
                            DocPage { name, blocks, format, orientation }
                        })
                        .collect();
                }
            }
        }
    }
    // Legacy formats → one formula block per page.
    parse_pages(Some(s))
        .into_iter()
        .map(|(name, latex)| DocPage {
            name,
            blocks: vec![MathBlock::Formula { latex, align: None, numbered: None, frame: None, scale: None }],
            format: None,
            orientation: None,
        })
        .collect()
}

/// Read the legacy page forms (`mathPages.ts` `parsePages`): `(name, latex)` per page.
fn parse_pages(src: Option<&str>) -> Vec<(String, String)> {
    let s = src.unwrap_or("");
    if let Some(rest) = s.strip_prefix(HEADER) {
        if let Ok(j) = serde_json::from_str::<Value>(rest) {
            if let Some(pages) = j.get("pages").and_then(Value::as_array) {
                if !pages.is_empty() {
                    return pages
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            (
                                p.get("name").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("Page {}", i + 1)),
                                p.get("latex").and_then(Value::as_str).unwrap_or("").to_string(),
                            )
                        })
                        .collect();
                }
            }
        }
    }
    vec![("Page 1".to_string(), s.to_string())]
}

// ── Writing ─────────────────────────────────────────────────────────────────────

fn frame_json(f: &BlockFrame) -> String {
    let mut s = format!("{{\"x\":{},\"y\":{},\"w\":{}", json_number(f.x), json_number(f.y), json_number(f.w));
    if let Some(h) = f.h {
        s += &format!(",\"h\":{}", json_number(h));
    }
    if let Some(r) = f.r {
        s += &format!(",\"r\":{}", json_number(r));
    }
    s.push('}');
    s
}

fn block_json(b: &MathBlock) -> String {
    match b {
        MathBlock::Graph { spec, frame } => {
            let mut s = format!("{{\"type\":\"graph\",\"spec\":{}", spec_to_stored_json(spec));
            if let Some(f) = frame {
                s += &format!(",\"frame\":{}", frame_json(f));
            }
            s.push('}');
            s
        }
        MathBlock::Text { text, style, frame, scale } => {
            let style = match style {
                TextStyle::H1 => "h1",
                TextStyle::H2 => "h2",
                TextStyle::P => "p",
            };
            let mut s = format!("{{\"type\":\"text\",\"text\":{},\"style\":\"{style}\"", json_string(text));
            if let Some(f) = frame {
                s += &format!(",\"frame\":{}", frame_json(f));
            }
            if let Some(sc) = scale {
                s += &format!(",\"scale\":{}", json_number(*sc));
            }
            s.push('}');
            s
        }
        MathBlock::Formula { latex, align, numbered, frame, scale } => {
            let mut s = format!("{{\"type\":\"formula\",\"latex\":{}", json_string(latex));
            if let Some(a) = align {
                s += match a {
                    BlockAlign::Left => ",\"align\":\"left\"",
                    BlockAlign::Right => ",\"align\":\"right\"",
                };
            }
            if numbered == &Some(true) {
                s += ",\"numbered\":true";
            }
            if let Some(f) = frame {
                s += &format!(",\"frame\":{}", frame_json(f));
            }
            if let Some(sc) = scale {
                s += &format!(",\"scale\":{}", json_number(*sc));
            }
            s.push('}');
            s
        }
    }
}

fn page_json(p: &DocPage) -> String {
    let blocks = p.blocks.iter().map(block_json).collect::<Vec<_>>().join(",");
    let mut s = format!("{{\"name\":{},\"blocks\":[{blocks}]", json_string(&p.name));
    if let Some(fmt) = &p.format {
        s += &format!(",\"format\":{}", json_string(fmt));
    }
    if let Some(o) = p.orientation {
        s += &format!(",\"orientation\":\"{}\"", o.as_str());
    }
    s.push('}');
    s
}

/// `true` when a frame is exactly the default full-width placement (carries no information).
fn is_default_frame(f: Option<&BlockFrame>, page: &DocPage) -> bool {
    match f {
        None => true,
        Some(f) => {
            let (sw, _) = paper_px(
                page.format.as_deref().unwrap_or(DEFAULT_FORMAT),
                page.orientation.unwrap_or(Orientation::Portrait),
                3.78,
            );
            f.x == SHEET_PAD && f.y == SHEET_PAD && f.w == sw - SHEET_PAD * 2.0 && f.h.is_none() && f.r.is_none()
        }
    }
}

/// Serialise pages-of-blocks (`mathPages.ts` `serializeDoc`). The trivial "one page, one centred
/// default formula" case stays plain LaTeX; anything richer uses the `%%KBMATH-DOC%%` envelope.
pub fn serialize_doc(pages: &[DocPage]) -> String {
    let Some(p0) = pages.first() else {
        return String::new();
    };
    let Some(b0) = p0.blocks.first() else {
        return String::new();
    };

    if let MathBlock::Formula { latex, align, numbered, frame, scale } = b0 {
        let trivial = pages.len() == 1
            && p0.blocks.len() == 1
            && align.is_none()
            && numbered != &Some(true)
            && is_default_frame(frame.as_ref(), p0)
            && (scale.is_none() || *scale == Some(1.0))
            && (p0.format.is_none() || p0.format.as_deref() == Some(DEFAULT_FORMAT))
            && (p0.orientation.is_none() || p0.orientation == Some(Orientation::Portrait));
        if trivial {
            return latex.clone();
        }
    }

    let pages_json = pages.iter().map(page_json).collect::<Vec<_>>().join(",");
    format!("{DOC_HEADER}{{\"v\":1,\"pages\":[{pages_json}]}}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trivial_plain_latex() {
        // A single default formula round-trips as bare LaTeX.
        let pages = parse_doc(Some("x^{2}"));
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].blocks.len(), 1);
        assert_eq!(serialize_doc(&pages), "x^{2}");
    }

    #[test]
    fn doc_envelope_round_trip() {
        let src = "%%KBMATH-DOC%%{\"v\":1,\"pages\":[{\"name\":\"Page 1\",\"blocks\":[{\"type\":\"formula\",\"latex\":\"a+b\",\"align\":\"left\",\"numbered\":true,\"frame\":{\"x\":10,\"y\":20,\"w\":300}},{\"type\":\"text\",\"text\":\"Hi\",\"style\":\"h1\"}]}]}";
        let pages = parse_doc(Some(src));
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].blocks.len(), 2);
        // byte-identical re-serialisation
        assert_eq!(serialize_doc(&pages), src);
    }

    #[test]
    fn multipage_legacy() {
        let src = "%%KBMATH-MULTIPAGE%%{\"pages\":[{\"name\":\"P1\",\"latex\":\"a\"},{\"name\":\"P2\",\"latex\":\"b\"}]}";
        let pages = parse_doc(Some(src));
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[1].name, "P2");
        if let MathBlock::Formula { latex, .. } = &pages[1].blocks[0] {
            assert_eq!(latex, "b");
        } else {
            panic!("expected formula");
        }
    }

    #[test]
    fn graph_block_round_trip() {
        let src = "%%KBMATH-DOC%%{\"v\":1,\"pages\":[{\"name\":\"Page 1\",\"blocks\":[{\"type\":\"graph\",\"spec\":{\"fns\":[{\"expr\":\"sin(x)\",\"color\":\"#1a73e8\"}],\"xmin\":-10,\"xmax\":10,\"ymin\":null,\"ymax\":null,\"grid\":true,\"axisNumbers\":true,\"height\":280}}]}]}";
        let pages = parse_doc(Some(src));
        assert_eq!(serialize_doc(&pages), src);
    }
}
