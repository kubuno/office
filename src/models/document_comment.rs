use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Comment {
    pub id:          Uuid,
    pub document_id: Uuid,
    pub author_id:   Uuid,
    pub parent_id:   Option<Uuid>,
    pub content:     String,
    pub is_resolved: bool,
    pub created_at:  DateTime<Utc>,
    pub updated_at:  DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCommentDto {
    pub content:   String,
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCommentDto {
    pub content: String,
}
