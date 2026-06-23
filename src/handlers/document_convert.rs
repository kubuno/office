use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    converters::{
        docx::{export_docx, import_docx},
        odt::{export_odt, import_odt},
        types::PmNode,
    },
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    models::document::Document,
    services::content_files as cf,
    state::AppState,
};

/// Load a document's title + ProseMirror content for export. The content lives in a
/// Drive file (`draft_file_id` or `file_id`), NOT in a table column — same source as
/// `get_document`. Access: owner or collaborator.
async fn load_doc_pm(state: &AppState, user_id: Uuid, id: Uuid) -> Result<(String, PmNode)> {
    let doc = sqlx::query_as::<_, Document>(
        r#"SELECT id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                  trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id,
                  created_at, updated_at
           FROM documents
           WHERE id = $1 AND (owner_id = $2 OR EXISTS (
               SELECT 1 FROM document_collaborators WHERE document_id = $1 AND user_id = $2
           ))"#,
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Document '{id}' introuvable")))?;

    let content_file_id = doc.draft_file_id.or(doc.file_id)
        .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Document {id} sans fichier de contenu")))?;
    let (_fname, file_content) = cf::read_content_named(state, doc.owner_id, content_file_id).await?;
    let pm_json = flatten_pm(cf::extract_document_pm(&file_content));
    let pm_doc: PmNode = serde_json::from_value(pm_json)
        .map_err(|e| OfficeError::Conversion(format!("Contenu invalide: {e}")))?;
    Ok((doc.title, pm_doc))
}

/// Normalise the stored content into a single ProseMirror `doc` node for conversion.
/// The editor persists a multi-page envelope `{ _type:"multi-page", pages:[{content:{type:"doc",…}}] }`;
/// flatten every page's body into one doc. Also tolerates a bare content array.
fn flatten_pm(pm: Value) -> Value {
    if pm.get("_type").and_then(|v| v.as_str()) == Some("multi-page") {
        let mut content: Vec<Value> = Vec::new();
        if let Some(pages) = pm.get("pages").and_then(|v| v.as_array()) {
            for page in pages {
                if let Some(arr) = page.pointer("/content/content").and_then(|v| v.as_array()) {
                    content.extend(arr.iter().cloned());
                }
            }
        }
        return json!({ "type": "doc", "content": content });
    }
    if pm.is_array() {
        return json!({ "type": "doc", "content": pm });
    }
    pm
}

pub async fn export_as_docx(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Response> {
    let (title, pm_doc) = load_doc_pm(&state, user.id, id).await?;

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
    let (title, pm_doc) = load_doc_pm(&state, user.id, id).await?;

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
    let mut parsed_content: Option<Value> = None;   // valeur stockée (enveloppe ou doc plat)
    let mut parsed_body: Option<Value> = None;       // corps seul (pour le compte de mots)
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

                let to_v = |n: &PmNode| serde_json::to_value(n).map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)));
                let (content, body) = if is_docx {
                    // DOCX : corps + en-tête/pied éventuels → enveloppe multi-page.
                    let (body, header, footer, section) = import_docx(&bytes)?;
                    let body_v = to_v(&body)?;
                    // Enveloppe multi-page dès qu'il y a un en-tête/pied OU une mise en
                    // page personnalisée (marges/orientation/format ≠ défaut Word).
                    let content = if header.is_some() || footer.is_some() || section.is_custom() {
                        build_doc_envelope(body_v.clone(), header.as_ref(), footer.as_ref(), &section)?
                    } else {
                        body_v.clone()
                    };
                    (content, body_v)
                } else {
                    let body = import_odt(&bytes)?;
                    let v = to_v(&body)?;
                    (v.clone(), v)
                };
                parsed_content = Some(content);
                parsed_body = Some(body);
            }
            Some("parent_id") => {
                let val = field.text().await.unwrap_or_default();
                parent_id = val.parse::<Uuid>().ok();
            }
            _ => {}
        }
    }

    let pm_json = parsed_content
        .ok_or_else(|| OfficeError::Validation("Aucun fichier fourni".into()))?;

    // Compte de mots calculé sur le CORPS (l'enveloppe multi-page n'expose pas de
    // `content` au sommet ; `extract_text` n'y trouverait rien).
    let word_count = parsed_body
        .as_ref()
        .map(|b| extract_text(b).split_whitespace().count() as i32)
        .unwrap_or(0);

    // Content lives in a Drive file (.kbdoc), not a table column — mirror create_document.
    let (file_id, pm_json) = cf::create_document_content_file(&state, user.id, &file_name, pm_json).await?;

    let doc = sqlx::query_as::<_, Document>(
        r#"INSERT INTO documents (owner_id, title, parent_id, word_count, file_id)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                     trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id,
                     created_at, updated_at"#,
    )
    .bind(user.id)
    .bind(&file_name)
    .bind(parent_id)
    .bind(word_count)
    .bind(file_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "document": doc, "content_json": pm_json })))
}

/// Construit l'enveloppe multi-page `{_type:"multi-page", sections, pages, header,
/// footer, …}` attendue par l'éditeur (cf. `parseDocContent`/`serializeDoc` du front)
/// quand un DOCX importé porte un en-tête et/ou un pied de page.
pub(crate) fn build_doc_envelope(
    body: Value,
    header: Option<&PmNode>,
    footer: Option<&PmNode>,
    section: &crate::converters::docx::SectionInfo,
) -> Result<Value> {
    let sec_id = Uuid::new_v4().to_string();
    let page_id = Uuid::new_v4().to_string();
    let empty_hf = json!({ "type": "doc", "content": [{ "type": "paragraph" }] });
    let to_hf = |o: Option<&PmNode>| -> Result<Value> {
        match o {
            Some(n) => serde_json::to_value(n).map_err(|e| OfficeError::Internal(anyhow::anyhow!(e))),
            None => Ok(empty_hf.clone()),
        }
    };
    Ok(json!({
        "_type": "multi-page",
        "sections": [{ "id": sec_id.clone(), "orientation": section.orientation,
                       "margins": { "top": section.margin_top, "right": section.margin_right,
                                    "bottom": section.margin_bottom, "left": section.margin_left },
                       // Mise en page (dialogue « Mise en page » façon Word).
                       "gutter": section.gutter, "headerDist": section.header_dist,
                       "footerDist": section.footer_dist, "vAlign": section.v_align,
                       "sectionStart": section.section_start }],
        "pages": [{ "id": page_id, "sectionId": sec_id, "content": body }],
        "header": to_hf(header)?,
        "footer": to_hf(footer)?,
        "pageNumbers": "none",
        "paperSize": section.paper,
        "hfFirstPage": section.title_pg,
    }))
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
