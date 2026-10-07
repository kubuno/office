//! Code-behind of the user control `FormatPanel` (`format_panel.kbcontrol`): the Propriétés panel.

use kubuno_desktop::views::prelude::*;

/// The Propriétés panel (see the module doc).
#[derive(UserControl, Default)]
#[user_control(view = "format_panel.kbcontrol")]
#[category("Diagrams")]
pub struct FormatPanel {
    base: UserControlCore,
}
