use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Document {
    pub id:             Uuid,
    pub owner_id:       Uuid,
    pub title:          String,
    pub icon:           Option<String>,
    pub cover_url:      Option<String>,
    pub word_count:     i32,
    pub is_starred:     bool,
    pub is_trashed:     bool,
    pub trashed_at:     Option<DateTime<Utc>>,
    pub parent_id:      Option<Uuid>,
    pub position:       f64,
    pub last_editor_id: Option<Uuid>,
    pub file_id:        Option<Uuid>,
    pub draft_file_id:  Option<Uuid>,
    pub created_at:     DateTime<Utc>,
    pub updated_at:     DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DocumentSummary {
    pub id:           Uuid,
    pub owner_id:     Uuid,
    pub title:        String,
    pub icon:         Option<String>,
    pub word_count:   i32,
    pub is_starred:   bool,
    pub is_trashed:   bool,
    pub parent_id:    Option<Uuid>,
    pub created_at:   DateTime<Utc>,
    pub updated_at:   DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DocumentVersion {
    pub id:           Uuid,
    pub document_id:  Uuid,
    pub author_id:    Uuid,
    pub content_json: serde_json::Value,
    pub word_count:   i32,
    pub label:        Option<String>,
    pub created_at:   DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDocumentDto {
    pub title:       Option<String>,
    pub icon:        Option<String>,
    pub parent_id:   Option<Uuid>,
    pub template_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDocumentDto {
    pub title:        Option<String>,
    pub icon:         Option<String>,
    pub cover_url:    Option<String>,
    /// Content update — written to the Files content file, not DB.
    pub content_json: Option<serde_json::Value>,
    pub parent_id:    Option<Uuid>,
    pub is_starred:   Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListDocumentsQuery {
    pub parent_id: Option<Uuid>,
    pub search:    Option<String>,
    pub starred:   Option<bool>,
    pub trashed:   Option<bool>,
    pub recent:    Option<bool>,
    pub shared:    Option<bool>,
    pub limit:     Option<i64>,
    pub offset:    Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateVersionDto {
    pub label: Option<String>,
}
