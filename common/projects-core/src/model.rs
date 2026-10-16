//! The project as the office API serves it (`GET /api/v1/office/projects/:id` →
//! `{ project, tasks, dependencies, resources, assignments }`, `server/src/handlers/projects.rs:216-278`),
//! with the side lists the editor loads next to it (baselines, calendars, settings, versions).
//!
//! A project is **not** a document file: it lives in rows (`office.projects`, `office.tasks`…) and every edit
//! is a REST call on one row. The `.kbprj` file the server mirrors into Drive
//! (`build_project_json`, `projects.rs:1604-1642`) is a reduced export, read here by [`KbprjFile`].
//!
//! Every record keeps what this crate does not model in `rest` (the original JSON values), so a record read
//! and written back is the same JSON value the server sent (tests: `round_trip`). Field names, types and
//! nullability are the server's (`server/src/models/project.rs`).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::date::Date;

fn s_asap() -> String {
    "ASAP".into()
}
fn s_not_started() -> String {
    "not_started".into()
}
fn s_medium() -> String {
    "medium".into()
}
fn s_task() -> String {
    "task".into()
}
fn s_fs() -> String {
    "FS".into()
}
fn one() -> f64 {
    1.0
}
fn s_person() -> String {
    "person".into()
}
fn s_management() -> String {
    "management".into()
}
fn s_active() -> String {
    "active".into()
}
fn s_color() -> String {
    "#1a73e8".into()
}
fn empty_object() -> Value {
    Value::Object(Map::new())
}

/// A project (`office.projects`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Project {
    pub id: String,
    #[serde(default)]
    pub owner_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub file_id: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default = "s_color")]
    pub color: String,
    #[serde(default = "s_active")]
    pub status: String,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub is_starred: bool,
    #[serde(default)]
    pub is_trashed: bool,
    #[serde(default)]
    pub trashed_at: Option<String>,
    #[serde(default)]
    pub last_edited_by: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    /// `management` (Gantt/CPM planning) or `cloud` (a resource container).
    #[serde(default = "s_management")]
    pub kind: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default = "empty_object")]
    pub labels: Value,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

impl Project {
    pub fn start(&self) -> Option<Date> {
        self.start_date.as_deref().and_then(Date::parse)
    }
    pub fn end(&self) -> Option<Date> {
        self.end_date.as_deref().and_then(Date::parse)
    }
}

/// A task (`office.tasks`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub wbs: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "s_not_started")]
    pub status: String,
    #[serde(default = "s_medium")]
    pub priority: String,
    /// `task`, `milestone` or `summary`.
    #[serde(default = "s_task")]
    pub task_type: String,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub duration_days: i32,
    #[serde(default)]
    pub progress: i32,
    #[serde(default)]
    pub early_start: Option<i32>,
    #[serde(default)]
    pub early_finish: Option<i32>,
    #[serde(default)]
    pub late_start: Option<i32>,
    #[serde(default)]
    pub late_finish: Option<i32>,
    #[serde(default)]
    pub total_float: Option<i32>,
    #[serde(default)]
    pub free_float: Option<i32>,
    #[serde(default)]
    pub is_critical: bool,
    #[serde(default)]
    pub cpm_dirty: bool,
    /// ASAP, ALAP, SNET, SNLT, FNET, FNLT, MSO, MFO.
    #[serde(default = "s_asap")]
    pub constraint_type: String,
    #[serde(default)]
    pub constraint_date: Option<String>,
    #[serde(default)]
    pub deadline_date: Option<String>,
    #[serde(default)]
    pub deadline_missed: bool,
    #[serde(default)]
    pub estimated_hours: Option<f64>,
    #[serde(default)]
    pub spent_hours: Option<f64>,
    #[serde(default)]
    pub budget_cost: Option<f64>,
    #[serde(default)]
    pub version_id: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

impl Task {
    /// A task as `POST /projects/:id/tasks` creates it (`create_task`, `projects.rs:652-691`): the server's
    /// defaults (« Nouvelle tâche », `task`, one day, ASAP, not started, medium).
    pub fn new(id: impl Into<String>, project_id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            project_id: project_id.into(),
            parent_id: None,
            position: 0,
            wbs: String::new(),
            name: "Nouvelle tâche".into(),
            description: String::new(),
            status: s_not_started(),
            priority: s_medium(),
            task_type: s_task(),
            start_date: None,
            end_date: None,
            duration_days: 1,
            progress: 0,
            early_start: None,
            early_finish: None,
            late_start: None,
            late_finish: None,
            total_float: None,
            free_float: None,
            is_critical: false,
            cpm_dirty: true,
            constraint_type: s_asap(),
            constraint_date: None,
            deadline_date: None,
            deadline_missed: false,
            estimated_hours: None,
            spent_hours: None,
            budget_cost: None,
            version_id: None,
            created_at: String::new(),
            updated_at: String::new(),
            rest: Map::new(),
        }
    }

    pub fn is_milestone(&self) -> bool {
        self.task_type == "milestone"
    }
    pub fn is_summary_type(&self) -> bool {
        self.task_type == "summary"
    }
    /// `early_start ?? 0`.
    pub fn es(&self) -> i32 {
        self.early_start.unwrap_or(0)
    }
    /// The bar's exclusive calendar end: `early_finish ?? (early_start ?? 0) + duration_days`.
    pub fn cal_end(&self) -> i32 {
        self.early_finish.unwrap_or(self.es() + self.duration_days)
    }
}

/// A link between two tasks (`office.task_dependencies`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dependency {
    pub id: String,
    #[serde(default)]
    pub project_id: String,
    pub from_task_id: String,
    pub to_task_id: String,
    /// FS, SS, FF or SF.
    #[serde(default = "s_fs")]
    pub dep_type: String,
    #[serde(default)]
    pub lag_days: i32,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// A person or a thing the tasks are assigned to (`office.project_resources`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    pub id: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub role: String,
    #[serde(default = "s_color")]
    pub color: String,
    #[serde(default = "one")]
    pub capacity: f64,
    #[serde(default)]
    pub hourly_rate: Option<f64>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    /// person, equipment, material or cost.
    #[serde(default = "s_person")]
    pub kind: String,
    #[serde(default)]
    pub unit_label: Option<String>,
    #[serde(default)]
    pub overtime_rate: Option<f64>,
    #[serde(default)]
    pub cost_per_use: Option<f64>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// A resource on a task (`office.task_assignments`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    pub id: String,
    pub task_id: String,
    pub resource_id: String,
    #[serde(default = "one")]
    pub units: f64,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// `GET /projects/:id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProjectData {
    pub project: Project,
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub resources: Vec<Resource>,
    #[serde(default)]
    pub assignments: Vec<Assignment>,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// One task of a baseline's snapshot (`project_baselines.tasks`, `project_baselines.rs:39-44`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineTask {
    pub task_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub early_start: Option<i32>,
    #[serde(default)]
    pub early_finish: Option<i32>,
    #[serde(default)]
    pub duration_days: i32,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// A baseline (`GET /projects/:id/baselines` → `{ baselines: [...] }`, newest first).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub project_start: Option<String>,
    #[serde(default)]
    pub tasks: Vec<BaselineTask>,
    #[serde(default)]
    pub captured_at: String,
    #[serde(default)]
    pub is_primary: bool,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// A working calendar of a project (`pm_calendars`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarRow {
    pub id: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub name: String,
    pub mon: bool,
    pub tue: bool,
    pub wed: bool,
    pub thu: bool,
    pub fri: bool,
    pub sat: bool,
    pub sun: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

impl CalendarRow {
    /// Monday first.
    pub fn week(&self) -> [bool; 7] {
        [self.mon, self.tue, self.wed, self.thu, self.fri, self.sat, self.sun]
    }
}

/// A day departing from a calendar's week (`pm_calendar_exceptions`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarException {
    #[serde(default)]
    pub id: String,
    pub calendar_id: String,
    pub day: String,
    pub is_working: bool,
    #[serde(default)]
    pub note: String,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// `GET /projects/:id/calendars`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Calendars {
    #[serde(default)]
    pub calendars: Vec<CalendarRow>,
    #[serde(default)]
    pub exceptions: Vec<CalendarException>,
    #[serde(default)]
    pub project_calendar_id: Option<String>,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// `GET /projects/:id/settings` - which artifacts the project shows (`project_settings.rs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub methodology: Option<String>,
    #[serde(default)]
    pub default_view: Option<String>,
    #[serde(default)]
    pub artifacts: Map<String, Value>,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

impl Settings {
    /// `uses(key)`: `settings?.artifacts?.[key]?.enabled ?? true` - everything is on until the settings say
    /// otherwise (`ProjectEditorPage.tsx:370-373`).
    pub fn uses(&self, key: &str) -> bool {
        self.artifacts.get(key).and_then(|a| a.get("enabled")).and_then(Value::as_bool).unwrap_or(true)
    }
}

/// A roadmap version (`GET /projects/:id/versions`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Version {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// The `.kbprj` file the server keeps in Drive (`build_project_json`): `kubuno-project/v1`, a reduced export
/// (no dates, no schedule). Opened read-only by the desktop as a project without a schedule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KbprjFile {
    pub format: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub tasks: Vec<Value>,
    #[serde(default)]
    pub deps: Vec<Value>,
    #[serde(default)]
    pub resources: Vec<Value>,
}

/// The format tag of [`KbprjFile`].
pub const KBPRJ_FORMAT: &str = "kubuno-project/v1";

/// Why a file could not be read as a project.
#[derive(Debug, Clone, PartialEq)]
pub enum ReadError {
    Json(String),
    UnknownFormat,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Json(e) => write!(f, "JSON illisible : {e}"),
            ReadError::UnknownFormat => f.write_str("ce fichier n'est pas un projet Kubuno"),
        }
    }
}

impl ProjectData {
    /// Reads a project from bytes: the API's `GET /projects/:id` body (what the desktop journals), or a
    /// `.kbprj` export (`kubuno-project/v1`). A gzipped `.kbprj` must be inflated by the caller.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ReadError> {
        let v: Value = serde_json::from_slice(bytes).map_err(|e| ReadError::Json(e.to_string()))?;
        if v.get("project").is_some() {
            return serde_json::from_value(v).map_err(|e| ReadError::Json(e.to_string()));
        }
        if v.get("format").and_then(Value::as_str) == Some(KBPRJ_FORMAT) {
            let f: KbprjFile = serde_json::from_value(v).map_err(|e| ReadError::Json(e.to_string()))?;
            return Ok(Self::from_kbprj(&f));
        }
        Err(ReadError::UnknownFormat)
    }

    /// A project built from a `.kbprj` export: the tasks keep their order, type, duration and progress; the
    /// dependencies get synthetic ids (`dep-<n>`); nothing is scheduled.
    pub fn from_kbprj(f: &KbprjFile) -> Self {
        let mut data = ProjectData { project: Project { id: String::new(), title: f.title.clone(), status: f.status.clone(), color: s_color(), kind: s_management(), labels: empty_object(), ..Project::default() }, ..Default::default() };
        for t in &f.tasks {
            if let Ok(task) = serde_json::from_value::<Task>(t.clone()) {
                data.tasks.push(task);
            }
        }
        for (i, d) in f.deps.iter().enumerate() {
            let mut d = d.clone();
            if let Some(o) = d.as_object_mut() {
                o.entry("id").or_insert_with(|| Value::String(format!("dep-{}", i + 1)));
            }
            if let Ok(dep) = serde_json::from_value::<Dependency>(d) {
                data.dependencies.push(dep);
            }
        }
        for r in &f.resources {
            if let Ok(res) = serde_json::from_value::<Resource>(r.clone()) {
                data.resources.push(res);
            }
        }
        data.tasks.sort_by_key(|t| t.position);
        data
    }

    /// The JSON the desktop journals (the API's own shape).
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn task_mut(&mut self, id: &str) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    pub fn task_index(&self, id: &str) -> Option<usize> {
        self.tasks.iter().position(|t| t.id == id)
    }

    pub fn resource(&self, id: &str) -> Option<&Resource> {
        self.resources.iter().find(|r| r.id == id)
    }

    /// The project's day 0: its start date, else today (`projectStart`, `ProjectEditorPage.tsx:529`).
    pub fn project_start(&self, today: Date) -> Date {
        self.project.start().unwrap_or(today)
    }

    /// The tasks in the server's order (`ORDER BY position ASC`; equal positions keep their current order).
    pub fn sort_tasks(&mut self) {
        self.tasks.sort_by_key(|t| t.position);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "project": {"id": "p1", "owner_id": "u1", "title": "Site", "file_id": null, "description": "", "color": "#1a73e8",
                "status": "active", "start_date": "2026-09-07", "end_date": null, "is_starred": false, "is_trashed": false,
                "trashed_at": null, "last_edited_by": null, "created_at": "2026-09-01T10:00:00Z", "updated_at": "2026-09-01T10:00:00Z",
                "kind": "management", "slug": null, "labels": {}, "parent_id": null, "calendar_id": null},
            "tasks": [{"id": "t1", "project_id": "p1", "parent_id": null, "position": 0, "wbs": "1", "name": "Dig", "description": "",
                "status": "not_started", "priority": "medium", "task_type": "task", "start_date": null, "end_date": null,
                "duration_days": 3, "progress": 0, "early_start": 0, "early_finish": 3, "late_start": 0, "late_finish": 3,
                "total_float": 0, "free_float": 0, "is_critical": true, "cpm_dirty": false, "constraint_type": "ASAP",
                "constraint_date": null, "deadline_date": null, "deadline_missed": false, "estimated_hours": 12.5,
                "spent_hours": null, "budget_cost": null, "version_id": null, "created_at": "x", "updated_at": "y", "future_field": [1, 2]}],
            "dependencies": [{"id": "d1", "project_id": "p1", "from_task_id": "t1", "to_task_id": "t1", "dep_type": "FS", "lag_days": -2}],
            "resources": [{"id": "r1", "project_id": "p1", "name": "Ana", "role": "", "color": "#188038", "capacity": 0.5,
                "hourly_rate": null, "user_id": null, "avatar_url": null, "kind": "person", "unit_label": null,
                "overtime_rate": null, "cost_per_use": null, "skills": ["sql"], "created_at": "z"}],
            "assignments": [{"id": "a1", "task_id": "t1", "resource_id": "r1", "units": 1.0}]
        })
    }

    #[test]
    fn round_trip() {
        let v = sample();
        let data: ProjectData = serde_json::from_value(v.clone()).expect("project");
        assert_eq!(data.tasks[0].estimated_hours, Some(12.5));
        assert_eq!(data.tasks[0].rest.get("future_field"), Some(&json!([1, 2])));
        let back = serde_json::to_value(&data).expect("json");
        assert_eq!(back, v);
    }

    #[test]
    fn reads_the_kbprj_export() {
        let f = json!({"format": "kubuno-project/v1", "title": "Site", "status": "active",
            "tasks": [{"id": "b", "name": "B", "status": "not_started", "priority": "high", "task_type": "task", "position": 1, "duration_days": 2, "progress": 0},
                      {"id": "a", "name": "A", "status": "completed", "priority": "low", "task_type": "milestone", "position": 0, "duration_days": 0, "progress": 100}],
            "deps": [{"from_task_id": "a", "to_task_id": "b", "dep_type": "FS", "lag_days": 0}],
            "resources": [{"id": "r", "name": "Ana", "role": "", "color": "#000", "capacity": 1.0}]});
        let data = ProjectData::from_bytes(f.to_string().as_bytes()).expect("kbprj");
        assert_eq!(data.project.title, "Site");
        assert_eq!(data.tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(data.dependencies[0].id, "dep-1");
        assert!(ProjectData::from_bytes(b"{}").is_err());
    }
}
