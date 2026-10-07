//! Code-behind of the user control `BackstageInfo` (`backstage_info.kbcontrol`): the « Informations »
//! tab of the Backstage. It has no state of its own: the form binds its properties.

use kubuno_desktop::views::prelude::*;

/// « Fichier › Informations » (see the module doc).
#[derive(UserControl, Default)]
#[user_control(view = "backstage_info.kbcontrol")]
#[category("Spreadsheets")]
pub struct BackstageInfo {
    base: UserControlCore,
    /// The workbook's title.
    #[property(bindable)]
    #[category("Data")]
    pub workbook_title: String,
    /// How many sheets it has.
    #[property(bindable)]
    #[category("Data")]
    pub sheets: String,
    /// Where it comes from (a local file, the server).
    #[property(bindable)]
    #[category("Data")]
    pub source: String,
}
