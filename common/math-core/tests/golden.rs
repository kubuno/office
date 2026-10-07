//! The layout compared with KaTeX in a browser: `golden-katex.json` holds, for every case of
//! `tools/golden/cases.json`, the position of every glyph KaTeX's HTML puts on screen (measured in headless
//! Chrome by `tools/golden/katex.mjs`), in ems of the formula's font size.

use kubuno_office_math_core::{fonts, render, Item};
use serde_json::Value;

/// The KaTeX font name of a browser font (family, weight, style).
fn font_name(family: &str, weight: &str, style: &str) -> String {
    let base = family.trim_start_matches("KaTeX_");
    let bold = weight.parse::<u32>().unwrap_or(400) >= 600;
    let italic = style == "italic";
    let suffix = match (bold, italic) {
        (true, true) => "BoldItalic",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (false, false) => "Regular",
    };
    format!("{base}-{suffix}")
}

const TOL: f64 = 0.012;

fn compare(case: &Value, verbose: bool) -> (usize, usize, Vec<String>) {
    let src = case["src"].as_str().unwrap_or("");
    let r = render(src);
    let mine: Vec<(char, String, f64, f64, f64)> = r
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Glyph { font, ch, x, y, size, .. } if !ch.is_whitespace() && *ch != '\u{a0}' => Some((*ch, font.clone(), *x, *y, *size)),
            _ => None,
        })
        .collect();
    let theirs: Vec<&Value> = case["glyphs"].as_array().map(|a| a.iter().filter(|g| g["ch"].as_str().is_some_and(|c| !c.trim().is_empty() && c != "\u{a0}")).collect()).unwrap_or_default();
    let mut problems = Vec::new();
    let width = case["width"].as_f64().unwrap_or(0.0);
    if (r.width - width).abs() > TOL * 2.0 {
        problems.push(format!("width {:.4} vs {:.4}", r.width, width));
    }
    if mine.len() != theirs.len() {
        problems.push(format!("glyph count {} vs {}: mine {:?} theirs {:?}", mine.len(), theirs.len(), mine.iter().map(|g| g.0).collect::<String>(), theirs.iter().filter_map(|g| g["ch"].as_str()).collect::<String>()));
    }
    // Rules (fraction bars, overlines, table lines): same count, same boxes.
    let mut my_rules: Vec<(f64, f64, f64)> = r.items.iter().filter_map(|i| if let Item::Rect { x, y, w, h, .. } = i { if w > h { Some((*x, *w, y + h)) } else { None } } else { None }).collect();
    let mut their_rules: Vec<(f64, f64, f64)> = case["rules"].as_array().map(|a| a.iter().map(|r| (r["x"].as_f64().unwrap_or(0.0), r["w"].as_f64().unwrap_or(0.0), r["bottom"].as_f64().unwrap_or(0.0))).collect()).unwrap_or_default();
    let key = |a: &(f64, f64, f64), b: &(f64, f64, f64)| (a.0, a.2).partial_cmp(&(b.0, b.2)).unwrap_or(std::cmp::Ordering::Equal);
    my_rules.sort_by(key);
    their_rules.sort_by(key);
    if my_rules.len() != their_rules.len() {
        problems.push(format!("rule count {} vs {}", my_rules.len(), their_rules.len()));
    } else {
        for (m, t) in my_rules.iter().zip(their_rules.iter()) {
            if (m.0 - t.0).abs() > TOL || (m.1 - t.1).abs() > 2.0 * TOL || (m.2 - t.2).abs() > TOL {
                problems.push(format!("rule x {:.4} w {:.4} bottom {:.4} vs x {:.4} w {:.4} bottom {:.4}", m.0, m.1, m.2, t.0, t.1, t.2));
            }
        }
    }
    let mut ok = 0;
    for (m, t) in mine.iter().zip(theirs.iter()) {
        let ch = t["ch"].as_str().unwrap_or("");
        let fname = font_name(t["family"].as_str().unwrap_or(""), t["weight"].as_str().unwrap_or("400"), t["style"].as_str().unwrap_or("normal"));
        let size = t["size"].as_f64().unwrap_or(1.0);
        let descent = fonts::line_metrics(&fname).map(|l| l.hhea_descent).unwrap_or(0.0);
        let baseline = t["bottom"].as_f64().unwrap_or(0.0) - descent * size;
        let tx = t["x"].as_f64().unwrap_or(0.0);
        let good_char = ch.starts_with(m.0);
        let good_font = fname == m.1 || (fname.starts_with("Main") && m.1.starts_with("Main") && fname == m.1);
        let good = good_char && good_font && (m.2 - tx).abs() < TOL && (m.3 - baseline).abs() < TOL && (m.4 - size).abs() < 0.01;
        if good {
            ok += 1;
        } else if verbose {
            problems.push(format!("'{}' {} x {:.4} y {:.4} s {:.3}  vs  '{}' {} x {:.4} y {:.4} s {:.3}", m.0, m.1, m.2, m.3, m.4, ch, fname, tx, baseline, size));
        }
    }
    (ok, theirs.len(), problems)
}

#[test]
fn glyphs_sit_where_katex_puts_them() {
    let mut cases = Vec::new();
    for text in [include_str!("golden-katex.json"), include_str!("golden-katex2.json")] {
        let data: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        cases.extend(data.as_array().cloned().unwrap_or_default());
    }
    let verbose = std::env::var("GOLDEN_VERBOSE").is_ok();
    let mut failed = 0;
    let (mut total_ok, mut total) = (0, 0);
    for case in &cases {
        let (ok, n, problems) = compare(case, verbose);
        total_ok += ok;
        total += n;
        if !problems.is_empty() || ok != n {
            failed += 1;
            eprintln!("✗ {} — {ok}/{n}", case["src"].as_str().unwrap_or(""));
            for p in problems.iter().take(12) {
                eprintln!("    {p}");
            }
        }
    }
    eprintln!("{total_ok}/{total} glyphs placed, {failed} cases with differences");
    assert_eq!(failed, 0);
}

/// The equation box of the web spreadsheet (`golden-box.json`, measured by `tools/golden/box.mjs`).
#[test]
fn the_equation_box_matches_the_web() {
    let data: Value = serde_json::from_str(include_str!("golden-box.json")).unwrap_or(Value::Null);
    let mut bad = 0;
    for c in data.as_array().cloned().unwrap_or_default() {
        let src = c["src"].as_str().unwrap_or("");
        let g = kubuno_office_math_core::sheet::geometry(&render(src), 1.0);
        let (ow, oh) = (c["outerW"].as_f64().unwrap_or(0.0), c["outerH"].as_f64().unwrap_or(0.0));
        let (iw, ih) = (c["innerW"].as_f64().unwrap_or(0.0), c["innerH"].as_f64().unwrap_or(0.0));
        let base = c["baseline"].as_f64().unwrap_or(0.0);
        let ok = (g.auto_w - ow).abs() < 0.2 && (g.auto_h - oh).abs() < 0.2 && (g.natural_w - (iw + 10.0)).abs() < 0.01 && (g.natural_h - (ih + 10.0)).abs() < 0.01 && (g.content_y + g.html_top - base).abs() < 0.2;
        if !ok {
            bad += 1;
            eprintln!("✗ {src}: auto {:.2}×{:.2} vs {ow}×{oh}, natural {:.1}×{:.1} vs {}×{}, baseline {:.2} vs {base}", g.auto_w, g.auto_h, g.natural_w, g.natural_h, iw + 10.0, ih + 10.0, g.content_y + g.html_top);
        }
    }
    // The array with a vertical rule: Chrome draws its 0.87 px border as 1 px (+1.1 px).
    assert!(bad <= 1, "{bad} boxes differ");
}
