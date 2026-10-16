//! Chart palettes and report themes, a port of `web/src/data/palettes.ts`. Themes mirror Power BI's
//! built-in report themes; palettes drive every visual. The diverging [`color_scale`] backs the heatmap
//! and the data table's conditional formatting.

/// A named chart palette (`ChartPalette`).
#[derive(Debug, Clone, Copy)]
pub struct ChartPalette {
    pub id: &'static str,
    pub name: &'static str,
    pub colors: &'static [&'static str],
}

/// The 10 palettes offered in the Format pane, in editor order (`PALETTES`).
pub const PALETTES: &[ChartPalette] = &[
    ChartPalette { id: "kubuno", name: "Kubuno", colors: &["#1a73e8", "#ea4335", "#fbbc04", "#34a853", "#ff6d00", "#a142f4", "#0097a7", "#e91e63"] },
    ChartPalette { id: "classic", name: "Classique", colors: &["#118dff", "#12239e", "#e66c37", "#6b007b", "#e044a7", "#744ec2", "#d9b300", "#d64550"] },
    ChartPalette { id: "vibrant", name: "Vibrant", colors: &["#f94144", "#f3722c", "#f8961e", "#f9c74f", "#90be6d", "#43aa8b", "#577590", "#277da1"] },
    ChartPalette { id: "cool", name: "Froid", colors: &["#0d3b66", "#1d70a2", "#3895d3", "#58cced", "#7ee8fa", "#80ffdb", "#64dfdf", "#48bfe3"] },
    ChartPalette { id: "warm", name: "Chaud", colors: &["#7f1d1d", "#b91c1c", "#dc2626", "#ef4444", "#f97316", "#fb923c", "#fbbf24", "#fde047"] },
    ChartPalette { id: "pastel", name: "Pastel", colors: &["#a7c7e7", "#c1e1c1", "#fdfd96", "#ffb7b2", "#e0bbe4", "#b5ead7", "#ffdac1", "#c7ceea"] },
    ChartPalette { id: "mono_blue", name: "Bleu mono", colors: &["#08306b", "#08519c", "#2171b5", "#4292c6", "#6baed6", "#9ecae1", "#c6dbef", "#deebf7"] },
    ChartPalette { id: "earth", name: "Terre", colors: &["#582f0e", "#7f4f24", "#936639", "#a68a64", "#b6ad90", "#a4ac86", "#656d4a", "#414833"] },
    ChartPalette { id: "colorblind", name: "Daltonien", colors: &["#1170aa", "#fc7d0b", "#a3acb9", "#57606c", "#5fa2ce", "#c85200", "#7b848f", "#a3cce9"] },
    ChartPalette { id: "grayscale", name: "Niveaux gris", colors: &["#1f2937", "#374151", "#4b5563", "#6b7280", "#9ca3af", "#d1d5db", "#e5e7eb", "#f3f4f6"] },
];

/// `paletteById` - the palette's colours, falling back to the first palette (`kubuno`).
pub fn palette_by_id(id: Option<&str>) -> &'static [&'static str] {
    id.and_then(|id| PALETTES.iter().find(|p| p.id == id))
        .unwrap_or(&PALETTES[0])
        .colors
}

/// A report theme (`ReportTheme` in `palettes.ts`, distinct from the server's free-form `report.theme`).
#[derive(Debug, Clone, Copy)]
pub struct ReportTheme {
    pub id: &'static str,
    pub name: &'static str,
    pub primary_color: &'static str,
    pub font_family: &'static str,
    pub background: &'static str,
    pub page_background: &'static str,
    pub card_background: &'static str,
    pub foreground: &'static str,
    pub palette_id: &'static str,
}

impl ReportTheme {
    /// The theme's chart palette (`chartPalette`), resolved from `palette_id`.
    pub fn chart_palette(&self) -> &'static [&'static str] {
        palette_by_id(Some(self.palette_id))
    }
}

/// The 7 built-in report themes, in editor order (`REPORT_THEMES`).
pub const REPORT_THEMES: &[ReportTheme] = &[
    ReportTheme { id: "default", name: "Par défaut", primary_color: "#1a73e8", font_family: "Inter, system-ui, sans-serif", background: "#f8f9fa", page_background: "#ffffff", card_background: "#ffffff", foreground: "#202124", palette_id: "kubuno" },
    ReportTheme { id: "executive", name: "Direction", primary_color: "#12239e", font_family: "Georgia, serif", background: "#eef1f6", page_background: "#ffffff", card_background: "#ffffff", foreground: "#1a1a2e", palette_id: "classic" },
    ReportTheme { id: "dark", name: "Sombre", primary_color: "#58cced", font_family: "Inter, system-ui, sans-serif", background: "#1a1d24", page_background: "#23272f", card_background: "#2b303b", foreground: "#e8eaed", palette_id: "cool" },
    ReportTheme { id: "sunset", name: "Coucher", primary_color: "#e66c37", font_family: "Inter, system-ui, sans-serif", background: "#fff7f0", page_background: "#ffffff", card_background: "#fffaf5", foreground: "#3d2c20", palette_id: "warm" },
    ReportTheme { id: "mint", name: "Menthe", primary_color: "#34a853", font_family: "Inter, system-ui, sans-serif", background: "#f0faf4", page_background: "#ffffff", card_background: "#ffffff", foreground: "#163a26", palette_id: "vibrant" },
    ReportTheme { id: "highcontrast", name: "Contraste élevé", primary_color: "#000000", font_family: "Inter, system-ui, sans-serif", background: "#ffffff", page_background: "#ffffff", card_background: "#ffffff", foreground: "#000000", palette_id: "colorblind" },
    ReportTheme { id: "accessible", name: "Accessible", primary_color: "#1170aa", font_family: "Inter, system-ui, sans-serif", background: "#f5f5f5", page_background: "#ffffff", card_background: "#ffffff", foreground: "#1a1a1a", palette_id: "colorblind" },
];

/// `themeById` - the theme, falling back to the first (`default`).
pub fn theme_by_id(id: Option<&str>) -> &'static ReportTheme {
    id.and_then(|id| REPORT_THEMES.iter().find(|t| t.id == id))
        .unwrap_or(&REPORT_THEMES[0])
}

fn hex(c: &str) -> (i32, i32, i32) {
    let p = |a: usize, b: usize| i32::from_str_radix(&c[a..b], 16).unwrap_or(0);
    (p(1, 3), p(3, 5), p(5, 7))
}

/// `colorScale` - a diverging scale `lo → mid → hi` for `t` in `0..=1`, returned as `rgb(r,g,b)`.
pub fn color_scale(t: f64, lo: &str, mid: &str, hi: &str) -> String {
    let clamp = t.clamp(0.0, 1.0);
    let lerp = |a: i32, b: i32, x: f64| (a as f64 + (b - a) as f64 * x).round() as i32;
    let (a, b) = if clamp < 0.5 { (hex(lo), hex(mid)) } else { (hex(mid), hex(hi)) };
    let x = if clamp < 0.5 { clamp * 2.0 } else { (clamp - 0.5) * 2.0 };
    format!("rgb({},{},{})", lerp(a.0, b.0, x), lerp(a.1, b.1, x), lerp(a.2, b.2, x))
}

/// The default diverging scale used by the heatmap and conditional formatting.
pub fn color_scale_default(t: f64) -> String {
    color_scale(t, "#fce8e6", "#fff3cd", "#e6f4ea")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_lookup_and_fallback() {
        assert_eq!(palette_by_id(Some("vibrant"))[0], "#f94144");
        assert_eq!(palette_by_id(Some("nope")), PALETTES[0].colors);
        assert_eq!(palette_by_id(None), PALETTES[0].colors);
        assert_eq!(PALETTES.len(), 10);
    }

    #[test]
    fn theme_lookup_and_palette() {
        assert_eq!(theme_by_id(Some("dark")).primary_color, "#58cced");
        assert_eq!(theme_by_id(Some("dark")).chart_palette(), palette_by_id(Some("cool")));
        assert_eq!(theme_by_id(None).id, "default");
        assert_eq!(REPORT_THEMES.len(), 7);
    }

    #[test]
    fn color_scale_endpoints_and_mid() {
        assert_eq!(color_scale_default(0.0), "rgb(252,232,230)");
        assert_eq!(color_scale_default(1.0), "rgb(230,244,234)");
        assert_eq!(color_scale_default(0.5), "rgb(255,243,205)");
    }
}
