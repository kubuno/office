use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Presentation {
    pub id:             Uuid,
    pub owner_id:       Uuid,
    pub title:          String,
    pub file_id:        Option<Uuid>,
    pub draft_file_id:  Option<Uuid>,
    pub theme:          serde_json::Value,
    pub aspect_ratio:   String,
    pub slide_width:    i32,
    pub slide_height:   i32,
    pub slide_count:    i32,
    pub is_starred:     bool,
    pub is_trashed:     bool,
    pub trashed_at:     Option<DateTime<Utc>>,
    pub last_edited_by: Option<Uuid>,
    pub created_at:     DateTime<Utc>,
    pub updated_at:     DateTime<Utc>,
}

/// Slide metadata — content (elements, background, notes, transition) lives in the
/// presentation's content file.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SlideSummary {
    pub id:              Uuid,
    pub presentation_id: Uuid,
    pub position:        i32,
    pub is_hidden:       bool,
    pub thumbnail_path:  Option<String>,
    pub thumbnail_dirty: bool,
    pub created_at:      DateTime<Utc>,
    pub updated_at:      DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PresentationShare {
    pub id:              Uuid,
    pub presentation_id: Uuid,
    pub created_by:      Uuid,
    pub token:           String,
    pub permission:      String,
    pub is_active:       bool,
    pub created_at:      DateTime<Utc>,
}

// ── DTOs ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreatePresentationDto {
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePresentationDto {
    pub title:      Option<String>,
    pub theme:      Option<serde_json::Value>,
    pub is_starred: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListPresentationsQuery {
    pub search:  Option<String>,
    pub starred: Option<bool>,
    pub trashed: Option<bool>,
    pub recent:  Option<bool>,
    pub shared:  Option<bool>,
    pub limit:   Option<i64>,
    pub offset:  Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSlideDto {
    pub position: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSlideElementsDto {
    pub elements: Option<serde_json::Value>,
    pub notes:    Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSlideMetaDto {
    pub background: Option<serde_json::Value>,
    pub transition: Option<serde_json::Value>,
    pub is_hidden:  Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SlideOrder {
    pub id:       Uuid,
    pub position: i32,
}

#[derive(Debug, Deserialize)]
pub struct ReorderSlidesDto {
    pub slides: Vec<SlideOrder>,
}
