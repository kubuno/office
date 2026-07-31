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
        docx::{export_docx_hf, import_docx},
        odt::{export_odt, import_odt},
        types::PmNode,
    },
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    models::document::Document,
    services::content_files as cf,
    state::AppState,
};

/// Load a document's title, ProseMirror content and page layout for export. The
/// content lives in a Drive file (`draft_file_id` or `file_id`), NOT in a table
/// column — same source as `get_document`. Access: owner or collaborator.
async fn load_doc_pm(
    state: &AppState,
    user_id: Uuid,
    id: Uuid,
) -> Result<(
    String,
    PmNode,
    crate::converters::docx::SectionInfo,
    HeaderFooter,
    HeaderFooter,
    Vec<crate::converters::docx::CommentThreadIn>,
)> {
    let doc = sqlx::query_as::<_, Document>(
        r#"SELECT id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                  trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id, source_format,
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
    let stored = cf::extract_document_pm(&file_content);
    // Read the layout and the header/footer BEFORE flattening: flatten_pm drops
    // the whole envelope, which is why page numbering never reached the export.
    let layout = layout_from_envelope(&stored);
    let hf = hf_from_envelope(&stored);
    let hf_even = hf_even_from_envelope(&stored);
    let threads = comments_from_envelope(&stored);
    let pm_json = flatten_pm(stored);
    let pm_doc: PmNode = serde_json::from_value(pm_json)
        .map_err(|e| OfficeError::Conversion(format!("Contenu invalide: {e}")))?;
    Ok((doc.title, pm_doc, layout, hf, hf_even, threads))
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

/// Formats a document can come from, and whether we can write them back.
///
/// A format is only a save target when we have a WRITER for it: `.doc` reads
/// fine but there is no `.doc` writer, so a document opened from one falls back
/// to `.docx` — losing the original format silently would be worse.
pub(crate) const READABLE: [&str; 3] = ["docx", "odt", "doc"];
pub(crate) const WRITABLE: [&str; 2] = ["docx", "odt"];

/// Origin format from a file name and its MIME type, or `None` when we cannot
/// read it at all.
pub(crate) fn detect_format(name: &str, mime: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    if mime.contains("wordprocessingml") || lower.ends_with(".docx") || lower.ends_with(".dotx") {
        return Some("docx");
    }
    if mime.contains("opendocument.text") || lower.ends_with(".odt") || lower.ends_with(".ott") {
        return Some("odt");
    }
    // `application/msword` is the Word 97-2003 binary format.
    if mime.contains("application/msword") || lower.ends_with(".doc") {
        return Some("doc");
    }
    None
}

/// Format to propose when saving a document that came from `origin`.
pub(crate) fn save_format(origin: Option<&str>) -> &'static str {
    match origin {
        Some("odt") => "odt",
        // `.doc` has no writer: the nearest faithful target is `.docx`.
        Some("docx") | Some("doc") => "docx",
        _ => "docx",
    }
}

/// Strip a readable extension from a file name to make a document title.
pub(crate) fn title_from_file_name(name: &str) -> String {
    let mut base = name.trim();
    for ext in [".docx", ".dotx", ".odt", ".ott", ".doc"] {
        if base.to_ascii_lowercase().ends_with(ext) {
            base = &base[..base.len() - ext.len()];
            break;
        }
    }
    let base = base.trim();
    if base.is_empty() {
        "Document importé".to_string()
    } else {
        base.to_string()
    }
}

/// The header and footer of the stored envelope, as ProseMirror docs.
type HeaderFooter = (Option<PmNode>, Option<PmNode>);

/// Comment threads mirrored into the stored envelope by the editor. They live in
/// the collaborative document, which the server only ever sees as opaque binary,
/// so this mirror is the only way they can reach an export.
fn comments_from_envelope(pm: &Value) -> Vec<crate::converters::docx::CommentThreadIn> {
    pm.get("comments")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

/// Read the header and footer out of the envelope. A document made only of
/// empty paragraphs counts as absent — writing an empty part would add a blank
/// header to every page.
fn hf_from_envelope(pm: &Value) -> HeaderFooter {
    hf_pair(pm, "header", "footer")
}

/// The even-page header and footer, used when the document has
/// "different odd and even pages" set.
fn hf_even_from_envelope(pm: &Value) -> HeaderFooter {
    hf_pair(pm, "headerEven", "footerEven")
}

/// Read a header/footer pair. A part made only of empty paragraphs counts as
/// absent: writing it would add a blank header to every page.
fn hf_pair(pm: &Value, header_key: &str, footer_key: &str) -> HeaderFooter {
    fn has_text(n: &PmNode) -> bool {
        n.text.as_deref().is_some_and(|t| !t.trim().is_empty())
            || n.children().iter().any(has_text)
    }
    let one = |key: &str| -> Option<PmNode> {
        let v = pm.get(key)?;
        let node: PmNode = serde_json::from_value(v.clone()).ok()?;
        has_text(&node).then_some(node)
    };
    (one(header_key), one(footer_key))
}

/// Rebuild a `SectionInfo` from the stored multi-page envelope — the inverse of
/// `build_doc_envelope`. Without this the DOCX export always used the default
/// A4 layout, silently discarding the paper size, orientation and margins the
/// user actually set.
fn layout_from_envelope(pm: &Value) -> crate::converters::docx::SectionInfo {
    use crate::converters::docx::SectionInfo;
    let mut s = SectionInfo::default();
    if pm.get("_type").and_then(|v| v.as_str()) != Some("multi-page") {
        return s;
    }
    if let Some(paper) = pm.get("paperSize").and_then(|v| v.as_str()) {
        s.paper = paper.to_string();
    }
    if let Some(t) = pm.get("hfFirstPage").and_then(|v| v.as_bool()) {
        s.title_pg = t;
    }
    // The editor may hold several sections; the body is exported as one, so the
    // first section carries the page setup.
    let Some(sec) = pm.pointer("/sections/0") else {
        return s;
    };
    let f = |ptr: &str| sec.pointer(ptr).and_then(|v| v.as_f64());
    if let Some(o) = sec.get("orientation").and_then(|v| v.as_str()) {
        s.orientation = o.to_string();
    }
    if let Some(v) = f("/margins/top") {
        s.margin_top = v;
    }
    if let Some(v) = f("/margins/right") {
        s.margin_right = v;
    }
    if let Some(v) = f("/margins/bottom") {
        s.margin_bottom = v;
    }
    if let Some(v) = f("/margins/left") {
        s.margin_left = v;
    }
    if let Some(v) = f("/gutter") {
        s.gutter = v;
    }
    if let Some(v) = f("/headerDist") {
        s.header_dist = v;
    }
    if let Some(v) = f("/footerDist") {
        s.footer_dist = v;
    }
    if let Some(v) = sec.get("vAlign").and_then(|v| v.as_str()) {
        s.v_align = v.to_string();
    }
    if let Some(v) = sec.get("sectionStart").and_then(|v| v.as_str()) {
        s.section_start = v.to_string();
    }
    if let Some(n) = sec.get("columns").and_then(|v| v.as_u64()) {
        s.columns.count = n.clamp(1, 45) as u16;
    }
    if let Some(v) = f("/colSpace") {
        s.columns.space = v;
    }
    // `evenOdd` is a DOCUMENT setting in the envelope, not a section one.
    if pm.get("evenOdd").and_then(|v| v.as_bool()) == Some(true) {
        s.even_odd_headers = true;
    }
    // Same for tracking: the editor persists it per document, and the export
    // turns it into `w:trackRevisions` so the file opens with tracking on.
    if pm.get("trackChanges").and_then(|v| v.as_bool()) == Some(true) {
        s.track_changes = true;
    }
    s
}

pub async fn export_as_docx(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Response> {
    let (title, pm_doc, layout, (header, footer), (h_even, f_even), threads) =
        load_doc_pm(&state, user.id, id).await?;

    let bytes = export_docx_hf(
        &pm_doc, &title, &layout, header.as_ref(), footer.as_ref(),
        h_even.as_ref(), f_even.as_ref(), &threads,
    )?;
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
    let (title, pm_doc, _layout, _hf, _hf_even, _threads) =
        load_doc_pm(&state, user.id, id).await?;

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
    let mut source_format: Option<&'static str> = None;

    while let Some(field) = multipart.next_field().await
        .map_err(|e| OfficeError::Validation(format!("Multipart invalide: {e}")))?
    {
        match field.name() {
            Some("file") => {
                let original_name = field.file_name().map(|s| s.to_string()).unwrap_or_default();
                let mime = field.content_type().map(|s| s.to_string()).unwrap_or_default();

                let Some(fmt) = detect_format(&original_name, &mime) else {
                    return Err(OfficeError::Validation(format!(
                        "Format non supporté. Formats lus : {}",
                        READABLE.join(", ")
                    )));
                };
                source_format = Some(fmt);
                file_name = title_from_file_name(&original_name);

                let bytes = field.bytes().await
                    .map_err(|e| OfficeError::Validation(format!("Lecture fichier échouée: {e}")))?;

                let to_v = |n: &PmNode| serde_json::to_value(n).map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)));
                let (content, body) = if fmt == "docx" || fmt == "doc" {
                    // DOCX : corps + en-tête/pied éventuels → enveloppe multi-page.
                    let (body, header, footer, section) = if fmt == "doc" {
                        crate::converters::doc::import_doc(&bytes)?
                    } else {
                        import_docx(&bytes)?
                    };
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
        r#"INSERT INTO documents (owner_id, title, parent_id, word_count, file_id, source_format)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                     trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id, source_format,
                     created_at, updated_at"#,
    )
    .bind(user.id)
    .bind(&file_name)
    .bind(parent_id)
    .bind(word_count)
    .bind(file_id)
    .bind(source_format)
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
        "evenOdd": section.even_odd_headers,
        "trackChanges": section.track_changes,
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

/// POST /:id/save-source — write the document back into the file it was opened
/// from, in that file's own format.
///
/// This is what a user expects after opening a `.docx`: saving updates the
/// `.docx`, not only our internal copy. It is refused when the origin format has
/// no writer (`.doc`) rather than silently changing the file's format behind the
/// user's back — the caller should then offer "save as" with `save_format`.
pub async fn save_to_source(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let (title, pm_doc, layout, (header, footer), (h_even, f_even), threads) =
        load_doc_pm(&state, user.id, id).await?;

    let doc = sqlx::query_as::<_, Document>(
        r#"SELECT id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                  trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id,
                  source_format, created_at, updated_at
           FROM documents WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Document '{id}' introuvable")))?;

    let source_file_id = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT source_file_id FROM documents WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?
    .ok_or_else(|| {
        OfficeError::Validation("Ce document ne provient pas d'un fichier importé".into())
    })?;

    let origin = doc.source_format.as_deref();
    let fmt = origin.filter(|f| WRITABLE.contains(f)).ok_or_else(|| {
        OfficeError::Validation(format!(
            "Le format d'origine ({}) ne peut pas être réécrit ; enregistrez en {}",
            origin.unwrap_or("inconnu"),
            save_format(origin)
        ))
    })?;

    let bytes = match fmt {
        "odt" => export_odt(&pm_doc, &title)?,
        _ => export_docx_hf(
            &pm_doc, &title, &layout, header.as_ref(), footer.as_ref(),
            h_even.as_ref(), f_even.as_ref(), &threads,
        )?,
    };

    state
        .files_client
        .update_file_content(user.id, source_file_id, bytes::Bytes::from(bytes))
        .await
        .map_err(OfficeError::Internal)?;

    Ok(Json(json!({ "saved": true, "format": fmt })))
}

#[cfg(test)]
mod format_tests {
    use super::*;

    #[test]
    fn formats_are_detected_from_name_or_mime() {
        assert_eq!(detect_format("Rapport.docx", ""), Some("docx"));
        assert_eq!(detect_format("Rapport.DOCX", ""), Some("docx"));
        assert_eq!(detect_format("note.odt", ""), Some("odt"));
        assert_eq!(detect_format("vieux.doc", ""), Some("doc"));
        assert_eq!(detect_format("sans-extension", "application/msword"), Some("doc"));
        assert_eq!(
            detect_format("x", "application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
            Some("docx")
        );
        assert_eq!(detect_format("image.png", "image/png"), None);
    }

    #[test]
    fn a_doc_falls_back_to_docx_because_we_cannot_write_doc() {
        assert_eq!(save_format(Some("docx")), "docx");
        assert_eq!(save_format(Some("odt")), "odt");
        // The point of the fallback: `.doc` is readable but not writable.
        assert!(READABLE.contains(&"doc"));
        assert!(!WRITABLE.contains(&"doc"));
        assert_eq!(save_format(Some("doc")), "docx");
        assert_eq!(save_format(None), "docx");
    }

    #[test]
    fn the_title_drops_the_extension_and_never_ends_up_empty() {
        assert_eq!(title_from_file_name("Rapport annuel.docx"), "Rapport annuel");
        assert_eq!(title_from_file_name("note.odt"), "note");
        assert_eq!(title_from_file_name("vieux.doc"), "vieux");
        // A name that is only an extension must not yield an empty title.
        assert_eq!(title_from_file_name(".docx"), "Document importé");
        // A dot inside the name is not an extension.
        assert_eq!(title_from_file_name("v1.2 final.docx"), "v1.2 final");
    }
}
