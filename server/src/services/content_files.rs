/// Helpers for reading and writing per-entity content files in the Files module.
///
/// File formats:
///   Document    → {"version":1,"content":{prosemirror json}}
///   Spreadsheet → {"version":1,"sheets":{"<id>":{cells,col_widths,row_heights,frozen_rows,frozen_cols}}}
///   Presentation→ {"version":1,"slides":{"<id>":{elements,background,notes,transition}}}
///   Diagram     → {"version":1,"pages":{"<id>":{shapes,connectors}}}
///
/// Main content files live in Office/<Type>/<title>.json (visible to user).
/// Draft files live in Office/<Type>/.drafts/<entity_id>.json (protected, invisible).
use anyhow::Result;
use bytes::Bytes;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{errors::OfficeError, state::AppState};

/// Delete the Drive files an entity owned, after its row has been deleted.
///
/// An entity's file IS its face in Drive: keeping the file once the entity is
/// gone leaves an orphan that opens onto nothing (the "open" call answers 404 and
/// the user is stuck). Call this on PERMANENT deletion only — trashing keeps the
/// row, so the file must stay too.
///
/// Best-effort: a Drive failure is logged, never fatal, because the row is
/// already gone and failing here would leave the caller unable to retry.
/// `source_file_id` is deliberately NOT passed by callers: that column points at
/// a file the user supplied (an imported .docx…), which is theirs to keep.
pub async fn delete_entity_files(
    state:    &AppState,
    user_id:  Uuid,
    file_ids: impl IntoIterator<Item = Uuid>,
) {
    for fid in file_ids {
        if let Err(e) = state.files_client.delete_file(user_id, fid).await {
            tracing::warn!(file_id = %fid, error = %e, "Drive: delete_file failed — file left orphaned");
        }
    }
}

// ── Read / write helpers ──────────────────────────────────────────────────────

pub async fn read_content(state: &AppState, user_id: Uuid, file_id: Uuid) -> Result<Value, OfficeError> {
    Ok(read_content_named(state, user_id, file_id).await?.1)
}

/// Comme `read_content` mais renvoie aussi le NOM du fichier (.kb***) — utile pour
/// dériver le titre de l'entité (titre = nom de fichier sans extension).
pub async fn read_content_named(state: &AppState, user_id: Uuid, file_id: Uuid) -> Result<(String, Value), OfficeError> {
    let (file_info, raw) = state.files_client.get_file_content(user_id, file_id).await
        .map_err(OfficeError::Internal)?;
    let fname = file_info.name.clone();

    // Happy path: enveloppe JSON Kubuno {"version":1,...}, gzippée (ou claire pour
    // les anciens fichiers — gunzip détecte la signature 0x1f8b et renvoie tel quel).
    let data = gunzip(&raw).unwrap_or_else(|_| raw.to_vec());
    if let Ok(v) = serde_json::from_slice::<Value>(&data) {
        return Ok((fname, v));
    }

    // Legacy: documents were once stored as raw ODT/DOCX binaries.
    // Convert on the fly so old content remains readable.
    let name = file_info.name.to_lowercase();
    let mime = file_info.mime_type.to_lowercase();

    if mime.contains("wordprocessingml") || name.ends_with(".docx") {
        let (body, header, footer, section) = crate::converters::docx::import_docx(&raw)
            .map_err(|e| OfficeError::Internal(anyhow::anyhow!("legacy docx import: {e}")))?;
        let body_v = serde_json::to_value(&body)
            .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;
        // Enveloppe multi-page si en-tête/pied OU mise en page personnalisée.
        let pm_json = if header.is_some() || footer.is_some() || section.is_custom() {
            crate::handlers::document_convert::build_doc_envelope(body_v.clone(), header.as_ref(), footer.as_ref(), &section)
                .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?
        } else {
            body_v
        };
        return Ok((fname, document_content_from(pm_json)));
    }

    if mime.contains("opendocument") || name.ends_with(".odt") || name.ends_with(".ods") || name.ends_with(".odp") {
        let pm = crate::converters::odt::import_odt(&raw)
            .map_err(|e| OfficeError::Internal(anyhow::anyhow!("legacy odt import: {e}")))?;
        let pm_json = serde_json::to_value(&pm)
            .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;
        return Ok((fname, document_content_from(pm_json)));
    }

    Err(OfficeError::Internal(anyhow::anyhow!("content file parse error: not a valid JSON envelope, ODT, or DOCX")))
}

/// Nom du fichier .kb*** d'une entité (best-effort, None si introuvable).
pub async fn file_name(state: &AppState, owner_id: Uuid, file_id: Uuid) -> Option<String> {
    state.files_client.get_file_meta(owner_id, file_id).await.ok().map(|info| info.name)
}

/// Nom de fichier sans son extension (délégué à la face client de `files`).
pub fn strip_ext(name: &str) -> String { crate::files_client::strip_ext(name) }

/// Renomme le fichier .kb*** principal pour qu'il reflète le titre (titre = nom).
pub async fn rename_content_file(state: &AppState, owner_id: Uuid, file_id: Uuid, title: &str, ext: &str) -> Result<(), OfficeError> {
    let name = kb_file_name(title, ext);
    state.files_client.rename_file(owner_id, file_id, &name).await
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!("rename .kb***: {e}")))?;
    Ok(())
}

pub async fn write_content(state: &AppState, user_id: Uuid, file_id: Uuid, content: &Value) -> Result<(), OfficeError> {
    let raw = serde_json::to_vec(content)
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!("content file serialize error: {e}")))?;
    let gz = gzip(&raw)?;
    state.files_client.update_file_content(user_id, file_id, Bytes::from(gz)).await
        .map_err(OfficeError::Internal)
        .map(|_| ())
}

/// Écrit le contenu dans le fichier d'édition (brouillon) ET le reflète dans le
/// fichier principal VISIBLE (.kb***) quand il diffère — sinon le brouillon caché
/// seul laisse « le fichier non enregistré ». Le miroir est best-effort.
pub async fn write_content_mirrored(
    state: &AppState, user_id: Uuid, content_file_id: Uuid, main_file_id: Option<Uuid>, content: &Value,
) -> Result<(), OfficeError> {
    write_content(state, user_id, content_file_id, content).await?;
    if let Some(main) = main_file_id {
        if main != content_file_id {
            let _ = write_content(state, user_id, main, content).await;
        }
    }
    Ok(())
}

// ── Document content helpers ──────────────────────────────────────────────────

pub fn empty_document_content() -> Value {
    json!({ "version": 1, "content": { "type": "doc", "content": [] } })
}

pub fn document_content_from(pm_json: Value) -> Value {
    json!({ "version": 1, "content": pm_json })
}

/// Wraps a brand-new document's body in the `multi-page` layout envelope the
/// editor parses on load, stamping the instance defaults the administrator chose:
/// margins, paper size and change tracking.
///
/// Why here and not in the editor: the layout belongs to the DOCUMENT, not to
/// the session that opens it. Written once at creation, it travels with the file,
/// survives an export, and a later change of policy leaves existing documents
/// alone — which is what "default" means. The editor already reads every one of
/// these keys back (`_type`, `sections[].margins`, `paperSize`, `trackChanges`),
/// so nothing on that side has to learn anything new.
///
/// Content that is ALREADY a `multi-page` envelope — a document created from a
/// template that carries its own layout — is returned untouched: the template's
/// author decided, and an instance default does not overrule a deliberate choice.
pub fn stamp_document_layout(
    body:          Value,
    margins_px:    Option<i64>,
    paper_size:    &str,
    track_changes: bool,
) -> Value {
    if body.get("_type").and_then(Value::as_str) == Some("multi-page") {
        return body;
    }

    // Absent an administrative margin, the editor's own factory value (one inch
    // at 96 dpi) is written explicitly — the envelope has no "unset" state.
    let margin = margins_px.unwrap_or(96);
    let section_id = Uuid::new_v4().to_string();

    let mut envelope = json!({
        "_type": "multi-page",
        "sections": [{
            "id": section_id,
            "orientation": "portrait",
            "margins": { "top": margin, "right": margin, "bottom": margin, "left": margin },
        }],
        "pages": [{
            "id": Uuid::new_v4().to_string(),
            "sectionId": section_id,
            "content": body,
        }],
        "paperSize": paper_size,
    });
    // Written only when on: the editor reads a missing key as false, and an
    // envelope free of noise is an envelope a human can still read.
    if track_changes {
        envelope["trackChanges"] = json!(true);
    }
    envelope
}

pub fn extract_document_pm(file_content: &Value) -> Value {
    file_content.get("content")
        .cloned()
        .unwrap_or_else(|| json!({"type":"doc","content":[]}))
}

/// Create the content file for a new document.
/// Returns (file_id, initial pm_json).
pub async fn create_document_content_file(
    state:       &AppState,
    user_id:     Uuid,
    title:       &str,
    initial_pm:  Value,
) -> Result<(Uuid, Value), OfficeError> {
    let folder = state.files_client
        .ensure_folder_path(user_id, "Office/Documents", true, Some("FileText")).await
        .map_err(OfficeError::Internal)?;

    let file_content = document_content_from(initial_pm.clone());
    let raw = serde_json::to_vec(&file_content)
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;

    let name = kb_file_name(title, "kbdoc");
    let file = state.files_client.create_file_with_content(
        user_id, Some(folder.id), &name,
        "application/vnd.kubuno.document+json",
        Bytes::from(gzip(&raw)?),
        Some(json!({ "module": "office", "subtype": "document" })),
        false,
    ).await.map_err(OfficeError::Internal)?;

    Ok((file.id, initial_pm))
}

// ── Spreadsheet content helpers ───────────────────────────────────────────────

/// The initial content of a brand-new spreadsheet. `header_row` = the instance
/// default: when set, the first row is frozen as a header.
pub fn empty_spreadsheet_content(sheet_id: Uuid, header_row: bool) -> Value {
    let mut sheet = empty_sheet_data();
    if header_row {
        sheet["frozen_rows"] = json!(1);
    }
    json!({
        "version": 1,
        "sheets": {
            sheet_id.to_string(): sheet
        }
    })
}

/// A blank sheet. Also the fallback for a sheet whose data is missing, which is
/// why it never carries the header-row default — that belongs to creation only.
pub fn empty_sheet_data() -> Value {
    json!({ "cells": {}, "col_widths": {}, "row_heights": {}, "frozen_rows": 0, "frozen_cols": 0 })
}

pub fn get_sheet_data(file_content: &Value, sheet_id: Uuid) -> Value {
    file_content
        .get("sheets")
        .and_then(|s| s.get(sheet_id.to_string()))
        .cloned()
        .unwrap_or_else(empty_sheet_data)
}

pub fn set_sheet_data(file_content: &mut Value, sheet_id: Uuid, data: Value) {
    if let Some(sheets) = file_content.get_mut("sheets").and_then(|s| s.as_object_mut()) {
        sheets.insert(sheet_id.to_string(), data);
    }
}

pub fn remove_sheet_data(file_content: &mut Value, sheet_id: Uuid) {
    if let Some(sheets) = file_content.get_mut("sheets").and_then(|s| s.as_object_mut()) {
        sheets.remove(&sheet_id.to_string());
    }
}

pub async fn create_spreadsheet_content_file(
    state:    &AppState,
    user_id:  Uuid,
    title:    &str,
    sheet_id: Uuid,
) -> Result<Uuid, OfficeError> {
    let folder = state.files_client
        .ensure_folder_path(user_id, "Office/Spreadsheets", true, Some("Table")).await
        .map_err(OfficeError::Internal)?;

    let file_content = empty_spreadsheet_content(sheet_id, state.instance().spreadsheet_header_row);
    let raw = serde_json::to_vec(&file_content)
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;

    let name = kb_file_name(title, "kbcal");
    let file = state.files_client.create_file_with_content(
        user_id, Some(folder.id), &name,
        "application/vnd.kubuno.spreadsheet+json",
        Bytes::from(gzip(&raw)?),
        Some(json!({ "module": "office", "subtype": "spreadsheet" })),
        false,
    ).await.map_err(OfficeError::Internal)?;

    Ok(file.id)
}

// ── Presentation content helpers ──────────────────────────────────────────────

pub fn empty_presentation_content(slide_id: Uuid) -> Value {
    json!({
        "version": 1,
        "slides": {
            slide_id.to_string(): empty_slide_data()
        }
    })
}

pub fn empty_slide_data() -> Value {
    json!({
        "elements":   [],
        "background": { "type": "color", "color": "#ffffff" },
        "notes":      "",
        "transition": { "type": "none", "duration": 0.3 }
    })
}

pub fn get_slide_data(file_content: &Value, slide_id: Uuid) -> Value {
    file_content
        .get("slides")
        .and_then(|s| s.get(slide_id.to_string()))
        .cloned()
        .unwrap_or_else(empty_slide_data)
}

pub fn set_slide_data(file_content: &mut Value, slide_id: Uuid, data: Value) {
    if let Some(slides) = file_content.get_mut("slides").and_then(|s| s.as_object_mut()) {
        slides.insert(slide_id.to_string(), data);
    }
}

pub fn remove_slide_data(file_content: &mut Value, slide_id: Uuid) {
    if let Some(slides) = file_content.get_mut("slides").and_then(|s| s.as_object_mut()) {
        slides.remove(&slide_id.to_string());
    }
}

pub async fn create_presentation_content_file(
    state:    &AppState,
    user_id:  Uuid,
    title:    &str,
    slide_id: Uuid,
) -> Result<Uuid, OfficeError> {
    let folder = state.files_client
        .ensure_folder_path(user_id, "Office/Presentations", true, Some("Presentation")).await
        .map_err(OfficeError::Internal)?;

    let file_content = empty_presentation_content(slide_id);
    let raw = serde_json::to_vec(&file_content)
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;

    let name = kb_file_name(title, "kbsld");
    let file = state.files_client.create_file_with_content(
        user_id, Some(folder.id), &name,
        "application/vnd.kubuno.presentation+json",
        Bytes::from(gzip(&raw)?),
        Some(json!({ "module": "office", "subtype": "presentation" })),
        false,
    ).await.map_err(OfficeError::Internal)?;

    Ok(file.id)
}

// ── Diagram content helpers ───────────────────────────────────────────────────

pub fn empty_diagram_content(page_id: Uuid) -> Value {
    json!({
        "version": 1,
        "pages": {
            page_id.to_string(): empty_page_data()
        }
    })
}

pub fn empty_page_data() -> Value {
    json!({ "shapes": [], "connectors": [] })
}

pub fn get_page_data(file_content: &Value, page_id: Uuid) -> Value {
    file_content
        .get("pages")
        .and_then(|p| p.get(page_id.to_string()))
        .cloned()
        .unwrap_or_else(empty_page_data)
}

pub fn set_page_data(file_content: &mut Value, page_id: Uuid, data: Value) {
    if let Some(pages) = file_content.get_mut("pages").and_then(|p| p.as_object_mut()) {
        pages.insert(page_id.to_string(), data);
    }
}

pub fn remove_page_data(file_content: &mut Value, page_id: Uuid) {
    if let Some(pages) = file_content.get_mut("pages").and_then(|p| p.as_object_mut()) {
        pages.remove(&page_id.to_string());
    }
}

pub async fn create_diagram_content_file(
    state:   &AppState,
    user_id: Uuid,
    title:   &str,
    page_id: Uuid,
) -> Result<Uuid, OfficeError> {
    let folder = state.files_client
        .ensure_folder_path(user_id, "Office/Diagrams", true, Some("Shapes")).await
        .map_err(OfficeError::Internal)?;

    let file_content = empty_diagram_content(page_id);
    let raw = serde_json::to_vec(&file_content)
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;

    let name = kb_file_name(title, "kbdia");
    let file = state.files_client.create_file_with_content(
        user_id, Some(folder.id), &name,
        "application/vnd.kubuno.diagram+json",
        Bytes::from(gzip(&raw)?),
        Some(json!({ "module": "office", "subtype": "diagram" })),
        false,
    ).await.map_err(OfficeError::Internal)?;

    Ok(file.id)
}

// ── Draft file helpers ────────────────────────────────────────────────────────

/// Creates a draft copy of a content file (invisible to the user).
/// The draft lives in Office/<Type>/.drafts/ (protected folder).
pub async fn create_draft_file(
    state:       &AppState,
    user_id:     Uuid,
    entity_type: &str,
    entity_id:   Uuid,
    content:     &Value,
) -> Result<Uuid, OfficeError> {
    let folder_path = format!("Office/{}/.drafts", capitalize(entity_type));
    let folder = state.files_client
        .ensure_folder_path(user_id, &folder_path, true, None).await
        .map_err(OfficeError::Internal)?;

    let raw = serde_json::to_vec(content)
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;

    let name = format!("{entity_id}.json");
    let file = state.files_client.create_file_with_content(
        user_id, Some(folder.id), &name,
        "application/json",
        Bytes::from(gzip(&raw)?),
        Some(json!({ "module": "office", "subtype": "draft", "entity_type": entity_type, "entity_id": entity_id })),
        true,
    ).await.map_err(OfficeError::Internal)?;

    Ok(file.id)
}

// ── Format Kubuno compressé (gzip) ────────────────────────────────────────────
// Les formats propres à Kubuno (.kb***) compressent leur JSON. En lecture on
// détecte la signature gzip (0x1f 0x8b) pour rester tolérant à un contenu clair.

use std::io::{Read as _, Write as _};

pub fn gzip(raw: &[u8]) -> Result<Vec<u8>, OfficeError> {
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(raw).map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;
    enc.finish().map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))
}

pub fn gunzip(raw: &[u8]) -> Result<Vec<u8>, OfficeError> {
    if raw.len() >= 2 && raw[0] == 0x1f && raw[1] == 0x8b {
        let mut dec = flate2::read::GzDecoder::new(raw);
        let mut out = Vec::new();
        dec.read_to_end(&mut out).map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;
        Ok(out)
    } else {
        Ok(raw.to_vec())
    }
}

// ── Script content helpers (.kbscr) ───────────────────────────────────────────
// Format Kubuno propre à Script : source TypeScript/JS compressée.
// La base ne garde que compiled_code (artefact transitoire) + métadonnée.

pub const SCRIPT_MIME: &str = "application/vnd.kubuno.script+json";

pub fn script_content_from(source: &str) -> Value {
    json!({ "version": 1, "source_code": source })
}

pub fn extract_script_source(content: &Value) -> String {
    content.get("source_code").and_then(|v| v.as_str()).unwrap_or("").to_string()
}

pub async fn create_script_file(
    state: &AppState, user_id: Uuid, title: &str, source: &str,
) -> Result<Uuid, OfficeError> {
    let folder = state.files_client
        .ensure_folder_path(user_id, "Office/Scripts", true, Some("FileCode")).await
        .map_err(OfficeError::Internal)?;
    let raw = serde_json::to_vec(&script_content_from(source))
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;
    let gz  = gzip(&raw)?;
    let name = kb_file_name(title, "kbscr");
    let file = state.files_client.create_file_with_content(
        user_id, Some(folder.id), &name,
        SCRIPT_MIME,
        Bytes::from(gz),
        Some(json!({ "module": "office", "subtype": "script" })),
        false,
    ).await.map_err(OfficeError::Internal)?;
    Ok(file.id)
}

pub async fn read_script_source(state: &AppState, user_id: Uuid, file_id: Uuid) -> Result<String, OfficeError> {
    let (_info, raw) = state.files_client.get_file_content(user_id, file_id).await
        .map_err(OfficeError::Internal)?;
    let json = gunzip(&raw)?;
    let content = serde_json::from_slice::<Value>(&json)
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!("contenu script illisible: {e}")))?;
    Ok(extract_script_source(&content))
}

pub async fn write_script_source(state: &AppState, user_id: Uuid, file_id: Uuid, source: &str) -> Result<(), OfficeError> {
    let raw = serde_json::to_vec(&script_content_from(source))
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;
    let gz = gzip(&raw)?;
    state.files_client.update_file_content(user_id, file_id, Bytes::from(gz)).await
        .map_err(OfficeError::Internal).map(|_| ())
}

// ── Maths content helpers (.kbmath) ───────────────────────────────────────────
// Format Kubuno propre à Maths : code LaTeX compressé. La base ne garde que la
// métadonnée (nom…).

pub const MATHS_MIME: &str = "application/vnd.kubuno.maths+json";

pub fn maths_content_from(latex: &str) -> Value {
    json!({ "version": 1, "latex": latex })
}

pub fn extract_maths_latex(content: &Value) -> String {
    content.get("latex").and_then(|v| v.as_str()).unwrap_or("").to_string()
}

pub async fn create_maths_file(
    state: &AppState, user_id: Uuid, title: &str, latex: &str,
) -> Result<Uuid, OfficeError> {
    let folder = state.files_client
        .ensure_folder_path(user_id, "Office/Maths", true, Some("Sigma")).await
        .map_err(OfficeError::Internal)?;
    let raw = serde_json::to_vec(&maths_content_from(latex))
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;
    let gz  = gzip(&raw)?;
    let name = kb_file_name(title, "kbmath");
    let file = state.files_client.create_file_with_content(
        user_id, Some(folder.id), &name,
        MATHS_MIME,
        Bytes::from(gz),
        Some(json!({ "module": "office", "subtype": "maths" })),
        false,
    ).await.map_err(OfficeError::Internal)?;
    Ok(file.id)
}

pub async fn read_maths_formula(state: &AppState, user_id: Uuid, file_id: Uuid) -> Result<String, OfficeError> {
    let (_info, raw) = state.files_client.get_file_content(user_id, file_id).await
        .map_err(OfficeError::Internal)?;
    let json = gunzip(&raw)?;
    let content = serde_json::from_slice::<Value>(&json)
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!("contenu maths illisible: {e}")))?;
    Ok(extract_maths_latex(&content))
}

pub async fn write_maths_formula(state: &AppState, user_id: Uuid, file_id: Uuid, latex: &str) -> Result<(), OfficeError> {
    let raw = serde_json::to_vec(&maths_content_from(latex))
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;
    let gz = gzip(&raw)?;
    state.files_client.update_file_content(user_id, file_id, Bytes::from(gz)).await
        .map_err(OfficeError::Internal).map(|_| ())
}

pub fn kb_file_name(title: &str, ext: &str) -> String {
    let base = std::path::Path::new(title).file_stem().and_then(|s| s.to_str()).unwrap_or(title);
    let base = if base.trim().is_empty() { "Sans titre" } else { base.trim() };
    format!("{base}.{ext}")
}

// ── Data : Dataset (.kbdst) ───────────────────────────────────────────────────
// Contenu = définition (raw_sql, query_steps), schéma calculé et cache de
// résultats (data_cache, potentiellement volumineux → gzip). La base ne garde
// que la métadonnée (nom, statut, row_count, datasource_id…).

pub const DATASET_MIME: &str = "application/vnd.kubuno.dataset+json";

pub fn dataset_content_from(raw_sql: &Value, query_steps: &Value, schema: &Value, data_cache: &Value) -> Value {
    json!({ "version": 1, "raw_sql": raw_sql, "query_steps": query_steps, "schema": schema, "data_cache": data_cache })
}

pub async fn create_dataset_file(state: &AppState, user_id: Uuid, name: &str, content: &Value) -> Result<Uuid, OfficeError> {
    let folder = state.files_client
        .ensure_folder_path(user_id, "Office/Data", true, Some("Database")).await
        .map_err(OfficeError::Internal)?;
    let raw = serde_json::to_vec(content).map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;
    let gz  = gzip(&raw)?;
    let file = state.files_client.create_file_with_content(
        user_id, Some(folder.id), &kb_file_name(name, "kbdst"), DATASET_MIME, Bytes::from(gz),
        Some(json!({ "module": "office", "subtype": "dataset" })), false,
    ).await.map_err(OfficeError::Internal)?;
    Ok(file.id)
}

pub async fn read_kb(state: &AppState, user_id: Uuid, file_id: Uuid) -> Result<Value, OfficeError> {
    let (_info, raw) = state.files_client.get_file_content(user_id, file_id).await
        .map_err(OfficeError::Internal)?;
    let json = gunzip(&raw)?;
    serde_json::from_slice::<Value>(&json)
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!("contenu .kb illisible: {e}")))
}

pub async fn write_kb_gzip(state: &AppState, user_id: Uuid, file_id: Uuid, content: &Value) -> Result<(), OfficeError> {
    let raw = serde_json::to_vec(content).map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;
    let gz  = gzip(&raw)?;
    state.files_client.update_file_content(user_id, file_id, Bytes::from(gz)).await
        .map_err(OfficeError::Internal).map(|_| ())
}

// ── Whiteboard : document Yjs (.kbwbd) ────────────────────────────────────────
// Le snapshot Yjs consolidé (document durable, potentiellement volumineux) vit
// dans un fichier binaire gzippé. Le journal d'updates incrémentaux reste en
// base comme tampon temps réel (consolidé périodiquement dans le fichier).

pub const WHITEBOARD_MIME: &str = "application/vnd.kubuno.whiteboard";

pub async fn create_whiteboard_file(state: &AppState, user_id: Uuid, title: &str) -> Result<Uuid, OfficeError> {
    let folder = state.files_client
        .ensure_folder_path(user_id, "Office/Whiteboards", true, Some("PenLine")).await
        .map_err(OfficeError::Internal)?;
    let gz = gzip(&[])?; // snapshot vide
    let file = state.files_client.create_file_with_content(
        user_id, Some(folder.id), &kb_file_name(title, "kbwbd"), WHITEBOARD_MIME, Bytes::from(gz),
        Some(json!({ "module": "office", "subtype": "whiteboard" })), false,
    ).await.map_err(OfficeError::Internal)?;
    Ok(file.id)
}

pub async fn read_whiteboard_snapshot(state: &AppState, user_id: Uuid, file_id: Uuid) -> Result<Vec<u8>, OfficeError> {
    let (_info, raw) = state.files_client.get_file_content(user_id, file_id).await
        .map_err(OfficeError::Internal)?;
    gunzip(&raw)
}

pub async fn write_whiteboard_snapshot(state: &AppState, user_id: Uuid, file_id: Uuid, data: &[u8]) -> Result<(), OfficeError> {
    let gz = gzip(data)?;
    state.files_client.update_file_content(user_id, file_id, Bytes::from(gz)).await
        .map_err(OfficeError::Internal).map(|_| ())
}

// ── Data : Report (.kbdrp) ────────────────────────────────────────────────────
// Contenu = mise en page complète du rapport : pages + widgets.

pub const REPORT_MIME: &str = "application/vnd.kubuno.report+json";

pub async fn create_report_file(state: &AppState, user_id: Uuid, title: &str, content: &Value) -> Result<Uuid, OfficeError> {
    let folder = state.files_client
        .ensure_folder_path(user_id, "Office/Data", true, Some("Database")).await
        .map_err(OfficeError::Internal)?;
    let raw = serde_json::to_vec(content).map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;
    let gz  = gzip(&raw)?;
    let file = state.files_client.create_file_with_content(
        user_id, Some(folder.id), &kb_file_name(title, "kbdrp"), REPORT_MIME, Bytes::from(gz),
        Some(json!({ "module": "office", "subtype": "report" })), false,
    ).await.map_err(OfficeError::Internal)?;
    Ok(file.id)
}

// ── Utils ─────────────────────────────────────────────────────────────────────

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None    => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str() + "s",
    }
}
