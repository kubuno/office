//! XLSX converter tests: export→import round-trip and corpus import checks.
use serde_json::{json, Value};
use uuid::Uuid;

use super::write::{export_xlsx, SheetExportMeta};
use super::{import_xlsx, util};

fn cell<'a>(wb: &'a super::XlsxWorkbook, sheet: usize, key: &str) -> &'a Value {
    wb.sheets[sheet].cells.get(key).unwrap_or(&Value::Null)
}

#[test]
fn roundtrip_values_formulas_styles_merges() {
    let s1 = Uuid::new_v4();
    let s2 = Uuid::new_v4();
    let content = json!({
        "version": 1,
        "names": { "TAUX": "=Data!$B$1" },
        "sheets": {
            s1.to_string(): {
                "cells": {
                    "A1": { "v": "Titre", "s": { "bold": true, "fontSize": 16.0, "color": "#FF0000", "bg": "#FFFF00", "align": "center" } },
                    "A2": { "v": 12.5 },
                    "B2": { "v": true },
                    "C2": { "v": "#DIV/0!" },
                    "A3": { "f": "=SUM(A2;B2)", "v": 13.5 },
                    "A4": { "f": "=XLOOKUP(A2,A2:A3,A2:A3)", "v": 12.5 },
                    "A5": { "v": "texte  avec espaces " },
                    "B5": { "v": 3.0, "s": { "numFmtCode": "0.00%" } },
                    "C5": { "v": 5.0, "s": { "bt": "#000000", "btw": 2, "bb": "#00FF00" } },
                    "D5": { "s": { "bg": "#00AAFF" } }
                },
                "col_widths": { "A": 152.0, "B": 0.0 },
                "row_heights": { "2": 40.0, "3": 0.0 },
                "frozen_rows": 1,
                "frozen_cols": 2,
                "merges": ["A1:C1"],
                "gridlines": false
            },
            s2.to_string(): {
                "cells": { "B1": { "v": 0.21 } },
                "frozen_rows": 0, "frozen_cols": 0
            }
        }
    });
    let metas = vec![
        SheetExportMeta { id: s1, name: "Feuille 1".into() },
        SheetExportMeta { id: s2, name: "Data".into() },
    ];
    let bytes = export_xlsx("Classeur test", &content, &metas).expect("export");
    // Optional dump for external validation (LibreOffice / Excel).
    if let Ok(path) = std::env::var("XLSX_DUMP_OUT") {
        std::fs::write(&path, &bytes).expect("dump");
    }
    let wb = import_xlsx(&bytes).expect("re-import");

    assert_eq!(wb.sheets.len(), 2);
    assert_eq!(wb.sheets[0].name, "Feuille 1");
    assert_eq!(wb.sheets[1].name, "Data");

    // Values.
    assert_eq!(cell(&wb, 0, "A1")["v"], json!("Titre"));
    assert_eq!(cell(&wb, 0, "A2")["v"], json!(12.5));
    assert_eq!(cell(&wb, 0, "B2")["v"], json!(true));
    assert_eq!(cell(&wb, 0, "C2")["v"], json!("#DIV/0!"));
    assert_eq!(cell(&wb, 0, "A5")["v"], json!("texte  avec espaces "));
    assert_eq!(cell(&wb, 1, "B1")["v"], json!(0.21));

    // Formulas: ';' normalised to ',', '_xlfn.' added at export + stripped at import.
    assert_eq!(cell(&wb, 0, "A3")["f"], json!("=SUM(A2,B2)"));
    assert_eq!(cell(&wb, 0, "A3")["v"], json!(13.5));
    assert_eq!(cell(&wb, 0, "A4")["f"], json!("=XLOOKUP(A2,A2:A3,A2:A3)"));

    // Styles.
    let s = &cell(&wb, 0, "A1")["s"];
    assert_eq!(s["bold"], json!(true));
    assert_eq!(s["color"], json!("#FF0000"));
    assert_eq!(s["bg"], json!("#FFFF00"));
    assert_eq!(s["align"], json!("center"));
    assert_eq!(s["fontSize"], json!(16.0)); // 16px → 12pt → 16px
    assert_eq!(cell(&wb, 0, "B5")["s"]["numFmtCode"], json!("0.00%"));
    let borders = &cell(&wb, 0, "C5")["s"];
    assert_eq!(borders["bt"], json!("#000000"));
    assert_eq!(borders["btw"], json!(2));
    assert_eq!(borders["bb"], json!("#00FF00"));
    // Style-only cell survives.
    assert_eq!(cell(&wb, 0, "D5")["s"]["bg"], json!("#00AAFF"));

    // Merges, sizes, gridlines.
    assert_eq!(wb.sheets[0].merges, vec!["A1:C1".to_string()]);
    assert_eq!(wb.sheets[0].col_widths.get("A"), Some(&152.0));
    assert_eq!(wb.sheets[0].col_widths.get("B"), Some(&0.0)); // hidden
    assert_eq!(wb.sheets[0].row_heights.get(&2), Some(&40.0));
    assert_eq!(wb.sheets[0].row_heights.get(&3), Some(&0.0)); // hidden
    assert!(!wb.sheets[0].show_gridlines);
    assert!(wb.sheets[1].show_gridlines);

    // Defined names.
    assert_eq!(wb.defined_names, vec![("TAUX".to_string(), "=Data!$B$1".to_string())]);
}

#[test]
fn roundtrip_sheet_and_workbook_features() {
    let s1 = Uuid::new_v4();
    let s2 = Uuid::new_v4();
    let content = json!({
        "version": 1,
        "sheets": {
            s1.to_string(): {
                "cells": {
                    "A1": { "v": "Site", "link": "https://kubuno.com/docs?a=1&b=2" },
                    "A2": { "v": "Interne", "link": "#'Feuille 2'!B4" },
                    "B1": { "v": 4.0 },
                },
                "frozen_rows": 2,
                "frozen_cols": 1,
                "auto_filter": "A1:C9",
                "tab_color": "#FF8800",
                "protection": { "algorithmName": "SHA-512", "hashValue": "aGFzaA==", "saltValue": "c2VsbA==", "spinCount": 100000 },
                "validations": [
                    { "ranges": ["C1:C5"], "rule": { "crit": { "kind": "list", "values": ["Oui", "Non"] }, "reject": true } },
                    { "ranges": ["D1:D5"], "rule": { "crit": { "kind": "listRange", "source": "A1:A9" }, "reject": false, "help": "Choisir une valeur" } },
                    { "ranges": ["E1:E5"], "rule": { "crit": { "kind": "number", "op": "between", "v1": 1.5, "v2": 9.0 }, "reject": true } },
                    { "ranges": ["F1:F5"], "rule": { "crit": { "kind": "textLen", "op": "le", "v1": 10.0 }, "reject": true } }
                ],
                "row_groups": [ { "start": 2, "end": 5, "collapsed": false }, { "start": 3, "end": 4, "collapsed": true } ],
                "col_groups": [ { "start": 1, "end": 3, "collapsed": false } ],
                "print_area": "$A$1:$F$20",
                "print_titles": "$1:$1"
            },
            s2.to_string(): { "cells": { "B4": { "v": 1.0 } }, "hidden": true }
        }
    });
    let metas = vec![
        SheetExportMeta { id: s1, name: "Feuille 1".into() },
        SheetExportMeta { id: s2, name: "Feuille 2".into() },
    ];
    let bytes = export_xlsx("features", &content, &metas).expect("export");
    if let Ok(path) = std::env::var("XLSX_DUMP_OUT2") { std::fs::write(&path, &bytes).expect("dump"); }
    let wb = import_xlsx(&bytes).expect("re-import");

    let s = &wb.sheets[0];
    // Frozen panes.
    assert_eq!((s.frozen_rows, s.frozen_cols), (2, 1));
    // Hidden sheet + tab colour.
    assert!(!s.hidden);
    assert!(wb.sheets[1].hidden);
    assert_eq!(s.tab_color.as_deref(), Some("#FF8800"));
    // Hyperlinks (external via rels, internal via location).
    assert_eq!(cell(&wb, 0, "A1")["link"], json!("https://kubuno.com/docs?a=1&b=2"));
    assert_eq!(cell(&wb, 0, "A2")["link"], json!("#'Feuille 2'!B4"));
    // AutoFilter range.
    assert_eq!(s.auto_filter.as_deref(), Some("A1:C9"));
    // Protection hash round-trips verbatim.
    let p = s.protection.as_ref().expect("protection");
    assert_eq!(p["algorithmName"], json!("SHA-512"));
    assert_eq!(p["hashValue"], json!("aGFzaA=="));
    assert_eq!(p["saltValue"], json!("c2VsbA=="));
    assert_eq!(p["spinCount"], json!(100000));
    // Validations.
    assert_eq!(s.validations.len(), 4);
    let v0 = &s.validations[0];
    assert_eq!(v0["ranges"], json!(["C1:C5"]));
    assert_eq!(v0["rule"]["crit"]["kind"], json!("list"));
    assert_eq!(v0["rule"]["crit"]["values"], json!(["Oui", "Non"]));
    assert_eq!(v0["rule"]["reject"], json!(true));
    let v1 = &s.validations[1];
    assert_eq!(v1["rule"]["crit"]["kind"], json!("listRange"));
    assert_eq!(v1["rule"]["crit"]["source"], json!("A1:A9"));
    assert_eq!(v1["rule"]["reject"], json!(false));
    assert_eq!(v1["rule"]["help"], json!("Choisir une valeur"));
    let v2 = &s.validations[2];
    assert_eq!(v2["rule"]["crit"], json!({ "kind": "number", "op": "between", "v1": 1.5, "v2": 9.0 }));
    let v3 = &s.validations[3];
    assert_eq!(v3["rule"]["crit"], json!({ "kind": "textLen", "op": "le", "v1": 10.0 }));
    // Outline groups (incl. nesting + collapsed inner group).
    assert_eq!(s.row_groups, vec![
        json!({ "start": 2, "end": 5, "collapsed": false }),
        json!({ "start": 3, "end": 4, "collapsed": true }),
    ]);
    assert_eq!(s.col_groups, vec![json!({ "start": 1, "end": 3, "collapsed": false })]);
    // Print ranges (sheet-scoped defined names).
    assert_eq!(s.print_area.as_deref(), Some("$A$1:$F$20"));
    assert_eq!(s.print_titles.as_deref(), Some("$1:$1"));
}

#[test]
fn export_rejects_empty_sheet_list() {
    assert!(export_xlsx("x", &json!({ "sheets": {} }), &[]).is_err());
}

#[test]
fn sheet_names_are_sanitized_and_unique() {
    let ids: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();
    let content = json!({ "sheets": {} });
    let metas = vec![
        SheetExportMeta { id: ids[0], name: "Bad/Name:With*Chars?".into() },
        SheetExportMeta { id: ids[1], name: "Bad Name With Chars".into() },
        SheetExportMeta { id: ids[2], name: "".into() },
    ];
    let bytes = export_xlsx("t", &content, &metas).expect("export");
    let wb = import_xlsx(&bytes).expect("import");
    assert_eq!(wb.sheets[0].name, "Bad Name With Chars");
    assert_eq!(wb.sheets[1].name, "Bad Name With Chars (2)");
    assert_eq!(wb.sheets[2].name, "Feuille 3");
}

#[test]
fn prepare_formula_keeps_array_rows_and_strings() {
    use super::write::worksheet::prepare_formula;
    assert_eq!(prepare_formula("SUM(A1;B1)"), "SUM(A1,B1)");
    assert_eq!(prepare_formula("SUM({1,2;3,4})"), "SUM({1,2;3,4})");
    assert_eq!(prepare_formula("IF(A1=\"a;b\";1;2)"), "IF(A1=\"a;b\",1,2)");
    assert_eq!(prepare_formula("SORT(A1:A9)"), "_xlfn._xlws.SORT(A1:A9)");
    assert_eq!(prepare_formula("CEILING.MATH(A1)"), "_xlfn.CEILING.MATH(A1)");
    // Cell refs and unknown names are untouched.
    assert_eq!(prepare_formula("A1+CONCATZ(B2)"), "A1+CONCATZ(B2)");
}

#[test]
fn strip_xlfn_removes_prefixes_outside_strings() {
    assert_eq!(util::strip_xlfn("_xlfn.XLOOKUP(A1,B:B,C:C)"), "XLOOKUP(A1,B:B,C:C)");
    assert_eq!(util::strip_xlfn("_xlfn._xlws.SORT(A1:A9)"), "SORT(A1:A9)");
    assert_eq!(util::strip_xlfn("CONCAT(\"_xlfn.X\",A1)"), "CONCAT(\"_xlfn.X\",A1)");
}

// The LibreOffice corpus lives outside the repo — skip silently when absent so
// the suite still passes on machines without the checkout.
const LO_CORPUS: &str = "/home/martinien/libreoffice/core-master/sc/qa/unit/data/xlsx";

#[test]
fn corpus_universal_content_still_imports() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/universal-content.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import corpus");
    assert!(!wb.sheets.is_empty());
    // Numeric, string and formula content are present as before the module split.
    assert_eq!(cell(&wb, 0, "A1")["v"], json!(1.0));
    let has_strings = wb.sheets.iter().any(|s| s.cells.values().any(|c| c["v"].is_string()));
    assert!(has_strings, "expected some text cells in universal-content.xlsx");
}

#[test]
fn snapshot_corpus_files() {
    // Deterministic dump of a handful of corpus imports — diffed against the
    // pre-refactor snapshot during development (scratchpad snapshot_before.json).
    let files = ["universal-content.xlsx", "cell-borders.xlsx", "cell-value.xlsx",
        "condFormat_cellis.xlsx", "colorscale.xlsx", "bug-fixes.xlsx",
        "shared-formula/basic.xlsx", "tdf151755_stylesLostOnXLSXExport.xlsx"];
    let mut all = serde_json::Map::new();
    for fname in files {
        let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/{fname}")) else { continue };
        let Ok(wb) = import_xlsx(&bytes) else { continue };
        let mut out = Vec::new();
        for s in &wb.sheets {
            let cells: std::collections::BTreeMap<_, _> = s.cells.iter().collect();
            let cols: std::collections::BTreeMap<_, _> = s.col_widths.iter().collect();
            let rows: std::collections::BTreeMap<_, _> = s.row_heights.iter().collect();
            out.push(json!({
                "name": s.name, "cells": cells, "merges": s.merges,
                "col_widths": cols, "row_heights": rows,
                "cf": s.cond_formats, "gridlines": s.show_gridlines,
                "drh": s.default_row_height, "dcw": s.default_col_width,
                "images": s.images.len(), "charts": s.charts,
            }));
        }
        all.insert(fname.to_string(), json!({ "sheets": out, "names": wb.defined_names }));
    }
    if all.is_empty() { return } // corpus absent on this machine
    let path = std::env::var("XLSX_SNAPSHOT_OUT")
        .unwrap_or_else(|_| "/dev/null".to_string());
    if path != "/dev/null" {
        std::fs::write(&path, serde_json::to_string_pretty(&Value::Object(all)).unwrap_or_default())
            .expect("write snapshot");
    }
}

// ── Styles & strings domain tests ────────────────────────────────────────────

#[test]
fn roundtrip_rich_styles() {
    let s1 = Uuid::new_v4();
    let content = json!({
        "version": 1,
        "sheets": {
            s1.to_string(): {
                "cells": {
                    // Accounting builtin (id 44) must round-trip without a custom numFmt.
                    "A1": { "v": 1234.5, "s": { "numFmtCode": "_($* #,##0.00_);_($* (#,##0.00);_($* \"-\"??_);_(@_)" } },
                    // Gradient background → real gradientFill, re-imported as its first stop colour.
                    "B1": { "v": 2.0, "s": { "bgGradient": { "type": "linear", "angle": 90,
                        "stops": [ { "color": "#FF0000", "position": 0, "opacity": 100 },
                                   { "color": "#0000FF", "position": 1, "opacity": 100 } ] } } },
                    "C1": { "v": "x", "s": { "strike": true, "underline": true, "italic": true } },
                    "D1": { "v": 7.0, "s": { "numFmtCode": "h:mm AM/PM" } }
                },
                "frozen_rows": 0, "frozen_cols": 0
            }
        }
    });
    let metas = vec![SheetExportMeta { id: s1, name: "S".into() }];
    let bytes = export_xlsx("styles", &content, &metas).expect("export");
    let wb = import_xlsx(&bytes).expect("re-import");

    assert_eq!(cell(&wb, 0, "A1")["s"]["numFmtCode"],
        json!("_($* #,##0.00_);_($* (#,##0.00);_($* \"-\"??_);_(@_)"));
    assert_eq!(cell(&wb, 0, "B1")["s"]["bg"], json!("#FF0000"));
    let c1 = &cell(&wb, 0, "C1")["s"];
    assert_eq!(c1["strike"], json!(true));
    assert_eq!(c1["underline"], json!(true));
    assert_eq!(c1["italic"], json!(true));
    // "h:mm AM/PM" is builtin id 18 → no custom numFmt needed, code survives.
    assert_eq!(cell(&wb, 0, "D1")["s"]["numFmtCode"], json!("h:mm AM/PM"));
}

#[test]
fn shared_strings_rich_runs_and_phonetic_excluded() {
    use super::read::strings::parse_shared_strings;
    let xml = r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="3" uniqueCount="3">
      <si><t>plain</t></si>
      <si><r><t xml:space="preserve">Hello </t></r><r><rPr><b/><color rgb="FFFF0000"/></rPr><t>World</t></r></si>
      <si><r><t>&#28450;&#23383;</t></r><rPh sb="0" eb="2"><t>&#12363;&#12435;&#12376;</t></rPh><phoneticPr fontId="1" type="Hiragana"/></si>
    </sst>"#;
    let v = parse_shared_strings(xml);
    assert_eq!(v, vec!["plain".to_string(), "Hello World".into(), "漢字".into()]);
}

#[test]
fn builtin_numfmt_table_and_locale_reuse() {
    let custom = std::collections::HashMap::new();
    assert_eq!(util::numfmt_code(5, &custom).as_deref(), Some("$#,##0_);($#,##0)"));
    assert_eq!(util::numfmt_code(18, &custom).as_deref(), Some("h:mm AM/PM"));
    assert_eq!(util::numfmt_code(44, &custom).as_deref(),
        Some("_($* #,##0.00_);_($* (#,##0.00);_($* \"-\"??_);_(@_)"));
    // Locale variants reuse the base builtin's code.
    assert_eq!(util::numfmt_code(30, &custom), util::numfmt_code(14, &custom));
    assert_eq!(util::numfmt_code(63, &custom), util::numfmt_code(5, &custom));
    assert_eq!(util::numfmt_code(76, &custom), util::numfmt_code(20, &custom));
    assert_eq!(util::numfmt_code(0, &custom), None);
    assert_eq!(util::numfmt_code(23, &custom), None); // reuse of General
    // An explicit <numFmt> entry may redefine even a builtin id.
    let mut over = std::collections::HashMap::new();
    over.insert(14u32, "yyyy".to_string());
    assert_eq!(util::numfmt_code(14, &over).as_deref(), Some("yyyy"));
    // Reverse lookup used by the writer.
    assert_eq!(util::builtin_numfmt_id("0.00%"), Some(10));
    assert_eq!(util::builtin_numfmt_id("h:mm"), Some(20));
    assert_eq!(util::builtin_numfmt_id("dd/mm/yyyy"), Some(14));
}

#[test]
fn theme_clrscheme_parsed_with_lt_dk_swap() {
    let xml = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:clrScheme name="T">
      <a:dk1><a:sysClr val="windowText" lastClr="111111"/></a:dk1>
      <a:lt1><a:sysClr val="window" lastClr="EEEEEE"/></a:lt1>
      <a:dk2><a:srgbClr val="222222"/></a:dk2>
      <a:lt2><a:srgbClr val="DDDDDD"/></a:lt2>
      <a:accent1><a:srgbClr val="4472C4"/></a:accent1>
      <a:accent2><a:srgbClr val="ED7D31"/></a:accent2>
      <a:accent3><a:srgbClr val="A5A5A5"/></a:accent3>
      <a:accent4><a:srgbClr val="FFC000"/></a:accent4>
      <a:accent5><a:srgbClr val="5B9BD5"/></a:accent5>
      <a:accent6><a:srgbClr val="70AD47"/></a:accent6>
      <a:hlink><a:srgbClr val="0563C1"/></a:hlink>
      <a:folHlink><a:srgbClr val="954F72"/></a:folHlink>
    </a:clrScheme></a:themeElements></a:theme>"#;
    let t = util::parse_theme_colors(xml).expect("theme");
    // Slot order = the SpreadsheetML `theme` attribute: 0=lt1 1=dk1 2=lt2 3=dk2.
    assert_eq!(t[0], "EEEEEE");
    assert_eq!(t[1], "111111");
    assert_eq!(t[2], "DDDDDD");
    assert_eq!(t[3], "222222");
    assert_eq!(t[4], "4472C4");
    assert_eq!(t[9], "70AD47");
    assert_eq!(util::parse_theme_colors("<a:theme/>"), None);
}

#[test]
fn styles_indexed_pattern_gradient_and_val_flags() {
    use super::read::styles::{parse_styles, style_for};
    let xml = r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
      <fonts count="3">
        <font><sz val="11"/><color indexed="10"/><name val="Arial"/></font>
        <font><b val="0"/><u val="none"/><sz val="11"/><name val="Arial"/></font>
        <font><sz val="11"/><color indexed="0"/><name val="Arial"/></font>
      </fonts>
      <fills count="4">
        <fill><patternFill patternType="none"/></fill>
        <fill><patternFill patternType="gray125"/></fill>
        <fill><patternFill patternType="darkGray"><fgColor indexed="13"/><bgColor indexed="64"/></patternFill></fill>
        <fill><gradientFill degree="90"><stop position="0"><color rgb="FF00AAFF"/></stop><stop position="1"><color rgb="FFFFFFFF"/></stop></gradientFill></fill>
      </fills>
      <borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>
      <cellXfs count="3">
        <xf numFmtId="0" fontId="0" fillId="0" borderId="0"/>
        <xf numFmtId="0" fontId="1" fillId="2" borderId="0" applyFill="1"/>
        <xf numFmtId="0" fontId="2" fillId="3" borderId="0" applyFill="1"/>
      </cellXfs>
      <colors><indexedColors>
        <rgbColor rgb="00123456"/>
      </indexedColors></colors>
    </styleSheet>"#;
    let st = parse_styles(xml, None);
    // indexed 10 = legacy palette red.
    let s0 = style_for(&st, 0).expect("style 0");
    assert_eq!(s0["color"], json!("#FF0000"));
    // <b val="0"/> and <u val="none"/> are NOT bold/underlined; darkGray pattern
    // approximated by its fgColor (indexed 13 = yellow).
    let s1 = style_for(&st, 1).expect("style 1");
    assert!(s1.get("bold").is_none());
    assert!(s1.get("underline").is_none());
    assert_eq!(s1["bg"], json!("#FFFF00"));
    // Gradient fill approximated by its first stop; indexedColors override
    // replaces palette slot 0 (000000 → 123456).
    let s2 = style_for(&st, 2).expect("style 2");
    assert_eq!(s2["bg"], json!("#00AAFF"));
    assert_eq!(s2["color"], json!("#123456"));
}

#[test]
fn corpus_theme_colors_resolved_from_theme1() {
    // "Blue II" theme: accent5=3E8853 accent6=62A39F accent3=27CED7 — far from
    // the hard-coded Office 2007 palette this file used to be decoded with.
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/Test_ThemeColor_Text_Background_Border.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import");
    // C1 border top: thick, color theme="8" (slot 8 = accent5).
    let c1 = &cell(&wb, 0, "C1")["s"];
    assert_eq!(c1["bt"], json!(util::apply_tint("3E8853", 0.0)));
    assert_eq!(c1["btw"], json!(3));
    // B1 font: color theme="9" (slot 9 = accent6), no tint.
    assert_eq!(cell(&wb, 0, "B1")["s"]["color"], json!(util::apply_tint("62A39F", 0.0)));
    // A2 fill: fgColor theme="6" (accent3) with a strong lightening tint.
    assert_eq!(cell(&wb, 0, "A2")["s"]["bg"],
        json!(util::apply_tint("27CED7", 0.799_981_688_894_314_4)));
}

#[test]
fn corpus_formats_rich_strings_flattened() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/formats.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import");
    let all_values: Vec<String> = wb.sheets.iter()
        .flat_map(|s| s.cells.values())
        .filter_map(|c| c["v"].as_str().map(|v| v.to_string()))
        .collect();
    // Rich <r> runs are concatenated ("text14" + " space").
    assert!(all_values.iter().any(|v| v == "text14 space"), "rich runs flattened: {all_values:?}");
    // Embedded line breaks survive.
    assert!(all_values.iter().any(|v| v == "Hello,\nCalc!"));
}

#[test]
fn corpus_calcthemetest_imports_with_theme() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/CalcThemeTest.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import");
    assert!(!wb.sheets.is_empty());
    // At least one styled cell resolved a colour (theme or indexed).
    let has_colored = wb.sheets.iter().flat_map(|s| s.cells.values())
        .any(|c| c["s"]["color"].is_string() || c["s"]["bg"].is_string());
    assert!(has_colored);
}

// ── Sheet & workbook domain: corpus checks ───────────────────────────────────

#[test]
fn corpus_freeze_pane_imports() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/freezePaneStartCell.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import");
    assert_eq!(wb.sheets[0].frozen_cols, 5);
    assert_eq!(wb.sheets[0].frozen_rows, 10);
}

#[test]
fn corpus_hidden_sheets_import() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/hidden_sheets.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import");
    assert_eq!(wb.sheets.len(), 3);
    assert!(wb.sheets[0].hidden);
    assert!(!wb.sheets[1].hidden);
    assert!(wb.sheets[2].hidden);
}

#[test]
// 3.14 is the bound stored in the fixture, not an approximation of PI.
#[allow(clippy::approx_constant)]
fn corpus_data_validity_imports() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/dataValidity.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import");
    // sheet1: decimal greaterThan 3.14 on C3:C7 with errorStyle="warning".
    let v = wb.sheets.iter().flat_map(|s| &s.validations)
        .find(|v| v["ranges"] == json!(["C3:C7"])).expect("validation C3:C7");
    assert_eq!(v["rule"]["crit"], json!({ "kind": "number", "op": "gt", "v1": 3.14 }));
    assert_eq!(v["rule"]["reject"], json!(false));
}

#[test]
fn corpus_text_length_validity_imports() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/textLengthDataValidity.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import");
    let has_textlen = wb.sheets.iter().flat_map(|s| &s.validations)
        .any(|v| v["rule"]["crit"]["kind"] == json!("textLen"));
    assert!(has_textlen, "expected a textLen validation");
}

#[test]
fn corpus_autofilter_imports() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/autofilter.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import");
    assert_eq!(wb.sheets[0].auto_filter.as_deref(), Some("A1:C5"));
}

#[test]
fn corpus_hyperlinks_import() {
    // hyperlinks.xlsx: internal `location` links; hyperlink_export.xlsx: external rels link.
    if let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/hyperlinks.xlsx")) {
        let wb = import_xlsx(&bytes).expect("import");
        let a2 = wb.sheets.iter().find_map(|s| s.cells.get("A2")).expect("A2");
        assert_eq!(a2["link"], json!("#Offering!$B$2"));
    }
    if let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/hyperlink_export.xlsx")) {
        let wb = import_xlsx(&bytes).expect("import");
        let a1 = &wb.sheets[0].cells.get("A1").expect("A1");
        assert_eq!(a1["link"], json!("test.xlsx"));
    }
}

#[test]
fn corpus_legacy_protection_is_dropped_not_fatal() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/ProtecteSheet1234Pass.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import");
    // Legacy 16-bit `password` hash is not representable in the model.
    assert!(wb.sheets[0].protection.is_none());
}

// ── Drawings & charts domain tests ───────────────────────────────────────────

// 1×1 red PNG.
const PNG_1PX: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

#[test]
fn roundtrip_image_anchors_and_bytes() {
    let s1 = Uuid::new_v4();
    let src = format!("data:image/png;base64,{PNG_1PX}");
    let content = json!({
        "version": 1,
        "sheets": {
            s1.to_string(): {
                "cells": { "A1": { "v": 1.0 } },
                "col_widths": { "A": 100.0 },
                "frozen_rows": 0, "frozen_cols": 0,
                "images": [
                    // Imported-style twoCellAnchor with rotation + crop.
                    { "fromCol": 1, "fromColOff": 9525, "fromRow": 2, "fromRowOff": 0,
                      "toCol": 4, "toColOff": 19050, "toRow": 8, "toRowOff": 9525,
                      "rot": 15.0, "cropL": 0.1, "src": src },
                    // Imported-style oneCellAnchor with an explicit size.
                    { "fromCol": 0, "fromColOff": 0, "fromRow": 0, "fromRowOff": 4762,
                      "extCx": 952500, "extCy": 476250, "src": src },
                    // UI-manipulated image: pixel box, re-anchored on export.
                    // bx=150 with default col width 100 → col 1 + 50px; by=30 with
                    // default row height 24 → row 1 + 6px.
                    { "fromCol": 9, "fromRow": 9, "bx": 150.0, "by": 30.0, "bw": 200.0, "bh": 100.0, "src": src }
                ]
            }
        }
    });
    let metas = vec![SheetExportMeta { id: s1, name: "Feuille 1".into() }];
    let bytes = export_xlsx("images", &content, &metas).expect("export");
    if let Ok(path) = std::env::var("XLSX_DUMP_IMAGES") { std::fs::write(&path, &bytes).expect("dump"); }
    let wb = import_xlsx(&bytes).expect("re-import");

    let imgs = &wb.sheets[0].images;
    assert_eq!(imgs.len(), 3);
    // twoCellAnchor: anchors round-trip verbatim, no spurious extCx.
    assert_eq!(imgs[0], json!({
        "fromCol": 1, "fromColOff": 9525, "fromRow": 2, "fromRowOff": 0,
        "toCol": 4, "toColOff": 19050, "toRow": 8, "toRowOff": 9525,
        "rot": 15.0, "cropL": 0.1, "src": src
    }));
    // oneCellAnchor: from + ext round-trip.
    assert_eq!(imgs[1], json!({
        "fromCol": 0, "fromColOff": 0, "fromRow": 0, "fromRowOff": 4762,
        "extCx": 952500, "extCy": 476250, "src": src
    }));
    // Pixel box → computed cell anchor (px × 9525 EMU).
    assert_eq!(imgs[2], json!({
        "fromCol": 1, "fromColOff": 50 * 9525, "fromRow": 1, "fromRowOff": 6 * 9525,
        "extCx": 200 * 9525, "extCy": 100 * 9525, "src": src
    }));
}

#[test]
fn roundtrip_charts_multiseries_and_range() {
    let s1 = Uuid::new_v4();
    let content = json!({
        "version": 1,
        "sheets": {
            s1.to_string(): {
                "cells": {
                    "A1": { "v": "Cat" }, "B1": { "v": "Montant" },
                    "A2": { "v": "X" },   "B2": { "v": 4.0 },
                    "A3": { "v": "Y" },   "B3": { "v": 6.0 },
                    "C2": { "v": 1.0 },   "C3": { "v": 2.0 }
                },
                "frozen_rows": 0, "frozen_cols": 0,
                "charts": [
                    // Imported-style multi-series stacked bar with title + anchor.
                    { "type": "bar", "title": "Ventes", "grouping": "stacked",
                      "legend": true, "dataLabels": "value",
                      "fromCol": 5, "fromColOff": 0, "fromRow": 1, "fromRowOff": 9525,
                      "toCol": 10, "toColOff": 4762, "toRow": 12, "toRowOff": 0,
                      "vals": ["B2", "B3", "C2", "C3"], "cats": ["A2", "A3", "A2", "A3"],
                      "series": [
                          { "name": "2015", "cats": ["A2", "A3"], "vals": ["B2", "B3"] },
                          { "name": "2016", "cats": ["A2", "A3"], "vals": ["C2", "C3"] }
                      ] },
                    // UI-created pie chart from a range with a header row.
                    { "type": "pie", "bx": 50.0, "by": 20.0, "bw": 380.0, "bh": 240.0,
                      "range": "A1:B3", "colors": ["#FF0000", "#00FF00"] }
                ]
            }
        }
    });
    let metas = vec![SheetExportMeta { id: s1, name: "Feuille 1".into() }];
    let bytes = export_xlsx("charts", &content, &metas).expect("export");
    if let Ok(path) = std::env::var("XLSX_DUMP_CHARTS") { std::fs::write(&path, &bytes).expect("dump"); }
    let wb = import_xlsx(&bytes).expect("re-import");

    let charts = &wb.sheets[0].charts;
    assert_eq!(charts.len(), 2);

    let c0 = &charts[0];
    assert_eq!(c0["type"], json!("bar"));
    assert_eq!(c0["title"], json!("Ventes"));
    assert_eq!(c0["grouping"], json!("stacked"));
    assert_eq!(c0["legend"], json!(true));
    assert_eq!(c0["dataLabels"], json!("value"));
    // Anchor round-trips verbatim (and the graphicFrame's mandatory 0×0 a:ext
    // must not leak into extCx/extCy).
    assert_eq!(c0["fromCol"], json!(5));
    assert_eq!(c0["fromColOff"], json!(0));
    assert_eq!(c0["fromRow"], json!(1));
    assert_eq!(c0["fromRowOff"], json!(9525));
    assert_eq!(c0["toCol"], json!(10));
    assert_eq!(c0["toColOff"], json!(4762));
    assert_eq!(c0["toRow"], json!(12));
    assert_eq!(c0["toRowOff"], json!(0));
    assert!(c0.get("extCx").is_none());
    // Flat refs (concatenation of the series) + per-series structure.
    assert_eq!(c0["vals"], json!(["B2", "B3", "C2", "C3"]));
    assert_eq!(c0["cats"], json!(["A2", "A3", "A2", "A3"]));
    assert_eq!(c0["series"], json!([
        { "name": "2015", "cats": ["A2", "A3"], "vals": ["B2", "B3"] },
        { "name": "2016", "cats": ["A2", "A3"], "vals": ["C2", "C3"] }
    ]));

    let c1 = &charts[1];
    assert_eq!(c1["type"], json!("pie"));
    // Range "A1:B3" with header → series named "Montant", cats A2:A3, vals B2:B3.
    assert_eq!(c1["vals"], json!(["B2", "B3"]));
    assert_eq!(c1["cats"], json!(["A2", "A3"]));
    assert_eq!(c1["series"], json!([{ "name": "Montant", "cats": ["A2", "A3"], "vals": ["B2", "B3"] }]));
    assert_eq!(c1["legend"], json!(true)); // pie default
    assert_eq!(c1["colors"], json!(["#FF0000", "#00FF00"]));
    // Pixel box → oneCellAnchor (bx=50/by=20 land inside col 0 / row 0).
    assert_eq!(c1["fromCol"], json!(0));
    assert_eq!(c1["fromColOff"], json!(50 * 9525));
    assert_eq!(c1["fromRow"], json!(0));
    assert_eq!(c1["fromRowOff"], json!(20 * 9525));
    assert_eq!(c1["extCx"], json!(380 * 9525));
    assert_eq!(c1["extCy"], json!(240 * 9525));
}

#[test]
fn roundtrip_charts_v2_types_and_options() {
    let s1 = Uuid::new_v4();
    let content = json!({
        "version": 1,
        "sheets": {
            s1.to_string(): {
                "cells": {
                    "A1": { "v": "Mois" }, "B1": { "v": "CA" },   "C1": { "v": "Coût" },
                    "A2": { "v": "Jan" },  "B2": { "v": 10.0 },   "C2": { "v": 4.0 },
                    "A3": { "v": "Fév" },  "B3": { "v": 20.0 },   "C3": { "v": 5.0 },
                    "A4": { "v": "Mar" },  "B4": { "v": 30.0 },   "C4": { "v": 6.0 },
                    "A6": { "v": "R1" },   "B6": { "v": 4.0 },    "C6": { "v": 5.0 },
                    "A7": { "v": "R2" },   "B7": { "v": 6.0 },    "C7": { "v": 7.0 }
                },
                "frozen_rows": 0, "frozen_cols": 0,
                "charts": [
                    // Donut: hole size, exploded slices, positioned legend, category labels.
                    { "type": "donut", "bx": 0.0, "by": 0.0, "bw": 300.0, "bh": 200.0,
                      "holeSize": 0.5, "explode": 0.25, "legend": true, "legendPos": "bottom",
                      "dataLabels": "category",
                      "series": [{ "name": "S", "cats": ["A2", "A3", "A4"], "vals": ["B2", "B3", "B4"] }] },
                    // Filled radar.
                    { "type": "radar", "filled": true, "legend": false,
                      "vals": ["B2", "B3", "B4"], "cats": ["A2", "A3", "A4"] },
                    // Line: smoothing, markers, both gridlines, axis titles + manual Y bounds,
                    // series colour.
                    { "type": "line", "smooth": true, "symbols": true,
                      "grid": { "x": true, "y": true },
                      "axisX": { "title": "Mois" }, "axisY": { "title": "CA", "min": 0.0, "max": 100.0 },
                      "series": [{ "name": "CA", "cats": ["A2", "A3", "A4"], "vals": ["B2", "B3", "B4"],
                                   "color": "#112233" }] },
                    // Scatter, points only (no connecting lines).
                    { "type": "scatter", "lines": false, "symbols": true,
                      "series": [{ "name": "Pts", "cats": ["B2", "B3", "B4"], "vals": ["C2", "C3", "C4"] }] },
                    // Combo: explicit per-series kind + line colour.
                    { "type": "combo",
                      "series": [
                          { "name": "2015", "cats": ["A2", "A3", "A4"], "vals": ["B2", "B3", "B4"] },
                          { "name": "2016", "cats": ["A2", "A3", "A4"], "vals": ["C2", "C3", "C4"],
                            "kind": "line", "color": "#FF0000" }
                      ] },
                    // v2 range in columns: header row = live series names, first col = categories.
                    { "type": "bar", "range": "A1:C4", "seriesIn": "cols",
                      "firstRowHeader": true, "firstColHeader": true },
                    // v2 range in rows: first col = series names, no category row.
                    { "type": "line", "range": "A6:C7", "seriesIn": "rows",
                      "firstRowHeader": false, "firstColHeader": true }
                ]
            }
        }
    });
    let metas = vec![SheetExportMeta { id: s1, name: "Feuille 1".into() }];
    let bytes = export_xlsx("charts v2", &content, &metas).expect("export");
    if let Ok(path) = std::env::var("XLSX_DUMP_CHARTS_V2") { std::fs::write(&path, &bytes).expect("dump"); }
    let wb = import_xlsx(&bytes).expect("re-import");

    let charts = &wb.sheets[0].charts;
    assert_eq!(charts.len(), 7);

    let donut = &charts[0];
    assert_eq!(donut["type"], json!("donut"));
    assert_eq!(donut["holeSize"], json!(0.5));
    assert_eq!(donut["explode"], json!(0.25));
    assert_eq!(donut["legend"], json!(true));
    assert_eq!(donut["legendPos"], json!("bottom"));
    assert_eq!(donut["dataLabels"], json!("category"));
    assert_eq!(donut["series"], json!([{ "name": "S", "cats": ["A2", "A3", "A4"], "vals": ["B2", "B3", "B4"] }]));

    let radar = &charts[1];
    assert_eq!(radar["type"], json!("radar"));
    assert_eq!(radar["filled"], json!(true));
    assert_eq!(radar["legend"], json!(false));
    assert_eq!(radar["vals"], json!(["B2", "B3", "B4"]));

    let line = &charts[2];
    assert_eq!(line["type"], json!("line"));
    assert_eq!(line["smooth"], json!(true));
    assert_eq!(line["symbols"], json!(true));
    assert_eq!(line["grid"], json!({ "x": true, "y": true }));
    assert_eq!(line["axisX"], json!({ "title": "Mois" }));
    assert_eq!(line["axisY"], json!({ "title": "CA", "min": 0.0, "max": 100.0 }));
    assert_eq!(line["series"][0]["color"], json!("#112233"));
    assert_eq!(line["series"][0]["name"], json!("CA"));

    let scatter = &charts[3];
    assert_eq!(scatter["type"], json!("scatter"));
    assert_eq!(scatter["lines"], json!(false));
    assert_eq!(scatter["symbols"], json!(true));
    assert_eq!(scatter["series"][0]["cats"], json!(["B2", "B3", "B4"]));
    assert_eq!(scatter["series"][0]["vals"], json!(["C2", "C3", "C4"]));

    let combo = &charts[4];
    assert_eq!(combo["type"], json!("combo"));
    assert_eq!(combo["series"][0]["name"], json!("2015"));
    assert!(combo["series"][0].get("kind").is_none());
    assert_eq!(combo["series"][1]["name"], json!("2016"));
    assert_eq!(combo["series"][1]["kind"], json!("line"));
    assert_eq!(combo["series"][1]["color"], json!("#FF0000"));
    // Flat refs = concatenation of both plot elements' series.
    assert_eq!(combo["vals"], json!(["B2", "B3", "B4", "C2", "C3", "C4"]));

    // Column-wise range expansion: two named series sharing the category column.
    let cols = &charts[5];
    assert_eq!(cols["type"], json!("bar"));
    assert_eq!(cols["series"], json!([
        { "name": "CA",   "cats": ["A2", "A3", "A4"], "vals": ["B2", "B3", "B4"] },
        { "name": "Coût", "cats": ["A2", "A3", "A4"], "vals": ["C2", "C3", "C4"] }
    ]));

    // Row-wise range expansion: series names from the first column, no categories.
    let rows = &charts[6];
    assert_eq!(rows["type"], json!("line"));
    assert_eq!(rows["series"], json!([
        { "name": "R1", "vals": ["B6", "C6"] },
        { "name": "R2", "vals": ["B7", "C7"] }
    ]));
}

#[test]
fn corpus_chart_multiseries_names_import() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/forum-mso-de-104083.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import");
    let chart = wb.sheets.iter().flat_map(|s| &s.charts).next().expect("a chart");
    assert_eq!(chart["type"], json!("bar"));
    let series = chart["series"].as_array().expect("series");
    assert_eq!(series.len(), 3);
    let names: Vec<&str> = series.iter().filter_map(|s| s["name"].as_str()).collect();
    assert_eq!(names, vec!["2015", "2016", "2017"]);
    // Each series keeps its own value refs (B/C/D columns, rows 7..60).
    assert_eq!(series[0]["vals"][0], json!("B7"));
    assert_eq!(series[1]["vals"][0], json!("C7"));
    assert_eq!(series[2]["vals"][0], json!("D7"));
    // Flat refs = concatenation (3 series × 54 rows).
    assert_eq!(chart["vals"].as_array().map(|a| a.len()), Some(3 * 54));
    // clustered grouping is the default → not preserved as a key.
    assert!(chart.get("grouping").is_none());
}

#[test]
fn corpus_chart_line_series_names_import() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/shape-macro-ext-ref.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import");
    let chart = wb.sheets.iter().flat_map(|s| &s.charts).next().expect("a chart");
    assert_eq!(chart["type"], json!("line"));
    let names: Vec<&str> = chart["series"].as_array().expect("series")
        .iter().filter_map(|s| s["name"].as_str()).collect();
    assert_eq!(names, vec!["Einstecken", "Ausstecken"]);
}

#[test]
fn corpus_chart_files_import_without_panic() {
    for f in ["chart_hyperlink.xlsx", "orderOfCNumFmtElements.xlsx", "tdf107586.xlsx",
              "tdf134553.xlsx", "tdf165503.xlsx"] {
        let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/{f}")) else { continue };
        let wb = import_xlsx(&bytes).unwrap_or_else(|e| panic!("import {f}: {e}"));
        assert!(!wb.sheets.is_empty(), "{f}: no sheets");
    }
}

// ── Cell comments (notes) ────────────────────────────────────────────────────

#[test]
fn roundtrip_cell_comments() {
    let s1 = Uuid::new_v4();
    let content = json!({
        "version": 1,
        "sheets": {
            s1.to_string(): {
                "cells": {
                    "B2": { "v": 42.0, "c": "Vérifier ce chiffre\navec la compta" },
                    "D5": { "c": "Note sur cellule vide <&> \"quotes\"" },
                    "A1": { "v": "Titre" }
                }
            }
        }
    });
    let metas = vec![SheetExportMeta { id: s1, name: "Feuille 1".into() }];
    let bytes = export_xlsx("Notes", &content, &metas).expect("export");
    // Optional dump for external validation (LibreOffice / Excel).
    if let Ok(path) = std::env::var("XLSX_COMMENTS_DUMP_OUT") {
        std::fs::write(&path, &bytes).expect("dump");
    }

    // The generated package carries the comment part, its VML companion, the
    // sheet relationships and the content-type declarations.
    let cursor = std::io::Cursor::new(bytes.as_slice());
    let mut zip = zip::ZipArchive::new(cursor).expect("zip");
    let part = |zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>, name: &str| {
        util::read_zip_text(zip, name).unwrap_or_else(|| panic!("part {name} missing"))
    };
    let cm = part(&mut zip, "xl/comments1.xml");
    assert!(cm.contains("<comment ref=\"B2\""));
    assert!(cm.contains("Note sur cellule vide &lt;&amp;&gt; &quot;quotes&quot;"));
    let vml = part(&mut zip, "xl/drawings/vmlDrawing1.vml");
    assert!(vml.contains("ObjectType=\"Note\""));
    assert!(vml.contains("<x:Row>1</x:Row><x:Column>1</x:Column>")); // B2, 0-based
    let rels = part(&mut zip, "xl/worksheets/_rels/sheet1.xml.rels");
    assert!(rels.contains("/comments\" Target=\"../comments1.xml\""));
    assert!(rels.contains("/vmlDrawing\" Target=\"../drawings/vmlDrawing1.vml\""));
    let ct = part(&mut zip, "[Content_Types].xml");
    assert!(ct.contains("Extension=\"vml\""));
    assert!(ct.contains("/xl/comments1.xml"));
    let ws = part(&mut zip, "xl/worksheets/sheet1.xml");
    assert!(ws.contains("<legacyDrawing r:id=\"rIdVml1\"/>"));

    // Round-trip: notes survive, including on an otherwise empty cell.
    let wb = import_xlsx(&bytes).expect("re-import");
    assert_eq!(cell(&wb, 0, "B2")["v"], json!(42.0));
    assert_eq!(cell(&wb, 0, "B2")["c"], json!("Vérifier ce chiffre\navec la compta"));
    assert_eq!(cell(&wb, 0, "D5")["c"], json!("Note sur cellule vide <&> \"quotes\""));
    assert!(cell(&wb, 0, "A1").get("c").is_none());
}

#[test]
fn corpus_legacy_comment_imports() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/tdf117287_comment.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import");
    assert_eq!(cell(&wb, 0, "C9")["c"], json!("visible comment"));
}

#[test]
fn corpus_threaded_comments_import() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/threadedComment.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import");
    // A2: the thread (comment + reply, author names resolved through the
    // persons part) replaces the legacy "[Threaded comment]" placeholder.
    let a2 = cell(&wb, 0, "A2")["c"].as_str().expect("A2 note");
    assert_eq!(a2, "Mike Kaganski: a comment on A2\nMike Kaganski: A reply");
    // A5: plain legacy note, kept as-is.
    let a5 = cell(&wb, 0, "A5")["c"].as_str().expect("A5 note");
    assert_eq!(a5, "Mike Kaganski:\nsdadasd");
}

#[test]
fn threaded_comments_merge_without_persons() {
    // A thread whose personId is unknown falls back to the bare message text.
    let threads = super::read::comments::parse_threaded_comments(concat!(
        "<ThreadedComments xmlns=\"http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments\">",
        "<threadedComment ref=\"C3\" personId=\"{X}\" id=\"{1}\"><text>hello</text></threadedComment>",
        "<threadedComment ref=\"C3\" personId=\"{X}\" id=\"{2}\" parentId=\"{1}\"><text>world</text></threadedComment>",
        "</ThreadedComments>",
    ));
    let merged = super::read::comments::merge_comments(
        vec![("C3".into(), "[Threaded comment] placeholder".into())],
        threads,
        &std::collections::HashMap::new(),
    );
    assert_eq!(merged, vec![("C3".to_string(), "hello\nworld".to_string())]);
}

// ── Conditional formatting ───────────────────────────────────────────────────

// Rules of the n-th CF block of a sheet.
fn cf_rules(wb: &super::XlsxWorkbook, sheet: usize, block: usize) -> &Vec<Value> {
    wb.sheets[sheet].cond_formats[block]["rules"].as_array().expect("cf rules")
}

#[test]
fn roundtrip_conditional_formats_all_types() {
    let s1 = Uuid::new_v4();
    let content = json!({
        "version": 1,
        "sheets": {
            s1.to_string(): {
                "cells": { "A1": { "v": 1.0 }, "B1": { "v": 4.0 }, "C1": { "v": "ok" } },
                "cf": [
                    { "ranges": ["A1:A10"], "rules": [
                        { "type": "expression", "op": "", "formulas": ["=A1>5"],
                          "dxf": { "bg": "#FFC7CE", "color": "#9C0006", "bold": true, "italic": true }, "stop": true }
                    ]},
                    { "ranges": ["B1:B10"], "rules": [
                        { "type": "cellIs", "op": "between", "formulas": ["3", "7"], "dxf": { "bg": "#C6EFCE" }, "stop": false, "priority": 5 }
                    ]},
                    { "ranges": ["C1:C10", "E1:E10"], "rules": [
                        { "type": "containsText", "op": "containsText", "formulas": [], "dxf": { "color": "#9C6500" }, "stop": false, "text": "ok\"q", "priority": 2 },
                        { "type": "colorScale", "op": "3", "formulas": [], "dxf": {}, "stop": false,
                          "cs": { "lo": "#57BB8A", "mid": "#FFD666", "hi": "#E67C73" },
                          "csv": [ { "t": "min" }, { "t": "percentile", "v": 40.0 }, { "t": "max" } ] },
                        { "type": "dataBar", "op": "", "formulas": [], "dxf": {}, "stop": false,
                          "bar": { "color": "#8957E5", "showValue": false, "min": { "t": "num", "v": 0.0 }, "max": { "t": "percent", "v": 90.0 } } },
                        { "type": "iconSet", "op": "", "formulas": [], "dxf": {}, "stop": false,
                          "icons": { "set": "4Arrows", "reverse": true,
                                     "cfvo": [ { "t": "percent", "v": 0.0 }, { "t": "percent", "v": 25.0 }, { "t": "percent", "v": 50.0 }, { "t": "percent", "v": 75.0 } ] } },
                        { "type": "top10", "op": "", "formulas": [], "dxf": { "bg": "#FFEB9C" }, "stop": false, "rank": 3, "percent": true, "bottom": true },
                        { "type": "aboveAverage", "op": "", "formulas": [], "dxf": { "bg": "#DDDDFF" }, "stop": false, "above": false, "equal": true },
                        { "type": "duplicateValues", "op": "", "formulas": [], "dxf": { "bg": "#FFC7CE" }, "stop": false },
                        { "type": "timePeriod", "op": "", "formulas": [], "dxf": { "bg": "#CCFFCC" }, "stop": false, "period": "lastWeek" },
                        { "type": "containsBlanks", "op": "", "formulas": [], "dxf": { "bg": "#EEEEEE" }, "stop": false }
                    ]}
                ]
            }
        }
    });
    let metas = vec![SheetExportMeta { id: s1, name: "CF".into() }];
    let bytes = export_xlsx("CF", &content, &metas).expect("export");
    if let Ok(path) = std::env::var("XLSX_CF_DUMP_OUT") { std::fs::write(&path, &bytes).expect("dump"); }
    let wb = import_xlsx(&bytes).expect("re-import");

    assert_eq!(wb.sheets[0].cond_formats.len(), 3);

    // Block 1: expression + dxf. No stored priority → highest priority (1).
    let r = &cf_rules(&wb, 0, 0)[0];
    assert_eq!(r["type"], json!("expression"));
    assert_eq!(r["formulas"], json!(["A1>5"]));
    assert_eq!(r["stop"], json!(true));
    assert_eq!(r["priority"], json!(1));
    assert_eq!(r["dxf"]["bg"], json!("#FFC7CE"));
    assert_eq!(r["dxf"]["color"], json!("#9C0006"));
    assert_eq!(r["dxf"]["bold"], json!(true));
    assert_eq!(r["dxf"]["italic"], json!(true));

    // Block 2: cellIs between (stored priority 5 → renumbered after the
    // un-prioritised rules but ordered after stored priority 2).
    let r = &cf_rules(&wb, 0, 1)[0];
    assert_eq!(r["type"], json!("cellIs"));
    assert_eq!(r["op"], json!("between"));
    assert_eq!(r["formulas"], json!(["3", "7"]));
    let p_cellis = r["priority"].as_i64().expect("cellIs priority");
    assert_eq!(r["dxf"]["bg"], json!("#C6EFCE"));

    // Block 3: multi-range + every visual type.
    assert_eq!(wb.sheets[0].cond_formats[2]["ranges"], json!(["C1:C10", "E1:E10"]));
    let rules = cf_rules(&wb, 0, 2);
    assert_eq!(rules.len(), 9);

    let text = &rules[0];
    assert_eq!(text["type"], json!("containsText"));
    assert_eq!(text["op"], json!("containsText"));
    assert_eq!(text["text"], json!("ok\"q"));
    assert_eq!(text["dxf"]["color"], json!("#9C6500"));
    let p_text = text["priority"].as_i64().expect("text priority");
    assert!(p_text < p_cellis, "stored priority 2 must outrank stored 5");

    let cs = &rules[1];
    assert_eq!(cs["type"], json!("colorScale"));
    assert_eq!(cs["cs"], json!({ "lo": "#57BB8A", "mid": "#FFD666", "hi": "#E67C73" }));
    assert_eq!(cs["csv"], json!([ { "t": "min" }, { "t": "percentile", "v": 40.0 }, { "t": "max" } ]));

    let bar = &rules[2]["bar"];
    assert_eq!(rules[2]["type"], json!("dataBar"));
    assert_eq!(bar["color"], json!("#8957E5"));
    assert_eq!(bar["showValue"], json!(false));
    assert_eq!(bar["min"], json!({ "t": "num", "v": 0.0 }));
    assert_eq!(bar["max"], json!({ "t": "percent", "v": 90.0 }));

    let icons = &rules[3]["icons"];
    assert_eq!(rules[3]["type"], json!("iconSet"));
    assert_eq!(icons["set"], json!("4Arrows"));
    assert_eq!(icons["reverse"], json!(true));
    assert_eq!(icons["cfvo"].as_array().map(|a| a.len()), Some(4));
    assert_eq!(icons["cfvo"][1], json!({ "t": "percent", "v": 25.0 }));

    let top = &rules[4];
    assert_eq!(top["type"], json!("top10"));
    assert_eq!(top["rank"], json!(3));
    assert_eq!(top["percent"], json!(true));
    assert_eq!(top["bottom"], json!(true));
    assert_eq!(top["dxf"]["bg"], json!("#FFEB9C"));

    let avg = &rules[5];
    assert_eq!(avg["type"], json!("aboveAverage"));
    assert_eq!(avg["above"], json!(false));
    assert_eq!(avg["equal"], json!(true));

    assert_eq!(rules[6]["type"], json!("duplicateValues"));
    assert_eq!(rules[6]["dxf"]["bg"], json!("#FFC7CE"));

    let tp = &rules[7];
    assert_eq!(tp["type"], json!("timePeriod"));
    assert_eq!(tp["period"], json!("lastWeek"));

    let blanks = &rules[8];
    assert_eq!(blanks["type"], json!("containsBlanks"));
    // The exporter generates the anchored comparison formula.
    assert_eq!(blanks["formulas"], json!(["LEN(TRIM(C1))=0"]));
}

#[test]
fn corpus_colorscale_cfvos_import() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/colorscale.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import colorscale corpus");
    let blocks = &wb.sheets[0].cond_formats;
    assert!(blocks.len() >= 3);
    // B3:B6 — min/max scale, red → blue.
    let b = blocks.iter().find(|b| b["ranges"] == json!(["B3:B6"])).expect("B3:B6 block");
    let r = &b["rules"][0];
    assert_eq!(r["type"], json!("colorScale"));
    assert_eq!(r["cs"]["lo"], json!("#FF0000"));
    assert_eq!(r["cs"]["hi"], json!("#0000FF"));
    assert_eq!(r["csv"], json!([ { "t": "min", "v": 0.0 }, { "t": "max", "v": 0.0 } ]));
    // D3:D6 — percentile 10 → percent 90.
    let d = blocks.iter().find(|b| b["ranges"] == json!(["D3:D6"])).expect("D3:D6 block");
    assert_eq!(d["rules"][0]["csv"], json!([ { "t": "percentile", "v": 10.0 }, { "t": "percent", "v": 90.0 } ]));
    // F3:F6 — num 0 → formula "2*A1+2".
    let f = blocks.iter().find(|b| b["ranges"] == json!(["F3:F6"])).expect("F3:F6 block");
    assert_eq!(f["rules"][0]["csv"], json!([ { "t": "num", "v": 0.0 }, { "t": "formula", "v": "2*A1+2" } ]));
}

#[test]
fn corpus_databar_import() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/databar.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import databar corpus");
    let blocks = &wb.sheets[0].cond_formats;
    let b = blocks.iter().find(|b| b["ranges"] == json!(["B3:B6"])).expect("B3:B6 block");
    let r = &b["rules"][0];
    assert_eq!(r["type"], json!("dataBar"));
    assert_eq!(r["bar"]["color"], json!("#0000FF"));
    // The x14 <extLst> inside the rule must not pollute the thresholds.
    assert_eq!(r["bar"]["min"], json!({ "t": "min", "v": 0.0 }));
    assert_eq!(r["bar"]["max"], json!({ "t": "max", "v": 0.0 }));
    // H3:H6 — num -2 → formula "3*A1+2".
    let h = blocks.iter().find(|b| b["ranges"] == json!(["H3:H6"])).expect("H3:H6 block");
    assert_eq!(h["rules"][0]["bar"]["min"], json!({ "t": "num", "v": -2.0 }));
    assert_eq!(h["rules"][0]["bar"]["max"], json!({ "t": "formula", "v": "3*A1+2" }));
}

#[test]
fn corpus_top10_and_above_average_import() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/new_cond_format_test.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import top10 corpus");
    let blocks = &wb.sheets[0].cond_formats;
    assert_eq!(blocks.len(), 8);
    let rule_of = |range: &str| -> &Value {
        &blocks.iter().find(|b| b["ranges"] == json!([range])).expect("block")["rules"][0]
    };
    let b = rule_of("B2:B22");
    assert_eq!(b["type"], json!("top10"));
    assert_eq!(b["rank"], json!(5));
    assert!(b.get("percent").is_none() && b.get("bottom").is_none());
    // (The dxfs of this corpus file are numFmt-only — not modelled internally.)
    let d = rule_of("D2:D22");
    assert_eq!((d["rank"].clone(), d["bottom"].clone()), (json!(5), json!(true)));
    let f = rule_of("F2:F22");
    assert_eq!((f["rank"].clone(), f["percent"].clone()), (json!(10), json!(true)));
    let j = rule_of("J2:J22");
    assert_eq!(j["type"], json!("aboveAverage"));
    assert!(j.get("above").is_none() && j.get("equal").is_none());
    let l = rule_of("L2:L22");
    assert_eq!(l["above"], json!(false));
    let n = rule_of("N2:N22");
    assert_eq!(n["equal"], json!(true));
    let p = rule_of("P2:P22");
    assert_eq!((p["above"].clone(), p["equal"].clone()), (json!(false), json!(true)));
    // Stored priorities survive (P2 block has priority 1).
    assert_eq!(p["priority"], json!(1));
}

// ── Full-corpus stress test ──────────────────────────────────────────────────
//
// Iterates over every .xlsx of the LibreOffice test corpus (path from the
// KB_XLSX_CORPUS env var, falling back to the local checkout). For each file:
// import must never panic (clean errors are fine — some corpus files are
// intentionally corrupt or password-protected), and whatever imports must
// survive an export → re-import round trip with the same sheet and cell counts.
// Run with: KB_XLSX_CORPUS=/path cargo test -- --ignored --nocapture

/// Recursively collect every .xlsx under `dir`.
fn collect_xlsx(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_xlsx(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xlsx")) {
            out.push(p);
        }
    }
}

/// Mirror of the handler's sheet mapping (`open_by_file`): XlsxSheet → the
/// .kbcal sheet JSON the exporter consumes.
fn sheet_to_json(s: &super::XlsxSheet) -> Value {
    json!({
        "cells":       s.cells,
        "col_widths":  s.col_widths,
        "row_heights": s.row_heights,
        "frozen_rows": s.frozen_rows,
        "frozen_cols": s.frozen_cols,
        "merges":      s.merges,
        "cf":          s.cond_formats,
        "gridlines":   s.show_gridlines,
        "default_row_height": s.default_row_height,
        "default_col_width": s.default_col_width,
        "images":      s.images,
        "charts":      s.charts,
        "validations": s.validations,
        "row_groups":  s.row_groups,
        "col_groups":  s.col_groups,
        "protection":  s.protection,
        "hidden":      s.hidden,
        "tab_color":   s.tab_color,
        "auto_filter": s.auto_filter,
        "print_area":  s.print_area,
        "print_titles": s.print_titles,
    })
}

#[test]
#[ignore] // corpus lives outside the repo — run explicitly with --ignored
fn corpus_full_import_and_roundtrip() {
    let dir = std::env::var("KB_XLSX_CORPUS").unwrap_or_else(|_| LO_CORPUS.to_string());
    let mut files = Vec::new();
    collect_xlsx(std::path::Path::new(&dir), &mut files);
    if files.is_empty() { return } // corpus absent on this machine
    files.sort();

    // Silence the default panic backtrace printer while probing.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let (mut ok, mut clean_err, mut panics, mut rt_ok) = (0u32, 0u32, 0u32, 0u32);
    let mut failures: Vec<String> = Vec::new();
    for path in &files {
        let name = path.strip_prefix(&dir).unwrap_or(path).display().to_string();
        let Ok(bytes) = std::fs::read(path) else { continue };
        let wb = match std::panic::catch_unwind(|| import_xlsx(&bytes)) {
            Err(_) => { panics += 1; failures.push(format!("PANIC import {name}")); continue }
            Ok(Err(e)) => { clean_err += 1; println!("  clean error {name}: {e:#}"); continue }
            Ok(Ok(wb)) => { ok += 1; wb }
        };

        // Round trip: rebuild the .kbcal JSON exactly like the import handler.
        let mut sheets_obj = serde_json::Map::new();
        let mut metas = Vec::new();
        for s in &wb.sheets {
            let id = Uuid::new_v4();
            sheets_obj.insert(id.to_string(), sheet_to_json(s));
            metas.push(SheetExportMeta { id, name: s.name.clone() });
        }
        let names_obj: serde_json::Map<String, Value> = wb.defined_names.iter()
            .map(|(n, def)| (n.to_uppercase(), Value::String(def.clone()))).collect();
        let content = json!({ "version": 1, "sheets": sheets_obj, "names": names_obj });

        let out = match std::panic::catch_unwind(|| export_xlsx("corpus", &content, &metas)) {
            Err(_) => { panics += 1; failures.push(format!("PANIC export {name}")); continue }
            Ok(Err(e)) => { failures.push(format!("export error {name}: {e:#}")); continue }
            Ok(Ok(bytes)) => bytes,
        };
        let wb2 = match std::panic::catch_unwind(|| import_xlsx(&out)) {
            Err(_) => { panics += 1; failures.push(format!("PANIC re-import {name}")); continue }
            Ok(Err(e)) => { failures.push(format!("re-import error {name}: {e:#}")); continue }
            Ok(Ok(wb2)) => wb2,
        };

        if wb2.sheets.len() != wb.sheets.len() {
            failures.push(format!("sheet count {name}: {} → {}", wb.sheets.len(), wb2.sheets.len()));
            continue;
        }
        let mut cells_ok = true;
        for (a, b) in wb.sheets.iter().zip(&wb2.sheets) {
            if a.cells.len() != b.cells.len() {
                let only_a: Vec<_> = a.cells.keys().filter(|k| !b.cells.contains_key(*k)).take(4).collect();
                let only_b: Vec<_> = b.cells.keys().filter(|k| !a.cells.contains_key(*k)).take(4).collect();
                failures.push(format!(
                    "cell count {name} [{}]: {} → {} (lost {only_a:?}, gained {only_b:?})",
                    a.name, a.cells.len(), b.cells.len()));
                cells_ok = false;
            }
        }
        if cells_ok { rt_ok += 1; }
    }
    std::panic::set_hook(prev_hook);

    println!("corpus: {} files — import ok {ok}, clean errors {clean_err}, panics {panics}, round-trips ok {rt_ok}", files.len());
    for f in &failures { println!("  FAIL {f}"); }
    assert!(failures.is_empty(), "{} corpus failures (see stdout)", failures.len());
}

/// Charts carry a free rotation (grid rotation knob) through the graphicFrame's
/// `a:xfrm rot`, exactly like pictures do.
#[test]
fn roundtrip_chart_rotation() {
    let s1 = Uuid::new_v4();
    let content = json!({
        "version": 1,
        "sheets": {
            s1.to_string(): {
                "cells": {
                    "A1": { "v": "Jan" }, "B1": { "v": 10.0 },
                    "A2": { "v": "Fév" }, "B2": { "v": 20.0 },
                    "A3": { "v": "Mar" }, "B3": { "v": 30.0 }
                },
                "charts": [
                    { "type": "bar", "range": "A1:B3", "bx": 320.0, "by": 20.0, "bw": 380.0, "bh": 240.0,
                      "rot": 30.0 },
                    // No rotation → no `rot` attribute, and none on the way back.
                    { "type": "line", "range": "A1:B3", "bx": 320.0, "by": 300.0, "bw": 380.0, "bh": 240.0 }
                ]
            }
        }
    });
    let metas = vec![SheetExportMeta { id: s1, name: "Feuille 1".into() }];
    let bytes = export_xlsx("chart rotation", &content, &metas).expect("export");
    let wb = import_xlsx(&bytes).expect("re-import");
    let charts = &wb.sheets[0].charts;
    assert_eq!(charts.len(), 2);
    assert_eq!(charts[0]["rot"], json!(30.0));
    assert!(charts[1].get("rot").is_none(), "unrotated chart must not gain a rot field");
}

// ── Drawing objects: chart area, picture properties, shapes ──────────────────

#[test]
fn roundtrip_chart_area_style_and_alt_text() {
    let s1 = Uuid::new_v4();
    let content = json!({
        "version": 1,
        "sheets": {
            s1.to_string(): {
                "cells": {
                    "A1": { "v": "Jan" }, "B1": { "v": 10.0 },
                    "A2": { "v": "Fév" }, "B2": { "v": 20.0 }
                },
                "charts": [
                    // Solid chart area + outline + font + accessibility description.
                    { "type": "bar", "range": "A1:B2", "bx": 20.0, "by": 20.0, "bw": 380.0, "bh": 240.0,
                      "fill": "#FFF3E0", "border": "#D93025", "borderWidth": 2.0,
                      "font": "Verdana", "altText": "Ventes par mois" },
                    // Explicitly transparent area, no outline.
                    { "type": "line", "range": "A1:B2", "bx": 20.0, "by": 300.0, "bw": 380.0, "bh": 240.0,
                      "fill": "none", "border": "none" },
                    // Nothing set → nothing gained on the way back.
                    { "type": "pie", "range": "A1:B2", "bx": 20.0, "by": 560.0, "bw": 380.0, "bh": 240.0 }
                ]
            }
        }
    });
    let metas = vec![SheetExportMeta { id: s1, name: "Feuille 1".into() }];
    let bytes = export_xlsx("chart style", &content, &metas).expect("export");
    if let Ok(path) = std::env::var("XLSX_DUMP_CHART_STYLE") { std::fs::write(&path, &bytes).expect("dump"); }
    let wb = import_xlsx(&bytes).expect("re-import");
    let charts = &wb.sheets[0].charts;
    assert_eq!(charts.len(), 3);

    assert_eq!(charts[0]["fill"], json!("#FFF3E0"));
    assert_eq!(charts[0]["border"], json!("#D93025"));
    assert_eq!(charts[0]["borderWidth"], json!(2.0));
    assert_eq!(charts[0]["font"], json!("Verdana"));
    assert_eq!(charts[0]["altText"], json!("Ventes par mois"));

    assert_eq!(charts[1]["fill"], json!("none"));
    assert_eq!(charts[1]["border"], json!("none"));
    // A "none" outline carries no width.
    assert!(charts[1].get("borderWidth").is_none());

    for k in ["fill", "border", "borderWidth", "font", "altText"] {
        assert!(charts[2].get(k).is_none(), "unstyled chart must not gain `{k}`");
    }
}

#[test]
fn roundtrip_image_alt_text_link_border_and_shadow() {
    let s1 = Uuid::new_v4();
    let src = format!("data:image/png;base64,{PNG_1PX}");
    let content = json!({
        "version": 1,
        "sheets": {
            s1.to_string(): {
                "cells": { "A1": { "v": 1.0 } },
                "images": [
                    { "fromCol": 0, "fromRow": 0, "bx": 0.0, "by": 0.0, "bw": 200.0, "bh": 100.0, "src": src,
                      "altText": "Logo Kubuno", "link": "https://kubuno.example/docs?a=1&b=2",
                      "border": "#1A73E8", "borderWidth": 1.5, "shadow": true },
                    { "fromCol": 0, "fromRow": 10, "bx": 0.0, "by": 240.0, "bw": 200.0, "bh": 100.0, "src": src,
                      "border": "none" },
                    { "fromCol": 0, "fromRow": 20, "bx": 0.0, "by": 480.0, "bw": 200.0, "bh": 100.0, "src": src }
                ]
            }
        }
    });
    let metas = vec![SheetExportMeta { id: s1, name: "Feuille 1".into() }];
    let bytes = export_xlsx("image props", &content, &metas).expect("export");
    if let Ok(path) = std::env::var("XLSX_DUMP_IMAGE_PROPS") { std::fs::write(&path, &bytes).expect("dump"); }
    let wb = import_xlsx(&bytes).expect("re-import");
    let imgs = &wb.sheets[0].images;
    assert_eq!(imgs.len(), 3);

    assert_eq!(imgs[0]["altText"], json!("Logo Kubuno"));
    assert_eq!(imgs[0]["link"], json!("https://kubuno.example/docs?a=1&b=2"));
    assert_eq!(imgs[0]["border"], json!("#1A73E8"));
    assert_eq!(imgs[0]["borderWidth"], json!(1.5));
    assert_eq!(imgs[0]["shadow"], json!(true));

    assert_eq!(imgs[1]["border"], json!("none"));
    for k in ["altText", "link", "border", "borderWidth", "shadow"] {
        assert!(imgs[2].get(k).is_none(), "plain picture must not gain `{k}`");
    }
}

#[test]
fn roundtrip_shapes_every_kind() {
    let s1 = Uuid::new_v4();
    let kinds = ["rect", "roundRect", "ellipse", "triangle", "diamond",
                 "arrow", "line", "star", "callout", "plus"];
    // One shape per kind, stacked 120 px apart on a default-geometry sheet
    // (100 px columns, 24 px rows) so the anchor arithmetic is exact.
    let shapes: Vec<Value> = kinds.iter().enumerate().map(|(i, k)| json!({
        "id": format!("s{i}"), "kind": k,
        "bx": 200.0, "by": (i as f64) * 120.0, "bw": 160.0, "bh": 96.0,
        "fill": "#DBE7FF", "border": "#1A73E8", "borderWidth": 1.0
    })).collect();
    let content = json!({
        "version": 1,
        "sheets": { s1.to_string(): {
            "cells": { "A1": { "v": 1.0 } },
            // Declared explicitly: the worksheet writer falls back to Excel's 15 pt
            // (= 20 px) when the key is absent, while the drawing writer walks the
            // grid with the frontend's 24 px default — the two must agree for a
            // pixel box to survive the trip.
            "default_row_height": 24.0, "default_col_width": 100.0,
            "shapes": shapes,
        } }
    });
    let metas = vec![SheetExportMeta { id: s1, name: "Feuille 1".into() }];
    let bytes = export_xlsx("shapes", &content, &metas).expect("export");
    if let Ok(path) = std::env::var("XLSX_DUMP_SHAPES") { std::fs::write(&path, &bytes).expect("dump"); }
    let wb = import_xlsx(&bytes).expect("re-import");
    let out = &wb.sheets[0].shapes;
    assert_eq!(out.len(), kinds.len());
    for (i, k) in kinds.iter().enumerate() {
        assert_eq!(out[i]["kind"], json!(k), "kind of shape {i}");
        assert_eq!(out[i]["fill"], json!("#DBE7FF"));
        assert_eq!(out[i]["border"], json!("#1A73E8"));
        assert_eq!(out[i]["borderWidth"], json!(1.0));
        // Geometry: oneCellAnchor from the pixel box, resolved back to pixels.
        assert_eq!(out[i]["bx"], json!(200.0), "bx of shape {i}");
        assert_eq!(out[i]["by"], json!((i as f64) * 120.0), "by of shape {i}");
        assert_eq!(out[i]["bw"], json!(160.0));
        assert_eq!(out[i]["bh"], json!(96.0));
        // Every imported shape gets a stable id (the model requires one).
        assert!(out[i]["id"].is_string());
    }
}

#[test]
fn roundtrip_shape_text_style_rotation_alt_text_and_link() {
    let s1 = Uuid::new_v4();
    let content = json!({
        "version": 1,
        "sheets": {
            s1.to_string(): {
                "cells": { "A1": { "v": 1.0 } },
                "default_row_height": 24.0, "default_col_width": 100.0,
                "shapes": [
                    { "id": "a", "kind": "roundRect", "bx": 100.0, "by": 0.0, "bw": 240.0, "bh": 120.0,
                      "rot": 30.0, "fill": "none", "border": "#202124", "borderWidth": 2.0,
                      "text": "Ligne 1\nLigne <2> & \"3\"",
                      "textStyle": { "bold": true, "italic": true, "color": "#D93025", "size": 14.0, "align": "left" },
                      "altText": "Étiquette", "link": "https://kubuno.example/a?x=1&y=2" },
                    // No caption, no style, no link.
                    { "id": "b", "kind": "ellipse", "bx": 500.0, "by": 0.0, "bw": 120.0, "bh": 120.0 }
                ]
            }
        }
    });
    let metas = vec![SheetExportMeta { id: s1, name: "Feuille 1".into() }];
    let bytes = export_xlsx("shape text", &content, &metas).expect("export");
    if let Ok(path) = std::env::var("XLSX_DUMP_SHAPE_TEXT") { std::fs::write(&path, &bytes).expect("dump"); }
    let wb = import_xlsx(&bytes).expect("re-import");
    let out = &wb.sheets[0].shapes;
    assert_eq!(out.len(), 2);

    let a = &out[0];
    assert_eq!(a["kind"], json!("roundRect"));
    assert_eq!(a["rot"], json!(30.0));
    assert_eq!(a["fill"], json!("none"));
    assert_eq!(a["border"], json!("#202124"));
    assert_eq!(a["borderWidth"], json!(2.0));
    assert_eq!(a["text"], json!("Ligne 1\nLigne <2> & \"3\""));
    assert_eq!(a["textStyle"], json!({ "bold": true, "italic": true, "color": "#D93025", "size": 14.0, "align": "left" }));
    assert_eq!(a["altText"], json!("Étiquette"));
    assert_eq!(a["link"], json!("https://kubuno.example/a?x=1&y=2"));
    // A rotated shape keeps its upright size and its centre.
    assert_eq!((a["bw"].clone(), a["bh"].clone()), (json!(240.0), json!(120.0)));
    assert_eq!((a["bx"].clone(), a["by"].clone()), (json!(100.0), json!(0.0)));

    let b = &out[1];
    assert_eq!(b["kind"], json!("ellipse"));
    for k in ["text", "textStyle", "altText", "link", "rot", "fill", "border", "borderWidth"] {
        assert!(b.get(k).is_none(), "plain shape must not gain `{k}`");
    }
}

#[test]
fn shape_preset_mapping_is_exact() {
    use super::read::shape::kind_from_preset;
    // Canonical presets.
    for (prst, kind) in [("rect", "rect"), ("roundRect", "roundRect"), ("ellipse", "ellipse"),
                         ("triangle", "triangle"), ("diamond", "diamond"), ("rightArrow", "arrow"),
                         ("line", "line"), ("star5", "star"), ("wedgeRectCallout", "callout"),
                         ("mathPlus", "plus")] {
        assert_eq!(kind_from_preset(prst), kind);
    }
    // Exact mappings from the generated LibreOffice table (no approximations).
    for (prst, kind) in [("flowChartDecision", "flowDecision"), ("star6", "star6"),
                         ("upArrow", "arrowUp"), ("wedgeEllipseCallout", "calloutOval"),
                         ("chevron", "chevron"), ("can", "cylinder")] {
        assert_eq!(kind_from_preset(prst), kind);
    }
    // Truly unknown presets still degrade to a rectangle.
    assert_eq!(kind_from_preset("noSuchPreset"), "rect");
}

#[test]
fn corpus_shape_with_caption_imports() {
    // A single oneCellAnchor <xdr:sp> (prst="rect") carrying a long caption.
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/tdf119565.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import");
    let shape = wb.sheets.iter().flat_map(|s| &s.shapes).next().expect("a shape");
    assert_eq!(shape["kind"], json!("rect"));
    assert!(shape["text"].as_str().unwrap_or_default().starts_with("Lorem ipsum"));
    // sz="1100" → 11 pt; the caption is left-aligned by OOXML default.
    assert_eq!(shape["textStyle"]["size"], json!(11.0));
    assert_eq!(shape["textStyle"]["align"], json!("left"));
    // xdr:ext 1390650 × 2331279 EMU → px.
    assert_eq!(shape["bw"], json!(146.0));
    assert_eq!(shape["bh"], json!(244.8));
}

#[test]
fn corpus_shape_unknown_preset_and_rotation_import() {
    // prst="upArrow" inside a twoCellAnchor, rot="4616172". Once approximated to a
    // rectangle; the generated LibreOffice table now maps it exactly.
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/tdf135828_Shape_Rect.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import");
    let shape = wb.sheets.iter().flat_map(|s| &s.shapes).next().expect("a shape");
    assert_eq!(shape["kind"], json!("arrowUp"), "preset mapped exactly, no approximation");
    assert_eq!(shape["rot"].as_f64().map(|r| (r * 100.0).round() / 100.0), Some(76.94));
    // a:ext wins over the (rotated) anchor rectangle: 295275 × 1981200 EMU.
    assert_eq!(shape["bw"], json!(31.0));
    assert_eq!(shape["bh"], json!(208.0));
}

#[test]
fn corpus_image_hyperlink_imports() {
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/image_hyperlink.xlsx")) else { return };
    let wb = import_xlsx(&bytes).expect("import");
    let img = wb.sheets.iter().flat_map(|s| &s.images).next().expect("a picture");
    assert!(img["link"].as_str().is_some_and(|l| l.starts_with("http")), "picture keeps its hyperlink");
}

#[test]
fn corpus_alternate_content_shape_is_not_duplicated() {
    // The drawing wraps its anchor in <mc:AlternateContent>: only <mc:Choice>
    // counts, <mc:Fallback> holds the same object for legacy readers.
    let Ok(bytes) = std::fs::read(format!("{LO_CORPUS}/universal-content-strict.xlsx")) else { return };
    let Ok(wb) = import_xlsx(&bytes) else { return };
    for s in &wb.sheets {
        let mut boxes: Vec<String> = s.shapes.iter()
            .map(|sh| format!("{}/{}/{}/{}", sh["bx"], sh["by"], sh["bw"], sh["bh"]))
            .collect();
        let before = boxes.len();
        boxes.sort();
        boxes.dedup();
        assert_eq!(boxes.len(), before, "sheet {} has duplicated shapes", s.name);
    }
}

/// Shapes carry their mirroring through `a:xfrm flipH/flipV`, next to the rotation.
#[test]
fn roundtrip_shape_flip() {
    let s1 = Uuid::new_v4();
    let content = json!({
        "version": 1,
        "sheets": {
            s1.to_string(): {
                "cells": {},
                "shapes": [
                    { "id": "s1", "kind": "arrow", "bx": 20.0, "by": 20.0, "bw": 120.0, "bh": 60.0,
                      "flipH": true, "rot": 30.0 },
                    { "id": "s2", "kind": "triangle", "bx": 200.0, "by": 20.0, "bw": 90.0, "bh": 90.0,
                      "flipV": true },
                    { "id": "s3", "kind": "rect", "bx": 340.0, "by": 20.0, "bw": 80.0, "bh": 40.0 }
                ]
            }
        }
    });
    let metas = vec![SheetExportMeta { id: s1, name: "Feuille 1".into() }];
    let bytes = export_xlsx("shape flip", &content, &metas).expect("export");
    let wb = import_xlsx(&bytes).expect("re-import");
    let shapes = &wb.sheets[0].shapes;
    assert_eq!(shapes.len(), 3);
    assert_eq!(shapes[0]["flipH"], json!(true));
    assert_eq!(shapes[0]["rot"], json!(30.0));
    assert_eq!(shapes[1]["flipV"], json!(true));
    assert!(shapes[2].get("flipH").is_none(), "an unmirrored shape must not gain a flip flag");
    assert!(shapes[2].get("flipV").is_none());
}

/// Preset shapes round-trip their exact OOXML name AND their adjustment values
/// (avLst), using the adjustment names from the generated LibreOffice table.
#[test]
fn roundtrip_shape_presets_avlst() {
    let s1 = Uuid::new_v4();
    let content = json!({
        "version": 1,
        "sheets": {
            s1.to_string(): {
                "cells": {},
                "shapes": [
                    { "id": "s1", "kind": "chevron", "bx": 10.0, "by": 10.0, "bw": 120.0, "bh": 80.0,
                      "adj": [37500.0] },
                    { "id": "s2", "kind": "star6", "bx": 200.0, "by": 10.0, "bw": 100.0, "bh": 100.0,
                      "adj": [22000.0, 115470.0] },
                    { "id": "s3", "kind": "hexagon", "bx": 340.0, "by": 10.0, "bw": 110.0, "bh": 90.0 }
                ]
            }
        }
    });
    let metas = vec![SheetExportMeta { id: s1, name: "Feuille 1".into() }];
    let bytes = export_xlsx("presets avlst", &content, &metas).expect("export");
    let wb = import_xlsx(&bytes).expect("re-import");
    let shapes = &wb.sheets[0].shapes;
    assert_eq!(shapes.len(), 3);
    assert_eq!(shapes[0]["kind"], json!("chevron"));
    assert_eq!(shapes[0]["adj"], json!([37500.0]));
    assert_eq!(shapes[1]["kind"], json!("star6"));
    assert_eq!(shapes[1]["adj"], json!([22000.0, 115470.0]));
    assert_eq!(shapes[2]["kind"], json!("hexagon"));
    assert!(shapes[2].get("adj").is_none(), "no avLst → no adj field");
}
