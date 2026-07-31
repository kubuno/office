//! Spreadsheet export endpoints (XLSX / ODS) — mirror of the document export
//! handlers (`document_convert.rs`): load the ACTIVE content file (draft over
//! main), convert, stream back as an attachment.
use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Extension,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    converters::ods::{export_ods, OdsCell, OdsSheetData},
    converters::xlsx::{export_xlsx, SheetExportMeta},
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    models::spreadsheet::{Spreadsheet, SpreadsheetSheet},
    services::content_files as cf,
    state::AppState,
};

/// Load a spreadsheet's title, ordered sheet list and content file for export.
/// Access: owner or collaborator. The content file is keyed by sheet UUID; the
/// order comes from SQL (`position`).
async fn load_ss_content(
    state: &AppState,
    user_id: Uuid,
    id: Uuid,
) -> Result<(String, Vec<SpreadsheetSheet>, Value)> {
    let ss = sqlx::query_as::<_, Spreadsheet>(
        r#"SELECT id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, source_format, created_at, updated_at
           FROM spreadsheets WHERE id = $1 AND is_trashed = FALSE AND (owner_id = $2 OR EXISTS (
               SELECT 1 FROM spreadsheet_collaborators c WHERE c.spreadsheet_id = $1 AND c.user_id = $2
           ))"#,
    )
    .bind(id).bind(user_id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Tableur introuvable".into()))?;

    let sheets: Vec<SpreadsheetSheet> = sqlx::query_as::<_, SpreadsheetSheet>(
        r#"SELECT id, spreadsheet_id, name, position, created_at, updated_at
           FROM spreadsheet_sheets WHERE spreadsheet_id = $1 ORDER BY position ASC"#,
    )
    .bind(id).fetch_all(&state.db).await?;

    let content_file_id = ss.draft_file_id.or(ss.file_id)
        .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Spreadsheet {id} sans fichier de contenu")))?;
    let file_content = cf::read_content(state, ss.owner_id, content_file_id).await?;

    // An encrypted workbook stores empty cells server-side (client-side keys) —
    // a server export would silently produce an empty file. Refuse instead.
    let encrypted = sheets.iter().any(|sh| {
        file_content.get("sheets")
            .and_then(|s| s.get(sh.id.to_string()))
            .and_then(|d| d.get("enc"))
            .map(|e| !e.is_null())
            .unwrap_or(false)
    });
    if encrypted {
        return Err(OfficeError::Validation(
            "Classeur chiffré : déverrouillez-le avant l'export".into(),
        ));
    }

    Ok((ss.title, sheets, file_content))
}

fn attachment_response(bytes: Vec<u8>, mime: &'static str, filename: &str) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime),
            (header::CONTENT_DISPOSITION, &format!("attachment; filename=\"{filename}\"")),
        ],
        Body::from(bytes),
    ).into_response()
}

fn sanitize_filename(title: &str, ext: &str) -> String {
    let safe: String = title.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
        .collect();
    let trimmed = safe.trim();
    if trimmed.is_empty() { format!("classeur.{ext}") } else { format!("{trimmed}.{ext}") }
}

pub async fn export_as_xlsx(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Response> {
    let (title, sheets, file_content) = load_ss_content(&state, user.id, id).await?;
    let metas: Vec<SheetExportMeta> = sheets.into_iter()
        .map(|s| SheetExportMeta { id: s.id, name: s.name })
        .collect();

    let bytes = export_xlsx(&title, &file_content, &metas)
        .map_err(OfficeError::Internal)?;
    Ok(attachment_response(
        bytes,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        &sanitize_filename(&title, "xlsx"),
    ))
}

/// Project the internal sheet_data into the ODS writer's flat model. Shared by the
/// download endpoint and by save-to-source.
fn ods_sheets_from(sheets: &[SpreadsheetSheet], file_content: &Value) -> Vec<OdsSheetData> {
    sheets.iter().map(|sh| {
        let data = file_content.get("sheets")
            .and_then(|s| s.get(sh.id.to_string()))
            .cloned()
            .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
        let mut cells = std::collections::HashMap::new();
        if let Some(map) = data.get("cells").and_then(|c| c.as_object()) {
            for (key, c) in map {
                let value = c.get("v").map(|v| match v {
                    Value::String(s) => s.clone(),
                    Value::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
                    other => other.to_string(),
                });
                let formula = c.get("f").and_then(|f| f.as_str())
                    .map(|f| f.strip_prefix('=').unwrap_or(f).to_string());
                if value.is_some() || formula.is_some() {
                    cells.insert(key.clone(), OdsCell { value, formula });
                }
            }
        }
        let num_map = |k: &str| data.get(k).and_then(|m| m.as_object())
            .map(|m| m.iter().filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f))).collect())
            .unwrap_or_default();
        let row_map = data.get("row_heights").and_then(|m| m.as_object())
            .map(|m| m.iter().filter_map(|(k, v)| Some((k.parse::<i32>().ok()?, v.as_f64()?))).collect())
            .unwrap_or_default();
        OdsSheetData {
            name:        sh.name.clone(),
            cells,
            col_widths:  num_map("col_widths"),
            row_heights: row_map,
            frozen_rows: data.get("frozen_rows").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            frozen_cols: data.get("frozen_cols").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
        }
    }).collect()
}

/// Formats a workbook can be written back to (the ones we can export losslessly
/// enough to overwrite the user's own file).
const WRITABLE: [&str; 2] = ["xlsx", "ods"];

/// POST /:id/save-source — write the workbook back into the file it was opened
/// from, in that file's own format. Excel's behaviour: opening `budget.xlsx` and
/// saving updates `budget.xlsx`, it does not silently switch to our native format.
/// Refuses (rather than converting behind the user's back) when the workbook is
/// native or came from a format we cannot write.
pub async fn save_to_source(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let row = sqlx::query_as::<_, (Option<Uuid>, Option<String>)>(
        "SELECT source_file_id, source_format FROM spreadsheets WHERE id = $1 AND owner_id = $2",
    )
    .bind(id).bind(user.id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Classeur '{id}' introuvable")))?;

    let source_file_id = row.0.ok_or_else(|| {
        OfficeError::Validation("Ce classeur ne provient pas d'un fichier importé".into())
    })?;
    let fmt = row.1.filter(|f| WRITABLE.contains(&f.as_str())).ok_or_else(|| {
        OfficeError::Validation("Le format d'origine ne peut pas être réécrit".into())
    })?;

    let (title, sheets, file_content) = load_ss_content(&state, user.id, id).await?;
    let bytes = if fmt == "ods" {
        let ods_sheets = ods_sheets_from(&sheets, &file_content);
        export_ods(&title, &ods_sheets).map_err(OfficeError::Internal)?
    } else {
        let metas: Vec<SheetExportMeta> = sheets.into_iter()
            .map(|s| SheetExportMeta { id: s.id, name: s.name })
            .collect();
        export_xlsx(&title, &file_content, &metas).map_err(OfficeError::Internal)?
    };

    state.files_client
        .update_file_content(user.id, source_file_id, bytes::Bytes::from(bytes))
        .await
        .map_err(OfficeError::Internal)?;

    Ok(Json(json!({ "saved": true, "format": fmt })))
}

pub async fn export_as_ods(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Response> {
    let (title, sheets, file_content) = load_ss_content(&state, user.id, id).await?;

    // Project the internal sheet_data into the ODS writer's flat model.
    let ods_sheets = ods_sheets_from(&sheets, &file_content);

    let bytes = export_ods(&title, &ods_sheets).map_err(OfficeError::Internal)?;
    Ok(attachment_response(
        bytes,
        "application/vnd.oasis.opendocument.spreadsheet",
        &sanitize_filename(&title, "ods"),
    ))
}
