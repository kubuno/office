//! Code-behind of the user control `BackstageInfo` (`backstage_info.kbcontrol`): « Fichier › Informations ».
//! It has no state of its own: the form binds its properties.

use kubuno_desktop::views::prelude::*;

/// « Fichier › Informations » (see the module doc).
#[derive(UserControl, Default)]
#[user_control(view = "backstage_info.kbcontrol")]
#[category("Projects")]
pub struct BackstageInfo {
    base: UserControlCore,
    #[property(bindable)]
    #[category("Data")]
    pub project_title: String,
    #[property(bindable)]
    #[category("Data")]
    pub modified: String,
    #[property(bindable)]
    #[category("Data")]
    pub tasks: String,
    #[property(bindable)]
    #[category("Data")]
    pub resource_count: String,
    #[property(bindable)]
    #[category("Data")]
    pub estimated: String,
    #[property(bindable)]
    #[category("Data")]
    pub spent: String,
}
