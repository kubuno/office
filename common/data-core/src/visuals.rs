//! The visual catalogue, a port of `VISUALS` in `web/src/data/visuals.tsx` (the type/label/category of
//! every visual the editor offers) plus the static and slicer sets `renderVisual` uses. Icons live in the
//! platform layer, not here.

/// A visual's category in the picker (`VisualCategory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualCategory {
    Cards,
    Bars,
    Lines,
    Parts,
    Tables,
    Distribution,
    Filters,
    Other,
}

impl VisualCategory {
    /// The stable string key (`cards`, `bars`, …).
    pub fn as_str(self) -> &'static str {
        match self {
            VisualCategory::Cards => "cards",
            VisualCategory::Bars => "bars",
            VisualCategory::Lines => "lines",
            VisualCategory::Parts => "parts",
            VisualCategory::Tables => "tables",
            VisualCategory::Distribution => "distribution",
            VisualCategory::Filters => "filters",
            VisualCategory::Other => "other",
        }
    }
}

/// A visual definition (`VisualDef`), without the React icon.
#[derive(Debug, Clone, Copy)]
pub struct VisualDef {
    pub visual_type: &'static str,
    pub label: &'static str,
    pub category: VisualCategory,
}

use VisualCategory::*;

/// The catalogue, in editor order (`VISUALS`).
pub const VISUALS: &[VisualDef] = &[
    VisualDef { visual_type: "card", label: "Carte", category: Cards },
    VisualDef { visual_type: "kpi_card", label: "Indicateur (KPI)", category: Cards },
    VisualDef { visual_type: "multi_row_card", label: "Carte multi-lignes", category: Cards },
    VisualDef { visual_type: "gauge", label: "Jauge", category: Cards },
    VisualDef { visual_type: "bullet", label: "Jauge linéaire", category: Cards },
    VisualDef { visual_type: "kpi_grid", label: "Grille de KPI", category: Cards },
    VisualDef { visual_type: "bar_chart", label: "Histogramme groupé", category: Bars },
    VisualDef { visual_type: "column_h", label: "Barres horizontales", category: Bars },
    VisualDef { visual_type: "stacked_bar", label: "Histogramme empilé", category: Bars },
    VisualDef { visual_type: "stacked_bar_100", label: "Empilé 100 %", category: Bars },
    VisualDef { visual_type: "ribbon_chart", label: "Graphique ruban", category: Bars },
    VisualDef { visual_type: "line_chart", label: "Courbe", category: Lines },
    VisualDef { visual_type: "smooth_line", label: "Courbe lissée", category: Lines },
    VisualDef { visual_type: "step_line", label: "Courbe en escalier", category: Lines },
    VisualDef { visual_type: "area_chart", label: "Aires", category: Lines },
    VisualDef { visual_type: "stacked_area", label: "Aires empilées", category: Lines },
    VisualDef { visual_type: "combo_chart", label: "Histogramme + courbe", category: Lines },
    VisualDef { visual_type: "sparkline", label: "Sparkline", category: Lines },
    VisualDef { visual_type: "pie_chart", label: "Secteurs", category: Parts },
    VisualDef { visual_type: "donut_chart", label: "Anneau", category: Parts },
    VisualDef { visual_type: "funnel", label: "Entonnoir", category: Parts },
    VisualDef { visual_type: "waterfall", label: "Cascade", category: Parts },
    VisualDef { visual_type: "treemap", label: "Compartimentage", category: Parts },
    VisualDef { visual_type: "data_table", label: "Table", category: Tables },
    VisualDef { visual_type: "matrix", label: "Matrice (TCD)", category: Tables },
    VisualDef { visual_type: "scatter_chart", label: "Nuage de points", category: Distribution },
    VisualDef { visual_type: "bubble_chart", label: "Bulles", category: Distribution },
    VisualDef { visual_type: "histogram", label: "Distribution", category: Distribution },
    VisualDef { visual_type: "box_plot", label: "Boîte à moustaches", category: Distribution },
    VisualDef { visual_type: "radar_chart", label: "Radar", category: Distribution },
    VisualDef { visual_type: "heatmap", label: "Carte de chaleur", category: Distribution },
    VisualDef { visual_type: "slicer", label: "Segment (liste)", category: Filters },
    VisualDef { visual_type: "slicer_dropdown", label: "Segment (menu)", category: Filters },
    VisualDef { visual_type: "slicer_range", label: "Segment numérique", category: Filters },
    VisualDef { visual_type: "text", label: "Zone de texte", category: Other },
    VisualDef { visual_type: "image", label: "Image", category: Other },
    VisualDef { visual_type: "shape", label: "Forme", category: Other },
    VisualDef { visual_type: "progress_ring", label: "Anneau de progression", category: Other },
];

/// `visualDef` — the definition for a type, if the catalogue knows it. `scorecard` is an alias of
/// `kpi_card` at render time; it is not a catalogue entry (matching the web).
pub fn visual_def(visual_type: &str) -> Option<&'static VisualDef> {
    VISUALS.iter().find(|v| v.visual_type == visual_type)
}

/// `STATIC_VISUALS` — visuals that render without a dataset/query.
pub fn is_static(visual_type: &str) -> bool {
    matches!(visual_type, "text" | "image" | "shape")
}

/// `SLICER_VISUALS` — visuals that act as filters (raw distinct values, not an aggregated query).
pub fn is_slicer(visual_type: &str) -> bool {
    matches!(
        visual_type,
        "slicer" | "slicer_dropdown" | "slicer_range" | "filter_date" | "filter_dropdown"
    )
}

/// True when the widget's data is fetched through `executeApi.query` (not static, not a slicer).
pub fn needs_query(visual_type: &str) -> bool {
    !is_static(visual_type) && !is_slicer(visual_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_size_and_lookup() {
        assert_eq!(VISUALS.len(), 38);
        assert_eq!(visual_def("treemap").unwrap().category, Parts);
        assert!(visual_def("scorecard").is_none()); // alias only, not a catalogue entry
    }

    #[test]
    fn static_and_slicer_classification() {
        assert!(is_static("image"));
        assert!(!is_static("bar_chart"));
        assert!(is_slicer("slicer_range"));
        assert!(needs_query("bar_chart"));
        assert!(!needs_query("text"));
        assert!(!needs_query("slicer"));
    }
}
