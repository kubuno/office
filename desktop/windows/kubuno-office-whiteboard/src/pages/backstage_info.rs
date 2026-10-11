//! Code-behind of the user control `BackstageInfo` (`backstage_info.kbcontrol`): « Fichier › Informations ».
//! It has no state of its own: the window binds its properties to what the canvas reports.

use kubuno_desktop::views::prelude::*;

/// « Fichier › Informations » (see the module doc).
#[derive(UserControl, Default)]
#[user_control(view = "backstage_info.kbcontrol")]
#[category("Whiteboard")]
pub struct BackstageInfo {
    base: UserControlCore,
    /// The board's title.
    #[property(bindable)]
    #[category("Data")]
    pub document_title: String,
    /// The canvas background (`white | grid | dots | lines`).
    #[property(bindable)]
    #[category("Data")]
    pub background: String,
    #[property(bindable)]
    #[category("Data")]
    pub elements: String,
    #[property(bindable)]
    #[category("Data")]
    pub strokes: String,
}
