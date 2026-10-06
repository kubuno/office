use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleEntry {
    pub level:   String,
    pub args:    Vec<serde_json::Value>,
    pub time_ms: u64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ScriptRun {
    pub id:              Uuid,
    pub script_id:       Uuid,
    pub owner_id:        Uuid,
    pub trigger_id:      Option<Uuid>,
    pub run_source:      String,
    pub status:          String,
    pub duration_ms:     Option<i32>,
    pub memory_used_kb:  Option<i32>,
    pub console_output:  serde_json::Value,
    pub return_value:    Option<serde_json::Value>,
    pub error_message:   Option<String>,
    pub error_stack:     Option<String>,
    pub trigger_data:    Option<serde_json::Value>,
    pub started_at:      DateTime<Utc>,
    pub finished_at:     Option<DateTime<Utc>>,
}
