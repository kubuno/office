//! Code-behind of Kubuno Projects' window (`project_window.kbview`) — Windows Forms' `Form1.cs`.
//!
//! The window owns the shared model (the editor and the view state, `controls::model`) that the plan views
//! paint and edit, the server session of a server project (`--doc <id>`: the outbox worker of
//! desktop/common), and turns the ribbon's commands, the menus, the inspector, the properties page, the filter
//! and baseline bars into the editor's commands — the web editor's (`ProjectEditorPage.tsx`).

use std::path::PathBuf;

use kubuno_desktop::prelude::*;
use kubuno_desktop::views::component::Control as _;
use kubuno_desktop::{Row, Rows, Value as BValue};
use kubuno_office_projects_common::api::journal::FileJournal;
use kubuno_office_projects_common::api::worker::{self, Event as WorkerEvent, Worker};
use kubuno_office_projects_common::model::columns::{Col, Layout};
use kubuno_office_projects_common::session::{ProjectSession, SyncState};
use kubuno_office_projects_core::change::Refused;
use kubuno_office_projects_core::date::{self, DateFormat, Lang};
use kubuno_office_projects_core::editor::Editor;
use kubuno_office_projects_core::gantt::{self, Zoom};
use kubuno_office_projects_core::model::ProjectData;
use kubuno_office_projects_core::view;
use serde_json::{json, Map, Value};

use crate::controls::board::BoardView;
use crate::controls::gantt::{GanttContextEventArgs, GanttEditedEventArgs, GanttStateEventArgs, GanttView};
use crate::controls::lists::{CheckList, ListActionEventArgs};
use crate::controls::load::LoadView;
use crate::controls::model::{shared, Shared};
use crate::controls::month::MonthView;
use crate::controls::network::NetworkView;
use crate::controls::plan_tabs::PlanTabEventArgs;
use crate::controls::roadmap::{RoadmapActionEventArgs, RoadmapView};
use crate::controls::timeline_band::{BandSelectEventArgs, TimelineBand};
use crate::pages::project_properties::{ProjectProperties, PropertiesEditEventArgs};
use crate::pages::task_inspector::{InspectorEditEventArgs, TaskInspector};
use crate::Resources;

pub use kubuno_office_projects_common::app::Options;

/// The plan views (`planViews`) and their artifact keys (`uses`).
const PLAN_VIEWS: [(&str, &str, &str); 6] = [("gantt", "schedule", "BarChart2"), ("board", "board", "KanbanSquare"), ("calendar", "calendar", "CalendarDays"), ("resources", "workload", "Users"), ("pert", "network", "Network"), ("roadmap", "roadmap", "Milestone")];

/// Kubuno Projects' main window.
#[kubuno_desktop::view("project_window.kbview")]
pub struct ProjectWindow {
    #[control]
    gantt: Custom<GanttView>,
    #[control]
    board: Custom<BoardView>,
    #[control]
    month: Custom<MonthView>,
    #[control]
    load: Custom<LoadView>,
    #[control]
    network: Custom<NetworkView>,
    #[control]
    roadmap: Custom<RoadmapView>,
    #[control]
    band: Custom<TimelineBand>,
    #[control]
    inspector: Custom<TaskInspector>,
    #[control]
    properties: Custom<ProjectProperties>,
    #[control]
    res_list: Custom<CheckList>,
    #[bind]
    window_title: String,
    #[bind]
    can_undo: bool,
    #[bind]
    can_redo: bool,
    #[bind]
    starred: bool,
    #[bind]
    online: bool,
    #[bind]
    has_project: bool,
    #[bind]
    has_selection: bool,
    #[bind]
    has_parent: bool,
    #[bind]
    can_link: bool,
    #[bind]
    can_set_progress: bool,
    #[bind]
    st_todo: bool,
    #[bind]
    st_prog: bool,
    #[bind]
    st_hold: bool,
    #[bind]
    st_done: bool,
    #[bind]
    st_cancel: bool,
    #[bind]
    pr_low: bool,
    #[bind]
    pr_medium: bool,
    #[bind]
    pr_high: bool,
    #[bind]
    pr_critical: bool,
    #[bind]
    ty_task: bool,
    #[bind]
    ty_milestone: bool,
    #[bind]
    ty_summary: bool,
    #[bind]
    show_timeline: bool,
    #[bind]
    show_band: bool,
    #[bind]
    show_filter_bar: bool,
    #[bind]
    can_zoom_in: bool,
    #[bind]
    can_zoom_out: bool,
    // Baselines.
    #[bind]
    uses_baselines: bool,
    #[bind]
    has_baselines: bool,
    #[bind]
    has_baseline: bool,
    #[bind]
    can_set_primary: bool,
    #[bind]
    baseline_value: String,
    #[bind]
    baseline_items: Rows,
    #[bind]
    baseline_text: String,
    // Columns.
    #[bind]
    col_idx: bool,
    #[bind]
    col_mode: bool,
    #[bind]
    col_dur: bool,
    #[bind]
    col_progress: bool,
    #[bind]
    col_priority: bool,
    #[bind]
    col_start: bool,
    #[bind]
    col_end: bool,
    #[bind]
    col_variance: bool,
    #[bind]
    col_pred: bool,
    #[bind]
    col_res: bool,
    #[bind]
    compact: bool,
    #[bind]
    cols_customised: bool,
    #[bind]
    variance_label: String,
    // The plan views.
    #[bind]
    plan_items: String,
    #[bind]
    active_view: String,
    #[bind]
    is_gantt: bool,
    #[bind]
    is_board: bool,
    #[bind]
    is_calendar: bool,
    #[bind]
    is_load: bool,
    #[bind]
    is_roadmap: bool,
    #[bind]
    is_network: bool,
    // The network view's menus.
    #[bind]
    net_timescale: bool,
    #[bind]
    net_snap: bool,
    #[bind]
    net_snap_enabled: bool,
    #[bind]
    net_can_rearrange: bool,
    #[bind]
    net_edge_title: String,
    #[bind]
    net_lag_label: String,
    #[bind]
    net_dep_fs: bool,
    #[bind]
    net_dep_ss: bool,
    #[bind]
    net_dep_ff: bool,
    #[bind]
    net_dep_sf: bool,
    /// The version whose status menu is open (Roadmap).
    menu_version: Option<String>,
    #[bind]
    is_todo: bool,
    // The filter bar.
    #[bind]
    filter_text: String,
    #[bind]
    filter_status: String,
    #[bind]
    filter_priority: String,
    #[bind]
    sort_by: String,
    #[bind]
    group_by: String,
    #[bind]
    status_filter_items: Rows,
    #[bind]
    priority_filter_items: Rows,
    #[bind]
    sort_items: Rows,
    #[bind]
    group_items: Rows,
    #[bind]
    filter_any: bool,
    #[bind]
    filter_count: String,
    // Backstage › Informations.
    #[bind]
    project_title: String,
    #[bind]
    modified_text: String,
    #[bind]
    task_count_text: String,
    #[bind]
    resource_count_text: String,
    #[bind]
    estimated_text: String,
    #[bind]
    spent_text: String,
    // The status bar.
    #[bind]
    status_tasks: String,
    #[bind]
    status_critical: String,
    #[bind]
    status_done: String,
    #[bind]
    status_finish: String,
    #[bind]
    has_finish: bool,
    #[bind]
    has_overrun: bool,
    #[bind]
    note: String,
    #[bind]
    has_note: bool,
    #[bind]
    sync_status: String,
    #[bind]
    has_sync: bool,
    #[bind]
    zoom_text: String,
    // The resources panel.
    #[bind]
    new_res_name: String,
    #[bind]
    can_add_resource: bool,
    #[bind]
    resource_list_items: String,
    model: Shared,
    pending: Option<ProjectData>,
    options: Options,
    session: Option<ProjectSession>,
    worker: Option<Worker>,
    file: Option<PathBuf>,
    /// A local project changed since it was opened or saved.
    local_dirty: bool,
    /// The task a menu was opened for, the link an inspector menu is about.
    ctx_task: Option<String>,
    ctx_dep: Option<String>,
    last_revision: u64,
}

fn rows(items: &[(&str, String)]) -> Rows {
    Rows::from(items.iter().map(|(v, l)| Row::new().with("Value", BValue::Str((*v).into())).with("Label", BValue::Str(l.clone()))).collect::<Vec<_>>())
}

fn now_ms() -> i64 {
    kubuno_office_projects_common::platform::current().clock.now_ms()
}

impl ProjectWindow {
    pub fn new(options: Options, project: Option<ProjectData>) -> Self {
        let model = shared();
        {
            let mut m = model.borrow_mut();
            m.view.lang = options.lang();
            m.view.today = kubuno_office_projects_common::platform::current().clock.today();
            m.view.zoom = match options.zoom.as_deref() {
                Some("week") => Zoom::Week,
                Some("month") => Zoom::Month,
                _ => Zoom::Day,
            };
            m.view.selected = options.select.clone();
        }
        let view = options.view.clone().filter(|v| PLAN_VIEWS.iter().any(|p| p.0 == v) || v == "load").unwrap_or_else(|| "gantt".into());
        let mut window = Self {
            window_title: Resources::app_title().to_string(),
            model,
            pending: project,
            file: options.file.clone(),
            options,
            active_view: view,
            show_timeline: true,
            ..Self::default()
        };
        window.initialize_component();
        window
    }

    fn project_window_load(&mut self) {
        let m = self.model.clone();
        self.gantt.with(|g| g.set_model(m.clone()));
        self.board.with(|b| b.set_model(m.clone()));
        self.month.with(|v| v.set_model(m.clone()));
        self.load.with(|v| v.set_model(m.clone()));
        self.network.with(|v| v.set_model(m.clone()));
        self.roadmap.with(|v| v.set_model(m.clone()));
        self.band.with(|b| b.set_model(m.clone()));
        self.status_filter_items = rows(&[
            ("", Resources::all_statuses().into()),
            ("not_started", Resources::status_not_started().into()),
            ("in_progress", Resources::status_in_progress().into()),
            ("on_hold", Resources::status_on_hold().into()),
            ("completed", Resources::status_completed().into()),
            ("cancelled", Resources::status_cancelled().into()),
        ]);
        self.priority_filter_items = rows(&[("", Resources::all_priorities().into()), ("low", Resources::priority_low().into()), ("medium", Resources::priority_medium().into()), ("high", Resources::priority_high().into()), ("critical", Resources::priority_critical().into())]);
        self.sort_items = rows(&[
            ("", Resources::sort_none().into()),
            ("name", Resources::sort_name().into()),
            ("start", Resources::sort_start().into()),
            ("end", Resources::sort_end().into()),
            ("duration", Resources::sort_duration().into()),
            ("priority", Resources::sort_priority().into()),
            ("progress", Resources::sort_progress().into()),
            ("status", Resources::sort_status().into()),
        ]);
        self.group_items = rows(&[("", Resources::group_none().into()), ("status", Resources::group_status().into()), ("priority", Resources::group_priority().into()), ("resource", Resources::group_resource().into())]);
        if let Some(data) = self.pending.take() {
            self.open_local(data);
        }
        if let Some(id) = self.options.doc.clone() {
            self.start_session(id);
        }
        let mut header = kubuno_desktop_header_data::FeedConfig::new("kubuno-projects");
        if !self.options.sample {
            header.proxy = kubuno_desktop_sync::get_proxy();
        }
        kubuno_desktop_header_data::start(kubuno_desktop_header_data::HeaderOptions::for_app(&["office-projects"]), header, self.options.sample, self.dispatcher());
        self.refresh();
    }

    fn open_local(&mut self, data: ProjectData) {
        let today = self.model.borrow().view.today;
        let mut ed = Editor::new(data, today, false);
        ed.set_now(now_ms());
        self.model.borrow_mut().ed = Some(ed);
        self.local_dirty = false;
    }

    // ── A project of the server ─────────────────────────────────────────────

    fn start_session(&mut self, id: String) {
        let Some(dispatcher) = self.dispatcher() else {
            kubuno_desktop::tracing::warn!("[projects] the window has no dispatcher: the server project cannot open");
            return;
        };
        let post = dispatcher.clone();
        let sink: worker::Sink = std::sync::Arc::new(move |event| {
            drop(post.begin_invoke(move |w: &mut ProjectWindow| w.on_worker(event)));
        });
        let w = Worker::start(id.clone(), Box::new(worker::connect_account), sink);
        let tx = w.sender();
        let session = ProjectSession::new(id, Box::new(move |job| {
            let _ = tx.send(job);
        }), FileJournal::for_user());
        // What the journal kept shows at once.
        let today = self.model.borrow().view.today;
        if let Some(ed) = session.restore(today) {
            self.model.borrow_mut().ed = Some(ed);
        }
        self.session = Some(session);
        self.worker = Some(w);
        self.sync_status = Resources::sync_opening().to_string();
        self.has_sync = true;
    }

    fn on_worker(&mut self, ev: WorkerEvent) {
        let Some(session) = self.session.as_mut() else { return };
        let today = self.model.borrow().view.today;
        let out = {
            let mut m = self.model.borrow_mut();
            session.on_event(&mut m.ed, ev, today, now_ms())
        };
        self.after_session(out);
    }

    fn after_session(&mut self, out: kubuno_office_projects_common::session::Outcome) {
        if !out.messages.is_empty() {
            self.note = out.messages.join(" · ");
            self.has_note = true;
        }
        if let Some(names) = out.conflict {
            let text = Resources::conflict_text().replace("{names}", &names.join(", "));
            let answer = super::confirm_dialog::ConfirmDialog::choose(self, Resources::conflict_title(), &text, Resources::conflict_mine(), Resources::conflict_theirs());
            let keep = match answer {
                super::confirm_dialog::Answer::First => Some(true),
                super::confirm_dialog::Answer::Second => Some(false),
                super::confirm_dialog::Answer::Dismissed => None,
            };
            if let (Some(keep), Some(s)) = (keep, self.session.as_mut()) {
                let out = {
                    let mut m = self.model.borrow_mut();
                    match m.ed.as_mut() {
                        Some(ed) => s.resolve(ed, keep),
                        None => Default::default(),
                    }
                };
                return self.after_session(out);
            }
        }
        self.refresh();
    }

    fn sync_tick(&mut self) {
        if let Some(s) = self.session.as_mut() {
            let out = {
                let mut m = self.model.borrow_mut();
                s.tick(&mut m.ed, now_ms())
            };
            if out.changed || !out.messages.is_empty() || out.conflict.is_some() {
                self.after_session(out);
            } else {
                self.refresh_sync();
            }
        }
    }

    fn project_window_closing(&mut self) {
        self.gantt.with(|g| g.commit_edit());
        if let Some(mut s) = self.session.take() {
            let m = self.model.borrow();
            if s.close(m.ed.as_ref()) {
                kubuno_desktop::tracing::warn!("[projects] unsent changes kept in the journal");
            }
        }
        if let Some(w) = self.worker.take() {
            if !w.close(std::time::Duration::from_secs(5)) {
                kubuno_desktop::tracing::warn!("[projects] the worker did not finish in time");
            }
        }
    }

    // ── Commands ────────────────────────────────────────────────────────────

    fn selected(&self) -> Option<String> {
        self.ctx_task.clone().filter(|id| self.model.borrow().data().is_some_and(|d| d.task(id).is_some())).or_else(|| self.model.borrow().selected_task())
    }

    /// Runs an editor command, then hands the outbox to the session (or marks the local file changed).
    fn cmd(&mut self, f: impl FnOnce(&mut Editor) -> Result<(), Refused>) {
        self.clear_note();
        {
            let mut m = self.model.borrow_mut();
            let Some(ed) = m.ed.as_mut() else { return };
            ed.set_now(now_ms());
            let _ = f(ed);
        }
        self.after_edit();
    }

    fn after_edit(&mut self) {
        let msgs = {
            let mut m = self.model.borrow_mut();
            match (self.session.as_mut(), m.ed.as_mut()) {
                (Some(s), Some(ed)) => {
                    let out = s.after_command(ed);
                    out.messages
                }
                (None, Some(ed)) => {
                    self.local_dirty = true;
                    // A local file has no server: its outbox has nowhere to go.
                    ed.take_messages()
                }
                _ => Vec::new(),
            }
        };
        if !msgs.is_empty() {
            self.note = msgs.join(" · ");
            self.has_note = true;
        }
        self.refresh();
    }

    fn with_selected(&mut self, f: impl FnOnce(&mut Editor, &str) -> Result<(), Refused>) {
        let Some(id) = self.selected() else { return };
        self.cmd(|ed| f(ed, &id));
    }

    fn clear_note(&mut self) {
        self.has_note = false;
        self.note.clear();
    }

    fn view_edited(&mut self, _e: &GanttEditedEventArgs) {
        self.after_edit();
    }

    fn gantt_state_changed(&mut self, _e: &GanttStateEventArgs) {
        self.ctx_task = None;
        self.refresh();
    }

    // ── Roadmap ─────────────────────────────────────────────────────────────

    fn roadmap_action(&mut self, e: &RoadmapActionEventArgs) {
        match e.action.as_str() {
            "new" => self.cmd(|ed| ed.create_version(None).map(|_| ())),
            "delete" => {
                let id = e.id.clone();
                self.cmd(|ed| ed.delete_version(&id));
            }
            "task" => self.refresh(),
            "status" => {
                self.menu_version = Some(e.id.clone());
                kubuno_desktop::views::window::show_context_menu("version_status_menu", kubuno_desktop::views::window::MenuAnchor::Point(e.x, e.y));
            }
            "rename" => {
                let current = self.version_name(&e.id);
                if let Some(name) = super::prompt_dialog::PromptDialog::ask(self, Resources::version_name(), Resources::version_rename_prompt(), &current) {
                    let id = e.id.clone();
                    self.cmd(|ed| ed.update_version(&id, Map::from_iter([("name".to_string(), json!(name))])));
                }
            }
            "due" => {
                let current = self.version_due(&e.id);
                if let Some(text) = super::prompt_dialog::PromptDialog::ask(self, Resources::version_due(), Resources::version_due_prompt(), &current) {
                    // `e.target.value || null`: an empty field sends null, which the server's COALESCE ignores.
                    let value = match parse_day(text.trim()) {
                        Some(iso) => json!(iso),
                        None if text.trim().is_empty() => Value::Null,
                        None => {
                            self.note = Resources::version_due_prompt().to_string();
                            self.has_note = true;
                            return;
                        }
                    };
                    let id = e.id.clone();
                    self.cmd(|ed| ed.update_version(&id, Map::from_iter([("due_date".to_string(), value)])));
                }
            }
            _ => {}
        }
    }

    fn version_name(&self, id: &str) -> String {
        self.model.borrow().ed.as_ref().and_then(|ed| ed.versions.iter().find(|v| v.id == id).map(|v| v.name.clone())).unwrap_or_default()
    }

    fn version_due(&self, id: &str) -> String {
        let m = self.model.borrow();
        let due = m.ed.as_ref().and_then(|ed| ed.versions.iter().find(|v| v.id == id).and_then(|v| v.rest.get("due_date").and_then(Value::as_str).map(str::to_string)));
        due.and_then(|d| kubuno_office_projects_core::date::Date::parse(&d)).map(|d| format!("{:02}/{:02}/{}", d.day, d.month, d.year)).unwrap_or_default()
    }

    fn set_version_status(&mut self, status: &str) {
        let Some(id) = self.menu_version.take() else { return };
        self.cmd(|ed| ed.update_version(&id, Map::from_iter([("status".to_string(), json!(status))])));
    }

    fn ver_open_execute(&mut self) {
        self.set_version_status("open");
    }
    fn ver_locked_execute(&mut self) {
        self.set_version_status("locked");
    }
    fn ver_closed_execute(&mut self) {
        self.set_version_status("closed");
    }

    fn view_context_menu(&mut self, e: &GanttContextEventArgs) {
        self.ctx_task = if e.task.is_empty() { None } else { Some(e.task.clone()) };
        self.refresh();
        let menu = match e.kind.as_str() {
            "header" => "header_menu",
            "priority" => "priority_menu",
            _ => "task_menu",
        };
        kubuno_desktop::views::window::show_context_menu(menu, kubuno_desktop::views::window::MenuAnchor::Point(e.x, e.y));
    }

    fn save_execute(&mut self) {
        self.gantt.with(|g| g.commit_edit());
        if self.session.is_some() {
            // Every change is sent as it is made (the web's rule): Ctrl+S sends what waits now.
            self.after_edit();
            return;
        }
        let Some(path) = self.file.clone() else { return };
        let bytes = self.model.borrow().data().map(ProjectData::to_bytes);
        if let Some(b) = bytes {
            match std::fs::write(&path, b) {
                Ok(()) => self.local_dirty = false,
                Err(e) => {
                    self.note = Resources::save_failed().replace("{why}", &e.to_string());
                    self.has_note = true;
                }
            }
        }
        self.refresh();
    }

    fn undo_execute(&mut self) {
        if self.gantt.with(|g| g.is_editing()).unwrap_or(false) {
            return;
        }
        self.cmd(|ed| {
            ed.undo();
            Ok(())
        });
    }

    fn redo_execute(&mut self) {
        self.cmd(|ed| {
            ed.redo();
            Ok(())
        });
    }

    fn star_execute(&mut self) {
        let s = !self.starred;
        self.cmd(|ed| ed.set_starred(s));
    }

    fn paste_execute(&mut self) {
        let text = crate::platform::clipboard::get_text();
        self.gantt.with(|g| g.edit_clipboard("paste", text.as_deref()));
    }

    fn cut_execute(&mut self) {
        if let Some(Some(t)) = self.gantt.with(|g| g.edit_clipboard("cut", None)) {
            crate::platform::clipboard::set_text(&t);
        }
    }

    fn copy_execute(&mut self) {
        if let Some(Some(t)) = self.gantt.with(|g| g.edit_clipboard("copy", None)) {
            crate::platform::clipboard::set_text(&t);
        }
    }

    fn new_execute(&mut self) {
        self.note = Resources::new_in_web().to_string();
        self.has_note = true;
    }

    fn duplicate_execute(&mut self) {
        self.note = Resources::new_in_web().to_string();
        self.has_note = true;
    }

    fn export_csv_execute(&mut self) {
        let Some(text) = ({
            let m = self.model.borrow();
            m.ed.as_ref().map(|ed| {
                let h = view::CsvHeaders { name: Resources::col_task(), status: Resources::insp_status(), priority: Resources::col_priority(), start: Resources::col_start(), end: Resources::col_end(), duration: Resources::col_duration(), predecessors: Resources::col_pred(), resources: Resources::col_res() };
                (view::export_csv(ed.data(), ed.project_start(), &h), ed.data().project.title.clone())
            })
        }) else {
            return;
        };
        let (csv, title) = text;
        let name = format!("{}.csv", if title.is_empty() { "projet".to_string() } else { title.replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_") });
        let dir = std::env::var_os("USERPROFILE").map(|h| std::path::PathBuf::from(h).join("Downloads")).filter(|p| p.is_dir()).unwrap_or_else(std::env::temp_dir);
        let path = dir.join(name);
        match std::fs::write(&path, csv.as_bytes()) {
            Ok(()) => self.note = Resources::exported().replace("{path}", &path.display().to_string()),
            Err(e) => self.note = Resources::save_failed().replace("{why}", &e.to_string()),
        }
        self.has_note = true;
    }

    fn insert_task_execute(&mut self) {
        let sel = self.selected();
        self.cmd(|ed| ed.insert_task(sel.as_deref(), "task").map(|_| ()));
    }

    fn insert_milestone_execute(&mut self) {
        let sel = self.selected();
        self.cmd(|ed| ed.insert_task(sel.as_deref(), "milestone").map(|_| ()));
    }

    fn insert_summary_execute(&mut self) {
        let sel = self.selected();
        self.cmd(|ed| ed.insert_task(sel.as_deref(), "summary").map(|_| ()));
    }

    fn delete_execute(&mut self) {
        self.with_selected(|ed, id| ed.delete_task(id));
        self.model.borrow_mut().view.selected = None;
        self.refresh();
    }

    fn set_progress(&mut self, p: i32) {
        self.with_selected(|ed, id| ed.set_progress(id, p));
    }

    fn p0_execute(&mut self) {
        self.set_progress(0);
    }
    fn p25_execute(&mut self) {
        self.set_progress(25);
    }
    fn p50_execute(&mut self) {
        self.set_progress(50);
    }
    fn p75_execute(&mut self) {
        self.set_progress(75);
    }
    fn p100_execute(&mut self) {
        self.set_progress(100);
    }

    fn cpm_execute(&mut self) {
        self.cmd(|ed| ed.compute_cpm());
    }

    fn bl_capture_execute(&mut self) {
        self.cmd(|ed| ed.capture_baseline(None));
    }

    fn bl_delete_execute(&mut self) {
        let Some(id) = self.model.borrow().view.baseline_id.clone() else { return };
        self.model.borrow_mut().view.baseline_id = None;
        self.cmd(|ed| ed.delete_baseline(&id));
    }

    fn bl_rename_execute(&mut self) {
        let Some(id) = self.model.borrow().view.baseline_id.clone() else { return };
        let current = self.model.borrow().ed.as_ref().and_then(|e| e.baselines.iter().find(|b| b.id == id).map(|b| b.name.clone())).unwrap_or_default();
        if let Some(name) = super::prompt_dialog::PromptDialog::ask(self, Resources::cmd_bl_rename(), Resources::bl_rename_msg(), &current) {
            if !name.trim().is_empty() {
                let mut p = Map::new();
                p.insert("name".into(), json!(name.trim()));
                self.cmd(|ed| ed.update_baseline(&id, p));
            }
        }
    }

    fn bl_primary_execute(&mut self) {
        let Some(id) = self.model.borrow().view.baseline_id.clone() else { return };
        let mut p = Map::new();
        p.insert("is_primary".into(), json!(true));
        self.cmd(|ed| ed.update_baseline(&id, p));
    }

    fn baseline_changed(&mut self, e: &TextChangedEventArgs) {
        self.model.borrow_mut().view.baseline_id = if e.new.is_empty() { None } else { Some(e.new.clone()) };
        self.refresh();
    }

    fn baseline_hide_click(&mut self) {
        self.model.borrow_mut().view.baseline_id = None;
        self.refresh();
    }

    fn indent_execute(&mut self) {
        self.with_selected(|ed, id| ed.indent(id));
    }
    fn outdent_execute(&mut self) {
        self.with_selected(|ed, id| ed.outdent(id));
    }
    fn up_execute(&mut self) {
        self.with_selected(|ed, id| ed.move_task(id, true));
    }
    fn down_execute(&mut self) {
        self.with_selected(|ed, id| ed.move_task(id, false));
    }

    fn dup_task_execute(&mut self) {
        let word = Resources::copy_word().to_string();
        self.with_selected(|ed, id| ed.duplicate_task(id, &word).map(|_| ()));
    }

    fn rename_execute(&mut self) {
        self.set_view("gantt");
        self.gantt.with(|g| g.rename_selected());
    }

    fn link_execute(&mut self) {
        self.with_selected(|ed, id| ed.link_to_previous(id));
    }
    fn unlink_execute(&mut self) {
        self.with_selected(|ed, id| ed.unlink(id));
    }

    fn set_status(&mut self, st: &str) {
        let st = st.to_string();
        self.with_selected(|ed, id| ed.set_status(id, &st));
    }
    fn st_todo_execute(&mut self) {
        self.set_status("not_started");
    }
    fn st_prog_execute(&mut self) {
        self.set_status("in_progress");
    }
    fn st_hold_execute(&mut self) {
        self.set_status("on_hold");
    }
    fn st_done_execute(&mut self) {
        self.set_status("completed");
    }
    fn st_cancel_execute(&mut self) {
        self.set_status("cancelled");
    }

    fn set_priority(&mut self, p: &str) {
        let p = p.to_string();
        self.with_selected(|ed, id| ed.set_priority(id, &p));
    }
    fn pr_low_execute(&mut self) {
        self.set_priority("low");
    }
    fn pr_medium_execute(&mut self) {
        self.set_priority("medium");
    }
    fn pr_high_execute(&mut self) {
        self.set_priority("high");
    }
    fn pr_critical_execute(&mut self) {
        self.set_priority("critical");
    }

    fn set_type(&mut self, t: &str) {
        let t = t.to_string();
        self.with_selected(|ed, id| ed.set_task_field(id, "task_type", json!(t)));
    }
    fn type_task_execute(&mut self) {
        self.set_type("task");
    }
    fn type_milestone_execute(&mut self) {
        self.set_type("milestone");
    }
    fn type_summary_execute(&mut self) {
        self.set_type("summary");
    }

    /// « Insérer au-dessus / en dessous »: at the row's index in the stored order (0 appends, QUIRK of the server).
    fn insert_at(&mut self, delta: usize) {
        let Some(id) = self.selected() else { return };
        let i = self.model.borrow().data().and_then(|d| d.task_index(&id)).unwrap_or(0);
        self.cmd(|ed| ed.create_task(None, None, Some((i + delta) as i32)).map(|_| ()));
    }
    fn ins_above_execute(&mut self) {
        self.insert_at(0);
    }
    fn ins_below_execute(&mut self) {
        self.insert_at(1);
    }
    fn add_subtask_execute(&mut self) {
        self.with_selected(|ed, id| ed.create_task(Some(id), None, None).map(|_| ()));
    }

    fn timeline_execute(&mut self) {
        let v = !self.model.borrow().view.show_timeline;
        self.model.borrow_mut().view.show_timeline = v;
        self.refresh();
    }

    fn filters_execute(&mut self) {
        self.show_filter_bar = !self.show_filter_bar;
        self.refresh();
    }

    fn info_execute(&mut self) {
        if let Some(id) = self.ctx_task.clone() {
            self.model.borrow_mut().view.selected = Some(id);
        }
        self.refresh();
    }

    fn expand_all_execute(&mut self) {
        self.model.borrow_mut().view.collapsed.clear();
        self.refresh();
    }

    fn collapse_all_execute(&mut self) {
        let ids = self.model.borrow().data().map(gantt::summary_ids).unwrap_or_default();
        self.model.borrow_mut().view.collapsed = ids;
        self.refresh();
    }

    fn zoom_out_execute(&mut self) {
        let z = self.model.borrow().view.zoom.out();
        self.model.borrow_mut().view.zoom = z;
        self.refresh();
    }

    fn zoom_in_execute(&mut self) {
        let z = self.model.borrow().view.zoom.zoom_in();
        self.model.borrow_mut().view.zoom = z;
        self.refresh();
    }

    // ── Columns ─────────────────────────────────────────────────────────────

    fn toggle_col(&mut self, c: Col) {
        self.model.borrow_mut().view.layout.toggle(c);
        self.refresh();
    }
    fn col_idx_execute(&mut self) {
        self.toggle_col(Col::Idx);
    }
    fn col_mode_execute(&mut self) {
        self.toggle_col(Col::Mode);
    }
    fn col_dur_execute(&mut self) {
        self.toggle_col(Col::Dur);
    }
    fn col_progress_execute(&mut self) {
        self.toggle_col(Col::Progress);
    }
    fn col_priority_execute(&mut self) {
        self.toggle_col(Col::Priority);
    }
    fn col_start_execute(&mut self) {
        self.toggle_col(Col::Start);
    }
    fn col_end_execute(&mut self) {
        self.toggle_col(Col::End);
    }
    fn col_variance_execute(&mut self) {
        self.toggle_col(Col::Variance);
    }
    fn col_pred_execute(&mut self) {
        self.toggle_col(Col::Pred);
    }
    fn col_res_execute(&mut self) {
        self.toggle_col(Col::Res);
    }
    fn compact_execute(&mut self) {
        let c = !self.model.borrow().view.layout.compact;
        self.model.borrow_mut().view.layout.compact = c;
        self.refresh();
    }
    fn cols_reset_execute(&mut self) {
        self.model.borrow_mut().view.layout = Layout::default();
        self.refresh();
    }

    // ── Plan views, filter bar, band ────────────────────────────────────────

    fn set_view(&mut self, v: &str) {
        self.active_view = v.to_string();
        self.refresh();
    }

    fn plan_tab_selected(&mut self, e: &PlanTabEventArgs) {
        self.set_view(&e.id.clone());
    }

    fn band_task_selected(&mut self, e: &BandSelectEventArgs) {
        self.model.borrow_mut().view.selected = Some(e.task.clone());
        self.gantt.with(|g| g.reveal_selection());
        self.refresh();
    }

    fn filter_text_changed(&mut self, e: &TextChangedEventArgs) {
        self.filter_text = e.new.clone();
        self.model.borrow_mut().view.filter.text = e.new.clone();
        self.refresh();
    }
    fn filter_status_changed(&mut self, e: &TextChangedEventArgs) {
        self.model.borrow_mut().view.filter.status = e.new.clone();
        self.refresh();
    }
    fn filter_priority_changed(&mut self, e: &TextChangedEventArgs) {
        self.model.borrow_mut().view.filter.priority = e.new.clone();
        self.refresh();
    }
    fn sort_changed(&mut self, e: &TextChangedEventArgs) {
        self.model.borrow_mut().view.filter.sort_by = e.new.clone();
        self.refresh();
    }
    fn group_changed(&mut self, e: &TextChangedEventArgs) {
        self.model.borrow_mut().view.filter.group_by = e.new.clone();
        self.refresh();
    }
    fn filter_clear_click(&mut self) {
        self.model.borrow_mut().view.filter = view::Filter::default();
        self.filter_text.clear();
        self.refresh();
    }

    // ── Inspector, properties, resources ────────────────────────────────────

    fn inspector_edited(&mut self, e: &InspectorEditEventArgs) {
        let Some(id) = self.inspector.with(|i| i.task().to_string()).filter(|s| !s.is_empty()) else { return };
        let value: Value = serde_json::from_str(&e.value).unwrap_or(Value::Null);
        match e.field.as_str() {
            "assign" => {
                if let Some(r) = value.as_str() {
                    let r = r.to_string();
                    self.cmd(|ed| ed.assign(&id, &r));
                }
            }
            "unassign" => {
                if let Some(r) = value.as_str() {
                    let r = r.to_string();
                    self.cmd(|ed| ed.unassign(&id, &r));
                }
            }
            "status" => {
                if let Some(s) = value.as_str() {
                    let s = s.to_string();
                    // The panel's status sets the field only (`onUpdate({ status })`).
                    self.cmd(|ed| ed.set_task_field(&id, "status", json!(s)));
                }
            }
            "constraint" => {
                if let Some(o) = value.as_object() {
                    let patch = o.clone();
                    self.cmd(|ed| ed.patch_task(&id, patch, "task-edit"));
                }
            }
            "dep_add" => {
                if let Some(from) = value.as_str() {
                    let from = from.to_string();
                    self.cmd(|ed| ed.add_dependency(&from, &id, false));
                }
            }
            "dep_delete" => {
                if let Some(dep) = value.as_str() {
                    let dep = dep.to_string();
                    self.cmd(|ed| ed.remove_dependencies(&[dep]));
                }
            }
            "dep_type" => {
                self.ctx_dep = value.as_str().map(str::to_string);
                kubuno_desktop::views::window::show_context_menu("dep_type_menu", kubuno_desktop::views::window::MenuAnchor::Element("pred_list".into()));
            }
            "dep_lag" => {
                let Some(dep) = value.as_str().map(str::to_string) else { return };
                let cur = self.model.borrow().data().and_then(|d| d.dependencies.iter().find(|x| x.id == dep).cloned());
                if let Some(d) = cur {
                    if let Some(text) = super::prompt_dialog::PromptDialog::ask(self, Resources::lag_title(), Resources::lag_label(), &d.lag_days.to_string()) {
                        if let Ok(lag) = text.trim().replace('−', "-").parse::<i32>() {
                            self.cmd(|ed| ed.set_dependency(&d.from_task_id, &d.to_task_id, &d.dep_type, lag));
                        }
                    }
                }
            }
            field => {
                let field = field.to_string();
                self.cmd(|ed| ed.set_task_field(&id, &field, value));
            }
        }
    }

    fn set_dep_type(&mut self, ty: &str) {
        let Some(dep) = self.ctx_dep.take() else { return };
        let d = self.model.borrow().data().and_then(|d| d.dependencies.iter().find(|x| x.id == dep).cloned());
        if let Some(d) = d {
            let ty = ty.to_string();
            self.cmd(|ed| ed.set_dependency(&d.from_task_id, &d.to_task_id, &ty, d.lag_days));
        }
    }
    fn dep_fs_execute(&mut self) {
        self.set_dep_type("FS");
    }
    fn dep_ss_execute(&mut self) {
        self.set_dep_type("SS");
    }
    fn dep_ff_execute(&mut self) {
        self.set_dep_type("FF");
    }
    fn dep_sf_execute(&mut self) {
        self.set_dep_type("SF");
    }

    // ── The network view ─────────────────────────────────────────────────────

    /// A right click in the network: a card or a frame opens the task's menu, a link its menu, the background
    /// the view's menu.
    fn network_context_menu(&mut self, e: &GanttContextEventArgs) {
        use kubuno_desktop::views::window::{show_context_menu, MenuAnchor};
        match e.kind.as_str() {
            "net_canvas" => {
                let s = self.network.with(|n| (n.timescale(), n.snap(), n.can_rearrange())).unwrap_or((false, false, false));
                self.net_timescale = s.0;
                self.net_snap = s.1;
                self.net_snap_enabled = !s.0;
                self.net_can_rearrange = s.2;
                show_context_menu("net_canvas_menu", MenuAnchor::Point(e.x, e.y));
            }
            "net_edge" => {
                let found = self.model.borrow().data().and_then(|d| {
                    let dep = d.dependencies.iter().find(|x| x.id == e.task)?.clone();
                    let name = |id: &str| d.task(id).map(|t| t.name.clone()).unwrap_or_else(|| "?".into());
                    Some((format!("{} → {}", name(&dep.from_task_id), name(&dep.to_task_id)), dep))
                });
                let Some((title, dep)) = found else { return };
                self.ctx_dep = Some(dep.id.clone());
                self.net_edge_title = title;
                self.net_lag_label = Resources::net_dep_lag_menu().replace("{n}", &dep.lag_days.to_string());
                self.net_dep_fs = dep.dep_type == "FS";
                self.net_dep_ss = dep.dep_type == "SS";
                self.net_dep_ff = dep.dep_type == "FF";
                self.net_dep_sf = dep.dep_type == "SF";
                show_context_menu("net_edge_menu", MenuAnchor::Point(e.x, e.y));
            }
            _ => self.view_context_menu(e),
        }
    }

    fn net_timescale_execute(&mut self) {
        self.network.with(|n| n.toggle_timescale());
    }
    fn net_snap_execute(&mut self) {
        self.network.with(|n| n.toggle_snap());
    }
    fn net_fit_execute(&mut self) {
        self.network.with(|n| n.fit_to_content());
    }
    fn net_reset_execute(&mut self) {
        self.network.with(|n| n.reset_view());
    }
    fn net_rearrange_execute(&mut self) {
        self.network.with(|n| n.rearrange());
    }

    /// « Décalage… »: the link's lag (`proj_dep_lag_msg`).
    fn net_lag_execute(&mut self) {
        let Some(dep) = self.ctx_dep.clone() else { return };
        let cur = self.model.borrow().data().and_then(|d| d.dependencies.iter().find(|x| x.id == dep).cloned());
        let Some(d) = cur else { return };
        if let Some(text) = super::prompt_dialog::PromptDialog::ask(self, Resources::lag_title(), Resources::net_dep_lag_msg(), &d.lag_days.to_string()) {
            if let Ok(lag) = text.trim().replace('−', "-").parse::<i32>() {
                if lag != d.lag_days {
                    self.cmd(|ed| ed.set_dependency(&d.from_task_id, &d.to_task_id, &d.dep_type, lag));
                }
            }
        }
    }

    fn net_dep_delete_execute(&mut self) {
        let Some(dep) = self.ctx_dep.take() else { return };
        self.cmd(|ed| ed.remove_dependencies(&[dep]));
    }

    fn properties_edited(&mut self, e: &PropertiesEditEventArgs) {
        let value: Value = serde_json::from_str(&e.value).unwrap_or(Value::Null);
        let mut patch = Map::new();
        patch.insert(e.field.clone(), value);
        self.cmd(|ed| ed.save_project_props(patch));
    }

    fn new_res_changed(&mut self, e: &TextChangedEventArgs) {
        self.new_res_name = e.new.clone();
        self.can_add_resource = !self.new_res_name.trim().is_empty();
    }

    fn new_res_key_down(&mut self, e: &KeyEventArgs) {
        if e.key.0 == kubuno_desktop::controls::host::vk::ENTER {
            self.new_res_add_click();
        }
    }

    fn new_res_add_click(&mut self) {
        let name = self.new_res_name.trim().to_string();
        if name.is_empty() {
            return;
        }
        self.cmd(|ed| ed.create_resource(&name, None, None).map(|_| ()));
        self.new_res_name.clear();
        self.can_add_resource = false;
    }

    /// A click on a resource of the panel assigns it to the selected task, or removes the assignment.
    fn res_list_action(&mut self, e: &ListActionEventArgs) {
        let Some(id) = self.selected() else { return };
        let on = self.model.borrow().data().is_some_and(|d| d.assignments.iter().any(|a| a.task_id == id && a.resource_id == e.key));
        let r = e.key.clone();
        if on {
            self.cmd(|ed| ed.unassign(&id, &r));
        } else {
            self.cmd(|ed| ed.assign(&id, &r));
        }
    }

    fn bs_close_click(&mut self) {
        self.close();
    }

    // ── Binding ─────────────────────────────────────────────────────────────

    fn refresh_sync(&mut self) {
        let Some(s) = self.session.as_ref() else { return };
        let pending = self.model.borrow().ed.as_ref().map(|e| e.pending_count()).unwrap_or(0);
        self.sync_status = match s.state {
            SyncState::Opening => Resources::sync_opening().to_string(),
            SyncState::Saved => Resources::sync_saved().to_string(),
            SyncState::Saving => Resources::sync_saving().replace("{n}", &pending.to_string()),
            SyncState::Offline => Resources::sync_offline().replace("{n}", &pending.to_string()),
            SyncState::Conflict => Resources::sync_conflict().to_string(),
            SyncState::Failed => Resources::sync_failed().to_string(),
        };
        self.has_sync = true;
        self.online = matches!(s.state, SyncState::Saved | SyncState::Saving);
    }

    /// Re-binds everything from the model.
    fn refresh(&mut self) {
        self.refresh_sync();
        let model = self.model.clone();
        let m = model.borrow();
        let lang: Lang = m.view.lang;
        let sel = m.selected_task();
        let has_baseline = m.view.baseline_id.is_some();
        // The columns.
        let l = &m.view.layout;
        self.col_idx = l.shown(Col::Idx, true);
        self.col_mode = l.shown(Col::Mode, true);
        self.col_dur = l.shown(Col::Dur, true);
        self.col_progress = l.shown(Col::Progress, true);
        self.col_priority = l.shown(Col::Priority, true);
        self.col_start = l.shown(Col::Start, true);
        self.col_end = l.shown(Col::End, true);
        self.col_variance = l.shown(Col::Variance, has_baseline);
        self.col_pred = l.shown(Col::Pred, true);
        self.col_res = l.shown(Col::Res, true);
        self.compact = l.compact;
        self.cols_customised = l.customised();
        self.variance_label = if has_baseline { Resources::col_variance().to_string() } else { Resources::col_needs_baseline().to_string() };
        self.show_timeline = m.view.show_timeline;
        self.can_zoom_out = m.view.zoom != Zoom::Month;
        self.can_zoom_in = m.view.zoom != Zoom::Day;
        self.zoom_text = match m.view.zoom {
            Zoom::Day => Resources::zoom_day(),
            Zoom::Week => Resources::zoom_week(),
            Zoom::Month => Resources::zoom_month(),
        }
        .to_string();
        let Some(ed) = m.ed.as_ref() else {
            self.has_project = false;
            self.is_gantt = self.active_view == "gantt";
            drop(m);
            self.inspector.with(|i| i.show(None, None, &[], false));
            return;
        };
        let d = ed.data();
        self.has_project = true;
        let start = ed.project_start();
        // The plan views the project uses (`uses`): the tab strip, and the open view's fallback.
        let uses = |k: &str| ed.settings.uses(k);
        let items: Vec<String> = PLAN_VIEWS
            .iter()
            .filter(|(_, k, _)| uses(k))
            .map(|(id, _, icon)| {
                let label = match *id {
                    "gantt" => Resources::view_gantt(),
                    "board" => Resources::view_board(),
                    "calendar" => Resources::view_calendar(),
                    "resources" => Resources::view_resources(),
                    "pert" => Resources::view_pert(),
                    _ => Resources::view_roadmap(),
                };
                format!("{id}|{label}|{icon}")
            })
            .collect();
        self.plan_items = items.join("\n");
        if !items.iter().any(|i| i.starts_with(&format!("{}|", self.active_view))) && self.active_view != "load" {
            if let Some(first) = items.first().and_then(|i| i.split('|').next()) {
                self.active_view = first.to_string();
            }
        }
        self.is_gantt = self.active_view == "gantt";
        self.is_board = self.active_view == "board";
        self.is_calendar = self.active_view == "calendar";
        self.is_load = self.active_view == "resources" || self.active_view == "load";
        self.is_roadmap = self.active_view == "roadmap";
        self.is_network = self.active_view == "pert";
        self.is_todo = !(self.is_gantt || self.is_board || self.is_calendar || self.is_load || self.is_roadmap || self.is_network);
        self.show_band = self.is_gantt && m.view.show_timeline;
        // Title and history.
        let title = if d.project.title.is_empty() { Resources::untitled().to_string() } else { d.project.title.clone() };
        let dirty = self.local_dirty || ed.has_pending();
        self.window_title = format!("{} — {}", if dirty { Resources::title_modified().replace("{title}", &title) } else { title.clone() }, Resources::app_title());
        self.project_title = title;
        self.can_undo = ed.can_undo();
        self.can_redo = ed.can_redo();
        self.starred = d.project.is_starred;
        if self.session.is_none() {
            self.online = false;
        }
        // The selection.
        let task = sel.as_deref().and_then(|id| d.task(id));
        self.has_selection = task.is_some();
        self.has_parent = task.is_some_and(|t| t.parent_id.is_some());
        let parent = sel.as_deref().is_some_and(|id| ed.is_parent(id));
        self.can_set_progress = task.is_some() && !parent;
        self.can_link = sel.as_deref().and_then(|id| d.task_index(id)).is_some_and(|i| i > 0);
        let st = task.map(|t| t.status.as_str()).unwrap_or("");
        self.st_todo = st == "not_started";
        self.st_prog = st == "in_progress";
        self.st_hold = st == "on_hold";
        self.st_done = st == "completed";
        self.st_cancel = st == "cancelled";
        let pr = task.map(|t| t.priority.as_str()).unwrap_or("");
        self.pr_low = pr == "low";
        self.pr_medium = pr == "medium";
        self.pr_high = pr == "high";
        self.pr_critical = pr == "critical";
        let ty = task.map(|t| t.task_type.as_str()).unwrap_or("");
        self.ty_task = ty == "task";
        self.ty_milestone = ty == "milestone";
        self.ty_summary = ty == "summary";
        // Baselines.
        self.uses_baselines = uses("baselines");
        self.has_baselines = !ed.baselines.is_empty();
        let bl = m.view.baseline_id.as_deref().and_then(|id| ed.baselines.iter().find(|b| b.id == id));
        self.has_baseline = bl.is_some();
        self.can_set_primary = bl.is_some_and(|b| !b.is_primary);
        let mut bitems: Vec<(String, String)> = vec![(String::new(), Resources::baseline_none().into())];
        bitems.extend(ed.baselines.iter().map(|b| (b.id.clone(), if b.is_primary { format!("★ {}", b.name) } else { b.name.clone() })));
        self.baseline_items = Rows::from(bitems.into_iter().map(|(v, l)| Row::new().with("Value", BValue::Str(v)).with("Label", BValue::Str(l))).collect::<Vec<_>>());
        self.baseline_value = m.view.baseline_id.clone().unwrap_or_default();
        self.baseline_text = bl
            .map(|b| {
                let when = kubuno_office_projects_core::date::Date::parse(&b.captured_at).map(|x| date::format(x, DateFormat::Date, lang)).unwrap_or_default();
                Resources::baseline_compared().replace("{name}", &b.name).replace("{date}", &when)
            })
            .unwrap_or_default();
        // The filter bar.
        let f = &m.view.filter;
        self.filter_status = f.status.clone();
        self.filter_priority = f.priority.clone();
        self.sort_by = f.sort_by.clone();
        self.group_by = f.group_by.clone();
        self.filter_any = f.any();
        if f.any() {
            self.show_filter_bar = true;
        }
        let shown = view::display_tasks(d, f).len();
        self.filter_count = Resources::filter_count().replace("{n}", &shown.to_string());
        // Backstage › Informations.
        self.modified_text = kubuno_office_projects_core::date::Date::parse(&d.project.updated_at).map(|x| date::format(x, DateFormat::Date, lang)).unwrap_or_default();
        self.task_count_text = d.tasks.len().to_string();
        self.resource_count_text = d.resources.len().to_string();
        self.estimated_text = (d.tasks.iter().map(|t| t.estimated_hours.unwrap_or(0.0)).sum::<f64>()).round().to_string();
        self.spent_text = (d.tasks.iter().map(|t| t.spent_hours.unwrap_or(0.0)).sum::<f64>()).round().to_string();
        // The status bar.
        let s = view::status_counts(d, start);
        self.status_tasks = Resources::status_tasks().replace("{n}", &s.tasks.to_string());
        self.status_critical = Resources::status_critical().replace("{n}", &s.critical.to_string());
        self.status_done = Resources::status_done().replace("{done}", &s.completed.to_string()).replace("{total}", &s.tasks.to_string());
        match s.finish_vs_target {
            Some((finish, target, over)) => {
                let mut t = Resources::finish_vs_target().replace("{f}", &date::format(finish, DateFormat::DayMonthYear2, lang)).replace("{c}", &date::format(target, DateFormat::DayMonthYear2, lang));
                if over > 0 {
                    t.push(' ');
                    t.push_str(&Resources::overrun().replace("{n}", &over.to_string()));
                }
                self.status_finish = t;
                self.has_finish = over <= 0;
                self.has_overrun = over > 0;
            }
            None => {
                self.has_finish = false;
                self.has_overrun = false;
            }
        }
        // The resources panel.
        self.resource_list_items = d
            .resources
            .iter()
            .map(|r| {
                let on = sel.as_deref().is_some_and(|id| d.assignments.iter().any(|a| a.task_id == id && a.resource_id == r.id));
                let n = d.assignments.iter().filter(|a| a.resource_id == r.id).count();
                format!("{}|{} · {}|{}|{}", r.id, r.name.replace('|', "/"), Resources::res_task_count().replace("{n}", &n.to_string()), r.color, if on { 1 } else { 0 })
            })
            .collect::<Vec<_>>()
            .join("\n");
        let versions: Vec<(String, String)> = ed.versions.iter().map(|v| (v.id.clone(), v.name.clone())).collect();
        let show_versions = uses("roadmap");
        let project = d.project.clone();
        let data = d.clone();
        let revision = ed.revision();
        drop(m);
        self.inspector.with(|i| i.show(Some(&data), sel.as_deref(), &versions, show_versions));
        self.properties.with(|p| p.show(&project));
        if revision != self.last_revision {
            self.last_revision = revision;
        }
        self.gantt.with(|g| g.invalidate());
        self.board.with(|b| b.invalidate());
        self.month.with(|v| v.invalidate());
        self.load.with(|v| v.invalidate());
        self.network.with(|v| v.invalidate());
        self.roadmap.with(|v| v.invalidate());
        self.band.with(|b| b.invalidate());
    }
}

/// A day typed as `jj/mm/aaaa` (or ISO `aaaa-mm-jj`), as ISO; `None` when it is not a date.
fn parse_day(text: &str) -> Option<String> {
    use kubuno_office_projects_core::date::Date;
    if let Some(d) = Date::parse(text) {
        return Some(d.to_string());
    }
    let parts: Vec<&str> = text.split(['/', '.', '-']).collect();
    let [d, m, y] = parts[..] else { return None };
    let d = Date::new(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)?;
    Some(d.to_string())
}
