//! Parity with the web shape engine: every catalogue kind and every kind a slide stores, drawn by
//! `office/web/src/shapes` (recorded by `tools/golden.ts` into `tests/golden/shapes.json`) and by this
//! crate, must give the same sub-paths, text frames and adjustment knobs.

use kubuno_office_shapes_core::{adjust, shape_text_box, shape_view, Cmd, ViewOptions};
use serde_json::Value;

/// The web's path string as commands (`H`/`V` written as lines, like this crate does).
fn parse_svg(d: &str) -> Vec<Cmd> {
    let tokens: Vec<String> = d.replace(',', " ").split_whitespace().map(str::to_string).collect();
    let mut out = Vec::new();
    let (mut i, mut cx, mut cy) = (0usize, 0.0f64, 0.0f64);
    let num = |t: &[String], k: usize| -> f64 { t.get(k).and_then(|s| s.parse().ok()).unwrap_or(f64::NAN) };
    while i < tokens.len() {
        let c = tokens[i].clone();
        i += 1;
        match c.as_str() {
            "M" => {
                (cx, cy) = (num(&tokens, i), num(&tokens, i + 1));
                out.push(Cmd::MoveTo(cx, cy));
                i += 2;
            }
            "L" => {
                (cx, cy) = (num(&tokens, i), num(&tokens, i + 1));
                out.push(Cmd::LineTo(cx, cy));
                i += 2;
            }
            "H" => {
                cx = num(&tokens, i);
                out.push(Cmd::LineTo(cx, cy));
                i += 1;
            }
            "V" => {
                cy = num(&tokens, i);
                out.push(Cmd::LineTo(cx, cy));
                i += 1;
            }
            "C" => {
                let v: Vec<f64> = (0..6).map(|k| num(&tokens, i + k)).collect();
                (cx, cy) = (v[4], v[5]);
                out.push(Cmd::CubicTo(v[0], v[1], v[2], v[3], v[4], v[5]));
                i += 6;
            }
            "Q" => {
                let v: Vec<f64> = (0..4).map(|k| num(&tokens, i + k)).collect();
                (cx, cy) = (v[2], v[3]);
                out.push(Cmd::QuadTo(v[0], v[1], v[2], v[3]));
                i += 4;
            }
            "A" => {
                let v: Vec<f64> = (0..7).map(|k| num(&tokens, i + k)).collect();
                (cx, cy) = (v[5], v[6]);
                out.push(Cmd::ArcTo { rx: v[0], ry: v[1], large: v[3] != 0.0, sweep: v[4] != 0.0, x: v[5], y: v[6] });
                i += 7;
            }
            "Z" => out.push(Cmd::Close),
            other => panic!("unexpected path token {other:?} in {d}"),
        }
    }
    out
}

fn same(a: &Cmd, b: &Cmd) -> bool {
    let close = |x: f64, y: f64| (x - y).abs() < 0.006 || (x.is_nan() && y.is_nan());
    match (*a, *b) {
        (Cmd::MoveTo(a1, a2), Cmd::MoveTo(b1, b2)) | (Cmd::LineTo(a1, a2), Cmd::LineTo(b1, b2)) => close(a1, b1) && close(a2, b2),
        (Cmd::CubicTo(a1, a2, a3, a4, a5, a6), Cmd::CubicTo(b1, b2, b3, b4, b5, b6)) => {
            close(a1, b1) && close(a2, b2) && close(a3, b3) && close(a4, b4) && close(a5, b5) && close(a6, b6)
        }
        (Cmd::QuadTo(a1, a2, a3, a4), Cmd::QuadTo(b1, b2, b3, b4)) => close(a1, b1) && close(a2, b2) && close(a3, b3) && close(a4, b4),
        (Cmd::ArcTo { rx, ry, large, sweep, x, y }, Cmd::ArcTo { rx: rx2, ry: ry2, large: l2, sweep: s2, x: x2, y: y2 }) => {
            close(rx, rx2) && close(ry, ry2) && large == l2 && sweep == s2 && close(x, x2) && close(y, y2)
        }
        (Cmd::Close, Cmd::Close) => true,
        _ => false,
    }
}

#[test]
fn every_shape_matches_the_web_engine() {
    let golden: Vec<Value> = serde_json::from_str(include_str!("golden/shapes.json")).expect("golden json");
    assert!(golden.len() > 300, "{} records", golden.len());
    let f = |v: &Value| v.as_f64().unwrap_or(f64::NAN);
    let mut checked = 0;
    for rec in &golden {
        let kind = rec["kind"].as_str().unwrap_or_default();
        let (w, h, stroke) = (f(&rec["w"]), f(&rec["h"]), f(&rec["stroke"]));
        let ctx = format!("{kind} {w}x{h} stroke {stroke}");
        let ours = shape_view(kind, w, h, &ViewOptions { adj: None, stroke: stroke > 0.0, stroke_width: stroke });
        match rec["paths"].as_array() {
            None => assert!(ours.is_none(), "{ctx}: the web draws nothing, we do"),
            Some(paths) => {
                let ours = ours.unwrap_or_else(|| panic!("{ctx}: no geometry"));
                assert_eq!(ours.len(), paths.len(), "{ctx}: sub-path count");
                for (o, p) in ours.iter().zip(paths) {
                    let web = parse_svg(p["d"].as_str().unwrap_or_default());
                    let mine = o.path.rounded().cmds;
                    assert_eq!(mine.len(), web.len(), "{ctx}: {} vs {}", o.path.to_svg(), p["d"]);
                    for (a, b) in mine.iter().zip(&web) {
                        assert!(same(a, b), "{ctx}: {a:?} vs {b:?}\n ours {}\n web  {}", o.path.to_svg(), p["d"]);
                    }
                    assert_eq!(o.fill, p["fill"].as_bool().unwrap_or(true), "{ctx}: fill");
                    assert_eq!(o.stroke, p["stroke"].as_bool().unwrap_or(true), "{ctx}: stroke");
                    assert!((o.shade - f(&p["shade"])).abs() < 1e-9, "{ctx}: shade");
                }
            }
        }
        let t = shape_text_box(kind, w, h, None);
        let wt = &rec["text"];
        for (a, b) in [(t.x, f(&wt["x"])), (t.y, f(&wt["y"])), (t.w, f(&wt["w"])), (t.h, f(&wt["h"]))] {
            assert!((a - b).abs() < 1e-6, "{ctx}: text frame {t:?} vs {wt}");
        }
        let knobs = adjust::adjust_handles(kind, kubuno_office_shapes_core::Rect { x: 0.0, y: 0.0, w, h }, None);
        let wk = rec["handles"].as_array().cloned().unwrap_or_default();
        assert_eq!(knobs.len(), wk.len(), "{ctx}: knobs");
        for (k, wk) in knobs.iter().zip(&wk) {
            assert!((k.x - f(&wk["x"])).abs() < 1e-6 && (k.y - f(&wk["y"])).abs() < 1e-6, "{ctx}: knob {k:?} vs {wk}");
        }
        checked += 1;
    }
    assert!(checked > 300);
}
