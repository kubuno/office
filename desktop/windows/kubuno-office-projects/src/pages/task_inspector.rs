//! Code-behind of the user control `TaskInspector` (`task_inspector.kbcontrol`): the « Informations » panel.
//!
//! The window hands it the selected task ([`TaskInspector::show`]); what the user changes leaves as ONE event,
//! `InspectorEdited` (`field`, JSON `value`), which the window turns into the editor's command - the same
//! `patchTaskCmd` / assign / link calls as the web's panel. A value written by the window
//! (`ChangeSource::Binding`) never comes back as an edit. The constraint keeps the web's rule
//! (`TaskDetailPanel.tsx:57-84`): a dated type waits for its date before anything is sent.

use kubuno_desktop::views::events::ChangeSource;
use kubuno_desktop::views::prelude::*;
use kubuno_desktop::{Row, Rows, Value as BValue};
use kubuno_office_projects_core::model::ProjectData;
use kubuno_office_projects_core::view;
use serde_json::{json, Value};

use crate::controls::lists::ListActionEventArgs;
use crate::Resources;

/// One edit of the panel: `field` names what changed (`name`, `task_type`, `priority`, `status`,
/// `duration_days`, `progress`, `estimated_hours`, `budget_cost`, `constraint`, `deadline_date`, `version_id`,
/// `description`, `assign`, `unassign`, `dep_type`, `dep_lag`, `dep_delete`, `dep_add`), `value` is JSON.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct InspectorEditEventArgs {
    pub field: String,
    pub value: String,
    /// For the list actions: where the click was (to place a menu).
    pub x: f32,
    pub y: f32,
}

const DATED: [&str; 6] = ["SNET", "SNLT", "FNET", "FNLT", "MSO", "MFO"];

fn rows(items: &[(&str, String)]) -> Rows {
    Rows::from(items.iter().map(|(v, l)| Row::new().with("Value", BValue::Str((*v).into())).with("Label", BValue::Str(l.clone()))).collect::<Vec<_>>())
}

/// The panel (see the module doc).
#[derive(UserControl, Default)]
#[user_control(view = "task_inspector.kbcontrol")]
#[category("Projects")]
pub struct TaskInspector {
    base: UserControlCore,
    /// Occurs when the user changes something of the task.
    #[event]
    #[category("Action")]
    pub inspector_edited: Event<InspectorEditEventArgs>,
    #[property(bindable)]
    #[browsable(false)]
    pub show_hint: bool,
    #[property(bindable)]
    #[browsable(false)]
    pub show_task: bool,
    #[property(bindable)]
    #[browsable(false)]
    pub task_name: String,
    #[property(bindable)]
    #[browsable(false)]
    pub task_type: String,
    #[property(bindable)]
    #[browsable(false)]
    pub types: Rows,
    #[property(bindable)]
    #[browsable(false)]
    pub priority: String,
    #[property(bindable)]
    #[browsable(false)]
    pub priorities: Rows,
    #[property(bindable)]
    #[browsable(false)]
    pub status: String,
    #[property(bindable)]
    #[browsable(false)]
    pub statuses: Rows,
    #[property(bindable)]
    #[browsable(false)]
    pub duration: f32,
    #[property(bindable)]
    #[browsable(false)]
    pub duration_text: String,
    #[property(bindable)]
    #[browsable(false)]
    pub is_leaf: bool,
    #[property(bindable)]
    #[browsable(false)]
    pub is_parent: bool,
    #[property(bindable)]
    #[browsable(false)]
    pub progress: f32,
    #[property(bindable)]
    #[browsable(false)]
    pub progress_label: String,
    #[property(bindable)]
    #[browsable(false)]
    pub estimated: f32,
    #[property(bindable)]
    #[browsable(false)]
    pub spent_text: String,
    #[property(bindable)]
    #[browsable(false)]
    pub budget: String,
    #[property(bindable)]
    #[browsable(false)]
    pub constraint: String,
    #[property(bindable)]
    #[browsable(false)]
    pub constraints: Rows,
    #[property(bindable)]
    #[browsable(false)]
    pub needs_date: bool,
    #[property(bindable)]
    #[browsable(false)]
    pub constraint_date: String,
    #[property(bindable)]
    #[browsable(false)]
    pub constraint_pending: bool,
    #[property(bindable)]
    #[browsable(false)]
    pub deadline: String,
    #[property(bindable)]
    #[browsable(false)]
    pub deadline_missed: bool,
    #[property(bindable)]
    #[browsable(false)]
    pub has_versions: bool,
    #[property(bindable)]
    #[browsable(false)]
    pub version: String,
    #[property(bindable)]
    #[browsable(false)]
    pub versions: Rows,
    #[property(bindable)]
    #[browsable(false)]
    pub description: String,
    #[property(bindable)]
    #[browsable(false)]
    pub has_cpm: bool,
    #[property(bindable)]
    #[browsable(false)]
    pub cpm_text: String,
    #[property(bindable)]
    #[browsable(false)]
    pub resource_items: String,
    #[property(bindable)]
    #[browsable(false)]
    pub pred_items: String,
    #[property(bindable)]
    #[browsable(false)]
    pub add_pred: String,
    #[property(bindable)]
    #[browsable(false)]
    pub pred_candidates: Rows,
    /// The task shown (its id), to tell a new task from a refresh of the same one.
    shown: String,
}

impl TaskInspector {
    /// Shows task `id` of `data` (or the hint when `None`); `progress` is the rolled-up value of a parent.
    pub fn show(&mut self, data: Option<&ProjectData>, id: Option<&str>, versions: &[(String, String)], show_versions: bool) {
        let task = data.zip(id).and_then(|(d, id)| d.task(id).cloned());
        let Some(t) = task else {
            self.show_hint = true;
            self.show_task = false;
            self.shown.clear();
            return;
        };
        let Some(d) = data else { return };
        let new_task = self.shown != t.id;
        self.shown = t.id.clone();
        self.show_hint = false;
        self.show_task = true;
        if self.types.is_empty() {
            self.types = rows(&[("task", Resources::type_task().into()), ("milestone", Resources::type_milestone().into()), ("summary", Resources::type_summary().into())]);
            self.priorities = rows(&[("low", Resources::priority_low().into()), ("medium", Resources::priority_medium().into()), ("high", Resources::priority_high().into()), ("critical", Resources::priority_critical().into())]);
            self.statuses = rows(&[
                ("not_started", Resources::status_not_started().into()),
                ("in_progress", Resources::status_in_progress().into()),
                ("completed", Resources::status_completed().into()),
                ("on_hold", Resources::status_on_hold().into()),
                ("cancelled", Resources::status_cancelled().into()),
            ]);
            self.constraints = rows(&[
                ("ASAP", Resources::c_asap().into()),
                ("ALAP", Resources::c_alap().into()),
                ("SNET", Resources::c_snet().into()),
                ("SNLT", Resources::c_snlt().into()),
                ("FNET", Resources::c_fnet().into()),
                ("FNLT", Resources::c_fnlt().into()),
                ("MSO", Resources::c_mso().into()),
                ("MFO", Resources::c_mfo().into()),
            ]);
        }
        let parent = d.tasks.iter().any(|x| x.parent_id.as_deref() == Some(t.id.as_str()));
        let rollup = kubuno_office_projects_core::rollup::effective_progress(&d.tasks).get(&t.id).copied().unwrap_or(t.progress);
        // A field being typed in keeps its text (the echo of the same value would move the caret).
        if new_task || self.task_name.trim() != t.name.trim() {
            self.task_name = t.name.clone();
        }
        self.task_type = t.task_type.clone();
        self.priority = t.priority.clone();
        self.status = t.status.clone();
        self.is_parent = parent;
        self.is_leaf = !parent;
        self.duration = t.duration_days as f32;
        self.duration_text = Resources::days_short().replace("{n}", &t.duration_days.to_string());
        let pct = if parent { rollup } else { t.progress };
        self.progress = pct as f32;
        self.progress_label = if parent { Resources::insp_progress_rollup().to_string() } else { Resources::insp_progress().replace("{value}", &t.progress.to_string()) };
        self.estimated = t.estimated_hours.unwrap_or(0.0) as f32;
        self.spent_text = t.spent_hours.map(|h| format!("{} h", trim_num(h))).unwrap_or_else(|| "-".into());
        if new_task {
            self.budget = t.budget_cost.map(trim_num).unwrap_or_default();
            self.description = t.description.clone();
        }
        if new_task || DATED.contains(&t.constraint_type.as_str()) || !DATED.contains(&self.constraint.as_str()) {
            self.constraint = t.constraint_type.clone();
            self.constraint_date = t.constraint_date.clone().unwrap_or_default();
        }
        self.needs_date = DATED.contains(&self.constraint.as_str());
        self.constraint_pending = self.needs_date && self.constraint_date.is_empty();
        self.deadline = t.deadline_date.clone().unwrap_or_default();
        self.deadline_missed = t.deadline_missed;
        self.has_versions = show_versions && !versions.is_empty();
        let mut vs: Vec<(&str, String)> = vec![("", Resources::insp_version_none().into())];
        vs.extend(versions.iter().map(|(id, n)| (id.as_str(), n.clone())));
        self.versions = rows(&vs);
        self.version = t.version_id.clone().unwrap_or_default();
        self.has_cpm = t.early_start.is_some();
        let ds = |n: Option<i32>| Resources::days_short().replace("{n}", &n.unwrap_or(0).to_string());
        let negative = t.total_float.is_some_and(|f| f < 0);
        let flag = if negative { format!(" {}", Resources::insp_negative_float()) } else if t.is_critical { format!(" {}", Resources::insp_critical()) } else { String::new() };
        let mut cpm = format!("ES : {}\nEF : {}\nLS : {}\nLF : {}\n{} {}{}", ds(t.early_start), ds(t.early_finish), ds(t.late_start), ds(t.late_finish), Resources::insp_float(), ds(t.total_float), flag);
        if let Some(ff) = t.free_float {
            cpm.push_str(&format!("\n{} {}", Resources::insp_free_float(), ds(Some(ff))));
        }
        self.cpm_text = cpm;
        self.resource_items = d
            .resources
            .iter()
            .map(|r| {
                let on = d.assignments.iter().any(|a| a.task_id == t.id && a.resource_id == r.id);
                let label = if r.role.is_empty() { r.name.clone() } else { format!("{} · {}", r.name, r.role) };
                format!("{}|{}|{}|{}", r.id, label.replace('|', "/"), r.color, if on { 1 } else { 0 })
            })
            .collect::<Vec<_>>()
            .join("\n");
        let numbers = view::task_numbers(d);
        self.pred_items = d
            .dependencies
            .iter()
            .filter(|x| x.to_task_id == t.id)
            .map(|x| {
                let name = d.task(&x.from_task_id).map(|f| f.name.clone()).unwrap_or_default();
                format!("{}|{} · {}|{}|{}", x.id, numbers.get(&x.from_task_id).copied().unwrap_or(0), name.replace('|', "/"), x.dep_type, x.lag_days)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let mut cand: Vec<(String, String)> = vec![(String::new(), Resources::insp_add_predecessor().into())];
        for o in &d.tasks {
            if o.id != t.id && !d.dependencies.iter().any(|x| x.to_task_id == t.id && x.from_task_id == o.id) {
                cand.push((o.id.clone(), format!("{} · {}", numbers.get(&o.id).copied().unwrap_or(0), o.name)));
            }
        }
        self.pred_candidates = Rows::from(cand.into_iter().map(|(v, l)| Row::new().with("Value", BValue::Str(v)).with("Label", BValue::Str(l))).collect::<Vec<_>>());
        self.add_pred.clear();
    }

    /// The task shown.
    pub fn task(&self) -> &str {
        &self.shown
    }

    fn emit(&mut self, field: &str, value: Value) {
        self.raise_inspector_edited(InspectorEditEventArgs { field: field.into(), value: value.to_string(), x: 0.0, y: 0.0 });
    }

    fn user(source: ChangeSource) -> bool {
        source == ChangeSource::User
    }
}

fn trim_num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

#[kubuno_desktop::views::event_handlers]
impl TaskInspector {
    fn name_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e.source) {
            self.task_name = e.new.clone();
            self.emit("name", json!(e.new));
        }
    }

    fn type_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e.source) && !e.new.is_empty() {
            self.emit("task_type", json!(e.new));
        }
    }

    fn priority_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e.source) && !e.new.is_empty() {
            self.emit("priority", json!(e.new));
        }
    }

    fn status_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e.source) && !e.new.is_empty() {
            self.emit("status", json!(e.new));
        }
    }

    fn duration_changed(&mut self, e: &NumericValueChangedEventArgs) {
        if Self::user(e.source) {
            // `parseInt(value) || 1`.
            let v = (e.new.round() as i64).max(1);
            self.emit("duration_days", json!(v));
        }
    }

    fn progress_changed(&mut self, e: &NumericValueChangedEventArgs) {
        if Self::user(e.source) {
            // The web's slider steps by 5.
            let v = ((e.new / 5.0).round() * 5.0).clamp(0.0, 100.0) as i64;
            self.emit("progress", json!(v));
        }
    }

    fn estimated_changed(&mut self, e: &NumericValueChangedEventArgs) {
        if Self::user(e.source) {
            self.emit("estimated_hours", json!(e.new as f64));
        }
    }

    fn budget_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e.source) {
            self.budget = e.new.clone();
            // Empty (or not a number) is « not costed », never 0.
            let v = e.new.trim().replace(',', ".").parse::<f64>().ok().filter(|n| n.is_finite());
            self.emit("budget_cost", v.map(|n| json!(n)).unwrap_or(Value::Null));
        }
    }

    fn constraint_changed(&mut self, e: &TextChangedEventArgs) {
        if !Self::user(e.source) || e.new.is_empty() {
            return;
        }
        self.constraint = e.new.clone();
        self.needs_date = DATED.contains(&e.new.as_str());
        if !self.needs_date {
            self.constraint_date.clear();
            self.constraint_pending = false;
            self.emit("constraint", json!({ "constraint_type": e.new, "constraint_date": null }));
        } else if !self.constraint_date.is_empty() {
            self.constraint_pending = false;
            self.emit("constraint", json!({ "constraint_type": e.new, "constraint_date": self.constraint_date }));
        } else {
            // Held locally until a date is supplied.
            self.constraint_pending = true;
        }
    }

    fn constraint_date_changed(&mut self, e: &TextChangedEventArgs) {
        if !Self::user(e.source) {
            return;
        }
        self.constraint_date = e.new.clone();
        self.constraint_pending = self.constraint_date.is_empty();
        if !self.constraint_date.is_empty() && DATED.contains(&self.constraint.as_str()) {
            self.emit("constraint", json!({ "constraint_type": self.constraint, "constraint_date": self.constraint_date }));
        }
    }

    fn deadline_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e.source) {
            self.emit("deadline_date", if e.new.is_empty() { Value::Null } else { json!(e.new) });
        }
    }

    fn version_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e.source) {
            self.emit("version_id", if e.new.is_empty() { Value::Null } else { json!(e.new) });
        }
    }

    fn description_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e.source) {
            self.description = e.new.clone();
            self.emit("description", json!(e.new));
        }
    }

    fn resources_action(&mut self, e: &ListActionEventArgs) {
        let on = self.resource_items.lines().find(|l| l.starts_with(&format!("{}|", e.key))).is_some_and(|l| l.ends_with("|1"));
        self.emit(if on { "unassign" } else { "assign" }, json!(e.key));
    }

    fn pred_action(&mut self, e: &ListActionEventArgs) {
        let field = match e.action.as_str() {
            "type" => "dep_type",
            "lag" => "dep_lag",
            _ => "dep_delete",
        };
        self.raise_inspector_edited(InspectorEditEventArgs { field: field.into(), value: json!(e.key).to_string(), x: e.x, y: e.y });
    }

    fn add_pred_changed(&mut self, e: &TextChangedEventArgs) {
        if Self::user(e.source) && !e.new.is_empty() {
            self.emit("dep_add", json!(e.new));
        }
    }
}
