//! Code-behind of « Partager la présentation » (`share_dialog.kbview`, the web's `CollaboratorsDialog`): the
//! server calls run off the UI thread; their answers refresh the lists.

use kubuno_desktop::prelude::*;
use kubuno_office_presentations_common::api::client::{Client, Person, Sharing};

use crate::Resources;

/// `PERMS` (`edit`, `comment`, `view`).
const PERMS: [&str; 3] = ["edit", "comment", "view"];

#[kubuno_desktop::view("share_dialog.kbview")]
pub struct ShareDialog {
    #[bind]
    search: String,
    #[bind]
    permissions: Rows,
    #[bind]
    new_permission: String,
    #[bind]
    results: Rows,
    #[bind]
    result_index: f64,
    #[bind]
    has_results: bool,
    #[bind]
    no_results: bool,
    #[bind]
    people: Rows,
    #[bind]
    person_index: f64,
    #[bind]
    person_permission: String,
    #[bind]
    can_edit_person: bool,
    #[bind]
    status: String,
    pres: String,
    found: Vec<Person>,
    sharing: Sharing,
    /// The search text the last request was made for (answers to older searches are dropped).
    asked: String,
}

fn perm_label(p: &str) -> &'static str {
    match p {
        "comment" => Resources::share_perm_comment(),
        "view" => Resources::share_perm_view(),
        _ => Resources::share_perm_edit(),
    }
}

fn client() -> Result<Client, String> {
    match kubuno_desktop_sync::current_account() {
        Ok(Some(account)) => Ok(Client::for_account(account)),
        Ok(None) => Err(Resources::start_signed_out().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

impl ShareDialog {
    pub fn new(pres: &str) -> Self {
        let mut d = Self {
            pres: pres.to_string(),
            new_permission: "edit".into(),
            result_index: -1.0,
            person_index: -1.0,
            permissions: Rows::from(PERMS.iter().map(|p| Row::new().with("Value", Value::Str(p.to_string())).with("Text", Value::Str(perm_label(p).to_string()))).collect::<Vec<_>>()),
            ..Self::default()
        };
        d.initialize_component();
        d
    }

    /// Shows the dialog for presentation `pres` over `owner`.
    pub fn open_for(owner: &dyn AsForm, pres: &str) {
        let mut d = Self::new(pres);
        d.show_dialog(owner);
    }

    /// Runs `call` on a worker thread and hands its answer to `then` on the UI thread.
    fn run<T: Send + 'static>(&mut self, call: impl FnOnce(&Client) -> Result<T, String> + Send + 'static, then: impl FnOnce(&mut ShareDialog, Result<T, String>) + Send + 'static) {
        let Some(d) = self.dispatcher() else { return };
        let spawned = std::thread::Builder::new().name("presentations-share".into()).spawn(move || {
            let result = client().and_then(|c| call(&c));
            drop(d.begin_invoke(move |w: &mut ShareDialog| then(w, result)));
        });
        if let Err(e) = spawned {
            kubuno_desktop::tracing::error!("[presentations] share thread: {e}");
        }
    }

    fn share_load(&mut self) {
        self.load();
    }

    /// (Re)reads who has access.
    fn load(&mut self) {
        let pres = self.pres.clone();
        self.run(move |c| c.collaborators(&pres).map_err(|e| format!("{e:?}")), |w, r| match r {
            Ok(s) => {
                w.sharing = s;
                w.show_people();
            }
            Err(e) => w.status = e,
        });
    }

    fn show_people(&mut self) {
        let mut rows = Vec::new();
        if let Some(o) = &self.sharing.owner {
            rows.push(Row::new().with("Text", Value::Str(format!("{} — {} · {}", o.label(), o.email, Resources::share_owner()))));
        }
        for p in &self.sharing.people {
            rows.push(Row::new().with("Text", Value::Str(format!("{} — {} · {}", p.label(), p.email, perm_label(&p.permission)))));
        }
        if self.sharing.people.is_empty() {
            rows.push(Row::new().with("Text", Value::Str(Resources::share_only_you().to_string())));
        }
        self.people = Rows::from(rows);
        self.person_index = -1.0;
        self.can_edit_person = false;
    }

    /// The collaborator the selected row is, if it is one (the owner's row comes first).
    fn selected_person(&self) -> Option<Person> {
        let i = self.person_index;
        if i < 0.0 {
            return None;
        }
        let i = i as usize;
        let offset = usize::from(self.sharing.owner.is_some());
        i.checked_sub(offset).and_then(|j| self.sharing.people.get(j)).cloned()
    }

    fn search_changed(&mut self, e: &TextChangedEventArgs) {
        let q = e.new.trim().to_string();
        self.asked = q.clone();
        if q.is_empty() {
            self.found.clear();
            self.results = Rows::default();
            self.has_results = false;
            self.no_results = false;
            return;
        }
        self.run(move |c| c.search_recipients(&q).map_err(|e| format!("{e:?}")).map(|r| (q, r)), |w, r| {
            let Ok((q, found)) = r else { return };
            if q != w.asked {
                return;
            }
            let have: Vec<String> = w.sharing.owner.iter().chain(w.sharing.people.iter()).map(|p| p.id.clone()).collect();
            w.found = found.into_iter().filter(|p| !have.contains(&p.id)).collect();
            w.results = Rows::from(w.found.iter().map(|p| Row::new().with("Text", Value::Str(if p.name.is_empty() { p.email.clone() } else { format!("{} — {}", p.name, p.email) }))).collect::<Vec<_>>());
            w.has_results = !w.found.is_empty();
            w.no_results = w.found.is_empty();
            w.result_index = -1.0;
        });
    }

    /// A click on a result shares the presentation with that person (`addMut`).
    fn result_chosen(&mut self, _e: &SelectionChangedEventArgs) {
        if self.result_index < 0.0 {
            return;
        }
        let Some(p) = self.found.get(self.result_index as usize).cloned() else { return };
        let (pres, perm) = (self.pres.clone(), if self.new_permission.is_empty() { "edit".to_string() } else { self.new_permission.clone() });
        self.run(move |c| c.add_collaborator(&pres, &p.id, &perm).map_err(|e| format!("{e:?}")), |w, r| {
            match r {
                Ok(()) => {
                    w.search.clear();
                    w.asked.clear();
                    w.found.clear();
                    w.results = Rows::default();
                    w.has_results = false;
                    w.no_results = false;
                }
                Err(e) => w.status = e,
            }
            w.load();
        });
    }

    fn person_chosen(&mut self, _e: &SelectionChangedEventArgs) {
        match self.selected_person() {
            Some(p) => {
                self.person_permission = p.permission.clone();
                self.can_edit_person = true;
            }
            None => self.can_edit_person = false,
        }
    }

    fn person_permission_changed(&mut self, e: &TextChangedEventArgs) {
        let Some(p) = self.selected_person() else { return };
        let perm = e.new.clone();
        if perm.is_empty() || perm == p.permission {
            return;
        }
        let pres = self.pres.clone();
        self.run(move |c| c.update_collaborator(&pres, &p.id, &perm).map_err(|e| format!("{e:?}")), |w, r| {
            if let Err(e) = r {
                w.status = e;
            }
            w.load();
        });
    }

    fn remove_click(&mut self) {
        let Some(p) = self.selected_person() else { return };
        let pres = self.pres.clone();
        self.run(move |c| c.remove_collaborator(&pres, &p.id).map_err(|e| format!("{e:?}")), |w, r| {
            if let Err(e) = r {
                w.status = e;
            }
            w.load();
        });
    }

    fn close_click(&mut self) {
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }
}
