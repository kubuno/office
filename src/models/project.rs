use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    pub is_critical:  bool,
    pub cpm_dirty:    bool,
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
}

#[derive(Debug, Deserialize)]
pub struct ListProjectsQuery {
    pub search:  Option<String>,
    pub starred: Option<bool>,
    pub trashed: Option<bool>,
    pub recent:  Option<bool>,
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
}

#[derive(Debug, Deserialize)]
pub struct AssignResourceDto {
    pub resource_id: Uuid,
    pub units:       Option<f64>,
}
