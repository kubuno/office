//! Fichier › Imprimer (Ctrl+P): the web's print dialog (`openPrint`, `SA:3931`) over the active sheet.

use kubuno_office_sheets_core::print;

use crate::views::print_dialog::{Areas, SheetPrintDialog};
use super::SpreadsheetWindow;

impl SpreadsheetWindow {
    pub(super) fn print_execute(&mut self) {
        self.grid.with(|g| g.commit_edit(None));
        let areas = self.grid.with(|g| g.editor().map(|ed| Areas { selection: print::build_grid(ed, true).filter(|_| ed.selected && { let r = ed.sel.rect(); r.c2 > r.c1 || r.r2 > r.r1 }), used: print::build_grid(ed, false) })).flatten();
        let Some(areas) = areas else { return };
        let mut options = self.print_options.clone();
        options.title = self.wb_title.clone();
        self.print_options = SheetPrintDialog::ask(self, areas, options);
    }
}
