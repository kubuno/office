//! What the window and the report canvas share: the open report's editor (`kubuno_office_data_core::editor`),
//! the datasets' cached rows (for the sample and offline rendering), and the app theme flag, behind one
//! `Rc<RefCell<…>>`. The palette of the canvas is here too: the web's theme tokens, light and dark.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use kubuno_desktop::ui::graphics::Color;
use kubuno_office_data_core::editor::Editor;
use kubuno_office_data_core::query::{self, FilterSpec, MetricSpec, Query, QueryResult, SortSpec};
use kubuno_office_data_core::model::Widget;
use serde_json::Value;

/// The open report (its editor), the datasets' cached rows, and the app theme.
#[derive(Default)]
pub struct Model {
    pub editor: Option<Editor>,
    /// dataset id → cached rows, what a `.kbdst`'s `data_cache` holds (sample/offline); the dev core
    /// returns the same rows from `POST /office/data/execute`.
    pub rows: HashMap<String, Vec<Value>>,
    pub dark: bool,
}

impl Model {
    /// The result a widget's query produces over the cached rows, or `None` for a static/unbound widget.
    pub fn widget_data(&self, widget: &Widget) -> Option<QueryResult> {
        let dataset = widget.dataset_id()?;
        let rows = self.rows.get(&dataset)?;
        Some(query::execute(rows, &widget_query(widget)))
    }
}

/// The shared model.
pub type Shared = Rc<RefCell<Model>>;

pub fn shared() -> Shared {
    Rc::new(RefCell::new(Model::default()))
}

/// Build the aggregation query a widget's config describes (`useWidgetData` in the web editor).
pub fn widget_query(widget: &Widget) -> Query {
    let cfg = &widget.config;
    let dimensions: Vec<String> = cfg
        .get("dimensions")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let metrics: Vec<MetricSpec> =
        cfg.get("metrics").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    let filters: Vec<FilterSpec> =
        cfg.get("filters").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    let sort: Vec<SortSpec> =
        cfg.get("sort").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    let limit = cfg.get("limit").and_then(Value::as_i64);
    Query { dimensions, metrics, filters, sort, limit }
}

/// `#rrggbb`.
pub const fn hex(v: u32) -> Color {
    Color::rgb(((v >> 16) & 0xff) as u8, ((v >> 8) & 0xff) as u8, (v & 0xff) as u8)
}

/// The theme tokens the canvas paints with (the web's report themes, `data/palettes.ts`, plus the app chrome
/// tokens shared with the other Office apps, light and dark).
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub surface0: Color,
    pub surface1: Color,
    pub surface2: Color,
    pub surface3: Color,
    pub border: Color,
    pub primary: Color,
    pub danger: Color,
    pub success: Color,
    pub warning: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_tertiary: Color,
    pub row_line: Color,
}

impl Palette {
    pub fn light() -> Self {
        Self {
            dark: false,
            surface0: hex(0xffffff),
            surface1: hex(0xf8f9fa),
            surface2: hex(0xf1f3f4),
            surface3: hex(0xe8eaed),
            border: hex(0xe0e0e0),
            primary: hex(0x1a73e8),
            danger: hex(0xd93025),
            success: hex(0x1e8e3e),
            warning: hex(0xf9ab00),
            text_primary: hex(0x202124),
            text_secondary: hex(0x5f6368),
            text_tertiary: hex(0x80868b),
            row_line: hex(0xf1f3f4),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            surface0: hex(0x202124),
            surface1: hex(0x292a2d),
            surface2: hex(0x35363a),
            surface3: hex(0x444746),
            border: hex(0x5f6368),
            primary: hex(0x8ab4f8),
            danger: hex(0xf28b82),
            success: hex(0x81c995),
            warning: hex(0xfdd663),
            text_primary: hex(0xe8eaed),
            text_secondary: hex(0x9aa0a6),
            text_tertiary: hex(0x80868b),
            row_line: hex(0x3c4043),
        }
    }

    pub fn of(dark: bool) -> Self {
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }
}

/// Parse a `#rgb`/`#rrggbb` string to a [`Color`], falling back to `default`.
pub fn parse_hex(s: &str, default: Color) -> Color {
    let s = s.trim().trim_start_matches('#');
    let full = match s.len() {
        3 => s.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => s.to_string(),
        _ => return default,
    };
    let byte = |a: usize| u8::from_str_radix(&full[a..a + 2], 16).ok();
    match (byte(0), byte(2), byte(4)) {
        (Some(r), Some(g), Some(b)) => Color::rgb(r, g, b),
        _ => default,
    }
}

/// The chart colours of a report's palette id (`data/palettes.ts`), as [`Color`]s.
pub fn chart_colors(palette_id: &str) -> Vec<Color> {
    kubuno_office_data_core::palettes::palette_by_id(Some(palette_id))
        .iter()
        .map(|h| parse_hex(h, hex(0x1a73e8)))
        .collect()
}

/// `color-mix`-free tints: `c` at `alpha` over the background.
pub fn tint(c: Color, alpha: f32) -> Color {
    c.with_alpha(alpha)
}
