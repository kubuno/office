use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Formule mathématique/logique éditée en LaTeX. Le code LaTeX (`latex`) est lu/écrit
/// depuis le fichier `.kbmath` (JSON gzip) ; la base ne stocke que la métadonnée.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Formula {
    pub id:          Uuid,
    pub owner_id:    Uuid,
    pub name:        String,
    pub description: Option<String>,
    #[sqlx(default)]
    pub latex:       String, // peuplé depuis le fichier .kbmath après le SELECT
    pub file_id:     Option<Uuid>,
    pub is_starred:  bool,
    pub is_trashed:  bool,
    pub created_at:  DateTime<Utc>,
    pub updated_at:  DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateFormulaDto {
    pub name:        Option<String>,
    pub description: Option<String>,
    pub latex:       Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateFormulaDto {
    pub name:        Option<String>,
    pub description: Option<String>,
    pub latex:       Option<String>,
    pub is_starred:  Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListFormulasQuery {
    pub search:  Option<String>,
    pub trashed: Option<bool>,
    pub limit:   Option<i64>,
    pub offset:  Option<i64>,
}
