//! KaTeX's data tables, extracted from its sources by `tools/extract.ts` into `data/*.json` and embedded:
//! the character metrics of every font (`fontMetricsData.js`), the symbol table (`symbols.ts`), the spacing
//! between atom classes (`spacingData.ts`), the TeX font parameters per size class (`fontMetrics.ts`) and the
//! static SVG paths (`svgGeometry.ts`).

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;

/// `getCharacterMetrics`: depth, height, italic correction, skew and width, in ems.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CharMetrics {
    pub depth: f64,
    pub height: f64,
    pub italic: f64,
    pub skew: f64,
    pub width: f64,
}

/// The parsing mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Math,
    Text,
}

/// A symbol's group (`atoms.ts` and the symbol table's group names).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Bin,
    Close,
    Inner,
    Open,
    Punct,
    Rel,
    MathOrd,
    TextOrd,
    Spacing,
    AccentToken,
    OpToken,
}

impl Group {
    fn parse(s: &str) -> Option<Group> {
        Some(match s {
            "bin" => Group::Bin,
            "close" => Group::Close,
            "inner" => Group::Inner,
            "open" => Group::Open,
            "punct" => Group::Punct,
            "rel" => Group::Rel,
            "mathord" => Group::MathOrd,
            "textord" => Group::TextOrd,
            "spacing" => Group::Spacing,
            "accent-token" => Group::AccentToken,
            "op-token" => Group::OpToken,
            _ => return None,
        })
    }

    /// `isAtom`: the groups that parse as an `atom` node.
    pub fn is_atom(self) -> bool {
        matches!(self, Group::Bin | Group::Close | Group::Inner | Group::Open | Group::Punct | Group::Rel)
    }

    /// The atom family name (`"bin"`, `"rel"`…).
    pub fn family(self) -> &'static str {
        match self {
            Group::Bin => "bin",
            Group::Close => "close",
            Group::Inner => "inner",
            Group::Open => "open",
            Group::Punct => "punct",
            Group::Rel => "rel",
            _ => "ord",
        }
    }
}

/// An entry of the symbol table.
#[derive(Clone, Debug)]
pub struct SymInfo {
    /// `"main"` or `"ams"`.
    pub font: String,
    pub group: Group,
    pub replace: Option<String>,
}

/// A TeX dimension (`units.ts` Measurement).
#[derive(Clone, Debug, PartialEq)]
pub struct Measurement {
    pub number: f64,
    pub unit: String,
}

/// The TeX font parameters of one size class (`getGlobalMetrics`).
#[derive(Clone, Debug, Default)]
pub struct FontParams {
    pub css_em_per_mu: f64,
    pub slant: f64,
    pub x_height: f64,
    pub quad: f64,
    pub num1: f64,
    pub num2: f64,
    pub num3: f64,
    pub denom1: f64,
    pub denom2: f64,
    pub sup1: f64,
    pub sup2: f64,
    pub sup3: f64,
    pub sub1: f64,
    pub sub2: f64,
    pub sup_drop: f64,
    pub sub_drop: f64,
    pub delim1: f64,
    pub delim2: f64,
    pub axis_height: f64,
    pub default_rule_thickness: f64,
    pub big_op_spacing1: f64,
    pub big_op_spacing2: f64,
    pub big_op_spacing3: f64,
    pub big_op_spacing4: f64,
    pub big_op_spacing5: f64,
    pub sqrt_rule_thickness: f64,
    pub pt_per_em: f64,
    pub double_rule_sep: f64,
    pub array_rule_width: f64,
    pub fboxsep: f64,
    pub fboxrule: f64,
}

struct Tables {
    metrics: HashMap<String, HashMap<u32, [f64; 5]>>,
    symbols: [HashMap<String, SymInfo>; 2],
    spacings: HashMap<String, HashMap<String, Measurement>>,
    tight_spacings: HashMap<String, HashMap<String, Measurement>>,
    params: [FontParams; 3],
    svg_paths: HashMap<String, String>,
}

fn parse_json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or(Value::Null)
}

fn measurement(v: &Value) -> Option<Measurement> {
    Some(Measurement { number: v.get("number")?.as_f64()?, unit: v.get("unit")?.as_str()?.to_string() })
}

fn spacing_table(v: &Value) -> HashMap<String, HashMap<String, Measurement>> {
    let mut out = HashMap::new();
    if let Some(o) = v.as_object() {
        for (prev, row) in o {
            let mut r = HashMap::new();
            if let Some(ro) = row.as_object() {
                for (cur, m) in ro {
                    if let Some(m) = measurement(m) {
                        r.insert(cur.clone(), m);
                    }
                }
            }
            out.insert(prev.clone(), r);
        }
    }
    out
}

fn params(v: &Value) -> FontParams {
    let f = |k: &str| v.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    FontParams {
        css_em_per_mu: f("cssEmPerMu"),
        slant: f("slant"),
        x_height: f("xHeight"),
        quad: f("quad"),
        num1: f("num1"),
        num2: f("num2"),
        num3: f("num3"),
        denom1: f("denom1"),
        denom2: f("denom2"),
        sup1: f("sup1"),
        sup2: f("sup2"),
        sup3: f("sup3"),
        sub1: f("sub1"),
        sub2: f("sub2"),
        sup_drop: f("supDrop"),
        sub_drop: f("subDrop"),
        delim1: f("delim1"),
        delim2: f("delim2"),
        axis_height: f("axisHeight"),
        default_rule_thickness: f("defaultRuleThickness"),
        big_op_spacing1: f("bigOpSpacing1"),
        big_op_spacing2: f("bigOpSpacing2"),
        big_op_spacing3: f("bigOpSpacing3"),
        big_op_spacing4: f("bigOpSpacing4"),
        big_op_spacing5: f("bigOpSpacing5"),
        sqrt_rule_thickness: f("sqrtRuleThickness"),
        pt_per_em: f("ptPerEm"),
        double_rule_sep: f("doubleRuleSep"),
        array_rule_width: f("arrayRuleWidth"),
        fboxsep: f("fboxsep"),
        fboxrule: f("fboxrule"),
    }
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let m = parse_json(include_str!("../data/metrics.json"));
        let mut metrics = HashMap::new();
        if let Some(o) = m.as_object() {
            for (font, chars) in o {
                let mut map = HashMap::new();
                if let Some(co) = chars.as_object() {
                    for (code, arr) in co {
                        let (Ok(code), Some(a)) = (code.parse::<u32>(), arr.as_array()) else { continue };
                        let mut v = [0.0; 5];
                        for (i, slot) in v.iter_mut().enumerate() {
                            *slot = a.get(i).and_then(Value::as_f64).unwrap_or(0.0);
                        }
                        map.insert(code, v);
                    }
                }
                metrics.insert(font.clone(), map);
            }
        }
        let s = parse_json(include_str!("../data/symbols.json"));
        let sym = |mode: &str| {
            let mut out = HashMap::new();
            if let Some(o) = s.get(mode).and_then(Value::as_object) {
                for (name, e) in o {
                    let font = e.get("font").and_then(Value::as_str).unwrap_or("main").to_string();
                    let Some(group) = e.get("group").and_then(Value::as_str).and_then(Group::parse) else { continue };
                    let replace = e.get("replace").and_then(Value::as_str).map(str::to_string);
                    out.insert(name.clone(), SymInfo { font, group, replace });
                }
            }
            out
        };
        let sp = parse_json(include_str!("../data/spacing.json"));
        let sg = parse_json(include_str!("../data/sigmas.json"));
        let p = |i: usize| params(sg.get(i).unwrap_or(&Value::Null));
        let paths = parse_json(include_str!("../data/svgpaths.json"));
        let mut svg_paths = HashMap::new();
        if let Some(o) = paths.as_object() {
            for (k, v) in o {
                if let Some(s) = v.as_str() {
                    svg_paths.insert(k.clone(), s.to_string());
                }
            }
        }
        Tables {
            metrics,
            symbols: [sym("math"), sym("text")],
            spacings: spacing_table(sp.get("spacings").unwrap_or(&Value::Null)),
            tight_spacings: spacing_table(sp.get("tightSpacings").unwrap_or(&Value::Null)),
            params: [p(0), p(1), p(2)],
            svg_paths,
        }
    })
}

/// `extraCharacterMap`: accented Latin and Cyrillic letters measured as a similar Latin letter.
fn extra_character(c: char) -> Option<char> {
    Some(match c {
        'Å' => 'A',
        'Ð' => 'D',
        'Þ' => 'o',
        'å' => 'a',
        'ð' => 'd',
        'þ' => 'o',
        'А' => 'A',
        'Б' => 'B',
        'В' => 'B',
        'Г' => 'F',
        'Д' => 'A',
        'Е' => 'E',
        'Ж' => 'K',
        'З' => '3',
        'И' => 'N',
        'Й' => 'N',
        'К' => 'K',
        'Л' => 'N',
        'М' => 'M',
        'Н' => 'H',
        'О' => 'O',
        'П' => 'N',
        'Р' => 'P',
        'С' => 'C',
        'Т' => 'T',
        'У' => 'y',
        'Ф' => 'O',
        'Х' => 'X',
        'Ц' => 'U',
        'Ч' => 'h',
        'Ш' => 'W',
        'Щ' => 'W',
        'Ъ' => 'B',
        'Ы' => 'X',
        'Ь' => 'B',
        'Э' => '3',
        'Ю' => 'X',
        'Я' => 'R',
        'а' => 'a',
        'б' => 'b',
        'в' => 'a',
        'г' => 'r',
        'д' => 'y',
        'е' => 'e',
        'ж' => 'm',
        'з' => 'e',
        'и' => 'n',
        'й' => 'n',
        'к' => 'n',
        'л' => 'n',
        'м' => 'm',
        'н' => 'n',
        'о' => 'o',
        'п' => 'n',
        'р' => 'p',
        'с' => 'c',
        'т' => 'o',
        'у' => 'y',
        'ф' => 'b',
        'х' => 'x',
        'ц' => 'n',
        'ч' => 'n',
        'ш' => 'w',
        'щ' => 'w',
        'ъ' => 'a',
        'ы' => 'm',
        'ь' => 'a',
        'э' => 'e',
        'ю' => 'm',
        'я' => 'r',
        _ => return None,
    })
}

/// `supportedCodepoint` (unicodeScripts.ts): the scripts KaTeX measures as an `M` in text mode.
fn supported_codepoint(c: u32) -> bool {
    const BLOCKS: &[(u32, u32)] = &[
        (0x0100, 0x024f),
        (0x0300, 0x036f),
        (0x0400, 0x04ff),
        (0x0530, 0x058f),
        (0x0900, 0x109f),
        (0x10a0, 0x10ff),
        (0x3000, 0x30ff),
        (0x4e00, 0x9faf),
        (0xff00, 0xff60),
        (0xac00, 0xd7af),
        (0xd800, 0xdfff),
    ];
    BLOCKS.iter().any(|&(a, b)| c >= a && c <= b)
}

/// `getCharacterMetrics(character, font, mode)`: the first UTF-16 unit of `ch` looked up in `font`.
pub fn char_metrics(ch: &str, font: &str, mode: Mode) -> Option<CharMetrics> {
    let t = tables();
    let table = t.metrics.get(font)?;
    let first = ch.encode_utf16().next()? as u32;
    let mut m = table.get(&first);
    if m.is_none() {
        if let Some(e) = ch.chars().next().and_then(extra_character) {
            m = table.get(&(e as u32));
        }
    }
    if m.is_none() && mode == Mode::Text && supported_codepoint(first) {
        m = table.get(&77);
    }
    m.map(|a| CharMetrics { depth: a[0], height: a[1], italic: a[2], skew: a[3], width: a[4] })
}

/// The raw metrics entry of one code point (`fontMetricsData[font][code]`).
pub fn metrics_entry(font: &str, code: u32) -> Option<[f64; 5]> {
    tables().metrics.get(font)?.get(&code).copied()
}

/// The symbol table entry of `name` in `mode`.
pub fn symbol(mode: Mode, name: &str) -> Option<&'static SymInfo> {
    let t = tables();
    t.symbols[match mode {
        Mode::Math => 0,
        Mode::Text => 1,
    }]
    .get(name)
}

/// The spacing between two atom classes (`spacings` / `tightSpacings`).
pub fn spacing(prev: &str, cur: &str, tight: bool) -> Option<&'static Measurement> {
    let t = tables();
    let table = if tight { &t.tight_spacings } else { &t.spacings };
    table.get(prev)?.get(cur)
}

/// `getGlobalMetrics(size)`.
pub fn font_params(size: u8) -> &'static FontParams {
    let t = tables();
    let i = if size >= 5 {
        0
    } else if size >= 3 {
        1
    } else {
        2
    };
    &t.params[i]
}

/// A static SVG path of `svgGeometry.ts`.
pub fn svg_path(name: &str) -> Option<&'static str> {
    tables().svg_paths.get(name).map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_load() {
        assert!(char_metrics("x", "Math-Italic", Mode::Math).is_some());
        let m = char_metrics("(", "Main-Regular", Mode::Math).unwrap_or_default();
        assert!((m.height - 0.75).abs() < 1e-9, "{m:?}");
        assert_eq!(symbol(Mode::Math, "\\alpha").map(|s| s.group), Some(Group::MathOrd));
        assert_eq!(symbol(Mode::Math, "+").map(|s| s.group), Some(Group::Bin));
        assert!(spacing("mord", "mbin", false).is_some());
        assert!((font_params(6).axis_height - 0.25).abs() < 1e-9);
        assert!((font_params(6).css_em_per_mu - 1.0 / 18.0).abs() < 1e-9);
        assert!(svg_path("vec").is_some());
        for k in ["\\neq", "\\ne", "\\le", "\\dots", "\\ldots", "\\cdots", "\\iff", "\\not", "\\prime", "\\{", "\\lbrace", "\\infty", "\\colon", "\\to", "\\pm", "\\quad", "\\,", "\\hat", "\\sum", "\\int", "\\cdot", "\\mid", "\\vert", "|"] {
            eprintln!("{k}: {:?}", symbol(Mode::Math, k));
        }
        eprintln!("text space: {:?}", symbol(Mode::Text, " "));
    }
}
