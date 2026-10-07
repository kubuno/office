//! Code-behind of the Name Manager (`names_dialog.kbview`): the web's `NameManagerDialog`. The names live in
//! the window's editor; each change goes through `edit`, which also publishes them to the collaboration room.

use kubuno_desktop::prelude::*;
use kubuno_office_sheets_core::dialogs::NameError;
use kubuno_office_sheets_core::editor::Editor;

use crate::Resources;

/// Runs a change of the names on the window's editor and returns the names after it (`None`: no workbook).
pub type EditFn = Box<dyn FnMut(&mut dyn FnMut(&mut Editor) -> Result<(), NameError>) -> Option<Result<Vec<(String, String)>, NameError>>>;

#[kubuno_desktop::view("names_dialog.kbview")]
pub struct NamesDialog {
    #[bind]
    name_rows: Rows,
    #[bind]
    form_title: String,
    #[bind]
    name_field: String,
    #[bind]
    def_field: String,
    #[bind]
    error: String,
    #[bind]
    has_error: bool,
    #[bind]
    is_editing: bool,
    #[bind]
    submit_text: String,
    names: Vec<(String, String)>,
    editing: Option<String>,
    selection: String,
    edit: Option<EditFn>,
}

impl NamesDialog {
    pub fn new(names: Vec<(String, String)>, selection: &str, edit: EditFn) -> Self {
        let mut d = Self { names, selection: selection.to_string(), edit: Some(edit), ..Self::default() };
        d.start_new();
        d.initialize_component();
        d
    }

    /// Shows it over `owner` until « Fermer ».
    pub fn manage(owner: &dyn AsForm, names: Vec<(String, String)>, selection: &str, edit: EditFn) {
        let mut d = Self::new(names, selection, edit);
        d.show_dialog(owner);
    }

    fn refresh(&mut self) {
        self.name_rows = Rows::from(self.names.iter().map(|(n, d)| Row::new().with("Name", Value::Str(n.clone())).with("Def", Value::Str(d.clone()))).collect::<Vec<_>>());
        self.is_editing = self.editing.is_some();
        self.form_title = match &self.editing {
            Some(n) => Resources::names_editing().replace("{n}", n),
            None => Resources::names_new().into(),
        };
        self.submit_text = if self.is_editing { Resources::names_save().into() } else { Resources::names_add().into() };
    }

    /// `startNew`: the selection pre-filled (`=A1:C3`).
    fn start_new(&mut self) {
        self.editing = None;
        self.name_field.clear();
        self.def_field = if self.selection.is_empty() { String::new() } else { format!("={}", self.selection) };
        self.has_error = false;
        self.refresh();
    }

    fn run(&mut self, f: &mut dyn FnMut(&mut Editor) -> Result<(), NameError>) -> Result<(), NameError> {
        let Some(edit) = self.edit.as_mut() else { return Ok(()) };
        match edit(f) {
            Some(Ok(names)) => {
                self.names = names;
                Ok(())
            }
            Some(Err(e)) => Err(e),
            None => Ok(()),
        }
    }

    /// `submit`: validates, renames when the name changed, then starts a new one.
    fn submit(&mut self) {
        let (editing, name, def) = (self.editing.clone(), self.name_field.clone(), self.def_field.clone());
        match self.run(&mut |ed: &mut Editor| ed.set_defined_name(editing.as_deref(), &name, &def)) {
            Ok(()) => self.start_new(),
            Err(NameError::Name) => {
                self.error = Resources::names_err_name().into();
                self.has_error = true;
            }
            Err(NameError::Definition(e)) => {
                self.error = Resources::names_err_def().replace("{e}", &e);
                self.has_error = true;
            }
        }
    }

    fn submit_button_click(&mut self) {
        self.submit();
    }

    fn name_edit_click(&mut self) {
        let Some((n, d)) = kubuno_desktop::views::binding::current_item().and_then(|i| self.names.get(i.index).cloned()) else { return };
        self.editing = Some(n.clone());
        self.name_field = n;
        self.def_field = d;
        self.has_error = false;
        self.refresh();
    }

    fn name_delete_click(&mut self) {
        let Some(n) = kubuno_desktop::views::binding::current_item().and_then(|i| self.names.get(i.index).map(|x| x.0.clone())) else { return };
        let _ = self.run(&mut |ed: &mut Editor| {
            ed.delete_defined_name(&n);
            Ok(())
        });
        self.refresh();
    }

    fn cancel_edit_button_click(&mut self) {
        self.start_new();
    }

    fn close_button_click(&mut self) {
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn names_are_added_refused_and_renamed_through_the_editor() {
        let ed = Rc::new(RefCell::new(kubuno_office_sheets_core::editor::Editor::new(
            kubuno_office_sheets_core::model::workbook::Workbook::new_local("t", "s1"),
            Rc::new(kubuno_office_spreadsheets_common::platform::SystemEnv::default()),
        )));
        let shared = ed.clone();
        let edit: EditFn = Box::new(move |f| {
            let mut e = shared.borrow_mut();
            Some(f(&mut e).map(|()| e.defined_names()))
        });
        let mut d = NamesDialog { selection: "A1:B2".into(), edit: Some(edit), ..NamesDialog::default() };
        d.start_new();
        assert_eq!(d.def_field, "=A1:B2");
        d.name_field = "B2".into();
        d.submit();
        assert!(d.has_error && d.error == Resources::names_err_name());
        d.name_field = "Zone".into();
        d.submit();
        assert!(!d.has_error);
        assert_eq!(d.names, vec![("ZONE".to_string(), "=A1:B2".to_string())]);
        assert_eq!(ed.borrow().wb.names.get("ZONE").and_then(|v| v.as_str()), Some("=A1:B2"));
    }
}
