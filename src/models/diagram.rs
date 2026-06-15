use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Diagram {
    pub id:             Uuid,
    pub owner_id:       Uuid,
    pub title:          String,
    pub file_id:        Option<Uuid>,
    pub draft_file_id:  Option<Uuid>,
    pub diagram_type:   String,
    pub settings:       serde_json::Value,
    pub is_starred:     bool,
    pub is_trashed:     bool,
    pub trashed_at:     Option<DateTime<Utc>>,
    pub last_edited_by: Option<Uuid>,
    pub created_at:     DateTime<Utc>,
    pub updated_at:     DateTime<Utc>,
}

/// Diagram page metadata — page content lives in the diagram's content file.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DiagramPage {
    pub id:         Uuid,
    pub diagram_id: Uuid,
    pub name:       String,
    pub position:   i32,
    pub bg_color:   String,
    pub width:      i32,
    pub height:     i32,
    pub is_hidden:  bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CustomShape {
    pub id:         Uuid,
    pub owner_id:   Uuid,
    pub name:       String,
    pub category:   String,
    pub shape_def:  serde_json::Value,
    pub thumbnail:  Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DiagramShare {
    pub id:         Uuid,
    pub diagram_id: Uuid,
    pub created_by: Uuid,
    pub token:      String,
    pub permission: String,
    pub is_active:  bool,
    pub view_count: i32,
    pub created_at: DateTime<Utc>,
}

// ── DTOs ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateDiagramDto {
    pub title:        Option<String>,
    pub diagram_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDiagramDto {
    pub title:        Option<String>,
    pub diagram_type: Option<String>,
    pub settings:     Option<serde_json::Value>,
    pub is_starred:   Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListDiagramsQuery {
    pub search:  Option<String>,
    pub starred: Option<bool>,
    pub trashed: Option<bool>,
    pub recent:  Option<bool>,
    pub limit:   Option<i64>,
    pub offset:  Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePageDto {
    pub name:     Option<String>,
    pub bg_color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePageMetaDto {
    pub name:      Option<String>,
    pub bg_color:  Option<String>,
    pub is_hidden: Option<bool>,
    pub position:  Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePageDataDto {
    pub data: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct ReorderPagesDto {
    pub pages: Vec<PageOrder>,
}

#[derive(Debug, Deserialize)]
pub struct PageOrder {
    pub id:       Uuid,
    pub position: i32,
}

#[derive(Debug, Deserialize)]
pub struct CreateCustomShapeDto {
    pub name:      String,
    pub category:  Option<String>,
    pub shape_def: serde_json::Value,
    pub thumbnail: Option<String>,
}
