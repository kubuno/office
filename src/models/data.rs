use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Datasource ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Datasource {
    pub id:                Uuid,
    pub owner_id:          Uuid,
    pub name:              String,
    pub description:       Option<String>,
    pub source_type:       String,
    pub config:            serde_json::Value,
    pub connection_status: String,
    pub last_tested_at:    Option<DateTime<Utc>>,
    pub connection_error:  Option<String>,
    pub created_at:        DateTime<Utc>,
    pub updated_at:        DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDatasourceDto {
    pub name:         String,
    pub description:  Option<String>,
    pub source_type:  String,
    pub config:       Option<serde_json::Value>,
    pub credentials:  Option<DatasourceCredentials>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDatasourceDto {
    pub name:         Option<String>,
    pub description:  Option<String>,
    pub config:       Option<serde_json::Value>,
    pub credentials:  Option<DatasourceCredentials>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DatasourceCredentials {
    pub password: Option<String>,
    pub api_key:  Option<String>,
    pub token:    Option<String>,
}

// ── Dataset ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Dataset {
    pub id:              Uuid,
    pub owner_id:        Uuid,
    pub datasource_id:   Option<Uuid>,
    pub name:            String,
    pub description:     Option<String>,
    // Définition + schéma + cache de résultats vivent dans un fichier .kbdst ;
    // peuplés après le SELECT.
    #[sqlx(default)]
    pub raw_sql:         Option<String>,
    #[sqlx(default)]
    pub query_steps:     serde_json::Value,
    #[sqlx(default)]
    pub schema_cache:    serde_json::Value,
    pub file_id:         Option<Uuid>,
    pub row_count:       Option<i64>,
    pub last_refresh_at: Option<DateTime<Utc>>,
    pub refresh_error:   Option<String>,
    pub refresh_schedule: Option<String>,
    pub status:          String,
    pub created_at:      DateTime<Utc>,
    pub updated_at:      DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDatasetDto {
    pub name:          String,
    pub description:   Option<String>,
    pub datasource_id: Option<Uuid>,
    pub raw_sql:       Option<String>,
    pub query_steps:   Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDatasetDto {
    pub name:          Option<String>,
    pub description:   Option<String>,
    pub raw_sql:       Option<String>,
    pub query_steps:   Option<serde_json::Value>,
    pub datasource_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct DatasetPreviewQuery {
    pub step_index: Option<usize>,
    pub limit:      Option<i64>,
}

// ── Query Step ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryStep {
    Source {
        sql:    String,
        #[serde(default)]
        params: Vec<serde_json::Value>,
    },
    Filter {
        column:    String,
        operator:  String,
        value:     serde_json::Value,
    },
    Sort {
        column:    String,
        direction: String,
    },
    Group {
        by:            Vec<String>,
        aggregations:  Vec<Aggregation>,
    },
    Join {
        dataset_id:   Uuid,
        left_column:  String,
        right_column: String,
        join_type:    String,
    },
    AddColumn {
        name:       String,
        expression: String,
    },
    Rename {
        from: String,
        to:   String,
    },
    RemoveColumns {
        columns: Vec<String>,
    },
    ChangeType {
        column:    String,
        data_type: String,
    },
    Limit {
        count: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aggregation {
    pub column:   String,
    pub function: String,
    pub alias:    Option<String>,
}

// ── Measure ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Measure {
    pub id:             Uuid,
    pub owner_id:       Uuid,
    pub dataset_id:     Uuid,
    pub name:           String,
    pub description:    Option<String>,
    pub expression:     String,
    pub result_type:    String,
    pub format_string:  Option<String>,
    pub display_folder: Option<String>,
    pub is_valid:       bool,
    pub compile_error:  Option<String>,
    pub created_at:     DateTime<Utc>,
    pub updated_at:     DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMeasureDto {
    pub dataset_id:     Uuid,
    pub name:           String,
    pub description:    Option<String>,
    pub expression:     String,
    pub result_type:    Option<String>,
    pub format_string:  Option<String>,
    pub display_folder: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMeasureDto {
    pub name:           Option<String>,
    pub description:    Option<String>,
    pub expression:     Option<String>,
    pub result_type:    Option<String>,
    pub format_string:  Option<String>,
    pub display_folder: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ValidateMeasureDto {
    pub expression: String,
    pub dataset_id: Uuid,
}

// ── Relation ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Relation {
    pub id:              Uuid,
    pub owner_id:        Uuid,
    pub from_dataset_id: Uuid,
    pub from_column:     String,
    pub to_dataset_id:   Uuid,
    pub to_column:       String,
    pub cardinality:     String,
    pub cross_filter:    String,
    pub is_active:       bool,
    pub created_at:      DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRelationDto {
    pub from_dataset_id: Uuid,
    pub from_column:     String,
    pub to_dataset_id:   Uuid,
    pub to_column:       String,
    pub cardinality:     Option<String>,
    pub cross_filter:    Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRelationDto {
    pub cardinality:  Option<String>,
    pub cross_filter: Option<String>,
    pub is_active:    Option<bool>,
}

// ── Report ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Report {
    pub id:           Uuid,
    pub owner_id:     Uuid,
    pub title:        String,
    pub description:  Option<String>,
    pub theme:        serde_json::Value,
    pub page_count:   i32,
    pub dataset_ids:  Vec<Uuid>,
    pub share_token:  Option<String>,
    pub is_public:    bool,
    pub is_trashed:   bool,
    pub is_starred:   bool,
    pub thumbnail_url: Option<String>,
    pub created_at:   DateTime<Utc>,
    pub updated_at:   DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateReportDto {
    pub title:       Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateReportDto {
    pub title:        Option<String>,
    pub description:  Option<String>,
    pub theme:        Option<serde_json::Value>,
    pub dataset_ids:  Option<Vec<Uuid>>,
    pub is_starred:   Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListReportsQuery {
    pub search:  Option<String>,
    pub starred: Option<bool>,
    pub trashed: Option<bool>,
    pub limit:   Option<i64>,
    pub offset:  Option<i64>,
}

// ── Report Page ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ReportPage {
    pub id:         Uuid,
    pub report_id:  Uuid,
    pub title:      String,
    pub position:   i32,
    pub width:      i32,
    pub height:     i32,
    pub background: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePageDto {
    pub title:      Option<String>,
    pub width:      Option<i32>,
    pub height:     Option<i32>,
    pub background: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePageDto {
    pub title:      Option<String>,
    pub position:   Option<i32>,
    pub width:      Option<i32>,
    pub height:     Option<i32>,
    pub background: Option<String>,
}

// ── Widget ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Widget {
    pub id:          Uuid,
    pub page_id:     Uuid,
    pub report_id:   Uuid,
    pub widget_type: String,
    pub x:           i32,
    pub y:           i32,
    pub width:       i32,
    pub height:      i32,
    pub config:      serde_json::Value,
    pub z_index:     i32,
    pub created_at:  DateTime<Utc>,
    pub updated_at:  DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWidgetDto {
    pub widget_type: String,
    pub x:           Option<i32>,
    pub y:           Option<i32>,
    pub width:       Option<i32>,
    pub height:      Option<i32>,
    pub config:      Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWidgetDto {
    pub x:           Option<i32>,
    pub y:           Option<i32>,
    pub width:       Option<i32>,
    pub height:      Option<i32>,
    pub config:      Option<serde_json::Value>,
    pub z_index:     Option<i32>,
    pub widget_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BatchUpdateWidgetDto {
    pub id:          Uuid,
    pub x:           Option<i32>,
    pub y:           Option<i32>,
    pub width:       Option<i32>,
    pub height:      Option<i32>,
    pub z_index:     Option<i32>,
}

// ── Execute ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExecuteQueryDto {
    pub dataset_id:  Uuid,
    pub dimensions:  Option<Vec<String>>,
    pub metrics:     Option<Vec<MetricSpec>>,
    pub filters:     Option<Vec<FilterSpec>>,
    pub sort:        Option<Vec<SortSpec>>,
    pub limit:       Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct MetricSpec {
    pub column:    String,
    pub function:  String,
    pub alias:     Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FilterSpec {
    pub column:   String,
    pub operator: String,
    pub value:    serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct SortSpec {
    pub column:    String,
    pub direction: String,
}
