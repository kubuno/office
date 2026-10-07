//! Kubuno Projects for Windows — the Win32 window of the Office module's desktop project editor, written like a
//! Windows Forms application, over the portable app of desktop/common (`kubuno-office-projects-common`: the
//! launch options, the server session, the column layout) and the shared engine (`kubuno-office-projects-core`,
//! common/projects-core: the project, the scheduler, the Gantt's geometry, the editing commands).
//!
//! The library holds everything the Visual Studio designer links to render the views:
//!
//! - `views/` — the main form [`ProjectWindow`] with its declarative ribbon, and its dialogs;
//! - `pages/` — the user controls [`BackstageInfo`], [`ProjectProperties`] and [`TaskInspector`];
//! - `controls/` — the custom-drawn controls [`GanttView`], [`TimelineBand`], [`PlanTabs`], [`BoardView`],
//!   [`MonthView`], [`LoadView`], [`CheckList`], [`PredecessorList`];
//! - `platform/` — the Windows overrides of the common extension points (the local clock, the clipboard);
//! - `resources/` — the strings and the icon (`resources.kbres`, `resources.fr.kbres`).

pub mod controls;
pub mod pages;
pub mod platform;
pub mod views;
mod windows_ui;

pub use controls::board::BoardView;
pub use controls::gantt::GanttView;
pub use controls::lists::{CheckList, PredecessorList};
pub use controls::load::LoadView;
pub use controls::month::MonthView;
pub use controls::plan_tabs::PlanTabs;
pub use controls::timeline_band::TimelineBand;
pub use pages::backstage_info::BackstageInfo;
pub use pages::project_properties::ProjectProperties;
pub use pages::task_inspector::TaskInspector;
pub use views::project_window::{Options, ProjectWindow};
pub use windows_ui::WindowsUi;

// `Resources::app_title()`… — the strings of `resources/resources.kbres` (neutral English) and
// `resources/resources.fr.kbres`, in the current UI culture; `{Res key}` in the views.
kubuno_desktop::resources!(pub Resources, "resources/resources.kbres");
