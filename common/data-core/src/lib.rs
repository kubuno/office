//! `kubuno-office-data-core` — the Data (BI) engine every Kubuno Office client shares, ported from the
//! web Data editor (`office/web/src/data/*.ts`, `data-api.ts`, `DataReportEditor.tsx`).
//! Design of record: vskubuno `docs/DATA-DESKTOP.md`.
//!
//! | Module | Web origin | Content |
//! |---|---|---|
//! | [`model`] | `data-api.ts`, server `models/data.rs` | the report document, held losslessly (`.kbdrp` + REST payloads) |
//! | [`format`] | `data/format.ts` | Power BI-like value, axis and label formatting |
//! | [`query`] | server `data_execute.rs` | in-memory filter/group/sort/aggregate over a dataset's cached rows (offline + sample) |
//! | [`shape`] | `data/shape.ts` | turning query rows into chart series, histograms, quartiles, trend lines, treemap |
//! | [`palettes`] | `data/palettes.ts` | chart palettes, report themes, the diverging colour scale |
//! | [`wells`] | `data/wells.ts` | per-visual field wells, aggregation functions, filter operators |
//! | [`visuals`] | `data/visuals.tsx` | the catalogue of visual types the editor offers |
//! | [`editor`] | `DataReportEditor.tsx` | selection, the report-editing operations and undo/redo |
//!
//! The Data app is **server-coupled**: datasets refresh and query execution run SQL on the server, so this
//! crate never runs a query — it models the stored document, shapes the rows the server returns and formats
//! them. No clock, no randomness, no I/O: time and ids are given by the caller, files are bytes.

pub mod editor;
pub mod format;
pub mod model;
pub mod palettes;
pub mod query;
pub mod shape;
pub mod visuals;
pub mod wells;
