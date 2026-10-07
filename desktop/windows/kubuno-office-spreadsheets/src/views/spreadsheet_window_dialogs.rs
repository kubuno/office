//! The window's dialogs and popups (a child module of `spreadsheet_window`, so it reaches the window's fields):
//! conditional formatting, data validation, the custom sort, the column filter and the validation list over
//! the grid, « Nouveau » / « Dupliquer », and the honest « coming soon » of the web's placeholders.

use std::collections::HashSet;

use kubuno_desktop::popup::{Placement, Popup};
use kubuno_desktop::prelude::*;
use kubuno_desktop::views::component::Control as _;
use kubuno_office_sheets_core::validation::has_dropdown;
use kubuno_office_spreadsheets_common::model::state::selection_ref;

use super::SpreadsheetWindow;
use crate::controls::grid::GridPopupEventArgs;
use crate::controls::value_list::{decode_list, ValueEventArgs, ValueList};
use crate::pages::filter_popup::{FilterEventArgs, FilterPopup};
use crate::views::alert_dialog::AlertDialog;
use crate::views::cf_dialog::CfDialog;
use crate::views::dv_dialog::DvDialog;
use crate::views::sort_dialog::SortDialog;
use crate::Resources;

impl SpreadsheetWindow {
    /// Runs `f` once the current frame is done. A command runs inside the window's frame (a context menu's
    /// click still open, the ribbon mid-paint): a modal dialog opened there freezes the menu on screen and lays
    /// out against a half-built surface (Documents' `later`). Every dialog a command opens goes through here.
    pub(super) fn later(&mut self, f: fn(&mut SpreadsheetWindow)) {
        match self.dispatcher() {
            // Two frames: the first closes the context menu that ran the command, the second opens the dialog.
            Some(d) => {
                let again = d.clone();
                drop(d.begin_invoke(move |_w: &mut SpreadsheetWindow| drop(again.begin_invoke(move |w: &mut SpreadsheetWindow| f(w)))));
            }
            None => f(self),
        }
    }

    /// [`Self::later`] for a call carrying its arguments (an equation's id and text).
    pub(super) fn later_call(&mut self, f: impl FnOnce(&mut SpreadsheetWindow) + Send + 'static) {
        match self.dispatcher() {
            Some(d) => {
                let again = d.clone();
                drop(d.begin_invoke(move |_w: &mut SpreadsheetWindow| drop(again.begin_invoke(move |w: &mut SpreadsheetWindow| f(w)))));
            }
            None => f(self),
        }
    }

    /// The selection as the dialogs take it (`selectionRefNorm`).
    pub(super) fn selection_norm(&mut self) -> String {
        self.grid.with(|g| g.editor().map(selection_ref)).flatten().unwrap_or_else(|| "A1".into())
    }

    /// Accueil › Mise en forme conditionnelle.
    pub(super) fn cf_execute(&mut self) {
        self.later(Self::cf_open);
    }

    fn cf_open(&mut self) {
        self.grid.with(|g| g.commit_edit(None));
        let Some(blocks) = self.grid.with(|g| g.editor().map(|e| e.cf_blocks())).flatten() else { return };
        let selection = self.selection_norm();
        let grid = self.grid.clone();
        CfDialog::edit(self, blocks, &selection, Box::new(move |b| {
            grid.with(|g| {
                g.with_editor(|ed| ed.set_cf_blocks(b));
                g.invalidate();
            });
        }));
        self.grid.with(|g| g.focus());
    }

    /// Données › Validation des données.
    pub(super) fn validation_execute(&mut self) {
        self.later(Self::validation_open);
    }

    fn validation_open(&mut self) {
        self.grid.with(|g| g.commit_edit(None));
        let Some(blocks) = self.grid.with(|g| g.editor().map(|e| e.dv_blocks())).flatten() else { return };
        let selection = self.selection_norm();
        let grid = self.grid.clone();
        DvDialog::edit(self, blocks, &selection, Box::new(move |b| {
            grid.with(|g| {
                g.with_editor(|ed| ed.set_dv_blocks(b));
                g.invalidate();
            });
        }));
        self.grid.with(|g| g.focus());
    }

    /// Données › Tri personnalisé (`openSortDialog`).
    pub(super) fn sort_custom_execute(&mut self) {
        self.later(Self::sort_custom_open);
    }

    fn sort_custom_open(&mut self) {
        self.grid.with(|g| g.commit_edit(None));
        let Some(setup) = self.grid.with(|g| g.editor().and_then(|e| e.sort_setup())).flatten() else { return };
        if let Some((headers, levels)) = SortDialog::ask(self, &setup) {
            let range = setup.range;
            self.cmd(|ed| ed.sort_multi(range, headers, &levels));
        }
        self.grid.with(|g| g.focus());
    }

    /// A header's funnel: the column filter under it (`openFilterPopup`).
    pub(super) fn grid_filter_click(&mut self, e: &GridPopupEventArgs) {
        let col = e.col;
        let Some((values, allowed)) = self.grid.with(|g| g.editor().map(|ed| (ed.filter_values(col), ed.col_filters.get(&col).cloned()))).flatten() else { return };
        let owner = kubuno_desktop::popup::current_window();
        let content = Custom::<FilterPopup>::new().dock(DockStyle::Fill).name("filter");
        let popup = Popup::new(&content, 230.0, 320.0).corner_radius(8.0).back_color("Surface1").placement(Placement::BottomStart).gap(4.0).title(Resources::cmd_filter());
        let form: Form = popup.form().clone();
        {
            let content = content.clone();
            form.load().subscribe(move |_form, _e| {
                content.with(|p| p.open(values.clone(), allowed.clone()));
            });
        }
        {
            let grid = self.grid.clone();
            content.on::<FilterEventArgs>("OnApplied").subscribe(move |_p, e| {
                let allowed: Option<HashSet<String>> = if e.all { None } else { Some(decode_list(&e.values).into_iter().collect()) };
                grid.with(|g| {
                    g.with_editor(|ed| ed.set_col_filter(col, allowed.clone()));
                    g.invalidate();
                });
            });
        }
        {
            let form = form.clone();
            content.on::<kubuno_desktop::views::events::EmptyEventArgs>("OnCloseRequested").subscribe(move |_p, _e| form.close());
        }
        // Under the header, from the click (the web opens at the pointer's x).
        let anchor = kubuno_desktop::ui::Rect::new(e.left.max(e.right - 26.0), e.top, e.right, e.bottom);
        popup.show(owner, anchor);
    }

    /// A « liste » validation's arrow: its values under the cell; a pick writes the cell (`updateCell`).
    pub(super) fn grid_dropdown_click(&mut self, e: &GridPopupEventArgs) {
        let at = kubuno_office_sheets_core::addr::Cell::new(e.col, e.row);
        let Some(values) = self
            .grid
            .with(|g| g.editor().and_then(|ed| ed.validation_at(at).filter(|r| has_dropdown(Some(r))).map(|r| ed.validation_list(&r))))
            .flatten()
        else {
            return;
        };
        let owner = kubuno_desktop::popup::current_window();
        let width = (e.right - e.left).max(120.0);
        let height = ((values.len().max(1) as f32) * 32.0 + 8.0).min(256.0);
        let list = Custom::<ValueList>::new().dock(DockStyle::Fill).name("values");
        let popup = Popup::new(&list, width, height).corner_radius(8.0).back_color("Surface1").placement(Placement::BottomStart).gap(2.0).title(Resources::cmd_validation());
        let form: Form = popup.form().clone();
        {
            let list = list.clone();
            form.load().subscribe(move |_form, _e| {
                list.with(|l| {
                    l.row_height = 32.0;
                    l.empty_text = Resources::dv_no_values().to_string();
                    l.set_values(values.clone());
                });
            });
        }
        {
            let (grid, form) = (self.grid.clone(), form.clone());
            list.on::<ValueEventArgs>("OnItemClick").subscribe(move |_l, e| {
                let v = e.value.clone();
                grid.with(|g| {
                    g.run(|ed| ed.update_cell(at, &v));
                    g.focus();
                });
                form.close();
            });
        }
        popup.show(owner, kubuno_desktop::ui::Rect::new(e.left, e.top, e.right, e.bottom));
    }

    /// Accueil › Classeur › Nouveau: a new workbook on the server (`create({ title: common_untitled })`),
    /// opened in a window of its own (the web navigates to it).
    pub(super) fn new_execute(&mut self) {
        self.later(Self::new_open);
    }

    fn new_open(&mut self) {
        if self.live.is_none() {
            AlertDialog::say(self, Resources::needs_server());
            return;
        }
        let result = (|| -> Result<String, String> {
            use super::client::Transport;
            let account = kubuno_desktop_sync::current_account().map_err(|e| e.to_string())?.ok_or("no account")?;
            let t = super::client::SyncTransport::for_account(account);
            let r = t.send(&kubuno_office_spreadsheets_common::api::routes::create_workbook(Resources::untitled())).map_err(|e| e.to_string())?;
            if !r.ok() {
                return Err(format!("HTTP {}", r.status));
            }
            let v: serde_json::Value = serde_json::from_slice(&r.body).map_err(|e| e.to_string())?;
            v.get("spreadsheet").and_then(|s| s.get("id")).and_then(|i| i.as_str()).map(str::to_string).ok_or_else(|| "no id".to_string())
        })();
        match result {
            Ok(id) => open_window(&id),
            Err(why) => AlertDialog::say(self, &Resources::new_failed().replace("{why}", &why)),
        }
    }

    /// Accueil › Classeur › Dupliquer: a copy « … (copie) » on the server (the web stays on this workbook).
    pub(super) fn duplicate_execute(&mut self) {
        self.later(Self::duplicate_open);
    }

    fn duplicate_open(&mut self) {
        let Some(id) = self.live.as_ref().map(|l| l.id.clone()) else {
            AlertDialog::say(self, Resources::needs_server());
            return;
        };
        let result = (|| -> Result<(), String> {
            use super::client::Transport;
            let account = kubuno_desktop_sync::current_account().map_err(|e| e.to_string())?.ok_or("no account")?;
            let t = super::client::SyncTransport::for_account(account);
            let r = t.send(&kubuno_office_spreadsheets_common::api::routes::duplicate(&id)).map_err(|e| e.to_string())?;
            if r.ok() {
                Ok(())
            } else {
                Err(format!("HTTP {}", r.status))
            }
        })();
        let message = match result {
            Ok(()) => Resources::duplicate_done().replace("{title}", &format!("{} (copie)", self.wb_title)),
            Err(why) => Resources::new_failed().replace("{why}", &why),
        };
        AlertDialog::say(self, &message);
    }
}

/// Opens workbook `id` of the server in a new window of this app.
fn open_window(id: &str) {
    match std::env::current_exe() {
        Ok(exe) => {
            if let Err(e) = std::process::Command::new(exe).arg("--doc").arg(id).spawn() {
                kubuno_desktop::tracing::warn!("[spreadsheets] cannot open the new workbook: {e}");
            }
        }
        Err(e) => kubuno_desktop::tracing::warn!("[spreadsheets] cannot find the program: {e}"),
    }
}

// ── Dialogs-B: names, goal seek, pivot tables, protection, encryption ──────────

mod dialogs_b {
    use kubuno_desktop::views::component::Control as _;
    use kubuno_office_sheets_core::crypto;
    use kubuno_office_sheets_core::dialogs::NameError;
    use kubuno_office_sheets_core::editor::Editor;

    use super::super::SpreadsheetWindow;
    use crate::views::alert_dialog::AlertDialog;
    use crate::views::goalseek_dialog::GoalSeekDialog;
    use crate::views::names_dialog::NamesDialog;
    use crate::views::password_dialog::{PasswordDialog, PasswordMode};
    use crate::views::pivot_dialog::PivotDialog;
    use crate::Resources;

    impl SpreadsheetWindow {
        /// After each state the grid reports: the commands' labels, the pivot under the cursor, the stale pivots.
        pub(in crate::views) fn dialogs_state_changed(&mut self) {
            let (protected, encrypted, on_pivot) = self.grid.with(|g| g.editor().map(|e| (e.sheet().is_protected(), e.is_encrypted(), e.pivot_at_selection().is_some()))).flatten().unwrap_or_default();
            self.protect_label = if protected { Resources::sheet_unprotect().into() } else { Resources::sheet_protect().into() };
            self.encrypt_label = if encrypted { Resources::sheet_decrypt_wb().into() } else { Resources::sheet_encrypt_wb().into() };
            self.on_pivot = on_pivot;
            // The automatic recalculation of the pivots whose source changed (`SA:6304-6325`).
            let mut sigs = std::mem::take(&mut self.pivot_sigs);
            let changed = self.grid.with(|g| g.with_editor(|ed| ed.refresh_stale_pivots(&mut sigs).unwrap_or(false))).flatten().unwrap_or(false);
            self.pivot_sigs = sigs;
            if changed {
                self.grid.with(|g| g.invalidate());
            }
        }

        // Names

        pub(in crate::views) fn names_execute(&mut self) {
            self.later(Self::names_open);
        }

        fn names_open(&mut self) {
            self.grid.with(|g| g.commit_edit(None));
            let Some(names) = self.grid.with(|g| g.editor().map(|e| e.defined_names())).flatten() else { return };
            let selection = self.selection_norm();
            let grid = self.grid.clone();
            NamesDialog::manage(
                self,
                names,
                &selection,
                Box::new(move |f: &mut dyn FnMut(&mut Editor) -> Result<(), NameError>| {
                    grid.with(|g| {
                        let r = g.with_editor(|ed| f(ed).map(|()| ed.defined_names()));
                        g.invalidate();
                        r
                    })
                    .flatten()
                }),
            );
            // The names go to the collaboration room.
            self.collab_state_changed();
            self.grid.with(|g| g.focus());
        }

        // Goal seek

        pub(in crate::views) fn goalseek_execute(&mut self) {
            self.later(Self::goalseek_open);
        }

        fn goalseek_open(&mut self) {
            self.grid.with(|g| g.commit_edit(None));
            let target = self.grid.with(|g| g.editor().map(|e| e.sel.active.key())).flatten().unwrap_or_default();
            let grid = self.grid.clone();
            let found = GoalSeekDialog::ask(self, &target, Box::new(move |t, v, c| grid.with(|g| g.editor().map(|ed| ed.goal_seek(t, v, c))).flatten()));
            if let Some((changing, x)) = found {
                self.cmd(|ed| ed.apply_goal_seek(&changing, x));
            }
            self.grid.with(|g| g.focus());
        }

        // Pivot tables

        pub(in crate::views) fn pivot_execute(&mut self) {
            self.later(Self::pivot_open);
        }

        fn pivot_open(&mut self) {
            self.grid.with(|g| g.commit_edit(None));
            let Some(setup) = self.grid.with(|g| g.editor().and_then(|e| e.pivot_setup())).flatten() else { return };
            self.pivot_dialog(setup);
        }

        fn pivot_dialog(&mut self, setup: kubuno_office_sheets_core::pivot_table::PivotSetup) {
            let grid = self.grid.clone();
            let range = setup.range;
            let distinct = Box::new(move |f: usize| grid.with(|g| g.editor().map(|ed| ed.pivot_distinct(range, f))).flatten().unwrap_or_default());
            if let Some(choice) = PivotDialog::ask(self, setup.clone(), distinct) {
                self.cmd(|ed| ed.apply_pivot(&setup, &choice));
            }
            self.grid.with(|g| g.focus());
        }

        pub(in crate::views) fn pv_edit_execute(&mut self) {
            self.later(Self::pv_edit_open);
        }

        fn pv_edit_open(&mut self) {
            let Some(setup) = self.grid.with(|g| g.editor().and_then(|e| e.pivot_at_selection().and_then(|p| e.pivot_editor_setup(&p)))).flatten() else { return };
            self.pivot_dialog(setup);
        }

        pub(in crate::views) fn pv_refresh_execute(&mut self) {
            if let Some(id) = self.grid.with(|g| g.editor().and_then(|e| e.pivot_at_selection()).map(|p| p.id)).flatten() {
                self.cmd(|ed| ed.refresh_pivot(&id));
            }
        }

        pub(in crate::views) fn pv_delete_execute(&mut self) {
            if let Some(id) = self.grid.with(|g| g.editor().and_then(|e| e.pivot_at_selection()).map(|p| p.id)).flatten() {
                self.cmd(|ed| ed.delete_pivot(&id));
            }
        }

        /// Données › Actualiser tout (`refreshAllPivots`).
        pub(in crate::views) fn refresh_all_execute(&mut self) {
            self.cmd(|ed| ed.refresh_all_pivots());
        }

        // Protection

        pub(in crate::views) fn protect_execute(&mut self) {
            self.later(Self::protect_open);
        }

        fn protect_open(&mut self) {
            self.grid.with(|g| g.commit_edit(None));
            let protected = self.grid.with(|g| g.editor().map(|e| e.sheet().is_protected())).flatten().unwrap_or(false);
            let grid = self.grid.clone();
            if protected {
                PasswordDialog::ask(self, PasswordMode::Unprotect, Box::new(move |pwd| grid.with(|g| g.with_editor(|ed| ed.unprotect(pwd))).flatten().unwrap_or(false)));
            } else {
                PasswordDialog::ask(
                    self,
                    PasswordMode::Protect,
                    Box::new(move |pwd| match crypto::create_protection(pwd, crypto::EXCEL_SPIN_COUNT) {
                        Ok(p) => grid.with(|g| g.with_editor(|ed| ed.set_protection(Some(p)))).is_some(),
                        Err(_) => false,
                    }),
                );
            }
            self.grid.with(|g| {
                g.invalidate();
                g.focus();
            });
        }

        // Encryption (a workbook of the server only: the web stores the envelope there)

        pub(in crate::views) fn encrypt_execute(&mut self) {
            self.later(Self::encrypt_open);
        }

        fn encrypt_open(&mut self) {
            if self.live.is_none() {
                AlertDialog::say(self, Resources::needs_server());
                return;
            }
            self.grid.with(|g| g.commit_edit(None));
            let encrypted = self.grid.with(|g| g.editor().map(|e| e.is_encrypted())).flatten().unwrap_or(false);
            let (grid, crypt) = (self.grid.clone(), self.crypt.clone());
            if encrypted {
                // `decryptWorkbook`: the password unlocks (when still locked), then every sheet goes back in clear.
                let ok = PasswordDialog::ask(
                    self,
                    PasswordMode::Decrypt,
                    Box::new(move |pwd| {
                        let Some(d) = grid.with(|g| g.editor().and_then(|e| e.encryption())).flatten() else { return false };
                        let Ok(key) = crypto::unlock_workbook_key(&d, pwd) else { return false };
                        if let Ok(mut c) = crypt.lock() {
                            c.encrypted = false;
                            c.key = Some((d, key.clone()));
                        }
                        grid.with(|g| {
                            g.with_editor(|ed| {
                                let _ = ed.unlock_workbook(&key);
                                ed.mark_decrypted();
                            })
                        });
                        true
                    }),
                );
                if ok {
                    if let Some(id) = self.live.as_ref().map(|l| l.id.clone()) {
                        self.start_collab(&id);
                    }
                }
            } else {
                // `encryptWorkbook`: a fresh key; the collaboration stops; the next save encrypts every sheet.
                let ok = PasswordDialog::ask(
                    self,
                    PasswordMode::Encrypt,
                    Box::new(move |pwd| {
                        let Ok((d, key)) = crypto::create_workbook_key(pwd, crypto::EXCEL_SPIN_COUNT) else { return false };
                        if let Ok(mut c) = crypt.lock() {
                            c.encrypted = true;
                            c.key = Some((d.clone(), key));
                        }
                        grid.with(|g| g.with_editor(|ed| ed.mark_encrypted(&d)));
                        true
                    }),
                );
                if ok {
                    self.stop_collab();
                }
            }
            self.grid.with(|g| {
                g.invalidate();
                g.focus();
            });
        }

        /// An encrypted workbook just opened without its key: `unlockWorkbookNow`.
        pub(in crate::views) fn ask_unlock_if_locked(&mut self) {
            let encrypted = self.grid.with(|g| g.editor().map(|e| e.is_encrypted())).flatten().unwrap_or(false);
            let locked = encrypted && self.crypt.lock().map(|c| c.key.is_none()).unwrap_or(true);
            if !locked {
                return;
            }
            if let Ok(mut c) = self.crypt.lock() {
                c.encrypted = true;
            }
            self.later(Self::unlock_open);
        }

        fn unlock_open(&mut self) {
            let (grid, crypt) = (self.grid.clone(), self.crypt.clone());
            let ok = PasswordDialog::ask(
                self,
                PasswordMode::UnlockWorkbook,
                Box::new(move |pwd| {
                    let Some(d) = grid.with(|g| g.editor().and_then(|e| e.encryption())).flatten() else { return false };
                    let Ok(key) = crypto::unlock_workbook_key(&d, pwd) else { return false };
                    let opened = grid.with(|g| g.with_editor(|ed| ed.unlock_workbook(&key).is_ok())).flatten().unwrap_or(false);
                    if opened {
                        if let Ok(mut c) = crypt.lock() {
                            c.encrypted = true;
                            c.key = Some((d, key));
                        }
                    }
                    opened
                }),
            );
            if !ok {
                // The web's gate: no editing of a locked workbook; cancelling leaves it (`onCancel` → the list).
                self.show_note(Resources::enc_locked());
                self.close();
                return;
            }
            self.grid.with(|g| {
                g.invalidate();
                g.focus();
            });
        }
    }
}
