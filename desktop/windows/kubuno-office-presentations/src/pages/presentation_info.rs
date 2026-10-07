//! Code-behind of the user control `PresentationInfo` (`presentation_info.kbcontrol`): the « Informations » tab
//! of the Backstage. It has no state of its own: the form binds its three properties.

use kubuno_desktop::views::prelude::*;

/// « Fichier › Informations » (see the module doc).
#[derive(UserControl, Default)]
#[user_control(view = "presentation_info.kbcontrol")]
#[category("Presentations")]
pub struct PresentationInfo {
    base: UserControlCore,
    /// The presentation's title.
    #[property(bindable)]
    #[category("Data")]
    pub presentation_title: String,
    /// Where it comes from (server, local file, sample).
    #[property(bindable)]
    #[category("Data")]
    pub format: String,
    /// How many slides it has.
    #[property(bindable)]
    #[category("Data")]
    pub slides: String,
}
