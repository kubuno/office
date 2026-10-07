//! The spreadsheet chart engine (`web/src/sheet-chart`, « v2 »): normalization of a persisted chart of any
//! vintage, data resolution from the cells, chart2's value-axis auto-scaling, the page layout and the plot
//! renderers — producing the same drawing as the web's SVG, as [`scene::Prim`]s a platform painter draws.
//! The persisted chart JSON itself is never rewritten here (round trip).
//!
//! Checked against the web renderer by `tests/golden_charts.rs` (vectors from `tools/golden/charts-harness.tsx`).

pub mod axes;
pub mod data;
pub mod layout;
pub mod marks;
pub mod model;
pub mod render;
pub mod scale;
pub mod scene;
pub mod thumbs;
pub mod wizard;

pub use data::{CellGetter, CellVal};
pub use render::{render_chart, ChartWords};
pub use scene::{resolve_color, Anchor, Prim};
