use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Spreadsheet {
    pub id:            Uuid,
    pub owner_id:      Uuid,
    pub title:         String,
    pub file_id:       Option<Uuid>,
    pub draft_file_id: Option<Uuid>,
    pub is_starred:    bool,
    pub is_trashed:    bool,
    pub trashed_at:    Option<DateTime<Utc>>,
    pub created_at:    DateTime<Utc>,
    pub updated_at:    DateTime<Utc>,
}

/// Sheet metadata only — content lives in the spreadsheet's content file.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SpreadsheetSheet {
    pub id:             Uuid,
    pub spreadsheet_id: Uuid,
    pub name:           String,
    pub position:       i32,
    pub created_at:     DateTime<Utc>,
    pub updated_at:     DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SpreadsheetVersion {
    pub id:             Uuid,
    pub spreadsheet_id: Uuid,
    pub author_id:      Uuid,
    pub snapshot:       serde_json::Value,
    pub label:          Option<String>,
    pub created_at:     DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSpreadsheetDto {
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSpreadsheetDto {
    pub title:      Option<String>,
    pub is_starred: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSheetDto {
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSheetDto {
    pub name:        Option<String>,
    /// Content update — written to the Files content file, not DB.
    pub data:        Option<serde_json::Value>,
    pub col_widths:  Option<serde_json::Value>,
    pub row_heights: Option<serde_json::Value>,
    pub frozen_rows: Option<i32>,
    pub frozen_cols: Option<i32>,
    pub position:    Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ListSpreadsheetsQuery {
    pub search:  Option<String>,
    pub starred: Option<bool>,
    pub trashed: Option<bool>,
    pub recent:  Option<bool>,
    pub shared:  Option<bool>,
    pub limit:   Option<i64>,
    pub offset:  Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateVersionDto {
    pub label: Option<String>,
}
