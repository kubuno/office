//! What the window and its plan views share: the open project's editor and the view state the web keeps in
//! `ProjectEditorPage`'s React state (selection, folded summaries, filter, zoom, timeline, compared baseline,
//! the table's columns), behind one `Rc<RefCell<…>>` so the Gantt, the board, the calendar and the load views
//! show and edit the same thing. The palette of the plan views is here too: the web's theme tokens, light and
//! dark (`core/web/src/theme.css`, `core/web/themes/kubuno-dark/theme.json`).

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use kubuno_desktop::ui::graphics::Color;
use kubuno_office_data_common::model::columns::Layout;
use kubuno_office_data_core::date::{Date, Lang};
use kubuno_office_data_core::editor::Editor;
use kubuno_office_data_core::gantt::Zoom;
use kubuno_office_data_core::model::ProjectData;
use kubuno_office_data_core::view::{self, Filter, Row};

/// The view state of the open project.
#[derive(Debug, Clone)]
pub struct ViewState {
    pub selected: Option<String>,
    pub collapsed: HashSet<String>,
    pub filter: Filter,
    pub zoom: Zoom,
    pub show_timeline: bool,
    pub baseline_id: Option<String>,
    pub layout: Layout,
    pub lang: Lang,
    pub today: Date,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            selected: None,
            collapsed: HashSet::new(),
            filter: Filter::default(),
            zoom: Zoom::Day,
            show_timeline: true,
            baseline_id: None,
            layout: Layout::default(),
            lang: Lang::Fr,
            today: Date::new(2026, 10, 7).unwrap_or(Date { year: 2026, month: 10, day: 7 }),
        }
    }
}

/// The editor (once a project is open) and the view state.
#[derive(Default)]
pub struct Model {
    pub ed: Option<Editor>,
    pub view: ViewState,
}

impl Model {
    pub fn data(&self) -> Option<&ProjectData> {
        self.ed.as_ref().map(|e| e.data())
    }

    /// The table's rows (`visibleTasks`).
    pub fn rows(&self) -> Vec<Row> {
        match self.data() {
            Some(d) => view::visible_rows(d, &self.view.filter, &self.view.collapsed),
            None => Vec::new(),
        }
    }

    /// The selected task, if it still exists (an id the server renamed is followed).
    pub fn selected_task(&self) -> Option<String> {
        let ed = self.ed.as_ref()?;
        let id = ed.resolve(self.view.selected.as_deref()?);
        ed.data().task(&id).map(|t| t.id.clone())
    }

    /// The compared baseline: task id → (planned start, calendar span) (`baselineMap`).
    pub fn baseline_map(&self) -> Option<std::collections::HashMap<String, (i32, i32)>> {
        let ed = self.ed.as_ref()?;
        let b = ed.baselines.iter().find(|b| Some(&b.id) == self.view.baseline_id.as_ref())?;
        Some(
            b.tasks
                .iter()
                .map(|s| {
                    let es = s.early_start.unwrap_or(0);
                    (s.task_id.clone(), (es, (s.early_finish.unwrap_or(es + s.duration_days) - es).max(0)))
                })
                .collect(),
        )
    }
}

/// The shared model.
pub type Shared = Rc<RefCell<Model>>;

pub fn shared() -> Shared {
    Rc::new(RefCell::new(Model::default()))
}

/// A model with the sample project (the designer, `--sample`).
pub fn sample_model() -> Model {
    let data = kubuno_office_data_common::sample::project();
    let today = Date::new(2026, 10, 14).unwrap_or(Date { year: 2026, month: 10, day: 14 });
    let mut m = Model { ed: Some(Editor::new(data, today, false)), view: ViewState::default() };
    m.view.today = today;
    m.view.selected = Some("t3".into());
    m
}

/// `#rrggbb`.
pub const fn hex(v: u32) -> Color {
    Color::rgb(((v >> 16) & 0xff) as u8, ((v >> 8) & 0xff) as u8, (v & 0xff) as u8)
}

/// The theme tokens the plan views paint with (`GanttRenderer.theme()`).
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub surface0: Color,
    pub surface1: Color,
    pub surface2: Color,
    pub surface3: Color,
    pub border: Color,
    pub primary: Color,
    pub danger: Color,
    pub success: Color,
    pub warning: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_tertiary: Color,
    /// The rows' hairline (`border-[#f1f3f4]`, a literal: the same in both themes on the web).
    pub row_line: Color,
}

impl Palette {
    pub fn light() -> Self {
        Self {
            dark: false,
            surface0: hex(0xffffff),
            surface1: hex(0xf8f9fa),
            surface2: hex(0xf1f3f4),
            surface3: hex(0xe8eaed),
            border: hex(0xe0e0e0),
            primary: hex(0x1a73e8),
            danger: hex(0xd93025),
            success: hex(0x1e8e3e),
            warning: hex(0xf9ab00),
            text_primary: hex(0x444444),
            text_secondary: hex(0x5f6368),
            text_tertiary: hex(0x80868b),
            row_line: hex(0xf1f3f4),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            surface0: hex(0x202124),
            surface1: hex(0x292a2d),
            surface2: hex(0x35363a),
            surface3: hex(0x444746),
            border: hex(0x5f6368),
            primary: hex(0x8ab4f8),
            danger: hex(0xf28b82),
            success: hex(0x81c995),
            warning: hex(0xfdd663),
            text_primary: hex(0xe8eaed),
            text_secondary: hex(0x9aa0a6),
            text_tertiary: hex(0x80868b),
            row_line: hex(0xf1f3f4),
        }
    }

    pub fn of(dark: bool) -> Self {
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }
}

/// `color-mix`-free tints: `c` at `alpha` over the background.
pub fn tint(c: Color, alpha: f32) -> Color {
    c.with_alpha(alpha)
}
