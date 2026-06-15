use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Board {
    pub id:            Uuid,
    pub owner_id:      Uuid,
    pub title:         String,
    pub description:   Option<String>,
    pub thumbnail_path: Option<String>,
    pub share_token:   Option<String>,
    pub is_public:     bool,
    pub background:    String,
    pub collaborators: serde_json::Value,
    pub element_count: i32,
    pub frame_count:   i32,
    pub is_trashed:    bool,
    pub trashed_at:    Option<DateTime<Utc>>,
    pub last_edited_at: Option<DateTime<Utc>>,
    pub last_edited_by: Option<Uuid>,
    pub is_starred:    bool,
    pub created_at:    DateTime<Utc>,
    pub updated_at:    DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateBoardDto {
    pub title:      Option<String>,
    pub background: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBoardDto {
    pub title:        Option<String>,
    pub description:  Option<String>,
    pub background:   Option<String>,
    pub is_starred:   Option<bool>,
    pub is_public:    Option<bool>,
    pub element_count: Option<i32>,
    pub frame_count:  Option<i32>,
    pub last_edited_by: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct ListBoardsQuery {
    pub search:  Option<String>,
    pub trashed: Option<bool>,
    pub starred: Option<bool>,
    pub limit:   Option<i64>,
    pub offset:  Option<i64>,
}
