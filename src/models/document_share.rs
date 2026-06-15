use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Share {
    pub id:          Uuid,
    pub document_id: Uuid,
    pub token:       String,
    pub permission:  String,
    pub expires_at:  Option<DateTime<Utc>>,
    pub created_by:  Uuid,
    pub created_at:  DateTime<Utc>,
    pub revoked_at:  Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateShareDto {
    pub permission: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}
