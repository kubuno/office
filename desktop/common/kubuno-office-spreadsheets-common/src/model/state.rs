//! What the window shows of the open workbook around the grid - the Name Box, the formula bar, the status
//! bar (`SA:7955-7994`), the ribbon's checked states (`selectedCellStyle`, `SA:4745`), the sheet tabs -
//! computed from the editor in one place, so every platform shows the same thing.

use kubuno_office_sheets_core::addr::{self, MAX_COLS, MAX_ROWS};
use kubuno_office_sheets_core::editor::Editor;
use kubuno_office_sheets_core::format;
use kubuno_office_sheets_core::grid;
use kubuno_office_sheets_core::model::style::Align;
use kubuno_office_sheets_core::refs;

/// The window's view of the workbook.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridState {
    /// The Name Box: the active cell, or the selected range (`A1:C5`).
    pub name_box: String,
    /// The formula bar: the active cell's formula or value.
    pub formula: String,
    /// The status bar's cell (`A1`) or range reference.
    pub status_ref: String,
    /// « 3L × 2C » for a multi-cell selection, empty otherwise.
    pub status_size: String,
    /// The aggregates (« Moyenne : 2  Nombre : 3  Somme : 6 »…), empty when no number is selected.
    pub status_agg: Vec<(String, String)>,
    /// The active cell's error code and its explanation, when it shows one.
    pub error: Option<(String, String)>,
    /// The sheets (names) and the active one.
    pub sheets: Vec<String>,
    pub active_sheet: usize,
    // The ribbon's states, from the active cell's effective style.
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub wrap: bool,
    pub align: String,
    pub font_family: String,
    pub font_size: String,
    pub num_fmt: String,
    pub merged: bool,
    pub gridlines: bool,
    pub frozen_rows: u32,
    pub frozen_cols: u32,
    pub filter_mode: bool,
    pub protected: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub dirty: bool,
    pub revision: u64,
    pub zoom_percent: u32,
    pub has_clip: bool,
}

/// The labels of the status bar's aggregates (French, the web's defaults: `sheet_status_*`).
pub struct AggLabels {
    pub avg: String,
    pub count: String,
    pub sum: String,
    pub min: String,
    pub max: String,
}

impl Default for AggLabels {
    fn default() -> Self {
        Self { avg: "Moyenne".into(), count: "Nombre".into(), sum: "Somme".into(), min: "Min".into(), max: "Max".into() }
    }
}

/// The selection as the Name Box writes it (`A1`, `A1:C5`).
pub fn selection_ref(ed: &Editor) -> String {
    let r = ed.sel.rect();
    if r.c1 == r.c2 && r.r1 == r.r2 {
        addr::key(r.c1, r.r1)
    } else {
        format!("{}:{}", addr::key(r.c1, r.r1), addr::key(r.c2, r.r2))
    }
}

impl GridState {
    /// The state of `ed` now.
    pub fn of(ed: &Editor, labels: &AggLabels) -> GridState {
        let sheet = ed.sheet();
        // Nothing selected (the workbook just opened): empty Name Box and formula bar, no aggregates, the
        // ribbon unchecked (`selectedCellStyle` is `{}` on the web).
        let any = ed.selected;
        let rect = ed.sel.rect();
        let st = if any { ed.selected_style() } else { Default::default() };
        let shown = ed.shown(ed.sel.active.col, ed.sel.active.row);
        let is_multi = any && (rect.c1 != rect.c2 || rect.r1 != rect.r2);
        let agg = if any { ed.with_eval(|data, ctx, spill, _| grid::selection_aggregate(sheet, &rect, data, ctx, spill)) } else { Default::default() };
        let mut status_agg = Vec::new();
        let fmt = |n: f64| format::locale_number(n, 2, &ed.lang);
        if agg.num >= 1 {
            status_agg.push((labels.avg.clone(), fmt(agg.avg)));
            status_agg.push((labels.count.clone(), agg.count.to_string()));
            status_agg.push((labels.sum.clone(), fmt(agg.sum)));
        }
        if agg.num >= 2 {
            status_agg.push((labels.min.clone(), fmt(agg.min)));
            status_agg.push((labels.max.clone(), fmt(agg.max)));
        }
        let error = shown.error.as_ref().filter(|_| any).and_then(|e| refs::error_help(e).map(|h| (e.clone(), h.to_string())));
        GridState {
            name_box: if any { selection_ref(ed) } else { String::new() },
            formula: if any { ed.formula_bar_text() } else { String::new() },
            status_ref: if any { selection_ref(ed) } else { String::new() },
            status_size: if is_multi {
                let rows = if rect.r2 >= MAX_ROWS { MAX_ROWS - rect.r1 + 1 } else { rect.height() };
                let cols = if rect.c2 >= MAX_COLS - 1 { MAX_COLS - rect.c1 } else { rect.width() };
                format!("{rows}L × {cols}C")
            } else {
                String::new()
            },
            status_agg,
            error,
            sheets: ed.wb.sheets.iter().map(|s| s.meta.name.clone()).collect(),
            active_sheet: ed.active_index(),
            bold: st.is_bold(),
            italic: st.is_italic(),
            underline: st.is_underline(),
            strike: st.is_strike(),
            wrap: st.is_wrap(),
            align: match st.align {
                Some(Align::Center) => "center",
                Some(Align::Right) => "right",
                _ => "left",
            }
            .into(),
            font_family: st.font_family.clone().unwrap_or_else(|| "Arial".into()),
            font_size: kubuno_office_sheets_core::js::to_string(st.font_size.unwrap_or(11.0)),
            num_fmt: st.num_fmt.clone().unwrap_or_default(),
            merged: any && ed.selection_has_merge(),
            gridlines: sheet.gridlines(),
            frozen_rows: sheet.frozen_rows(),
            frozen_cols: sheet.frozen_cols(),
            filter_mode: ed.filter_mode || !ed.col_filters.is_empty(),
            protected: sheet.is_protected(),
            can_undo: ed.can_undo(),
            can_redo: ed.can_redo(),
            dirty: ed.is_dirty(),
            revision: ed.revision(),
            zoom_percent: (ed.zoom * 100.0).round() as u32,
            has_clip: ed.has_clip(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kubuno_office_sheets_core::addr::Cell as Pos;
    use std::rc::Rc;

    #[test]
    fn the_status_bar_and_the_ribbon_follow_the_selection() {
        let mut ed = Editor::new(crate::sample::workbook(), Rc::new(crate::platform::SystemEnv::default()));
        ed.select_rect(Pos::new(1, 3), Pos::new(1, 4));
        let s = GridState::of(&ed, &AggLabels::default());
        assert_eq!(s.name_box, "B3:B4");
        assert_eq!(s.status_size, "2L × 1C");
        assert_eq!(s.status_agg[2], ("Somme".to_string(), "97\u{202F}000".to_string()));
        assert_eq!(s.sheets, vec!["Budget".to_string(), "Paramètres".to_string()]);
        ed.select(Pos::new(0, 2));
        let s = GridState::of(&ed, &AggLabels::default());
        assert!(s.bold);
        assert_eq!(s.formula, "Poste");
    }
}
