//! Per-visual field wells and the aggregation/filter vocabularies, a port of `web/src/data/wells.ts`.
//! A "well" says where a dragged or clicked field lands in a visual's config (axis, legend, values, size…),
//! mirroring Power BI's field buckets.

/// Which config bucket a field lands in (`WellId`). Several wells can share an id (`values`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WellId {
    Axis,
    Legend,
    Values,
    Size,
    Tooltips,
    Category,
}

impl WellId {
    /// The stable string the config stores (matches the web `WellId` union).
    pub fn as_str(self) -> &'static str {
        match self {
            WellId::Axis => "axis",
            WellId::Legend => "legend",
            WellId::Values => "values",
            WellId::Size => "size",
            WellId::Tooltips => "tooltips",
            WellId::Category => "category",
        }
    }
}

/// The kind of field a well accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WellKind {
    Dimension,
    Metric,
}

/// A field well (`Well`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Well {
    pub id: WellId,
    pub label: &'static str,
    pub multi: bool,
    pub kind: WellKind,
}

const AXIS: Well = Well { id: WellId::Axis, label: "Axe", multi: false, kind: WellKind::Dimension };
const CATEGORY: Well = Well { id: WellId::Category, label: "Catégorie", multi: false, kind: WellKind::Dimension };
const LEGEND: Well = Well { id: WellId::Legend, label: "Légende", multi: false, kind: WellKind::Dimension };
const VALUES: Well = Well { id: WellId::Values, label: "Valeurs", multi: true, kind: WellKind::Metric };
const VALUE1: Well = Well { id: WellId::Values, label: "Valeur", multi: false, kind: WellKind::Metric };
const SIZE: Well = Well { id: WellId::Size, label: "Taille", multi: false, kind: WellKind::Metric };

/// `wellsFor` - the wells a visual type exposes, in editor order.
pub fn wells_for(visual_type: &str) -> Vec<Well> {
    match visual_type {
        "card" | "gauge" | "bullet" | "progress_ring" => vec![VALUE1],
        "kpi_card" | "scorecard" => vec![
            VALUE1,
            Well { id: WellId::Tooltips, label: "Objectif / comparaison", multi: false, kind: WellKind::Metric },
        ],
        "multi_row_card" | "kpi_grid" | "funnel" | "pie_chart" | "donut_chart" | "treemap" => vec![CATEGORY, VALUE1],
        "bar_chart" | "column_h" | "line_chart" | "smooth_line" | "step_line" | "area_chart" | "waterfall"
        | "sparkline" => vec![AXIS, LEGEND, VALUES],
        "stacked_bar" | "stacked_bar_100" | "stacked_area" | "ribbon_chart" | "heatmap" | "matrix" => {
            vec![AXIS, LEGEND, VALUE1]
        }
        "combo_chart" => vec![
            AXIS,
            Well { id: WellId::Values, label: "Colonnes puis courbe", multi: true, kind: WellKind::Metric },
        ],
        "scatter_chart" => vec![
            Well { id: WellId::Axis, label: "X", multi: false, kind: WellKind::Metric },
            Well { id: WellId::Values, label: "Y", multi: false, kind: WellKind::Metric },
        ],
        "bubble_chart" => vec![
            Well { id: WellId::Axis, label: "X", multi: false, kind: WellKind::Metric },
            Well { id: WellId::Values, label: "Y", multi: false, kind: WellKind::Metric },
            SIZE,
        ],
        "histogram" | "box_plot" => vec![CATEGORY, VALUE1],
        "radar_chart" => vec![CATEGORY, VALUES],
        "data_table" => vec![
            Well { id: WellId::Axis, label: "Colonnes", multi: true, kind: WellKind::Dimension },
            VALUES,
        ],
        "slicer" | "slicer_dropdown" | "slicer_range" => {
            vec![Well { id: WellId::Axis, label: "Champ", multi: false, kind: WellKind::Dimension }]
        }
        _ => vec![AXIS, VALUES],
    }
}

/// Aggregation functions offered on a metric chip (`AGG_FUNCTIONS`): `(value, label)`.
pub const AGG_FUNCTIONS: &[(&str, &str)] = &[
    ("SUM", "Somme"),
    ("AVG", "Moyenne"),
    ("COUNT", "Nombre"),
    ("COUNT_DISTINCT", "Nombre distinct"),
    ("MIN", "Minimum"),
    ("MAX", "Maximum"),
];

/// Filter operators (`FILTER_OPERATORS`): `(value, label)`.
pub const FILTER_OPERATORS: &[(&str, &str)] = &[
    ("eq", "est égal à"),
    ("neq", "est différent de"),
    ("gt", "supérieur à"),
    ("gte", "supérieur ou égal"),
    ("lt", "inférieur à"),
    ("lte", "inférieur ou égal"),
    ("like", "contient"),
    ("in", "fait partie de"),
    ("is_null", "est vide"),
    ("is_not_null", "n'est pas vide"),
];

/// `is_null`/`is_not_null` take no value (the editor hides the value input).
pub fn operator_takes_value(op: &str) -> bool {
    !matches!(op, "is_null" | "is_not_null")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wells_match_the_web() {
        assert_eq!(wells_for("gauge"), vec![VALUE1]);
        assert_eq!(wells_for("kpi_card").len(), 2);
        assert_eq!(wells_for("bar_chart"), vec![AXIS, LEGEND, VALUES]);
        assert_eq!(wells_for("scatter_chart").len(), 2);
        assert_eq!(wells_for("bubble_chart").len(), 3);
        // Unknown type -> default well set.
        assert_eq!(wells_for("mystery"), vec![AXIS, VALUES]);
    }

    #[test]
    fn vocabularies() {
        assert_eq!(AGG_FUNCTIONS.len(), 6);
        assert_eq!(FILTER_OPERATORS.len(), 10);
        assert!(operator_takes_value("eq"));
        assert!(!operator_takes_value("is_null"));
    }
}
