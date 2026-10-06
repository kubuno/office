//! Word 2010+ text effects and typography, stored in `w:rPr` under the
//! Microsoft `w14` extension namespace (`.../office/word/2010/wordml`).
//!
//! These map onto a single `textEffect` ProseMirror mark whose attributes mirror
//! the frontend contract in `frontend/src/documents/text-effects/model.ts`:
//! `fill`, `outline`, `shadow`, `glow`, `reflection` (visual effects) plus
//! `ligatures`, `numForm`, `numSpacing`, `stylisticSet` (OpenType typography).
//! Every attribute is nullable; `null` means the effect is off, exactly like the
//! editor mark. `read` builds the struct from the XML, `write` serialises it back.

use roxmltree::Node;
use serde_json::{json, Value};

use super::xml::{attr_val, local};

/// EMU per CSS pixel (1 px = 9525 EMU, i.e. 96 dpi).
const EMU_PER_PX: f64 = 9525.0;
/// Word 2010 extension namespace. Elements outside it (e.g. the legacy
/// `w:shadow` toggle, which shares the local name "shadow") must be ignored.
const W14_NS: &str = "http://schemas.microsoft.com/office/word/2010/wordml";

/// True when a node belongs to the w14 extension namespace.
fn is_w14(n: &Node<'_, '_>) -> bool {
    n.tag_name().namespace() == Some(W14_NS)
}

/// First direct child in the w14 namespace with the given local name.
fn w14_child<'a, 'd>(parent: &Node<'a, 'd>, name: &str) -> Option<Node<'a, 'd>> {
    parent
        .children()
        .find(|c| c.is_element() && is_w14(c) && local(c) == name)
}

/// Round a pixel/opacity value to 3 decimals so the JSON stays compact and the
/// EMU round-trip is stable.
fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

// ── Effect value shapes (mirror model.ts interfaces) ─────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Outline {
    pub color: String,
    pub width: f64,
    pub dash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Shadow {
    pub color: String,
    pub blur: f64,
    pub dx: f64,
    pub dy: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Glow {
    pub color: String,
    pub radius: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Reflection {
    pub opacity: f64,
    pub size: f64,
    pub blur: f64,
    pub distance: f64,
}

/// The optional « text effects » subset of a run's character properties. Every
/// field is `None` by default, so a run without any effect stays untouched.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct TextEffects {
    pub fill: Option<String>,
    pub outline: Option<Outline>,
    pub shadow: Option<Shadow>,
    pub glow: Option<Glow>,
    pub reflection: Option<Reflection>,
    pub ligatures: Option<String>,
    pub num_form: Option<String>,
    pub num_spacing: Option<String>,
    pub stylistic_set: Option<i64>,
}

impl TextEffects {
    fn is_empty(&self) -> bool {
        self.fill.is_none()
            && self.outline.is_none()
            && self.shadow.is_none()
            && self.glow.is_none()
            && self.reflection.is_none()
            && self.ligatures.is_none()
            && self.num_form.is_none()
            && self.num_spacing.is_none()
            && self.stylistic_set.is_none()
    }

    // ── Read: `<w:rPr>` → TextEffects ────────────────────────────────────────

    /// Parse the w14 run properties of an `<w:rPr>`. Returns `None` when the run
    /// carries no text effect at all (the common case), so callers add no mark.
    pub(crate) fn parse(rpr: &Node<'_, '_>) -> Option<TextEffects> {
        let mut fx = TextEffects::default();

        // Fill: <w14:textFill><w14:solidFill><w14:srgbClr w14:val="RRGGBB"/>.
        if let Some(node) = w14_child(rpr, "textFill") {
            fx.fill = solid_color(&node);
        }

        // Outline: stroke width in EMU + colour + dash preset.
        if let Some(node) = w14_child(rpr, "textOutline") {
            if let Some(color) = solid_color(&node) {
                let width = attr_val(&node, "w")
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|emu| round3(emu / EMU_PER_PX))
                    .unwrap_or(1.0);
                let dash = w14_child(&node, "prstDash")
                    .and_then(|d| attr_val(&d, "val"))
                    .map(|v| dash_from_ooxml(&v))
                    .unwrap_or("solid")
                    .to_string();
                fx.outline = Some(Outline { color, width, dash });
            }
        }

        // Shadow: dist(EMU) + dir(1/60000°) → dx/dy in px.
        if let Some(node) = w14_child(rpr, "shadow") {
            if let Some(color) = color_child(&node) {
                let dist = attr_val(&node, "dist").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                let dir = attr_val(&node, "dir").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                let blur = attr_val(&node, "blurRad").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                let dist_px = dist / EMU_PER_PX;
                let ang = (dir / 60000.0).to_radians();
                fx.shadow = Some(Shadow {
                    color,
                    blur: round3(blur / EMU_PER_PX),
                    dx: round3(dist_px * ang.cos()),
                    dy: round3(dist_px * ang.sin()),
                });
            }
        }

        // Glow: radius in EMU + colour.
        if let Some(node) = w14_child(rpr, "glow") {
            if let Some(color) = color_child(&node) {
                let radius = attr_val(&node, "rad")
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|emu| round3(emu / EMU_PER_PX))
                    .unwrap_or(0.0);
                fx.glow = Some(Glow { color, radius });
            }
        }

        // Reflection: attribute-only.
        if let Some(node) = w14_child(rpr, "reflection") {
            let attr = |k: &str| attr_val(&node, k).and_then(|v| v.parse::<f64>().ok());
            fx.reflection = Some(Reflection {
                opacity: round3(attr("stA").unwrap_or(50000.0) / 100000.0),
                size: round3(attr("endPos").unwrap_or(50000.0) / 100000.0),
                blur: round3(attr("blurRad").unwrap_or(0.0) / EMU_PER_PX),
                distance: round3(attr("dist").unwrap_or(0.0) / EMU_PER_PX),
            });
        }

        // Typography: plain string / int values.
        fx.ligatures = w14_child(rpr, "ligatures").and_then(|n| attr_val(&n, "val"));
        fx.num_form = w14_child(rpr, "numForm").and_then(|n| attr_val(&n, "val"));
        fx.num_spacing = w14_child(rpr, "numSpacing").and_then(|n| attr_val(&n, "val"));
        fx.stylistic_set = w14_child(rpr, "stylisticSets")
            .and_then(|sets| w14_child(&sets, "styleSet"))
            .and_then(|s| attr_val(&s, "id"))
            .and_then(|v| v.parse::<i64>().ok());

        if fx.is_empty() {
            None
        } else {
            Some(fx)
        }
    }

    /// Attributes of the `textEffect` mark (every key present, `null` when off).
    pub(crate) fn to_mark_attrs(&self) -> Value {
        json!({
            "fill": self.fill,
            "outline": self.outline.as_ref().map(|o| json!({
                "color": o.color, "width": o.width, "dash": o.dash,
            })),
            "shadow": self.shadow.as_ref().map(|s| json!({
                "color": s.color, "blur": s.blur, "dx": s.dx, "dy": s.dy,
            })),
            "glow": self.glow.as_ref().map(|g| json!({
                "color": g.color, "radius": g.radius,
            })),
            "reflection": self.reflection.as_ref().map(|r| json!({
                "opacity": r.opacity, "size": r.size, "blur": r.blur, "distance": r.distance,
            })),
            "ligatures": self.ligatures,
            "numForm": self.num_form,
            "numSpacing": self.num_spacing,
            "stylisticSet": self.stylistic_set,
        })
    }

    // ── Write: `textEffect` mark → TextEffects → `<w:rPr>` children ───────────

    /// Rebuild the effects from a `textEffect` mark's attributes.
    pub(crate) fn from_mark_attrs(attrs: &Value) -> Option<TextEffects> {
        let get = |k: &str| attrs.get(k).filter(|v| !v.is_null());
        let num = |v: &Value, k: &str| v.get(k).and_then(Value::as_f64);
        let mut fx = TextEffects {
            fill: get("fill").and_then(Value::as_str).map(str::to_string),
            ligatures: get("ligatures").and_then(Value::as_str).map(str::to_string),
            num_form: get("numForm").and_then(Value::as_str).map(str::to_string),
            num_spacing: get("numSpacing").and_then(Value::as_str).map(str::to_string),
            stylistic_set: get("stylisticSet").and_then(Value::as_i64),
            ..TextEffects::default()
        };
        if let Some(o) = get("outline") {
            if let Some(color) = o.get("color").and_then(Value::as_str) {
                fx.outline = Some(Outline {
                    color: color.to_string(),
                    width: num(o, "width").unwrap_or(1.0),
                    dash: o.get("dash").and_then(Value::as_str).unwrap_or("solid").to_string(),
                });
            }
        }
        if let Some(s) = get("shadow") {
            if let Some(color) = s.get("color").and_then(Value::as_str) {
                fx.shadow = Some(Shadow {
                    color: color.to_string(),
                    blur: num(s, "blur").unwrap_or(0.0),
                    dx: num(s, "dx").unwrap_or(0.0),
                    dy: num(s, "dy").unwrap_or(0.0),
                });
            }
        }
        if let Some(g) = get("glow") {
            if let Some(color) = g.get("color").and_then(Value::as_str) {
                fx.glow = Some(Glow { color: color.to_string(), radius: num(g, "radius").unwrap_or(0.0) });
            }
        }
        if let Some(r) = get("reflection") {
            fx.reflection = Some(Reflection {
                opacity: num(r, "opacity").unwrap_or(0.5),
                size: num(r, "size").unwrap_or(0.5),
                blur: num(r, "blur").unwrap_or(0.0),
                distance: num(r, "distance").unwrap_or(0.0),
            });
        }
        if fx.is_empty() {
            None
        } else {
            Some(fx)
        }
    }

    /// The w14 `<w:rPr>` children for these effects, in the CT_RPr extension
    /// order (glow, shadow, reflection, textOutline, textFill, then typography).
    pub(crate) fn to_rpr_xml(&self) -> String {
        let mut out = String::new();
        if let Some(g) = &self.glow {
            if let Some(clr) = srgb_xml(&g.color) {
                out.push_str(&format!(
                    r#"<w14:glow w14:rad="{}">{clr}</w14:glow>"#,
                    px_to_emu(g.radius)
                ));
            }
        }
        if let Some(s) = &self.shadow {
            if let Some(clr) = srgb_xml(&s.color) {
                let dist = (s.dx.hypot(s.dy) * EMU_PER_PX).round() as i64;
                // atan2 returns (-180°,180°]; w14:dir is an unsigned 1/60000°.
                let mut deg = s.dy.atan2(s.dx).to_degrees();
                if deg < 0.0 {
                    deg += 360.0;
                }
                let dir = (deg * 60000.0).round() as i64;
                out.push_str(&format!(
                    r#"<w14:shadow w14:blurRad="{}" w14:dist="{dist}" w14:dir="{dir}" w14:sx="100000" w14:sy="100000" w14:kx="0" w14:ky="0" w14:algn="tl">{clr}</w14:shadow>"#,
                    px_to_emu(s.blur)
                ));
            }
        }
        if let Some(r) = &self.reflection {
            out.push_str(&format!(
                r#"<w14:reflection w14:blurRad="{}" w14:stA="{}" w14:stPos="0" w14:endA="0" w14:endPos="{}" w14:dist="{}" w14:dir="5400000" w14:fadeDir="5400000" w14:sx="100000" w14:sy="-100000" w14:kx="0" w14:ky="0" w14:algn="bl"/>"#,
                px_to_emu(r.blur),
                (r.opacity.clamp(0.0, 1.0) * 100000.0).round() as i64,
                (r.size.clamp(0.0, 1.0) * 100000.0).round() as i64,
                px_to_emu(r.distance),
            ));
        }
        if let Some(o) = &self.outline {
            if let Some(clr) = srgb_xml(&o.color) {
                out.push_str(&format!(
                    r#"<w14:textOutline w14:w="{}" w14:cap="rnd" w14:cmpd="sng" w14:algn="ctr"><w14:solidFill>{clr}</w14:solidFill><w14:prstDash w14:val="{}"/><w14:round/></w14:textOutline>"#,
                    px_to_emu(o.width),
                    dash_to_ooxml(&o.dash),
                ));
            }
        }
        if let Some(fill) = &self.fill {
            if let Some(clr) = srgb_xml(fill) {
                out.push_str(&format!(
                    r#"<w14:textFill><w14:solidFill>{clr}</w14:solidFill></w14:textFill>"#
                ));
            }
        }
        if let Some(v) = &self.ligatures {
            out.push_str(&format!(r#"<w14:ligatures w14:val="{}"/>"#, sanitize_token(v)));
        }
        if let Some(v) = &self.num_form {
            out.push_str(&format!(r#"<w14:numForm w14:val="{}"/>"#, sanitize_token(v)));
        }
        if let Some(v) = &self.num_spacing {
            out.push_str(&format!(r#"<w14:numSpacing w14:val="{}"/>"#, sanitize_token(v)));
        }
        if let Some(n) = self.stylistic_set {
            if (1..=20).contains(&n) {
                out.push_str(&format!(
                    r#"<w14:stylisticSets><w14:styleSet w14:id="{n}"/></w14:stylisticSets>"#
                ));
            }
        }
        out
    }
}

/// px → EMU, rounded to a whole EMU (Word never emits fractional EMU here).
fn px_to_emu(px: f64) -> i64 {
    (px * EMU_PER_PX).round() as i64
}

/// A `<w14:srgbClr>` descendant colour of a `solidFill`-wrapped element (fill /
/// outline), as a CSS colour string.
fn solid_color(node: &Node<'_, '_>) -> Option<String> {
    node.descendants()
        .find(|n| n.is_element() && is_w14(n) && local(n) == "srgbClr")
        .and_then(|clr| srgb_color(&clr))
}

/// A `<w14:srgbClr>` colour that is a DIRECT child (shadow / glow put the colour
/// straight inside, without a `solidFill` wrapper).
fn color_child(node: &Node<'_, '_>) -> Option<String> {
    w14_child(node, "srgbClr").and_then(|clr| srgb_color(&clr))
}

/// `<w14:srgbClr w14:val="RRGGBB">` (+ optional `<w14:alpha>`) → CSS colour:
/// `#rrggbb`, or `rgba(r,g,b,a)` when the alpha is below 100 %.
fn srgb_color(clr: &Node<'_, '_>) -> Option<String> {
    let val = attr_val(clr, "val")?;
    let h = val.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let alpha = w14_child(clr, "alpha")
        .and_then(|a| attr_val(&a, "val"))
        .and_then(|v| v.parse::<f64>().ok());
    match alpha {
        Some(a) if a < 100000.0 => {
            let comp = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0);
            Some(format!(
                "rgba({},{},{},{})",
                comp(0),
                comp(2),
                comp(4),
                round3(a / 100000.0)
            ))
        }
        _ => Some(format!("#{}", h.to_ascii_lowercase())),
    }
}

/// A `<w14:srgbClr>` element (with `<w14:alpha>` when the colour is translucent)
/// for a CSS colour string, or `None` when it cannot be represented.
fn srgb_xml(css: &str) -> Option<String> {
    let (hex, alpha) = parse_css_color(css)?;
    Some(match alpha {
        Some(a) if a < 100000 => {
            format!(r#"<w14:srgbClr w14:val="{hex}"><w14:alpha w14:val="{a}"/></w14:srgbClr>"#)
        }
        _ => format!(r#"<w14:srgbClr w14:val="{hex}"/>"#),
    })
}

/// CSS colour → `RRGGBB` (upper case) + optional alpha in the w14 1/1000 %
/// scale (0..100000). Accepts `#rgb`, `#rrggbb`, `rgb(...)`, `rgba(...)`.
fn parse_css_color(css: &str) -> Option<(String, Option<i64>)> {
    let s = css.trim();
    if let Some(h) = s.strip_prefix('#') {
        let d: String = h.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        let hex = match d.len() {
            3 | 4 => d.chars().take(3).flat_map(|c| [c, c]).collect::<String>(),
            6 | 8 => d[..6].to_string(),
            _ => return None,
        };
        return Some((hex.to_ascii_uppercase(), None));
    }
    let inner = s
        .strip_prefix("rgba(")
        .or_else(|| s.strip_prefix("rgb("))
        .and_then(|v| v.strip_suffix(')'))?;
    let parts: Vec<f64> = inner
        .split([',', '/', ' '])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse::<f64>().ok())
        .collect();
    let cl = |v: f64| v.clamp(0.0, 255.0).round() as u8;
    let hex = format!("{:02X}{:02X}{:02X}", cl(*parts.first()?), cl(*parts.get(1)?), cl(*parts.get(2)?));
    let alpha = parts.get(3).map(|a| (a.clamp(0.0, 1.0) * 100000.0).round() as i64);
    Some((hex, alpha))
}

/// OOXML `prstDash` value → the model's 4-value dash enum.
fn dash_from_ooxml(v: &str) -> &'static str {
    match v {
        "dot" | "sysDot" => "dot",
        "dash" | "lgDash" | "sysDash" => "dash",
        "dashDot" | "lgDashDot" | "sysDashDot" | "lgDashDotDot" | "sysDashDotDot" => "dashDot",
        _ => "solid",
    }
}

/// Model dash enum → OOXML `prstDash` value.
fn dash_to_ooxml(v: &str) -> &'static str {
    match v {
        "dot" => "dot",
        "dash" => "dash",
        "dashDot" => "dashDot",
        _ => "solid",
    }
}

/// Keep only the ASCII letters of a typography token so it is always a safe
/// XML attribute value (the enum values are camelCase words).
fn sanitize_token(v: &str) -> String {
    v.chars().filter(|c| c.is_ascii_alphabetic()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_rpr(rpr_inner: &str) -> Option<TextEffects> {
        let xml = format!(
            r#"<w:rPr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                     xmlns:w14="{W14_NS}">{rpr_inner}</w:rPr>"#
        );
        let doc = roxmltree::Document::parse(&xml).expect("xml");
        let rpr = doc.root_element();
        TextEffects::parse(&rpr)
    }

    #[test]
    fn reads_fill_outline_and_typography() {
        let fx = parse_rpr(
            r#"<w14:textFill><w14:solidFill><w14:srgbClr w14:val="FF0000"/></w14:solidFill></w14:textFill>
               <w14:textOutline w14:w="19050"><w14:solidFill><w14:srgbClr w14:val="00FF00"/></w14:solidFill><w14:prstDash w14:val="sysDash"/></w14:textOutline>
               <w14:ligatures w14:val="all"/>
               <w14:numForm w14:val="oldStyle"/>
               <w14:numSpacing w14:val="tabular"/>
               <w14:stylisticSets><w14:styleSet w14:id="4"/></w14:stylisticSets>"#,
        )
        .expect("effects");
        assert_eq!(fx.fill.as_deref(), Some("#ff0000"));
        let o = fx.outline.expect("outline");
        assert_eq!(o.color, "#00ff00");
        assert_eq!(o.width, 2.0); // 19050 EMU / 9525
        assert_eq!(o.dash, "dash");
        assert_eq!(fx.ligatures.as_deref(), Some("all"));
        assert_eq!(fx.num_form.as_deref(), Some("oldStyle"));
        assert_eq!(fx.num_spacing.as_deref(), Some("tabular"));
        assert_eq!(fx.stylistic_set, Some(4));
    }

    #[test]
    fn shadow_dist_dir_to_dx_dy() {
        // dir=2700000 → 45°, dist=134679 EMU ≈ 14.14 px hypot → dx=dy≈10 px.
        let fx = parse_rpr(
            r#"<w14:shadow w14:blurRad="38100" w14:dist="134679" w14:dir="2700000"><w14:srgbClr w14:val="000000"><w14:alpha w14:val="60000"/></w14:srgbClr></w14:shadow>"#,
        )
        .expect("effects");
        let s = fx.shadow.expect("shadow");
        assert_eq!(s.color, "rgba(0,0,0,0.6)");
        assert_eq!(s.blur, 4.0);
        assert!((s.dx - 10.0).abs() < 0.05, "dx={}", s.dx);
        assert!((s.dy - 10.0).abs() < 0.05, "dy={}", s.dy);
    }

    #[test]
    fn legacy_w_shadow_toggle_is_ignored() {
        // `w:shadow` (no namespace prefix here → the w namespace) is the legacy
        // boolean text shadow, NOT the w14 rich shadow.
        assert!(parse_rpr(r#"<w:shadow/>"#).is_none());
    }

    #[test]
    fn round_trip_all_effects() {
        let before = TextEffects {
            fill: Some("#123456".into()),
            outline: Some(Outline { color: "#abcdef".into(), width: 1.5, dash: "dashDot".into() }),
            shadow: Some(Shadow { color: "rgba(0,0,0,0.5)".into(), blur: 3.0, dx: 2.0, dy: 2.0 }),
            glow: Some(Glow { color: "#00b0f0".into(), radius: 6.0 }),
            reflection: Some(Reflection { opacity: 0.5, size: 0.5, blur: 1.0, distance: 4.0 }),
            ligatures: Some("standardContextual".into()),
            num_form: Some("lining".into()),
            num_spacing: Some("proportional".into()),
            stylistic_set: Some(3),
        };
        let xml = before.to_rpr_xml();
        let after = parse_rpr(&xml).expect("re-read");
        assert_eq!(after.fill, before.fill);
        assert_eq!(after.outline, before.outline);
        let s = after.shadow.expect("shadow");
        assert_eq!(s.color, "rgba(0,0,0,0.5)");
        assert!((s.dx - 2.0).abs() < 0.05 && (s.dy - 2.0).abs() < 0.05);
        assert_eq!(after.glow, before.glow);
        assert_eq!(after.reflection, before.reflection);
        assert_eq!(after.ligatures, before.ligatures);
        assert_eq!(after.num_form, before.num_form);
        assert_eq!(after.num_spacing, before.num_spacing);
        assert_eq!(after.stylistic_set, before.stylistic_set);
    }

    #[test]
    fn mark_attrs_round_trip() {
        let fx = TextEffects {
            fill: Some("#112233".into()),
            glow: Some(Glow { color: "#445566".into(), radius: 8.0 }),
            stylistic_set: Some(2),
            ..TextEffects::default()
        };
        let attrs = fx.to_mark_attrs();
        // Absent effects are explicit nulls, matching the frontend contract.
        assert!(attrs.get("shadow").expect("key").is_null());
        assert_eq!(attrs.get("stylisticSet").and_then(Value::as_i64), Some(2));
        let back = TextEffects::from_mark_attrs(&attrs).expect("effects");
        assert_eq!(back, fx);
    }
}
