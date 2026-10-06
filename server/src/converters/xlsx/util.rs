//! Shared helpers for the XLSX reader and writer: cell references, colours,
//! unit conversions, XML escaping, ZIP access, number-format tables and the
//! `_xlfn.` modern-function list.
use std::collections::HashMap;
use std::io::{Cursor, Read};

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

// ── Column letter ↔ index ─────────────────────────────────────────────────────

/// "B7" → ("B", 7). Returns None when the reference has no digits.
pub fn split_ref(r: &str) -> Option<(String, i32)> {
    let pos = r.find(|c: char| c.is_ascii_digit())?;
    let row = r[pos..].parse::<i32>().ok()?;
    Some((r[..pos].to_uppercase(), row))
}

/// "A" → 0, "Z" → 25, "AA" → 26 …
pub fn col_to_idx(col: &str) -> usize {
    col.chars().fold(0usize, |a, c| a * 26 + (c as usize - 'A' as usize + 1)) - 1
}

/// 0 → "A", 26 → "AA" …
pub fn idx_to_col(mut i: usize) -> String {
    let mut s = String::new();
    loop { s.insert(0, (b'A' + (i % 26) as u8) as char); if i < 26 { break } i = i / 26 - 1 }
    s
}

// ── XML helpers ───────────────────────────────────────────────────────────────

/// Read a (namespace-agnostic) attribute off an element.
pub fn attr(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().flatten()
        .find(|a| a.key.local_name().as_ref() == name)
        .map(|a| String::from_utf8_lossy(&a.value).into_owned())
}

/// Escape a string for use inside an XML text node or attribute value.
/// Control characters below U+0020 (except TAB/LF/CR) are encoded with the
/// Excel `_xHHHH_` convention; consequently a literal `_xHHHH_` in the input
/// gets its underscore escaped as `_x005F_`.
pub fn esc_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let bytes = s.as_bytes();
    for (i, c) in s.char_indices() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 => out.push_str(&format!("_x{:04X}_", c as u32)),
            '_' => {
                // A literal that *looks like* an _xHHHH_ escape must have its
                // leading underscore escaped so readers don't decode it.
                if looks_like_x_escape(&bytes[i..]) { out.push_str("_x005F_"); } else { out.push('_'); }
            }
            c => out.push(c),
        }
    }
    out
}

// True when the byte slice starts with the pattern `_xHHHH_`.
fn looks_like_x_escape(b: &[u8]) -> bool {
    b.len() >= 7
        && b[0] == b'_'
        && (b[1] == b'x' || b[1] == b'X')
        && b[2..6].iter().all(|c| c.is_ascii_hexdigit())
        && b[6] == b'_'
}

// ── Colour helpers ───────────────────────────────────────────────────────────

// Fallback Office 2007 theme palette, indexed the SpreadsheetML way (0/1 and
// 2/3 are the lt/dk swap): 0=lt1 1=dk1 2=lt2 3=dk2 4..9=accent1..6 10=hlink
// 11=folHlink. Used only when the workbook has no readable xl/theme/theme1.xml.
pub const THEME: [&str; 12] = [
    "FFFFFF", "000000", "EEECE1", "1F497D", "4F81BD", "C0504D",
    "9BBB59", "8064A2", "4BACC6", "F79646", "0000FF", "800080",
];

/// Legacy indexed-colour palette (indices 0–63, see LibreOffice
/// sc/source/filter/oox/stylesbuffer.cxx). Indices 64/65 are the system
/// text/background ("automatic") colours and resolve to None here so callers
/// fall back to their defaults. A workbook may override this palette via
/// `<colors><indexedColors>` in styles.xml.
pub const INDEXED_COLORS: [&str; 64] = [
    "000000", "FFFFFF", "FF0000", "00FF00", "0000FF", "FFFF00", "FF00FF", "00FFFF",
    "000000", "FFFFFF", "FF0000", "00FF00", "0000FF", "FFFF00", "FF00FF", "00FFFF",
    "800000", "008000", "000080", "808000", "800080", "008080", "C0C0C0", "808080",
    "9999FF", "993366", "FFFFCC", "CCFFFF", "660066", "FF8080", "0066CC", "CCCCFF",
    "000080", "FF00FF", "FFFF00", "00FFFF", "800080", "800000", "008080", "0000FF",
    "00CCFF", "CCFFFF", "CCFFCC", "FFFF99", "99CCFF", "FF99CC", "CC99FF", "FFCC99",
    "3366FF", "33CCCC", "99CC00", "FFCC00", "FF9900", "FF6600", "666699", "969696",
    "003366", "339966", "003300", "333300", "993300", "993366", "333399", "333333",
];

/// Colour-resolution context: theme palette (from theme1.xml, SpreadsheetML
/// slot order) plus indexed palette (default legacy or redefined by the file).
pub struct ColorCtx {
    pub theme:   Vec<String>, // 12 entries "RRGGBB"
    pub indexed: Vec<String>, // 64 entries "RRGGBB"
}

impl Default for ColorCtx {
    fn default() -> Self {
        Self {
            theme:   THEME.iter().map(|s| s.to_string()).collect(),
            indexed: INDEXED_COLORS.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl ColorCtx {
    /// Resolve a colour element's attributes to "#RRGGBB". Resolution order
    /// mirrors LibreOffice (stylesbuffer.cxx): theme → rgb → indexed → auto
    /// (auto = None, the caller applies its default).
    pub fn resolve(&self, rgb: Option<&str>, theme: Option<usize>, indexed: Option<usize>, tint: f64) -> Option<String> {
        if let Some(t) = theme {
            if let Some(base) = self.theme.get(t) { return Some(apply_tint(base, tint)); }
        }
        if let Some(rgb) = rgb {
            // ARGB "FFRRGGBB" or "RRGGBB".
            let h = if rgb.len() == 8 { &rgb[2..] } else { rgb };
            if h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(if tint != 0.0 { apply_tint(h, tint) } else { format!("#{h}") });
            }
        }
        if let Some(i) = indexed {
            // 64/65 = system foreground/background → automatic.
            if let Some(base) = self.indexed.get(i) {
                return Some(if tint != 0.0 { apply_tint(base, tint) } else { format!("#{base}") });
            }
        }
        None
    }
}

/// Resolve a `<color rgb/theme tint>` element's attributes to "#RRGGBB" using
/// the DEFAULT palettes (no theme/indexed context). Kept for parts parsed
/// without a `ColorCtx` (e.g. tabColor); styles.xml colours go through
/// `ColorCtx::resolve` with the real theme instead.
pub fn resolve_color(rgb: Option<&str>, theme: Option<usize>, tint: f64) -> Option<String> {
    if let Some(rgb) = rgb {
        // ARGB "FFRRGGBB" or "RRGGBB".
        let h = if rgb.len() == 8 { &rgb[2..] } else { rgb };
        if h.len() == 6 { return Some(if tint != 0.0 { apply_tint(h, tint) } else { format!("#{h}") }); }
    }
    if let Some(t) = theme {
        if let Some(base) = THEME.get(t) { return Some(apply_tint(base, tint)); }
    }
    None
}

/// Parse `xl/theme/theme1.xml` and return the 12-slot theme palette in the
/// SpreadsheetML `theme` attribute order (0=lt1 1=dk1 2=lt2 3=dk2 — note the
/// lt/dk swap — 4..9=accent1..6 10=hlink 11=folHlink). Missing slots fall back
/// to the Office 2007 defaults; returns None when no `<clrScheme>` is found.
pub fn parse_theme_colors(xml: &str) -> Option<Vec<String>> {
    let mut reader = Reader::from_str(xml);
    let mut in_scheme = false;
    let mut saw_scheme = false;
    let mut cur_slot: Option<usize> = None;
    let mut out: Vec<Option<String>> = vec![None; 12];
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let ln = e.local_name();
                match ln.as_ref() {
                    b"clrScheme" => { in_scheme = true; saw_scheme = true; }
                    b"lt1" if in_scheme => cur_slot = Some(0),
                    b"dk1" if in_scheme => cur_slot = Some(1),
                    b"lt2" if in_scheme => cur_slot = Some(2),
                    b"dk2" if in_scheme => cur_slot = Some(3),
                    b"accent1" if in_scheme => cur_slot = Some(4),
                    b"accent2" if in_scheme => cur_slot = Some(5),
                    b"accent3" if in_scheme => cur_slot = Some(6),
                    b"accent4" if in_scheme => cur_slot = Some(7),
                    b"accent5" if in_scheme => cur_slot = Some(8),
                    b"accent6" if in_scheme => cur_slot = Some(9),
                    b"hlink" if in_scheme => cur_slot = Some(10),
                    b"folHlink" if in_scheme => cur_slot = Some(11),
                    b"srgbClr" | b"sysClr" if in_scheme => {
                        if let Some(slot) = cur_slot {
                            // sysClr carries the resolved value in lastClr; fall back
                            // to the conventional system colours when it is absent.
                            let v = if ln.as_ref() == b"sysClr" {
                                attr(&e, b"lastClr").or_else(|| match attr(&e, b"val").as_deref() {
                                    Some("windowText") => Some("000000".into()),
                                    Some("window") => Some("FFFFFF".into()),
                                    _ => None,
                                })
                            } else {
                                attr(&e, b"val")
                            };
                            if let Some(v) = v {
                                if out[slot].is_none() && v.len() == 6 { out[slot] = Some(v.to_uppercase()); }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                // Only the first colour scheme (the theme's) matters.
                b"clrScheme" => break,
                b"lt1" | b"dk1" | b"lt2" | b"dk2" | b"accent1" | b"accent2" | b"accent3"
                | b"accent4" | b"accent5" | b"accent6" | b"hlink" | b"folHlink" => cur_slot = None,
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    if !saw_scheme { return None; }
    Some(out.into_iter().enumerate()
        .map(|(i, c)| c.unwrap_or_else(|| THEME[i].to_string()))
        .collect())
}

// Apply an Excel tint to a colour in HSL luminance space (matches Excel closely).
pub fn apply_tint(hex: &str, tint: f64) -> String {
    let r = i64::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f64 / 255.0;
    let g = i64::from_str_radix(&hex[2..4], 16).unwrap_or(0) as f64 / 255.0;
    let b = i64::from_str_radix(&hex[4..6], 16).unwrap_or(0) as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let mut l = (max + min) / 2.0;
    let d = max - min;
    let (mut h, mut s) = (0.0, 0.0);
    if d > f64::EPSILON {
        s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
        h = if max == r { (g - b) / d + if g < b { 6.0 } else { 0.0 } }
            else if max == g { (b - r) / d + 2.0 }
            else { (r - g) / d + 4.0 };
        h /= 6.0;
    }
    l = if tint < 0.0 { l * (1.0 + tint) } else { l * (1.0 - tint) + tint };
    let hue = |p: f64, q: f64, mut t: f64| {
        if t < 0.0 { t += 1.0 }
        if t > 1.0 { t -= 1.0 }
        if t < 1.0 / 6.0 { p + (q - p) * 6.0 * t }
        else if t < 1.0 / 2.0 { q }
        else if t < 2.0 / 3.0 { p + (q - p) * (2.0 / 3.0 - t) * 6.0 }
        else { p }
    };
    let (nr, ng, nb) = if s.abs() < f64::EPSILON {
        (l, l, l)
    } else {
        let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
        let p = 2.0 * l - q;
        (hue(p, q, h + 1.0 / 3.0), hue(p, q, h), hue(p, q, h - 1.0 / 3.0))
    };
    format!("#{:02X}{:02X}{:02X}", (nr * 255.0).round() as u8, (ng * 255.0).round() as u8, (nb * 255.0).round() as u8)
}

// ── ZIP part access ──────────────────────────────────────────────────────────

/// Locate a part by name, tolerating archives written with backslash
/// separators or mismatched case in entry names (Excel and LibreOffice both
/// accept such files — e.g. the tdf76115/tdf131575 regression documents).
fn find_zip_entry(archive: &zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<String> {
    if archive.index_for_name(name).is_some() { return Some(name.to_string()); }
    let want = name.replace('\\', "/").to_ascii_lowercase();
    archive.file_names()
        .find(|n| n.replace('\\', "/").to_ascii_lowercase() == want)
        .map(|n| n.to_string())
}

pub fn read_zip_text(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<String> {
    let entry = find_zip_entry(archive, name)?;
    let mut f = archive.by_name(&entry).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    match String::from_utf8(buf) {
        Ok(s) => Some(s),
        // Some producers write ISO-8859-1 parts (e.g. the tdf76115 regression
        // document); latin-1 maps every byte to the same code point, so this
        // also degrades gracefully for close single-byte encodings.
        Err(e) => Some(e.into_bytes().iter().map(|&b| b as char).collect()),
    }
}

pub fn read_zip_bytes(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<Vec<u8>> {
    let entry = find_zip_entry(archive, name)?;
    let mut f = archive.by_name(&entry).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(buf)
}

// Resolve a relationship target (possibly "../media/x.png" or "/xl/media/x.png")
// against the directory of the part that declared it (e.g. "xl/worksheets/sheet1.xml").
pub fn resolve_path(base_part: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') { return abs.to_string(); }
    let base_dir = base_part.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let mut segs: Vec<&str> = if base_dir.is_empty() { Vec::new() } else { base_dir.split('/').collect() };
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => { segs.pop(); }
            p => segs.push(p),
        }
    }
    segs.join("/")
}

// Guess an image MIME type from the media part's file extension.
pub fn image_mime(path: &str) -> &'static str {
    match path.rsplit('.').next().map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

// ── Shared-formula translation ───────────────────────────────────────────────

// Translate a shared formula to a follower cell by shifting relative references
// (those without a `$`) by (d_col, d_row). Skips strings, sheet-qualified refs
// and function names (a letters+digits run immediately followed by '(').
pub fn translate_formula(f: &str, d_col: i64, d_row: i64) -> String {
    if d_col == 0 && d_row == 0 { return f.to_string(); }
    let b = f.as_bytes();
    let mut out = String::with_capacity(f.len());
    let mut i = 0usize;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i] as char;
        if in_str { out.push(c); if c == '"' { in_str = false } i += 1; continue; }
        if c == '"' { in_str = true; out.push(c); i += 1; continue; }
        let prev = out.chars().last().unwrap_or(' ');
        let boundary = !(prev.is_ascii_alphanumeric() || prev == '_' || prev == '!' || prev == '\'' || prev == '$' || prev == '.');
        if boundary && (c == '$' || c.is_ascii_alphabetic()) {
            let mut j = i;
            let col_abs = b[j] as char == '$'; if col_abs { j += 1; }
            let ls = j;
            while j < b.len() && (b[j] as char).is_ascii_alphabetic() { j += 1; }
            let letters = &f[ls..j];
            let row_abs = j < b.len() && b[j] as char == '$';
            let ds = if row_abs { j + 1 } else { j };
            let mut k = ds;
            while k < b.len() && (b[k] as char).is_ascii_digit() { k += 1; }
            let digits = &f[ds..k];
            let next = if k < b.len() { b[k] as char } else { ' ' };
            if !letters.is_empty() && letters.len() <= 3 && !digits.is_empty() && digits.len() <= 7 && next != '(' {
                let mut col = col_to_idx(&letters.to_uppercase()) as i64;
                let mut row = digits.parse::<i64>().unwrap_or(0);
                if !col_abs { col += d_col; }
                if !row_abs { row += d_row; }
                if col < 0 { col = 0; }
                if row < 1 { row = 1; }
                if col_abs { out.push('$'); }
                out.push_str(&idx_to_col(col as usize));
                if row_abs { out.push('$'); }
                out.push_str(&row.to_string());
                i = k;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

// ── Number-format tables ─────────────────────────────────────────────────────

/// Complete builtin numFmtId → format code table (ids 1–49, canonical en_US
/// codes per LibreOffice sc/source/filter/oox/numberformatsbuffer.cxx, with
/// the dd/mm date convention used by our renderer for ids 14/22).
pub const BUILTIN_NUMFMTS: &[(u32, &str)] = &[
    (1, "0"), (2, "0.00"), (3, "#,##0"), (4, "#,##0.00"),
    (5, "$#,##0_);($#,##0)"), (6, "$#,##0_);[Red]($#,##0)"),
    (7, "$#,##0.00_);($#,##0.00)"), (8, "$#,##0.00_);[Red]($#,##0.00)"),
    (9, "0%"), (10, "0.00%"), (11, "0.00E+00"), (12, "# ?/?"), (13, "# ??/??"),
    (14, "dd/mm/yyyy"), (15, "d-mmm-yy"), (16, "d-mmm"), (17, "mmm-yy"),
    (18, "h:mm AM/PM"), (19, "h:mm:ss AM/PM"), (20, "h:mm"), (21, "h:mm:ss"),
    (22, "dd/mm/yyyy h:mm"),
    (37, "#,##0_);(#,##0)"), (38, "#,##0_);[Red](#,##0)"),
    (39, "#,##0.00_);(#,##0.00)"), (40, "#,##0.00_);[Red](#,##0.00)"),
    (41, "_(* #,##0_);_(* (#,##0);_(* \"-\"_);_(@_)"),
    (42, "_($* #,##0_);_($* (#,##0);_($* \"-\"_);_(@_)"),
    (43, "_(* #,##0.00_);_(* (#,##0.00);_(* \"-\"??_);_(@_)"),
    (44, "_($* #,##0.00_);_($* (#,##0.00);_($* \"-\"??_);_(@_)"),
    (45, "mm:ss"), (46, "[h]:mm:ss"), (47, "mm:ss.0"), (48, "##0.0E+0"), (49, "@"),
];

/// Map locale-variant / reserved builtin ids onto the base builtin whose code
/// they reuse (LibreOffice NUMFMT_REUSE table). Ids not in the table map to
/// themselves; 0 stays 0 (General).
fn builtin_reuse(id: u32) -> u32 {
    match id {
        23..=26 => 0,                              // colour/parenthesis variants of General
        27..=31 | 36 | 50..=58 | 71 | 72 => 14,    // locale date variants
        32..=35 => 21,                             // locale time variants
        59..=62 => id - 58,                        // → 1..4
        63..=66 => id - 58,                        // → 5..8
        67 | 68 => id - 58,                        // → 9..10
        69 | 70 => id - 57,                        // → 12..13
        73..=75 => id - 58,                        // → 15..17
        76..=78 => id - 56,                        // → 20..22
        79..=81 => id - 34,                        // → 45..47
        _ => id,
    }
}

/// Resolve an xlsx number-format id to its format code. An explicit `<numFmt>`
/// entry wins (files may redefine even builtin ids); otherwise the builtin
/// table, following the locale-reuse aliases. Returns None for "General".
pub fn numfmt_code(id: u32, custom: &HashMap<u32, String>) -> Option<String> {
    if let Some(c) = custom.get(&id) {
        // Some producers (LibreOffice) re-declare General as an explicit
        // <numFmt formatCode="GENERAL">: treat it as no format at all,
        // otherwise the code would be rendered as a literal pattern.
        if c.trim().eq_ignore_ascii_case("general") { return None; }
        return Some(c.clone());
    }
    let id = builtin_reuse(id);
    if id == 0 { return None; }
    BUILTIN_NUMFMTS.iter().find(|(i, _)| *i == id).map(|(_, c)| (*c).to_string())
}

/// Reverse lookup: format code → builtin id (used by the writer to avoid
/// emitting custom numFmts for codes matching a builtin).
pub fn builtin_numfmt_id(code: &str) -> Option<u32> {
    BUILTIN_NUMFMTS.iter().find(|(_, c)| *c == code).map(|(i, _)| *i)
}

// ── `_xlfn.` modern functions ────────────────────────────────────────────────

/// Functions absent from the OOXML 2006 base set: stored in files with an
/// `_xlfn.` prefix (see LibreOffice sc/source/filter/oox/formulabase.cxx).
/// The import strips the prefix; the export re-adds it.
pub const XLFN_FUNCTIONS: &[&str] = &[
    "XLOOKUP", "XMATCH", "CONCAT", "TEXTJOIN", "IFS", "SWITCH", "MAXIFS", "MINIFS",
    "IFNA", "LET", "LAMBDA", "SEQUENCE", "UNIQUE", "SORTBY", "RANDARRAY",
    "TEXTBEFORE", "TEXTAFTER", "TEXTSPLIT", "VSTACK", "HSTACK", "TAKE", "DROP",
    "TOCOL", "TOROW", "WRAPROWS", "WRAPCOLS", "CHOOSECOLS", "CHOOSEROWS",
    "BYROW", "BYCOL", "MAP", "SCAN", "REDUCE", "MAKEARRAY", "ISOMITTED",
    "CEILING.MATH", "FLOOR.MATH", "CEILING.PRECISE", "FLOOR.PRECISE",
    "AGGREGATE", "ARABIC", "BASE", "DECIMAL",
    "BITAND", "BITOR", "BITXOR", "BITLSHIFT", "BITRSHIFT",
    "NORM.DIST", "NORM.INV", "NORM.S.DIST", "NORM.S.INV",
    "T.DIST", "T.DIST.2T", "T.DIST.RT", "T.INV", "T.INV.2T", "T.TEST",
    "CHISQ.DIST", "CHISQ.DIST.RT", "CHISQ.INV", "CHISQ.INV.RT", "CHISQ.TEST",
    "F.DIST", "F.DIST.RT", "F.INV", "F.INV.RT", "F.TEST",
    "GAMMA", "GAMMA.DIST", "GAMMA.INV", "GAMMALN.PRECISE",
    "LOGNORM.DIST", "LOGNORM.INV", "BINOM.DIST", "BINOM.DIST.RANGE", "BINOM.INV",
    "BETA.DIST", "BETA.INV", "EXPON.DIST", "POISSON.DIST", "WEIBULL.DIST",
    "HYPGEOM.DIST", "NEGBINOM.DIST",
    "CONFIDENCE.NORM", "CONFIDENCE.T", "COVARIANCE.P", "COVARIANCE.S",
    "MODE.SNGL", "MODE.MULT", "PERCENTILE.INC", "PERCENTILE.EXC",
    "QUARTILE.INC", "QUARTILE.EXC", "PERCENTRANK.INC", "PERCENTRANK.EXC",
    "RANK.EQ", "RANK.AVG", "STDEV.P", "STDEV.S", "VAR.P", "VAR.S", "SKEW.P",
    "Z.TEST", "FORECAST.LINEAR",
    "FORECAST.ETS", "FORECAST.ETS.CONFINT", "FORECAST.ETS.SEASONALITY", "FORECAST.ETS.STAT",
    "ISOWEEKNUM", "DAYS", "FORMULATEXT", "ISFORMULA", "SHEET", "SHEETS",
    "UNICHAR", "UNICODE", "NUMBERVALUE", "ENCODEURL", "FILTERXML", "WEBSERVICE",
    "ARRAYTOTEXT", "VALUETOTEXT", "GAUSS", "PHI", "PDURATION", "RRI", "MUNIT",
    "PERMUTATIONA", "COMBINA", "XOR",
    "COT", "COTH", "ACOT", "ACOTH", "CSC", "CSCH", "SEC", "SECH",
    "IMCOSH", "IMCOT", "IMCSC", "IMCSCH", "IMSEC", "IMSECH", "IMSINH", "IMTAN",
    "ERF.PRECISE", "ERFC.PRECISE",
];

/// Functions that additionally take the `_xlws.` sub-prefix (`_xlfn._xlws.SORT`).
pub const XLWS_FUNCTIONS: &[&str] = &["SORT", "FILTER"];

/// Recognised Excel error literals (values of `t="e"` cells).
pub const ERROR_LITERALS: &[&str] = &[
    "#NULL!", "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A", "#SPILL!", "#CALC!",
];

/// Strip `_xlfn.` / `_xlfn._xlws.` prefixes from a formula (outside strings).
pub fn strip_xlfn(f: &str) -> String {
    if !f.contains("_xlfn.") { return f.to_string(); }
    let mut out = String::with_capacity(f.len());
    let mut rest = f;
    let mut in_str = false;
    while !rest.is_empty() {
        if in_str {
            // Copy until the closing quote.
            match rest.find('"') {
                Some(p) => { out.push_str(&rest[..=p]); rest = &rest[p + 1..]; in_str = false; }
                None => { out.push_str(rest); break; }
            }
            continue;
        }
        // Find the next interesting position: a quote or an _xlfn. prefix.
        let q = rest.find('"');
        let x = rest.find("_xlfn.");
        match (q, x) {
            (Some(q), Some(x)) if q < x => { out.push_str(&rest[..=q]); rest = &rest[q + 1..]; in_str = true; }
            (_, Some(x)) => {
                out.push_str(&rest[..x]);
                rest = &rest[x + 6..]; // skip "_xlfn."
                if let Some(r) = rest.strip_prefix("_xlws.") { rest = r; }
            }
            (Some(q), None) => { out.push_str(&rest[..=q]); rest = &rest[q + 1..]; in_str = true; }
            (None, None) => { out.push_str(rest); break; }
        }
    }
    out
}
