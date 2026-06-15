use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Trigger {
    pub id:               Uuid,
    pub script_id:        Uuid,
    pub owner_id:         Uuid,
    pub name:             String,
    pub trigger_type:     String,
    pub cron_expression:  Option<String>,
    pub event_name:       Option<String>,
    pub event_module:     Option<String>,
    pub event_filter:     serde_json::Value,
    pub webhook_token:    Option<String>,
    pub input_vars:       serde_json::Value,
    pub is_active:        bool,
    pub last_fired_at:    Option<DateTime<Utc>>,
    pub fire_count:       i32,
    pub created_at:       DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTriggerDto {
    pub name:            Option<String>,
    pub trigger_type:    String,
    pub cron_expression: Option<String>,
    pub event_name:      Option<String>,
    pub event_module:    Option<String>,
    pub event_filter:    Option<serde_json::Value>,
    pub input_vars:      Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTriggerDto {
    pub name:            Option<String>,
    pub cron_expression: Option<String>,
    pub event_name:      Option<String>,
    pub event_module:    Option<String>,
    pub event_filter:    Option<serde_json::Value>,
    pub input_vars:      Option<serde_json::Value>,
    pub is_active:       Option<bool>,
}
