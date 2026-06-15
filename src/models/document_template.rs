use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Template {
    pub id:           Uuid,
    pub name:         String,
    pub description:  Option<String>,
    pub category:     String,
    pub icon:         Option<String>,
    pub content_json: serde_json::Value,
    pub is_builtin:   bool,
    pub created_by:   Option<Uuid>,
    pub created_at:   DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTemplateDto {
    pub name:         String,
    pub description:  Option<String>,
    pub category:     Option<String>,
    pub icon:         Option<String>,
    pub content_json: serde_json::Value,
}
