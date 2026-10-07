//! Code-behind of the user control `ProjectProperties` (`project_properties.kbcontrol`): « Fichier ›
//! Propriétés ». The form shows the project ([`ProjectProperties::show`]); a change leaves as
//! `PropertiesEdited` (`field`, JSON `value`).

use kubuno_desktop::views::events::ChangeSource;
use kubuno_desktop::views::prelude::*;
use kubuno_desktop::{Row, Rows, Value as BValue};
use kubuno_office_projects_core::model::Project;
use serde_json::{json, Value};

use crate::Resources;

/// A change of a project property: `start_date`, `end_date`, `status`, `color`, `description`.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct PropertiesEditEventArgs {
    pub field: String,
    pub value: String,
}

/// The web's eight project colours (`ProjectPropertiesPanel.tsx:9`).
pub const COLORS: [&str; 8] = ["#1a73e8", "#188038", "#e37400", "#d93025", "#9334e6", "#00897b", "#c2185b", "#5f6368"];

#[derive(UserControl, Default)]
#[user_control(view = "project_properties.kbcontrol")]
#[category("Projects")]
pub struct ProjectProperties {
    base: UserControlCore,
    #[event]
    #[category("Action")]
    pub properties_edited: Event<PropertiesEditEventArgs>,
    #[property(bindable)]
    #[browsable(false)]
    pub start_date: String,
    #[property(bindable)]
    #[browsable(false)]
    pub end_date: String,
    #[property(bindable)]
    #[browsable(false)]
    pub status: String,
    #[property(bindable)]
    #[browsable(false)]
    pub statuses: Rows,
    #[property(bindable)]
    #[browsable(false)]
    pub color: String,
    #[property(bindable)]
    #[browsable(false)]
    pub colors: Rows,
    #[property(bindable)]
    #[browsable(false)]
    pub description: String,
    shown: String,
}

impl ProjectProperties {
    pub fn show(&mut self, p: &Project) {
        if self.statuses.is_empty() {
            let s = [("active", Resources::pstatus_active()), ("on_hold", Resources::pstatus_on_hold()), ("completed", Resources::pstatus_completed()), ("cancelled", Resources::pstatus_cancelled())];
            self.statuses = Rows::from(s.iter().map(|(v, l)| Row::new().with("Value", BValue::Str((*v).into())).with("Label", BValue::Str((*l).into()))).collect::<Vec<_>>());
            self.colors = Rows::from(COLORS.iter().map(|c| Row::new().with("Value", BValue::Str((*c).into())).with("Label", BValue::Str((*c).into()))).collect::<Vec<_>>());
        }
        self.start_date = p.start_date.clone().unwrap_or_default();
        self.end_date = p.end_date.clone().unwrap_or_default();
        self.status = p.status.clone();
        self.color = p.color.clone();
        if self.shown != p.id {
            self.description = p.description.clone();
            self.shown = p.id.clone();
        }
    }

    fn emit(&mut self, field: &str, value: Value) {
        self.raise_properties_edited(PropertiesEditEventArgs { field: field.into(), value: value.to_string() });
    }
}

#[kubuno_desktop::views::event_handlers]
impl ProjectProperties {
    fn start_changed(&mut self, e: &TextChangedEventArgs) {
        if e.source == ChangeSource::User && !e.new.is_empty() {
            self.emit("start_date", json!(e.new));
        }
    }

    fn end_changed(&mut self, e: &TextChangedEventArgs) {
        if e.source == ChangeSource::User && !e.new.is_empty() {
            self.emit("end_date", json!(e.new));
        }
    }

    fn status_changed(&mut self, e: &TextChangedEventArgs) {
        if e.source == ChangeSource::User && !e.new.is_empty() {
            self.emit("status", json!(e.new));
        }
    }

    fn color_changed(&mut self, e: &TextChangedEventArgs) {
        if e.source == ChangeSource::User && !e.new.is_empty() {
            self.emit("color", json!(e.new));
        }
    }

    fn description_changed(&mut self, e: &TextChangedEventArgs) {
        if e.source == ChangeSource::User {
            self.description = e.new.clone();
        }
    }

    /// The web saves the description on blur; here this button saves it.
    fn description_save_click(&mut self) {
        let d = self.description.clone();
        self.emit("description", json!(d));
    }
}
