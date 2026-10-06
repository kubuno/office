//! XLSX import: one focused sub-module per part family. Each file owns its
//! parser so features can grow independently:
//!   - `strings`   → xl/sharedStrings.xml
//!   - `styles`    → xl/styles.xml (numFmts, fonts, fills, borders, cellXfs, dxfs)
//!   - `worksheet` → xl/worksheets/sheetN.xml (cells, merges, cols/rows, CF, views)
//!   - `workbook`  → xl/workbook.xml + relationship files
//!   - `drawing`   → xl/drawings/* (image + chart + shape anchors)
//!   - `chart`     → xl/charts/chartN.xml
//!   - `shape`     → `<xdr:sp>` bodies inside a drawing part
//!   - `comments`  → xl/commentsN.xml + xl/threadedComments/* (cell notes)
pub mod chart;
pub mod comments;
pub mod drawing;
pub mod shape;
pub mod strings;
pub mod styles;
pub mod workbook;
pub mod worksheet;
