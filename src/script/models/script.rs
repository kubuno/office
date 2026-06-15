use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Script {
    pub id:               Uuid,
    pub owner_id:         Uuid,
    pub name:             String,
    pub description:      Option<String>,
    // Le contenu vit dans un fichier .kbscr (module files) ; peuplé après le SELECT.
    #[sqlx(default)]
    pub source_code:      String,
    pub file_id:          Option<Uuid>,
    pub compiled_code:    Option<String>,
    pub compile_error:    Option<String>,
    pub timeout_secs:     i32,
    pub memory_limit_mb:  i32,
    pub run_count:        i32,
    pub last_run_at:      Option<DateTime<Utc>>,
    pub last_run_status:  Option<String>,
    pub is_trashed:       bool,
    pub created_at:       DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateScriptDto {
    pub name:            Option<String>,
    pub description:     Option<String>,
    pub source_code:     Option<String>,
    pub timeout_secs:    Option<i32>,
    pub memory_limit_mb: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateScriptDto {
    pub name:            Option<String>,
    pub description:     Option<String>,
    pub source_code:     Option<String>,
    pub timeout_secs:    Option<i32>,
    pub memory_limit_mb: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ListScriptsQuery {
    pub search:  Option<String>,
    pub trashed: Option<bool>,
    pub limit:   Option<i64>,
    pub offset:  Option<i64>,
}
