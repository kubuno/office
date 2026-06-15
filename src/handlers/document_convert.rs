use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    converters::{
        docx::{export_docx, import_docx},
        odt::{export_odt, import_odt},
        types::PmNode,
    },
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    state::AppState,
};

pub async fn export_as_docx(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Response> {
    let row = sqlx::query(
        "SELECT title, content_json FROM documents WHERE id = $1 AND owner_id = $2 AND is_trashed = false"
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Document '{id}' introuvable")))?;

    let title: String = row.get("title");
    let content_json: Value = row.get("content_json");

    let pm_doc: PmNode = serde_json::from_value(content_json)
        .map_err(|e| OfficeError::Conversion(format!("Contenu invalide: {e}")))?;

    let bytes = export_docx(&pm_doc, &title)?;
    let filename = sanitize_filename(&title, "docx");

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
            (header::CONTENT_DISPOSITION, &format!("attachment; filename=\"{filename}\"")),
        ],
        Body::from(bytes),
    ).into_response())
}

pub async fn export_as_odt(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Response> {
    let row = sqlx::query(
        "SELECT title, content_json FROM documents WHERE id = $1 AND owner_id = $2 AND is_trashed = false"
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Document '{id}' introuvable")))?;

    let title: String = row.get("title");
    let content_json: Value = row.get("content_json");

    let pm_doc: PmNode = serde_json::from_value(content_json)
        .map_err(|e| OfficeError::Conversion(format!("Contenu invalide: {e}")))?;

    let bytes = export_odt(&pm_doc, &title)?;
    let filename = sanitize_filename(&title, "odt");

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/vnd.oasis.opendocument.text"),
            (header::CONTENT_DISPOSITION, &format!("attachment; filename=\"{filename}\"")),
        ],
        Body::from(bytes),
    ).into_response())
}

pub async fn import_document(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    mut multipart: Multipart,
) -> Result<Json<Value>> {
    let mut parsed_content: Option<Value> = None;
    let mut file_name = String::from("Document importé");
    let mut parent_id: Option<Uuid> = None;

    while let Some(field) = multipart.next_field().await
        .map_err(|e| OfficeError::Validation(format!("Multipart invalide: {e}")))?
    {
        match field.name() {
            Some("file") => {
                let original_name = field.file_name().map(|s| s.to_string()).unwrap_or_default();
                let mime = field.content_type().map(|s| s.to_string()).unwrap_or_default();

                let is_docx = mime.contains("wordprocessingml") || original_name.ends_with(".docx");
                let is_odt  = mime.contains("opendocument.text") || original_name.ends_with(".odt");

                if !is_docx && !is_odt {
                    return Err(OfficeError::Validation(
                        "Format non supporté. Utilisez .docx ou .odt".into()
                    ));
                }

                // Build document title from filename (strip extension)
                let base = original_name
                    .trim_end_matches(".docx")
                    .trim_end_matches(".odt")
                    .trim();
                file_name = if base.is_empty() { "Document importé".to_string() } else { base.to_string() };

                let bytes = field.bytes().await
                    .map_err(|e| OfficeError::Validation(format!("Lecture fichier échouée: {e}")))?;

                let pm_doc = if is_docx {
                    import_docx(&bytes)?
                } else {
                    import_odt(&bytes)?
                };

                parsed_content = Some(serde_json::to_value(&pm_doc)
                    .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?);
            }
            Some("parent_id") => {
                let val = field.text().await.unwrap_or_default();
                parent_id = val.parse::<Uuid>().ok();
            }
            _ => {}
        }
    }

    let content_json = parsed_content
        .ok_or_else(|| OfficeError::Validation("Aucun fichier fourni".into()))?;

    let text = extract_text(&content_json);
    let word_count = text.split_whitespace().count() as i32;

    let row = sqlx::query(
        r#"INSERT INTO documents (owner_id, title, content_json, content_text, word_count, parent_id)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, owner_id, title, icon, cover_url, content_json, content_text,
                     word_count, is_starred, is_trashed, trashed_at, parent_id,
                     position, last_editor_id, file_id, created_at, updated_at"#
    )
    .bind(user.id)
    .bind(&file_name)
    .bind(&content_json)
    .bind(&text)
    .bind(word_count)
    .bind(parent_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({
        "document": {
            "id":           row.get::<Uuid, _>("id"),
            "owner_id":     row.get::<Uuid, _>("owner_id"),
            "title":        row.get::<String, _>("title"),
            "icon":         row.get::<Option<String>, _>("icon"),
            "cover_url":    row.get::<Option<String>, _>("cover_url"),
            "content_json": row.get::<Value, _>("content_json"),
            "word_count":   row.get::<i32, _>("word_count"),
            "is_starred":   row.get::<bool, _>("is_starred"),
            "is_trashed":   row.get::<bool, _>("is_trashed"),
            "parent_id":    row.get::<Option<Uuid>, _>("parent_id"),
            "file_id":      row.get::<Option<Uuid>, _>("file_id"),
            "created_at":   row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            "updated_at":   row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
        }
    })))
}

fn sanitize_filename(title: &str, ext: &str) -> String {
    let safe: String = title.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
        .collect();
    let trimmed = safe.trim();
    if trimmed.is_empty() {
        format!("document.{ext}")
    } else {
        format!("{trimmed}.{ext}")
    }
}

fn extract_text(content: &Value) -> String {
    match content {
        Value::Object(obj) => {
            let mut parts = Vec::new();
            if let Some(Value::Array(children)) = obj.get("content") {
                for child in children {
                    let t = extract_text(child);
                    if !t.is_empty() {
                        parts.push(t);
                    }
                }
            }
            if let Some(Value::String(t)) = obj.get("text") {
                parts.push(t.clone());
            }
            parts.join(" ")
        }
        Value::Array(arr) => arr.iter().map(extract_text).collect::<Vec<_>>().join(" "),
        _ => String::new(),
    }
}
