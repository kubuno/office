use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

// Distinguish an ABSENT JSON field from a PRESENT `null`. Plain `Option<Option<T>>`
// can't: serde collapses a top-level `null` to the OUTER `None`, so `{"x": null}`
// and an omitted `x` both become `None`. With `#[serde(default, deserialize_with =
// "double_option")]` the field is only visited when PRESENT, and this wraps the
// inner `Option` in `Some` — so absent → `None`, `null` → `Some(None)`, value →
// `Some(Some(v))`. Used by `parent_id` so outdent-to-root (`null`) can clear it.
fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(de).map(Some)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Project {
    pub id:            Uuid,
    pub owner_id:      Uuid,
    pub title:         String,
    pub file_id:       Option<Uuid>,
    pub description:   String,
    pub color:         String,
    pub status:        String,
    pub start_date:    Option<NaiveDate>,
    pub end_date:      Option<NaiveDate>,
    pub is_starred:    bool,
    pub is_trashed:    bool,
    pub trashed_at:    Option<DateTime<Utc>>,
    pub last_edited_by: Option<Uuid>,
    pub created_at:    DateTime<Utc>,
    pub updated_at:    DateTime<Utc>,
    // "management" (Gantt/CPM planning) or "cloud" (resource container).
    pub kind:          String,
    // Immutable, instance-unique identifier for cloud projects (NULL otherwise).
    pub slug:          Option<String>,
    // Key/value labels (JSONB object).
    pub labels:        serde_json::Value,
    // Optional parent project (a project is the "folder" of its children). NULL = root.
    pub parent_id:     Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Task {
    pub id:           Uuid,
    pub project_id:   Uuid,
    pub parent_id:    Option<Uuid>,
    pub position:     i32,
    pub wbs:          String,
    pub name:         String,
    pub description:  String,
    pub status:       String,
    pub priority:     String,
    pub task_type:    String,
    pub start_date:   Option<NaiveDate>,
    pub end_date:     Option<NaiveDate>,
    pub duration_days: i32,
    pub progress:     i32,
    pub early_start:  Option<i32>,
    pub early_finish: Option<i32>,
    pub late_start:   Option<i32>,
    pub late_finish:  Option<i32>,
    pub total_float:  Option<i32>,
    /// Slack before this task would push a successor. Distinct from `total_float`,
    /// which measures slack against the project end.
    pub free_float:   Option<i32>,
    pub is_critical:  bool,
    pub cpm_dirty:    bool,
    /// One of ASAP, ALAP, SNET, SNLT, FNET, FNLT, MSO, MFO. Everything but the
    /// first two needs `constraint_date`.
    pub constraint_type: String,
    pub constraint_date: Option<NaiveDate>,
    /// When the task was due. Never moves the schedule — only reports lateness.
    pub deadline_date:   Option<NaiveDate>,
    pub deadline_missed: bool,
    // Work tracking (hours): estimated effort and time spent. NULL = not set.
    pub estimated_hours: Option<f64>,
    pub spent_hours:     Option<f64>,
    /// Budget at completion of this work package; NULL means never costed.
    pub budget_cost:     Option<f64>,
    // Roadmap version this task belongs to (NULL = unassigned).
    pub version_id:      Option<Uuid>,
    pub created_at:   DateTime<Utc>,
    pub updated_at:   DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TaskDependency {
    pub id:           Uuid,
    pub project_id:   Uuid,
    pub from_task_id: Uuid,
    pub to_task_id:   Uuid,
    pub dep_type:     String,
    pub lag_days:     i32,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProjectResource {
    pub id:         Uuid,
    pub project_id: Uuid,
    pub name:       String,
    pub role:       String,
    pub color:      String,
    pub capacity:   f64,
    /// What an hour of this resource costs; NULL falls back to the project rate.
    pub hourly_rate: Option<f64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TaskAssignment {
    pub id:          Uuid,
    pub task_id:     Uuid,
    pub resource_id: Uuid,
    pub units:       f64,
}

// ── DTOs ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateProjectDto {
    pub title:       Option<String>,
    pub description: Option<String>,
    pub color:       Option<String>,
    pub start_date:  Option<NaiveDate>,
    pub end_date:    Option<NaiveDate>,
    // "management" (default) or "cloud".
    pub kind:        Option<String>,
    // Requested identifier for a cloud project; derived from the title if absent.
    pub slug:        Option<String>,
    pub labels:      Option<serde_json::Value>,
    // Optional parent project at creation (subproject).
    pub parent_id:   Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectDto {
    pub title:         Option<String>,
    pub description:   Option<String>,
    pub color:         Option<String>,
    pub status:        Option<String>,
    pub start_date:    Option<NaiveDate>,
    pub end_date:      Option<NaiveDate>,
    pub is_starred:    Option<bool>,
    // Cloud-project labels (JSONB object). `slug` and `kind` are immutable.
    pub labels:        Option<serde_json::Value>,
    // Re-parenting (tri-state via `double_option`): absent = keep; `null` = detach to
    // root; a value = move under that project. Cycles are rejected in the handler.
    #[serde(default, deserialize_with = "double_option")]
    pub parent_id:     Option<Option<Uuid>>,
}

#[derive(Debug, Deserialize)]
pub struct ListProjectsQuery {
    pub search:  Option<String>,
    pub starred: Option<bool>,
    pub trashed: Option<bool>,
    pub recent:  Option<bool>,
    /// Projects shared WITH me (I am a collaborator, not the owner). Without it a
    /// project someone shared could not be found anywhere in the interface.
    pub shared:  Option<bool>,
    pub limit:   Option<i64>,
    pub offset:  Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskDto {
    pub name:         Option<String>,
    pub parent_id:    Option<Uuid>,
    pub position:     Option<i32>,
    pub task_type:    Option<String>,
    pub start_date:   Option<NaiveDate>,
    pub duration_days: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTaskDto {
    pub name:          Option<String>,
    pub description:   Option<String>,
    pub status:        Option<String>,
    pub priority:      Option<String>,
    pub task_type:     Option<String>,
    pub start_date:    Option<NaiveDate>,
    pub end_date:      Option<NaiveDate>,
    pub duration_days: Option<i32>,
    pub progress:      Option<i32>,
    pub position:      Option<i32>,
    pub wbs:           Option<String>,
    pub estimated_hours: Option<f64>,
    pub spent_hours:     Option<f64>,
    // Budget at completion of this work package. Tri-state: a package nobody
    // costed is not a package costed at nothing.
    #[serde(default, deserialize_with = "double_option")]
    pub budget_cost:     Option<Option<f64>>,
    pub constraint_type: Option<String>,
    // Tri-state: absent = keep; `null` = clear the constraint/deadline; value = set.
    #[serde(default, deserialize_with = "double_option")]
    pub constraint_date: Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "double_option")]
    pub deadline_date:   Option<Option<NaiveDate>>,
    // Roadmap version. Tri-state: absent = keep; `null` = detach; value = assign.
    #[serde(default, deserialize_with = "double_option")]
    pub version_id:      Option<Option<Uuid>>,
    // Re-parenting (indent/outdent). Tri-state (see `double_option`): absent
    // (`None`) = keep the current parent; JSON `null` (`Some(None)`) = move to the
    // root; a value (`Some(Some(id))`) = re-parent under that task. The custom
    // deserializer is REQUIRED — a plain `#[serde(default)] Option<Option<Uuid>>`
    // collapses `null` to `None`, so outdent-to-root would silently keep the parent.
    #[serde(default, deserialize_with = "double_option")]
    pub parent_id:     Option<Option<Uuid>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDependencyDto {
    pub from_task_id: Uuid,
    pub to_task_id:   Uuid,
    pub dep_type:     Option<String>,
    pub lag_days:     Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct CreateResourceDto {
    pub name:     String,
    pub role:     Option<String>,
    pub color:    Option<String>,
    pub capacity: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateResourceDto {
    pub name:     Option<String>,
    pub role:     Option<String>,
    pub color:    Option<String>,
    pub capacity: Option<f64>,
    /// Tri-state: absent keeps the rate, `null` clears it back to the project's.
    #[serde(default, deserialize_with = "double_option")]
    pub hourly_rate: Option<Option<f64>>,
}

#[derive(Debug, Deserialize)]
pub struct AssignResourceDto {
    pub resource_id: Uuid,
    pub units:       Option<f64>,
}
