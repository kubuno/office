//! The background gradient as the gradient picker shows it (CSS, `@ui` `gradientToCss`) and back to the stored
//! `grad` (`{type, angle, stops: [{color, position, opacity}]}`).

use kubuno_office_slides_core::model::{num, Gradient};
use serde_json::{json, Value};

/// `gradientToCss`: stops sorted by position, `rgba(r, g, b, a) p%`.
pub fn to_css(g: &Gradient) -> String {
    let mut stops = g.stops.clone();
    stops.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let list = stops
        .iter()
        .map(|(c, p, o)| {
            let (r, gg, b) = hex_rgb(c);
            format!("rgba({r}, {gg}, {b}, {}) {}%", fmt_alpha(o.clamp(0.0, 100.0) / 100.0), (p * 100.0).round())
        })
        .collect::<Vec<_>>()
        .join(", ");
    if g.radial {
        format!("radial-gradient(circle, {list})")
    } else {
        format!("linear-gradient({}deg, {list})", g.angle.round())
    }
}

fn fmt_alpha(a: f64) -> String {
    format!("{a}")
}

fn hex_rgb(c: &str) -> (u8, u8, u8) {
    let h = c.trim().trim_start_matches('#');
    let h: String = if h.len() == 3 { h.chars().flat_map(|c| [c, c]).collect() } else { h.to_string() };
    let v = |i: usize| h.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok()).unwrap_or(0);
    (v(0), v(2), v(4))
}

/// The stored `grad` of a CSS gradient the picker produced (`None` when it is not one).
pub fn from_css(css: &str) -> Option<Value> {
    let css = css.trim();
    let (radial, inner) = if let Some(r) = css.strip_prefix("linear-gradient(") {
        (false, r.strip_suffix(')')?)
    } else {
        (true, css.strip_prefix("radial-gradient(")?.strip_suffix(')')?)
    };
    let parts = split_top(inner);
    let mut angle = 180.0;
    let mut stops = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let p = p.trim();
        if i == 0 {
            if let Some(a) = p.strip_suffix("deg") {
                angle = a.trim().parse::<f64>().ok()?;
                continue;
            }
            if p == "circle" || p.starts_with("circle") || p.starts_with("to ") {
                continue;
            }
        }
        stops.push(parse_stop(p)?);
    }
    if stops.len() < 2 {
        return None;
    }
    let n = stops.len();
    let stops: Vec<Value> = stops
        .into_iter()
        .enumerate()
        .map(|(i, (c, pos, op))| {
            let pos = pos.unwrap_or(if n > 1 { i as f64 / (n - 1) as f64 } else { 0.0 });
            json!({ "color": c, "position": num(pos), "opacity": num(op) })
        })
        .collect();
    Some(json!({ "type": if radial { "radial" } else { "linear" }, "angle": num(angle), "stops": stops }))
}

/// Splits on commas outside parentheses.
fn split_top(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut depth, mut cur) = (0i32, String::new());
    for c in s.chars() {
        match c {
            '(' => {
                depth += 1;
                cur.push(c)
            }
            ')' => {
                depth -= 1;
                cur.push(c)
            }
            ',' if depth == 0 => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// `rgba(r, g, b, a) p%`, `#rrggbb p%` → (`#rrggbb`, position 0..1, opacity 0..100).
fn parse_stop(s: &str) -> Option<(String, Option<f64>, f64)> {
    let (color, rest) = if s.starts_with("rgb") {
        let end = s.find(')')?;
        (&s[..=end], s[end + 1..].trim())
    } else {
        match s.split_once(char::is_whitespace) {
            Some((c, r)) => (c, r.trim()),
            None => (s, ""),
        }
    };
    let pos = rest.strip_suffix('%').and_then(|p| p.trim().parse::<f64>().ok()).map(|p| (p / 100.0).clamp(0.0, 1.0));
    if let Some(args) = color.strip_prefix("rgba(").or_else(|| color.strip_prefix("rgb(")) {
        let nums: Vec<f64> = args.trim_end_matches(')').split(',').filter_map(|v| v.trim().parse::<f64>().ok()).collect();
        if nums.len() < 3 {
            return None;
        }
        let ch = |v: f64| v.round().clamp(0.0, 255.0) as u8;
        let a = nums.get(3).copied().unwrap_or(1.0).clamp(0.0, 1.0);
        return Some((format!("#{:02x}{:02x}{:02x}", ch(nums[0]), ch(nums[1]), ch(nums[2])), pos, (a * 100.0).round()));
    }
    if color.starts_with('#') {
        let (r, g, b) = hex_rgb(color);
        return Some((format!("#{r:02x}{g:02x}{b:02x}"), pos, 100.0));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_css() {
        let g = Gradient { radial: false, angle: 90.0, stops: vec![("#4a90d9".into(), 0.0, 100.0), ("#9b59b6".into(), 1.0, 50.0)], legacy: false };
        let css = to_css(&g);
        assert_eq!(css, "linear-gradient(90deg, rgba(74, 144, 217, 1) 0%, rgba(155, 89, 182, 0.5) 100%)");
        let v = from_css(&css).unwrap_or_else(|| panic!("parse"));
        assert_eq!(v, json!({ "type": "linear", "angle": 90, "stops": [
            { "color": "#4a90d9", "position": 0, "opacity": 100 },
            { "color": "#9b59b6", "position": 1, "opacity": 50 } ] }));
        let r = from_css("radial-gradient(circle, #ffffff 0%, #000 100%)").unwrap_or_else(|| panic!("radial"));
        assert_eq!(r["type"], "radial");
        assert_eq!(r["stops"][1]["color"], "#000000");
        assert!(from_css("red").is_none());
    }
}
