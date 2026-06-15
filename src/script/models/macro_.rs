use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ScriptMacro {
    pub id:            Uuid,
    pub script_id:     Uuid,
    pub owner_id:      Uuid,
    pub document_type: Option<String>,
    pub document_id:   Option<Uuid>,
    pub button_label:  String,
    pub button_icon:   String,
    pub position:      i32,
    pub created_at:    DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMacroDto {
    pub script_id:     Uuid,
    pub document_type: Option<String>,
    pub document_id:   Option<Uuid>,
    pub button_label:  Option<String>,
    pub button_icon:   Option<String>,
    pub position:      Option<i32>,
}
