use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use bytes::Bytes;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    converters::{odt::export_odt, types::PmNode},
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    models::document::*,
    services::content_files as cf,
    state::AppState,
};

fn count_words(text: &str) -> i32 {
    text.split_whitespace().count() as i32
}

fn extract_text(content: &Value) -> String {
    match content {
        Value::Object(obj) => {
            let mut parts = Vec::new();
            // `content` peut être un tableau de nœuds (doc/paragraph) OU un objet
            // (cas d'une page multi-page : content = { type:"doc", content:[...] }).
            match obj.get("content") {
                Some(Value::Array(children)) => {
                    for child in children {
                        let t = extract_text(child);
                        if !t.is_empty() { parts.push(t); }
                    }
                }
                Some(v @ Value::Object(_)) => {
                    let t = extract_text(v);
                    if !t.is_empty() { parts.push(t); }
                }
                _ => {}
            }
            // Enveloppe multi-page : { _type:"multi-page", pages:[{content:{type:doc,...}}] }
            if let Some(Value::Array(pages)) = obj.get("pages") {
                for page in pages {
                    let t = extract_text(page);
                    if !t.is_empty() { parts.push(t); }
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

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Query(q): Query<ListDocumentsQuery>,
) -> Result<Json<Value>> {
    let limit  = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);
    let trashed = q.trashed.unwrap_or(false);

    let rows: Vec<DocumentSummary> = if let Some(ref search) = q.search {
        sqlx::query_as::<_, DocumentSummary>(
            r#"SELECT id, owner_id, title, icon, word_count, is_starred, is_trashed,
                      parent_id, created_at, updated_at
               FROM documents
               WHERE owner_id = $1 AND is_trashed = $2
                 AND title ILIKE $3
               ORDER BY updated_at DESC LIMIT $4 OFFSET $5"#,
        )
        .bind(user.id).bind(trashed).bind(format!("%{search}%"))
        .bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    } else if q.starred.unwrap_or(false) {
        sqlx::query_as::<_, DocumentSummary>(
            r#"SELECT id, owner_id, title, icon, word_count, is_starred, is_trashed,
                      parent_id, created_at, updated_at
               FROM documents
               WHERE owner_id = $1 AND is_starred = TRUE AND is_trashed = FALSE
               ORDER BY updated_at DESC LIMIT $2 OFFSET $3"#,
        )
        .bind(user.id).bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    } else if q.shared.unwrap_or(false) {
        // Documents partagés AVEC moi (où je suis collaborateur, pas propriétaire).
        sqlx::query_as::<_, DocumentSummary>(
            r#"SELECT d.id, d.owner_id, d.title, d.icon, d.word_count, d.is_starred, d.is_trashed,
                      d.parent_id, d.created_at, d.updated_at
               FROM documents d
               JOIN document_collaborators c ON c.document_id = d.id
               WHERE c.user_id = $1 AND d.is_trashed = FALSE
               ORDER BY d.updated_at DESC LIMIT $2 OFFSET $3"#,
        )
        .bind(user.id).bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    } else if q.recent.unwrap_or(false) {
        sqlx::query_as::<_, DocumentSummary>(
            r#"SELECT id, owner_id, title, icon, word_count, is_starred, is_trashed,
                      parent_id, created_at, updated_at
               FROM documents
               WHERE owner_id = $1 AND is_trashed = FALSE
               ORDER BY updated_at DESC LIMIT $2 OFFSET $3"#,
        )
        .bind(user.id).bind(limit.min(20)).bind(offset)
        .fetch_all(&state.db).await?
    } else {
        sqlx::query_as::<_, DocumentSummary>(
            r#"SELECT id, owner_id, title, icon, word_count, is_starred, is_trashed,
                      parent_id, created_at, updated_at
               FROM documents
               WHERE owner_id = $1 AND is_trashed = $2
                 AND ($3::uuid IS NULL AND parent_id IS NULL OR parent_id = $3)
               ORDER BY position ASC, updated_at DESC
               LIMIT $4 OFFSET $5"#,
        )
        .bind(user.id).bind(trashed).bind(q.parent_id).bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    };

    Ok(Json(json!({ "documents": rows, "total": rows.len() })))
}

fn unique_doc_title(desired: &str, existing: &[String]) -> String {
    if !existing.iter().any(|t| t == desired) {
        return desired.to_string();
    }
    let path = std::path::Path::new(desired);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(desired);
    let ext  = path.extension().and_then(|s| s.to_str());
    for i in 2usize.. {
        let candidate = match ext {
            Some(e) => format!("{stem} ({i}).{e}"),
            None    => format!("{stem} ({i})"),
        };
        if !existing.iter().any(|t| t == &candidate) {
            return candidate;
        }
    }
    unreachable!()
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<CreateDocumentDto>,
) -> Result<Json<Value>> {
    let base_title = dto.title.unwrap_or_else(|| "Nouveau document.odt".to_string());

    let existing_titles: Vec<String> = sqlx::query_scalar(
        "SELECT title FROM documents WHERE owner_id = $1 AND parent_id IS NOT DISTINCT FROM $2 AND is_trashed = FALSE"
    )
    .bind(user.id).bind(dto.parent_id)
    .fetch_all(&state.db).await?;
    let title = unique_doc_title(&base_title, &existing_titles);

    let max_pos: Option<f64> = sqlx::query_scalar(
        "SELECT MAX(position) FROM documents WHERE owner_id = $1 AND parent_id IS NOT DISTINCT FROM $2",
    )
    .bind(user.id).bind(dto.parent_id)
    .fetch_one(&state.db).await?;
    let position = max_pos.map(|p| p + 1.0).unwrap_or(0.0);

    let initial_pm = if let Some(tmpl_id) = dto.template_id {
        let r: Option<Option<Value>> = sqlx::query_scalar(
            "SELECT content_json FROM document_templates WHERE id = $1",
        )
        .bind(tmpl_id).fetch_optional(&state.db).await?;
        r.flatten().unwrap_or_else(|| json!({"type":"doc","content":[]}))
    } else {
        json!({"type":"doc","content":[]})
    };

    let (file_id, pm_json) = cf::create_document_content_file(&state, user.id, &title, initial_pm).await?;

    let content_text = extract_text(&pm_json);
    let word_count   = count_words(&content_text);

    let doc = sqlx::query_as::<_, Document>(
        r#"INSERT INTO documents (owner_id, title, icon, parent_id, position, word_count, file_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           RETURNING id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                     trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id,
                     created_at, updated_at"#,
    )
    .bind(user.id).bind(&title).bind(dto.icon)
    .bind(dto.parent_id).bind(position).bind(word_count).bind(file_id)
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "document": doc, "content_json": pm_json })))
}

pub async fn get(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let doc = sqlx::query_as::<_, Document>(
        r#"SELECT id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                  trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id,
                  created_at, updated_at
           FROM documents
           WHERE id = $1 AND (owner_id = $2 OR EXISTS (
               SELECT 1 FROM document_collaborators
               WHERE document_id = $1 AND user_id = $2
           ))"#,
    )
    .bind(id).bind(user.id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Document {id}")))?;

    let content_file_id = doc.draft_file_id.or(doc.file_id)
        .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Document {id} has no content file")))?;

    let (fname, file_content) = cf::read_content_named(&state, doc.owner_id, content_file_id).await?;
    let pm_json = cf::extract_document_pm(&file_content);

    // Titre = nom du fichier visible (.kbdoc) sans extension. Quand le contenu est lu
    // directement depuis le fichier principal (cas courant, sans brouillon), on tient
    // le nom à jour → suit un renommage fait depuis l'explorateur de fichiers.
    let mut doc = doc;
    if Some(content_file_id) == doc.file_id {
        let stem = cf::strip_ext(&fname);
        if !stem.is_empty() && stem != doc.title {
            sqlx::query("UPDATE documents SET title = $2 WHERE id = $1")
                .bind(id).bind(&stem).execute(&state.db).await?;
            doc.title = stem;
        }
    }

    Ok(Json(json!({ "document": doc, "content_json": pm_json })))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateDocumentDto>,
) -> Result<Json<Value>> {
    let doc = sqlx::query_as::<_, Document>(
        r#"SELECT id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                  trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id,
                  created_at, updated_at
           FROM documents WHERE id = $1"#,
    )
    .bind(id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Document {id}")))?;

    if doc.owner_id != user.id {
        let perm: Option<String> = sqlx::query_scalar(
            "SELECT permission FROM document_collaborators WHERE document_id = $1 AND user_id = $2",
        )
        .bind(id).bind(user.id).fetch_optional(&state.db).await?;
        if perm.as_deref() != Some("edit") {
            return Err(OfficeError::Forbidden);
        }
    }

    // Write content to Files if provided
    let (word_count, pm_json) = if let Some(ref new_pm) = dto.content_json {
        let content_text = extract_text(new_pm);
        let wc = count_words(&content_text);

        let content_file_id = draft_or_main_file_id(&doc, &state, user.id, id, "document").await?;
        // Écrit le brouillon ET reflète dans le .kbdoc visible.
        cf::write_content_mirrored(&state, doc.owner_id, content_file_id, doc.file_id,
            &cf::document_content_from(new_pm.clone())).await?;

        // Broadcast to collaborators
        state.hub.publish(id, crate::state::CollabMessage::ContentUpdated {
            user_id: user.id,
            content: new_pm.clone(),
            title:   dto.title.as_deref().unwrap_or(&doc.title).to_string(),
        }).await;

        (Some(wc), Some(new_pm.clone()))
    } else {
        (None, None)
    };

    let updated = sqlx::query_as::<_, Document>(
        r#"UPDATE documents SET
               title          = COALESCE($2, title),
               icon           = COALESCE($3, icon),
               cover_url      = COALESCE($4, cover_url),
               word_count     = COALESCE($5, word_count),
               is_starred     = COALESCE($6, is_starred),
               parent_id      = COALESCE($7, parent_id),
               last_editor_id = $8
           WHERE id = $1
           RETURNING id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                     trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id,
                     created_at, updated_at"#,
    )
    .bind(id)
    .bind(dto.title.clone())
    .bind(dto.icon)
    .bind(dto.cover_url)
    .bind(word_count)
    .bind(dto.is_starred)
    .bind(dto.parent_id)
    .bind(user.id)
    .fetch_one(&state.db).await?;

    // Titre modifié → on renomme le fichier visible (.kbdoc) pour qu'il corresponde
    // (titre = nom du fichier). Best-effort : un conflit/erreur ne casse pas l'édition.
    if let (Some(new_title), Some(file_id)) = (dto.title.as_ref(), updated.file_id) {
        if !new_title.trim().is_empty() {
            if let Err(e) = cf::rename_content_file(&state, updated.owner_id, file_id, new_title, "kbdoc").await {
                tracing::warn!(error = %e, doc = %id, "rename .kbdoc (titre) échoué");
            }
        }
    }

    // Return current pm_json (provided or read from file)
    let out_pm = match pm_json {
        Some(pm) => pm,
        None => {
            let file_id = updated.draft_file_id.or(updated.file_id);
            match file_id {
                Some(fid) => {
                    let fc = cf::read_content(&state, updated.owner_id, fid).await?;
                    cf::extract_document_pm(&fc)
                }
                None => json!({"type":"doc","content":[]}),
            }
        }
    };

    Ok(Json(json!({ "document": updated, "content_json": out_pm })))
}

pub async fn trash(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "UPDATE documents SET is_trashed = TRUE, trashed_at = NOW()
         WHERE id = $1 AND owner_id = $2 AND is_trashed = FALSE",
    )
    .bind(id).bind(user.id).execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound(format!("Document {id}"))); }
    Ok(Json(json!({ "ok": true })))
}

pub async fn restore(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "UPDATE documents SET is_trashed = FALSE, trashed_at = NULL
         WHERE id = $1 AND owner_id = $2 AND is_trashed = TRUE",
    )
    .bind(id).bind(user.id).execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound(format!("Document {id}"))); }
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "DELETE FROM documents WHERE id = $1 AND owner_id = $2 AND is_trashed = TRUE",
    )
    .bind(id).bind(user.id).execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound(format!("Document {id}"))); }
    Ok(Json(json!({ "ok": true })))
}

pub async fn list_versions(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM documents WHERE id = $1 AND owner_id = $2)",
    )
    .bind(id).bind(user.id).fetch_one(&state.db).await?;
    if !exists { return Err(OfficeError::NotFound(format!("Document {id}"))); }

    let versions = sqlx::query_as::<_, DocumentVersion>(
        r#"SELECT id, document_id, author_id, content_json, word_count, label, created_at
           FROM document_versions WHERE document_id = $1
           ORDER BY created_at DESC LIMIT 50"#,
    )
    .bind(id).fetch_all(&state.db).await?;
    Ok(Json(json!({ "versions": versions })))
}

pub async fn create_version(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<CreateVersionDto>,
) -> Result<Json<Value>> {
    #[derive(sqlx::FromRow)]
    struct DocSnap { owner_id: Uuid, word_count: i32, file_id: Option<Uuid>, draft_file_id: Option<Uuid> }

    let doc = sqlx::query_as::<_, DocSnap>(
        "SELECT owner_id, word_count, file_id, draft_file_id FROM documents WHERE id = $1",
    )
    .bind(id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Document {id}")))?;

    if doc.owner_id != user.id { return Err(OfficeError::Forbidden); }

    let content_file_id = doc.draft_file_id.or(doc.file_id)
        .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Document {id} has no content file")))?;

    let file_content = cf::read_content(&state, doc.owner_id, content_file_id).await?;
    let pm_json = cf::extract_document_pm(&file_content);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_versions WHERE document_id = $1",
    )
    .bind(id).fetch_one(&state.db).await?;

    if count >= state.settings.office.max_versions as i64 {
        sqlx::query(
            "DELETE FROM document_versions WHERE id = (
                SELECT id FROM document_versions WHERE document_id = $1 ORDER BY created_at ASC LIMIT 1
            )",
        )
        .bind(id).execute(&state.db).await?;
    }

    let version = sqlx::query_as::<_, DocumentVersion>(
        r#"INSERT INTO document_versions (document_id, author_id, content_json, word_count, label)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, document_id, author_id, content_json, word_count, label, created_at"#,
    )
    .bind(id).bind(user.id).bind(pm_json).bind(doc.word_count).bind(dto.label)
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "version": version })))
}

pub async fn restore_version(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((doc_id, ver_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    let version = sqlx::query_as::<_, DocumentVersion>(
        r#"SELECT id, document_id, author_id, content_json, word_count, label, created_at
           FROM document_versions WHERE id = $1 AND document_id = $2"#,
    )
    .bind(ver_id).bind(doc_id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Version {ver_id}")))?;

    let doc = sqlx::query_as::<_, Document>(
        r#"SELECT id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                  trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id,
                  created_at, updated_at
           FROM documents WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(doc_id).bind(user.id).fetch_optional(&state.db).await?
    .ok_or(OfficeError::Forbidden)?;

    let content_file_id = doc.draft_file_id.or(doc.file_id)
        .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Document has no content file")))?;

    cf::write_content(&state, doc.owner_id, content_file_id,
        &cf::document_content_from(version.content_json.clone())).await?;

    let updated = sqlx::query_as::<_, Document>(
        r#"UPDATE documents SET word_count = $2, last_editor_id = $3
           WHERE id = $1
           RETURNING id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                     trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id,
                     created_at, updated_at"#,
    )
    .bind(doc_id).bind(version.word_count).bind(user.id)
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "document": updated, "content_json": version.content_json })))
}

pub async fn duplicate(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let src = sqlx::query_as::<_, Document>(
        r#"SELECT id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                  trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id,
                  created_at, updated_at
           FROM documents WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id).bind(user.id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Document {id}")))?;

    let src_file_id = src.file_id
        .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Source document has no content file")))?;
    let file_content = cf::read_content(&state, src.owner_id, src_file_id).await?;
    let pm_json = cf::extract_document_pm(&file_content);

    let new_title = format!("{} (copie)", src.title);
    let (new_file_id, _) = cf::create_document_content_file(&state, user.id, &new_title, pm_json.clone()).await?;

    let doc = sqlx::query_as::<_, Document>(
        r#"INSERT INTO documents (owner_id, title, icon, word_count, parent_id, file_id)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                     trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id,
                     created_at, updated_at"#,
    )
    .bind(user.id).bind(new_title).bind(src.icon).bind(src.word_count)
    .bind(src.parent_id).bind(new_file_id)
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "document": doc, "content_json": pm_json })))
}

// ── Open by file_id ───────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct OpenByFileDto {
    pub file_id: Uuid,
}

pub async fn open_by_file(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<OpenByFileDto>,
) -> Result<Json<Value>> {
    // 1. Cherche un document DÉJÀ lié à ce fichier — soit comme fichier de contenu
    //    (file_id), soit comme fichier SOURCE importé (source_file_id). Sans la
    //    seconde condition, rouvrir un .docx/.odt importé recréait une copie à
    //    chaque fois (file_id pointe vers le contenu généré, jamais vers la source).
    if let Some(doc) = sqlx::query_as::<_, Document>(
        r#"SELECT id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                  trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id,
                  created_at, updated_at
           FROM documents
           WHERE (file_id = $1 OR source_file_id = $1) AND owner_id = $2 AND is_trashed = FALSE
           ORDER BY created_at ASC
           LIMIT 1"#,
    )
    .bind(dto.file_id).bind(user.id).fetch_optional(&state.db).await? {
        let content_file_id = doc.draft_file_id.or(doc.file_id).unwrap();
        let fc = cf::read_content(&state, doc.owner_id, content_file_id).await?;
        let pm_json = cf::extract_document_pm(&fc);
        return Ok(Json(json!({ "document": doc, "content_json": pm_json })));
    }

    // 2. Fetch the file from Files and import it
    let (file_info, content_bytes) = state.files_client
        .get_file_content(user.id, dto.file_id).await
        .map_err(anyhow::Error::from)?;

    let is_docx = file_info.mime_type.contains("wordprocessingml") || file_info.name.ends_with(".docx");
    let is_odt  = file_info.mime_type.contains("opendocument.text") || file_info.name.ends_with(".odt");

    if !is_docx && !is_odt {
        return Err(OfficeError::Validation(
            "Format non supporté (attendu : docx ou odt)".into(),
        ));
    }

    let pm_doc = if is_docx {
        crate::converters::docx::import_docx(&content_bytes)?
    } else {
        crate::converters::odt::import_odt(&content_bytes)?
    };
    let pm_json = serde_json::to_value(&pm_doc)
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;

    let base  = file_info.name.trim_end_matches(".docx").trim_end_matches(".odt").trim().to_string();
    let title = if base.is_empty() { "Document importé".to_string() } else { base };

    let word_count = count_words(&extract_text(&pm_json));

    // 3. Create content file and document record
    let (file_id, _) = cf::create_document_content_file(&state, user.id, &title, pm_json.clone()).await?;

    let doc = sqlx::query_as::<_, Document>(
        r#"INSERT INTO documents (owner_id, title, word_count, file_id, source_file_id)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                     trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id,
                     created_at, updated_at"#,
    )
    .bind(user.id).bind(&title).bind(word_count).bind(file_id).bind(dto.file_id)
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "document": doc, "content_json": pm_json })))
}

// ── Editing session (draft + presence) ───────────────────────────────────────

/// POST /:id/editing/join
/// Creates or refreshes an editing session, returning the current draft content.
/// If no draft exists yet, creates one from the main content file.
pub async fn join_editing(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let doc = sqlx::query_as::<_, Document>(
        r#"SELECT id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                  trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id,
                  created_at, updated_at
           FROM documents WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id).bind(user.id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Document {id}")))?;

    // Ensure a draft file exists
    let draft_file_id = ensure_draft(&state, &doc, user.id, id, "document").await?;

    // Register editing session
    sqlx::query(
        r#"INSERT INTO editing_sessions (entity_type, entity_id, user_id, display_name)
           VALUES ('document', $1, $2, $3)
           ON CONFLICT (entity_type, entity_id, user_id) DO UPDATE
               SET last_ping_at = NOW(), display_name = $3"#,
    )
    .bind(id).bind(user.id)
    .bind(format!("{}", user.id))
    .execute(&state.db).await?;

    // Broadcast presence
    state.hub.publish(id, crate::state::CollabMessage::PresenceChange {
        user_id: user.id,
        action:  "join".to_string(),
    }).await;

    let draft_content = cf::read_content(&state, doc.owner_id, draft_file_id).await?;
    let pm_json = cf::extract_document_pm(&draft_content);

    // List all active editors
    let editors = get_editing_sessions(&state, "document", id).await?;

    Ok(Json(json!({ "content_json": pm_json, "editors": editors })))
}

/// POST /:id/editing/save
/// Promotes the draft to the main content file.
pub async fn save_editing(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let doc = sqlx::query_as::<_, Document>(
        r#"SELECT id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                  trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id,
                  created_at, updated_at
           FROM documents WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id).bind(user.id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Document {id}")))?;

    let draft_file_id = match doc.draft_file_id {
        Some(d) => d,
        None    => return Ok(Json(json!({ "ok": true, "note": "no draft to save" }))),
    };
    let main_file_id = doc.file_id
        .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Document has no main content file")))?;

    let draft_content = cf::read_content(&state, doc.owner_id, draft_file_id).await?;
    cf::write_content(&state, doc.owner_id, main_file_id, &draft_content).await?;

    let pm_json = cf::extract_document_pm(&draft_content);
    let word_count = count_words(&extract_text(&pm_json));

    sqlx::query("UPDATE documents SET word_count = $1, last_editor_id = $2 WHERE id = $3")
        .bind(word_count).bind(user.id).bind(id)
        .execute(&state.db).await?;

    Ok(Json(json!({ "ok": true })))
}

/// DELETE /:id/editing/leave
/// Saves draft and removes the editing session.
pub async fn leave_editing(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let _ = save_editing_internal(&state, user.id, id).await;

    sqlx::query(
        "DELETE FROM editing_sessions WHERE entity_type = 'document' AND entity_id = $1 AND user_id = $2",
    )
    .bind(id).bind(user.id).execute(&state.db).await?;

    state.hub.publish(id, crate::state::CollabMessage::PresenceChange {
        user_id: user.id,
        action:  "leave".to_string(),
    }).await;

    // If no more editors, remove draft
    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM editing_sessions WHERE entity_type = 'document' AND entity_id = $1",
    )
    .bind(id).fetch_one(&state.db).await?;

    if remaining == 0 {
        cleanup_draft_document(&state, user.id, id).await;
    }

    Ok(Json(json!({ "ok": true })))
}

/// POST /:id/editing/ping  — keepalive
pub async fn ping_editing(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query(
        "UPDATE editing_sessions SET last_ping_at = NOW()
         WHERE entity_type = 'document' AND entity_id = $1 AND user_id = $2",
    )
    .bind(id).bind(user.id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── Internal helpers ─────────────────────────────────────────────────────────

async fn draft_or_main_file_id(
    doc:     &Document,
    state:   &AppState,
    user_id: Uuid,
    doc_id:  Uuid,
    entity_type: &str,
) -> Result<Uuid> {
    if let Some(d) = doc.draft_file_id { return Ok(d); }
    ensure_draft(state, doc, user_id, doc_id, entity_type).await
}

async fn ensure_draft(
    state:       &AppState,
    doc:         &Document,
    user_id:     Uuid,
    doc_id:      Uuid,
    entity_type: &str,
) -> Result<Uuid> {
    let main_file_id = doc.file_id
        .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Document has no content file")))?;
    let main_content = cf::read_content(state, doc.owner_id, main_file_id).await?;
    let draft_id = cf::create_draft_file(state, user_id, entity_type, doc_id, &main_content).await?;
    sqlx::query("UPDATE documents SET draft_file_id = $1 WHERE id = $2")
        .bind(draft_id).bind(doc_id).execute(&state.db).await?;
    Ok(draft_id)
}

async fn save_editing_internal(state: &AppState, user_id: Uuid, doc_id: Uuid) -> std::result::Result<(), ()> {
    let doc = sqlx::query_as::<_, Document>(
        r#"SELECT id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,
                  trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id,
                  created_at, updated_at
           FROM documents WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(doc_id).bind(user_id).fetch_optional(&state.db).await.ok().flatten().ok_or(())?;

    let draft_id = doc.draft_file_id.ok_or(())?;
    let main_id  = doc.file_id.ok_or(())?;
    let content  = cf::read_content(state, doc.owner_id, draft_id).await.map_err(|_| ())?;
    cf::write_content(state, doc.owner_id, main_id, &content).await.map_err(|_| ())?;
    let pm_json  = cf::extract_document_pm(&content);
    let word_count = count_words(&extract_text(&pm_json));
    let _ = sqlx::query("UPDATE documents SET word_count = $1, last_editor_id = $2 WHERE id = $3")
        .bind(word_count).bind(user_id).bind(doc_id).execute(&state.db).await;
    Ok(())
}

async fn cleanup_draft_document(state: &AppState, user_id: Uuid, doc_id: Uuid) {
    if let Ok(Some(draft_id)) = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT draft_file_id FROM documents WHERE id = $1 AND owner_id = $2",
    )
    .bind(doc_id).bind(user_id).fetch_optional(&state.db).await {
        if let Some(fid) = draft_id {
            let _ = state.files_client.delete_file(user_id, fid).await;
            let _ = sqlx::query("UPDATE documents SET draft_file_id = NULL WHERE id = $1")
                .bind(doc_id).execute(&state.db).await;
        }
    }
}

async fn get_editing_sessions(
    state:       &AppState,
    entity_type: &str,
    entity_id:   Uuid,
) -> Result<Vec<serde_json::Value>> {
    #[derive(sqlx::FromRow)]
    struct Session { user_id: Uuid, display_name: Option<String>, color: String, last_ping_at: chrono::DateTime<chrono::Utc> }

    let rows = sqlx::query_as::<_, Session>(
        "SELECT user_id, display_name, color, last_ping_at FROM editing_sessions
         WHERE entity_type = $1 AND entity_id = $2
           AND last_ping_at > NOW() - INTERVAL '2 minutes'",
    )
    .bind(entity_type).bind(entity_id)
    .fetch_all(&state.db).await?;

    Ok(rows.iter().map(|r| json!({
        "user_id":      r.user_id,
        "display_name": r.display_name,
        "color":        r.color,
        "last_ping_at": r.last_ping_at,
    })).collect())
}

// ── Export helpers (unchanged — read from file) ───────────────────────────────

pub fn export_as_odt_bytes(pm_json: &Value, title: &str) -> Result<Bytes> {
    let pm_doc: PmNode = serde_json::from_value(pm_json.clone())
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;
    export_odt(&pm_doc, title)
        .map(Bytes::from)
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_text_from_prosemirror_doc() {
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "paragraph", "content": [{ "type": "text", "text": "Hello" }] },
                { "type": "paragraph", "content": [{ "type": "text", "text": "world" }] },
            ]
        });
        let text = extract_text(&doc);
        assert!(text.contains("Hello"));
        assert!(text.contains("world"));
    }

    #[test]
    fn count_words_basic() {
        assert_eq!(count_words("hello world"), 2);
        assert_eq!(count_words("one two three four"), 4);
        assert_eq!(count_words(""), 0);
    }

    #[test]
    fn unique_doc_title_conflict_increments() {
        let existing = vec!["rapport.odt".to_string()];
        assert_eq!(unique_doc_title("rapport.odt", &existing), "rapport (2).odt");
    }
}
