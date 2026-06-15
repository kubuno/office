use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct UserFont {
    pub id:         Uuid,
    pub user_id:    Uuid,
    pub name:       String,
    pub css_family: String,
    pub source:     String,
    pub import_url: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct AddFontDto {
    pub name:       String,
    pub css_family: String,
    pub source:     Option<String>,
    pub import_url: String,
}
