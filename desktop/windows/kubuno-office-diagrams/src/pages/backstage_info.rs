//! Code-behind of the user control `BackstageInfo` (`backstage_info.kbcontrol`): « Fichier › Informations ».
//! It has no state of its own: the form binds its properties to what the canvas reports.

use kubuno_desktop::views::prelude::*;

/// « Fichier › Informations » (see the module doc).
#[derive(UserControl, Default)]
#[user_control(view = "backstage_info.kbcontrol")]
#[category("Diagrams")]
pub struct BackstageInfo {
    base: UserControlCore,
    /// The diagram's title (its file name).
    #[property(bindable)]
    #[category("Data")]
    pub document_title: String,
    /// When it was last modified (server diagrams).
    #[property(bindable)]
    #[category("Data")]
    pub modified: String,
    #[property(bindable)]
    #[category("Data")]
    pub shapes: String,
    #[property(bindable)]
    #[category("Data")]
    pub connectors: String,
    #[property(bindable)]
    #[category("Data")]
    pub pages: String,
}
