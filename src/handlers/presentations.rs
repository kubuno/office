use axum::{
    body::{Body, Bytes},
    extract::{Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;
use axum::Extension;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    models::presentation::*,
    services::content_files as cf,
    state::AppState,
};

// ── Presentations CRUD ────────────────────────────────────────────────────────

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Query(q): Query<ListPresentationsQuery>,
) -> Result<Json<Value>> {
    let limit   = q.limit.unwrap_or(50).min(200);
    let offset  = q.offset.unwrap_or(0);
    let trashed = q.trashed.unwrap_or(false);

    let rows: Vec<Presentation> = if let Some(ref search) = q.search {
        sqlx::query_as::<_, Presentation>(
            r#"SELECT id, owner_id, title, file_id, draft_file_id, theme, aspect_ratio, slide_width, slide_height,
                      slide_count, is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at
               FROM presentations WHERE owner_id = $1 AND is_trashed = $2 AND title ILIKE $3
               ORDER BY updated_at DESC LIMIT $4 OFFSET $5"#,
        ).bind(user.id).bind(trashed).bind(format!("%{search}%")).bind(limit).bind(offset)
         .fetch_all(&state.db).await?
    } else if q.starred.unwrap_or(false) {
        sqlx::query_as::<_, Presentation>(
            r#"SELECT id, owner_id, title, file_id, draft_file_id, theme, aspect_ratio, slide_width, slide_height,
                      slide_count, is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at
               FROM presentations WHERE owner_id = $1 AND is_starred = TRUE AND is_trashed = FALSE
               ORDER BY updated_at DESC LIMIT $2 OFFSET $3"#,
        ).bind(user.id).bind(limit).bind(offset).fetch_all(&state.db).await?
    } else if q.recent.unwrap_or(false) {
        sqlx::query_as::<_, Presentation>(
            r#"SELECT id, owner_id, title, file_id, draft_file_id, theme, aspect_ratio, slide_width, slide_height,
                      slide_count, is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at
               FROM presentations WHERE owner_id = $1 AND is_trashed = FALSE
               ORDER BY updated_at DESC LIMIT $2 OFFSET $3"#,
        ).bind(user.id).bind(limit.min(20)).bind(offset).fetch_all(&state.db).await?
    } else if q.shared.unwrap_or(false) {
        sqlx::query_as::<_, Presentation>(
            r#"SELECT p.id, p.owner_id, p.title, p.file_id, p.draft_file_id, p.theme, p.aspect_ratio, p.slide_width, p.slide_height,
                      p.slide_count, p.is_starred, p.is_trashed, p.trashed_at, p.last_edited_by, p.created_at, p.updated_at
               FROM presentations p
               JOIN presentation_collaborators c ON c.presentation_id = p.id
               WHERE c.user_id = $1 AND p.is_trashed = FALSE
               ORDER BY p.updated_at DESC LIMIT $2 OFFSET $3"#,
        ).bind(user.id).bind(limit).bind(offset).fetch_all(&state.db).await?
    } else {
        sqlx::query_as::<_, Presentation>(
            r#"SELECT id, owner_id, title, file_id, draft_file_id, theme, aspect_ratio, slide_width, slide_height,
                      slide_count, is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at
               FROM presentations WHERE owner_id = $1 AND is_trashed = $2
               ORDER BY updated_at DESC LIMIT $3 OFFSET $4"#,
        ).bind(user.id).bind(trashed).bind(limit).bind(offset).fetch_all(&state.db).await?
    };

    Ok(Json(json!({ "presentations": rows, "total": rows.len() })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    body: Option<Json<CreatePresentationDto>>,
) -> Result<Json<Value>> {
    let title = body.and_then(|Json(dto)| dto.title)
        .unwrap_or_else(|| "Présentation sans titre".to_string());

    let mut tx = state.db.begin().await?;

    let pres = sqlx::query_as::<_, Presentation>(
        r#"INSERT INTO presentations (owner_id, title)
           VALUES ($1, $2)
           RETURNING id, owner_id, title, file_id, draft_file_id, theme, aspect_ratio, slide_width, slide_height,
                      slide_count, is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at"#,
    )
    .bind(user.id).bind(&title).fetch_one(&mut *tx).await?;

    let slide: SlideSummary = sqlx::query_as::<_, SlideSummary>(
        r#"INSERT INTO slides (presentation_id, position) VALUES ($1, 0)
           RETURNING id, presentation_id, position, is_hidden, thumbnail_path, thumbnail_dirty, created_at, updated_at"#,
    )
    .bind(pres.id).fetch_one(&mut *tx).await?;

    tx.commit().await?;

    let file_id = cf::create_presentation_content_file(&state, user.id, &title, slide.id).await?;
    sqlx::query("UPDATE presentations SET file_id = $1 WHERE id = $2")
        .bind(file_id).bind(pres.id).execute(&state.db).await?;

    let pres = sqlx::query_as::<_, Presentation>(
        "SELECT id, owner_id, title, file_id, draft_file_id, theme, aspect_ratio, slide_width, slide_height, slide_count, is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at FROM presentations WHERE id = $1",
    ).bind(pres.id).fetch_one(&state.db).await?;

    Ok(Json(json!({ "presentation": pres, "slides": [slide] })))
}

pub async fn get(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let pres = require_pres_access(&state, id, user.id).await?;

    let slides = sqlx::query_as::<_, SlideSummary>(
        r#"SELECT id, presentation_id, position, is_hidden, thumbnail_path, thumbnail_dirty, created_at, updated_at
           FROM slides WHERE presentation_id = $1 ORDER BY position ASC"#,
    )
    .bind(id).fetch_all(&state.db).await?;

    // Titre = nom du fichier .kbsld (sans extension) ; self-heal si renommé ailleurs.
    let mut pres = pres;
    if let Some(fid) = pres.file_id {
        if let Some(name) = cf::file_name(&state, pres.owner_id, fid).await {
            let stem = cf::strip_ext(&name);
            if !stem.is_empty() && stem != pres.title {
                sqlx::query("UPDATE presentations SET title = $2 WHERE id = $1")
                    .bind(id).bind(&stem).execute(&state.db).await?;
                pres.title = stem;
            }
        }
    }

    Ok(Json(json!({ "presentation": pres, "slides": slides })))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdatePresentationDto>,
) -> Result<Json<Value>> {
    let new_title = dto.title.clone();
    let pres = sqlx::query_as::<_, Presentation>(
        r#"UPDATE presentations
           SET title      = COALESCE($2, title),
               theme      = COALESCE($3, theme),
               is_starred = COALESCE($4, is_starred),
               last_edited_by = $5
           WHERE id = $1 AND owner_id = $5
           RETURNING id, owner_id, title, file_id, draft_file_id, theme, aspect_ratio, slide_width, slide_height,
                      slide_count, is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at"#,
    )
    .bind(id).bind(dto.title).bind(dto.theme).bind(dto.is_starred).bind(user.id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Présentation {id}")))?;

    // Titre modifié → renommer le fichier .kbsld (titre = nom). Best-effort.
    if let (Some(t), Some(fid)) = (new_title.as_ref(), pres.file_id) {
        if !t.trim().is_empty() {
            if let Err(e) = cf::rename_content_file(&state, pres.owner_id, fid, t, "kbsld").await {
                tracing::warn!(error = %e, pres = %id, "rename .kbsld (titre) échoué");
            }
        }
    }

    Ok(Json(json!({ "presentation": pres })))
}

pub async fn trash(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "UPDATE presentations SET is_trashed = TRUE, trashed_at = NOW() WHERE id = $1 AND owner_id = $2 AND is_trashed = FALSE",
    )
    .bind(id).bind(user.id).execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound(format!("Présentation {id}"))); }
    Ok(Json(json!({ "ok": true })))
}

pub async fn restore(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "UPDATE presentations SET is_trashed = FALSE, trashed_at = NULL WHERE id = $1 AND owner_id = $2 AND is_trashed = TRUE",
    )
    .bind(id).bind(user.id).execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound(format!("Présentation {id}"))); }
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "DELETE FROM presentations WHERE id = $1 AND owner_id = $2 AND is_trashed = TRUE",
    )
    .bind(id).bind(user.id).execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound(format!("Présentation {id}"))); }
    Ok(Json(json!({ "ok": true })))
}

pub async fn duplicate(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    #[derive(sqlx::FromRow)]
    struct PresSource { title: String, theme: Value, aspect_ratio: String, slide_width: i32, slide_height: i32 }

    let src = sqlx::query_as::<_, PresSource>(
        "SELECT title, theme, aspect_ratio, slide_width, slide_height FROM presentations WHERE id = $1 AND owner_id = $2",
    )
    .bind(id).bind(user.id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Présentation {id}")))?;

    let src_pres = require_pres_access(&state, id, user.id).await?;
    let src_content_id = src_pres.file_id
        .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Source presentation has no content file")))?;
    let src_file_content = cf::read_content(&state, src_pres.owner_id, src_content_id).await?;

    let new_title = format!("{} (copie)", src.title);

    let mut tx = state.db.begin().await?;

    let new_pres = sqlx::query_as::<_, Presentation>(
        r#"INSERT INTO presentations (owner_id, title, theme, aspect_ratio, slide_width, slide_height)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, owner_id, title, file_id, draft_file_id, theme, aspect_ratio, slide_width, slide_height,
                      slide_count, is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at"#,
    )
    .bind(user.id).bind(&new_title).bind(src.theme).bind(src.aspect_ratio)
    .bind(src.slide_width).bind(src.slide_height)
    .fetch_one(&mut *tx).await?;

    let src_slides = sqlx::query_as::<_, SlideSummary>(
        "SELECT id, presentation_id, position, is_hidden, thumbnail_path, thumbnail_dirty, created_at, updated_at FROM slides WHERE presentation_id = $1 ORDER BY position ASC",
    )
    .bind(id).fetch_all(&mut *tx).await?;

    let mut new_file_content = json!({ "version": 1, "slides": {} });

    for src_slide in &src_slides {
        let new_slide: SlideSummary = sqlx::query_as::<_, SlideSummary>(
            r#"INSERT INTO slides (presentation_id, position, is_hidden)
               VALUES ($1, $2, $3)
               RETURNING id, presentation_id, position, is_hidden, thumbnail_path, thumbnail_dirty, created_at, updated_at"#,
        )
        .bind(new_pres.id).bind(src_slide.position).bind(src_slide.is_hidden)
        .fetch_one(&mut *tx).await?;

        let slide_data = cf::get_slide_data(&src_file_content, src_slide.id);
        cf::set_slide_data(&mut new_file_content, new_slide.id, slide_data);
    }

    tx.commit().await?;

    let folder = state.files_client
        .ensure_folder_path(user.id, "Office/Presentations", true, Some("Presentation")).await
        .map_err(|e| OfficeError::Internal(e))?;
    let raw = serde_json::to_vec(&new_file_content)
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;
    let file = state.files_client.create_file_with_content(
        user.id, Some(folder.id), &cf::kb_file_name(&new_title, "kbsld"),
        "application/vnd.kubuno.presentation+json", bytes::Bytes::from(cf::gzip(&raw)?),
        Some(json!({ "module": "office", "subtype": "presentation" })),
        false,
    ).await.map_err(|e| OfficeError::Internal(e))?;

    sqlx::query("UPDATE presentations SET file_id = $1 WHERE id = $2")
        .bind(file.id).bind(new_pres.id).execute(&state.db).await?;

    let new_pres = sqlx::query_as::<_, Presentation>(
        "SELECT id, owner_id, title, file_id, draft_file_id, theme, aspect_ratio, slide_width, slide_height, slide_count, is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at FROM presentations WHERE id = $1",
    ).bind(new_pres.id).fetch_one(&state.db).await?;

    Ok(Json(json!({ "presentation": new_pres })))
}

// ── Slides CRUD ───────────────────────────────────────────────────────────────

pub async fn list_slides(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_pres_access(&state, id, user.id).await?;
    let slides = sqlx::query_as::<_, SlideSummary>(
        "SELECT id, presentation_id, position, is_hidden, thumbnail_path, thumbnail_dirty, created_at, updated_at FROM slides WHERE presentation_id = $1 ORDER BY position ASC",
    ).bind(id).fetch_all(&state.db).await?;
    Ok(Json(json!({ "slides": slides })))
}

pub async fn create_slide(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    body: Option<Json<CreateSlideDto>>,
) -> Result<Json<Value>> {
    let pres = require_pres_access(&state, id, user.id).await?;

    let requested_pos = body.and_then(|Json(dto)| dto.position);
    let position = if let Some(pos) = requested_pos {
        sqlx::query("UPDATE slides SET position = position + 1 WHERE presentation_id = $1 AND position >= $2")
            .bind(id).bind(pos).execute(&state.db).await?;
        pos
    } else {
        let max_pos: Option<i32> = sqlx::query_scalar(
            "SELECT MAX(position) FROM slides WHERE presentation_id = $1",
        ).bind(id).fetch_one(&state.db).await?;
        max_pos.map(|p| p + 1).unwrap_or(0)
    };

    let slide = sqlx::query_as::<_, SlideSummary>(
        r#"INSERT INTO slides (presentation_id, position) VALUES ($1, $2)
           RETURNING id, presentation_id, position, is_hidden, thumbnail_path, thumbnail_dirty, created_at, updated_at"#,
    )
    .bind(id).bind(position).fetch_one(&state.db).await?;

    // Add empty slide data to content file
    let content_file_id = ensure_pres_content_file(&state, &pres, user.id, id).await?;
    let mut file_content = cf::read_content(&state, pres.owner_id, content_file_id).await?;
    cf::set_slide_data(&mut file_content, slide.id, cf::empty_slide_data());
    cf::write_content_mirrored(&state, pres.owner_id, content_file_id, pres.file_id, &file_content).await?;

    state.pres_hub.publish(id, crate::state::PresentationMessage::SlideAdded {
        user_id:  user.id,
        slide_id: slide.id,
        position: slide.position,
    }).await;

    Ok(Json(json!({ "slide": slide, "data": cf::empty_slide_data() })))
}

pub async fn get_slide(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((id, sid)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    let pres = require_pres_access(&state, id, user.id).await?;

    let slide = sqlx::query_as::<_, SlideSummary>(
        "SELECT id, presentation_id, position, is_hidden, thumbnail_path, thumbnail_dirty, created_at, updated_at FROM slides WHERE id = $1 AND presentation_id = $2",
    ).bind(sid).bind(id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Slide {sid}")))?;

    let content_file_id = active_pres_content_file_id(&pres)
        .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Presentation has no content file")))?;
    let file_content = cf::read_content(&state, pres.owner_id, content_file_id).await?;
    let slide_data   = cf::get_slide_data(&file_content, sid);

    Ok(Json(json!({ "slide": slide, "data": slide_data })))
}

pub async fn update_slide_elements(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((id, sid)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateSlideElementsDto>,
) -> Result<Json<Value>> {
    let pres = require_pres_access(&state, id, user.id).await?;

    sqlx::query("SELECT id FROM slides WHERE id = $1 AND presentation_id = $2")
        .bind(sid).bind(id).execute(&state.db).await?;

    let content_file_id = ensure_pres_content_file(&state, &pres, user.id, id).await?;
    let mut file_content = cf::read_content(&state, pres.owner_id, content_file_id).await?;
    let mut slide_data   = cf::get_slide_data(&file_content, sid);

    if let Some(ref elems) = dto.elements { slide_data["elements"] = elems.clone(); }
    if let Some(ref notes) = dto.notes    { slide_data["notes"]    = json!(notes); }

    cf::set_slide_data(&mut file_content, sid, slide_data.clone());
    cf::write_content_mirrored(&state, pres.owner_id, content_file_id, pres.file_id, &file_content).await?;

    sqlx::query("UPDATE slides SET thumbnail_dirty = TRUE WHERE id = $1")
        .bind(sid).execute(&state.db).await?;
    sqlx::query("UPDATE presentations SET last_edited_by = $1 WHERE id = $2")
        .bind(user.id).bind(id).execute(&state.db).await?;

    if let Some(ref elems) = dto.elements {
        state.pres_hub.publish(id, crate::state::PresentationMessage::SlideUpdated {
            user_id:  user.id,
            slide_id: sid,
            elements: elems.clone(),
        }).await;
    }

    let slide = sqlx::query_as::<_, SlideSummary>(
        "SELECT id, presentation_id, position, is_hidden, thumbnail_path, thumbnail_dirty, created_at, updated_at FROM slides WHERE id = $1",
    ).bind(sid).fetch_one(&state.db).await?;

    Ok(Json(json!({ "slide": slide, "data": slide_data })))
}

pub async fn update_slide_meta(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((id, sid)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateSlideMetaDto>,
) -> Result<Json<Value>> {
    let pres = require_pres_access(&state, id, user.id).await?;

    let content_file_id = ensure_pres_content_file(&state, &pres, user.id, id).await?;
    let mut file_content = cf::read_content(&state, pres.owner_id, content_file_id).await?;
    let mut slide_data   = cf::get_slide_data(&file_content, sid);

    if let Some(ref bg)  = dto.background { slide_data["background"] = bg.clone(); }
    if let Some(ref tr)  = dto.transition { slide_data["transition"] = tr.clone(); }

    cf::set_slide_data(&mut file_content, sid, slide_data.clone());
    cf::write_content_mirrored(&state, pres.owner_id, content_file_id, pres.file_id, &file_content).await?;

    let slide = if let Some(hidden) = dto.is_hidden {
        sqlx::query_as::<_, SlideSummary>(
            "UPDATE slides SET is_hidden = $1 WHERE id = $2 AND presentation_id = $3
             RETURNING id, presentation_id, position, is_hidden, thumbnail_path, thumbnail_dirty, created_at, updated_at",
        )
        .bind(hidden).bind(sid).bind(id).fetch_optional(&state.db).await?
        .ok_or_else(|| OfficeError::NotFound(format!("Slide {sid}")))?
    } else {
        sqlx::query_as::<_, SlideSummary>(
            "SELECT id, presentation_id, position, is_hidden, thumbnail_path, thumbnail_dirty, created_at, updated_at FROM slides WHERE id = $1 AND presentation_id = $2",
        ).bind(sid).bind(id).fetch_optional(&state.db).await?
        .ok_or_else(|| OfficeError::NotFound(format!("Slide {sid}")))?
    };

    Ok(Json(json!({ "slide": slide, "data": slide_data })))
}

pub async fn delete_slide(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((id, sid)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    let pres = require_pres_access(&state, id, user.id).await?;

    // Une présentation peut rester sans diapositive (état « Ajouter une diapositive »).
    let rows = sqlx::query("DELETE FROM slides WHERE id = $1 AND presentation_id = $2")
        .bind(sid).bind(id).execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound(format!("Slide {sid}"))); }

    // Remove slide from content file
    let fid = active_pres_content_file_id(&pres);
    if let Some(fid) = fid {
        if let Ok(mut fc) = cf::read_content(&state, pres.owner_id, fid).await {
            cf::remove_slide_data(&mut fc, sid);
            let _ = cf::write_content(&state, pres.owner_id, fid, &fc).await;
        }
    }

    state.pres_hub.publish(id, crate::state::PresentationMessage::SlideDeleted {
        user_id:  user.id,
        slide_id: sid,
    }).await;

    Ok(Json(json!({ "ok": true })))
}

pub async fn duplicate_slide(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((id, sid)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    let pres = require_pres_access(&state, id, user.id).await?;

    let src_slide = sqlx::query_as::<_, SlideSummary>(
        "SELECT id, presentation_id, position, is_hidden, thumbnail_path, thumbnail_dirty, created_at, updated_at FROM slides WHERE id = $1 AND presentation_id = $2",
    ).bind(sid).bind(id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Slide {sid}")))?;

    let new_pos = src_slide.position + 1;
    sqlx::query("UPDATE slides SET position = position + 1 WHERE presentation_id = $1 AND position >= $2")
        .bind(id).bind(new_pos).execute(&state.db).await?;

    let new_slide = sqlx::query_as::<_, SlideSummary>(
        r#"INSERT INTO slides (presentation_id, position, is_hidden) VALUES ($1, $2, $3)
           RETURNING id, presentation_id, position, is_hidden, thumbnail_path, thumbnail_dirty, created_at, updated_at"#,
    )
    .bind(id).bind(new_pos).bind(src_slide.is_hidden)
    .fetch_one(&state.db).await?;

    let content_file_id = ensure_pres_content_file(&state, &pres, user.id, id).await?;
    let mut file_content = cf::read_content(&state, pres.owner_id, content_file_id).await?;
    let src_data = cf::get_slide_data(&file_content, sid);
    cf::set_slide_data(&mut file_content, new_slide.id, src_data.clone());
    cf::write_content_mirrored(&state, pres.owner_id, content_file_id, pres.file_id, &file_content).await?;

    state.pres_hub.publish(id, crate::state::PresentationMessage::SlideAdded {
        user_id:  user.id,
        slide_id: new_slide.id,
        position: new_slide.position,
    }).await;

    Ok(Json(json!({ "slide": new_slide, "data": src_data })))
}

pub async fn reorder_slides(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<ReorderSlidesDto>,
) -> Result<Json<Value>> {
    require_pres_access(&state, id, user.id).await?;

    let mut tx = state.db.begin().await?;
    for item in &dto.slides {
        sqlx::query("UPDATE slides SET position = $3 WHERE id = $1 AND presentation_id = $2")
            .bind(item.id).bind(id).bind(item.position).execute(&mut *tx).await?;
    }
    tx.commit().await?;

    state.pres_hub.publish(id, crate::state::PresentationMessage::SlideReordered { user_id: user.id }).await;

    let slides = sqlx::query_as::<_, SlideSummary>(
        "SELECT id, presentation_id, position, is_hidden, thumbnail_path, thumbnail_dirty, created_at, updated_at FROM slides WHERE presentation_id = $1 ORDER BY position ASC",
    ).bind(id).fetch_all(&state.db).await?;

    Ok(Json(json!({ "slides": slides })))
}

pub async fn upload_thumbnail(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((id, sid)): Path<(Uuid, Uuid)>,
    _body: Bytes,
) -> Result<Json<Value>> {
    require_pres_access(&state, id, user.id).await?;

    let thumb_path = format!("/presentations/{id}/slides/{sid}/thumb.png");
    let rows = sqlx::query(
        "UPDATE slides SET thumbnail_path = $3, thumbnail_dirty = FALSE WHERE id = $1 AND presentation_id = $2",
    )
    .bind(sid).bind(id).bind(&thumb_path).execute(&state.db).await?.rows_affected();

    if rows == 0 { return Err(OfficeError::NotFound(format!("Slide {sid}"))); }
    Ok(Json(json!({ "ok": true, "thumbnail_path": thumb_path })))
}

// ── open-by-file ──────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct OpenByFileDto { pub file_id: Uuid }

pub async fn open_by_file(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<OpenByFileDto>,
) -> Result<Json<Value>> {
    let pres = sqlx::query_as::<_, Presentation>(
        r#"SELECT id, owner_id, title, file_id, draft_file_id, theme, aspect_ratio, slide_width, slide_height,
                  slide_count, is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at
           FROM presentations WHERE file_id = $1 AND owner_id = $2 AND is_trashed = FALSE"#,
    )
    .bind(dto.file_id).bind(user.id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Aucune présentation liée au fichier {}", dto.file_id)))?;

    Ok(Json(json!({ "presentation": pres })))
}

// ── Editing session ───────────────────────────────────────────────────────────

pub async fn join_editing(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let pres = require_pres_access(&state, id, user.id).await?;
    let draft_id = ensure_pres_content_file(&state, &pres, user.id, id).await?;

    sqlx::query(
        r#"INSERT INTO editing_sessions (entity_type, entity_id, user_id, display_name)
           VALUES ('presentation', $1, $2, $3)
           ON CONFLICT (entity_type, entity_id, user_id) DO UPDATE SET last_ping_at = NOW()"#,
    )
    .bind(id).bind(user.id).bind(format!("{}", user.id))
    .execute(&state.db).await?;

    state.pres_hub.publish(id, crate::state::PresentationMessage::PresenceChange {
        user_id: user.id,
        action:  "join".to_string(),
    }).await;

    let file_content = cf::read_content(&state, pres.owner_id, draft_id).await?;
    let editors = get_editing_sessions(&state, "presentation", id).await?;
    Ok(Json(json!({ "content": file_content, "editors": editors })))
}

pub async fn leave_editing(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let _ = save_pres_draft_internal(&state, user.id, id).await;

    sqlx::query("DELETE FROM editing_sessions WHERE entity_type = 'presentation' AND entity_id = $1 AND user_id = $2")
        .bind(id).bind(user.id).execute(&state.db).await?;

    state.pres_hub.publish(id, crate::state::PresentationMessage::PresenceChange {
        user_id: user.id,
        action:  "leave".to_string(),
    }).await;

    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM editing_sessions WHERE entity_type = 'presentation' AND entity_id = $1",
    ).bind(id).fetch_one(&state.db).await?;

    if remaining == 0 {
        cleanup_pres_draft(&state, user.id, id).await;
    }

    Ok(Json(json!({ "ok": true })))
}

pub async fn ping_editing(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query("UPDATE editing_sessions SET last_ping_at = NOW() WHERE entity_type = 'presentation' AND entity_id = $1 AND user_id = $2")
        .bind(id).bind(user.id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn active_pres_content_file_id(pres: &Presentation) -> Option<Uuid> {
    pres.draft_file_id.or(pres.file_id)
}

async fn require_pres_owner(state: &AppState, pres_id: Uuid, user_id: Uuid) -> Result<Presentation> {
    sqlx::query_as::<_, Presentation>(
        "SELECT id, owner_id, title, file_id, draft_file_id, theme, aspect_ratio, slide_width, slide_height, slide_count, is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at FROM presentations WHERE id = $1 AND owner_id = $2",
    ).bind(pres_id).bind(user_id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Présentation {pres_id}")))
}

/// Accès à la présentation : propriétaire OU collaborateur (partage user-à-user).
async fn require_pres_access(state: &AppState, pres_id: Uuid, user_id: Uuid) -> Result<Presentation> {
    sqlx::query_as::<_, Presentation>(
        r#"SELECT p.id, p.owner_id, p.title, p.file_id, p.draft_file_id, p.theme, p.aspect_ratio,
                  p.slide_width, p.slide_height, p.slide_count, p.is_starred, p.is_trashed,
                  p.trashed_at, p.last_edited_by, p.created_at, p.updated_at
           FROM presentations p
           WHERE p.id = $1 AND (p.owner_id = $2 OR EXISTS (
               SELECT 1 FROM presentation_collaborators c WHERE c.presentation_id = $1 AND c.user_id = $2
           ))"#,
    ).bind(pres_id).bind(user_id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Présentation {pres_id}")))
}

async fn ensure_pres_content_file(
    state:   &AppState,
    pres:    &Presentation,
    user_id: Uuid,
    pres_id: Uuid,
) -> Result<Uuid> {
    if let Some(d) = pres.draft_file_id { return Ok(d); }
    let main_id = pres.file_id.ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Presentation has no content file")))?;
    let content = cf::read_content(state, pres.owner_id, main_id).await?;
    let draft_id = cf::create_draft_file(state, user_id, "presentation", pres_id, &content).await?;
    sqlx::query("UPDATE presentations SET draft_file_id = $1 WHERE id = $2")
        .bind(draft_id).bind(pres_id).execute(&state.db).await?;
    Ok(draft_id)
}

async fn save_pres_draft_internal(state: &AppState, user_id: Uuid, pres_id: Uuid) -> std::result::Result<(), ()> {
    let pres = require_pres_access(state, pres_id, user_id).await.map_err(|_| ())?;
    let draft_id = pres.draft_file_id.ok_or(())?;
    let main_id  = pres.file_id.ok_or(())?;
    let content  = cf::read_content(state, pres.owner_id, draft_id).await.map_err(|_| ())?;
    cf::write_content(state, pres.owner_id, main_id, &content).await.map_err(|_| ())?;
    Ok(())
}

async fn cleanup_pres_draft(state: &AppState, user_id: Uuid, pres_id: Uuid) {
    if let Ok(Some(Some(fid))) = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT draft_file_id FROM presentations WHERE id = $1 AND owner_id = $2",
    ).bind(pres_id).bind(user_id).fetch_optional(&state.db).await {
        let _ = state.files_client.delete_file(user_id, fid).await;
        let _ = sqlx::query("UPDATE presentations SET draft_file_id = NULL WHERE id = $1")
            .bind(pres_id).execute(&state.db).await;
    }
}

async fn get_editing_sessions(state: &AppState, entity_type: &str, entity_id: Uuid) -> Result<Vec<Value>> {
    #[derive(sqlx::FromRow)]
    struct Session { user_id: Uuid, display_name: Option<String>, color: String }
    let rows = sqlx::query_as::<_, Session>(
        "SELECT user_id, display_name, color FROM editing_sessions WHERE entity_type = $1 AND entity_id = $2 AND last_ping_at > NOW() - INTERVAL '2 minutes'",
    ).bind(entity_type).bind(entity_id).fetch_all(&state.db).await?;
    Ok(rows.iter().map(|r| json!({ "user_id": r.user_id, "display_name": r.display_name, "color": r.color })).collect())
}

// ── Assets d'image des présentations (sortis du doc Yjs) ──────────────────────
//
// Les images ne sont plus stockées en base64 dans le document collaboratif (qui
// explosait en taille) mais comme fichiers dans un dossier CACHÉ + protégé du
// PROPRIÉTAIRE de la présentation : `Office/.media/<presentation_id>/`. Elles sont
// donc partagées par tous les collaborateurs et servies via cet endpoint (autorisé
// par l'accès à la présentation), jamais via le téléchargement Files générique.

fn image_ext(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/avif" => "avif",
        "image/bmp" => "bmp",
        _ => "bin",
    }
}

/// POST /presentations/:id/assets  (multipart, champ `file`) → { file_id, ref }
pub async fn upload_asset(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<Value>> {
    let pres = require_pres_access(&state, id, user.id).await?;
    let owner = pres.owner_id;

    let mut data: Option<Bytes> = None;
    let mut mime = String::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| OfficeError::Validation(format!("Multipart invalide: {e}")))?
    {
        if field.name() == Some("file") {
            mime = field.content_type().map(|s| s.to_string()).unwrap_or_default();
            data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| OfficeError::Validation(format!("Lecture du fichier échouée: {e}")))?,
            );
        }
    }
    let data = data.ok_or_else(|| OfficeError::Validation("Champ 'file' manquant".into()))?;
    if !mime.starts_with("image/") {
        return Err(OfficeError::Validation("Type non autorisé (image attendue)".into()));
    }
    const MAX_BYTES: usize = 25 * 1024 * 1024;
    if data.len() > MAX_BYTES {
        return Err(OfficeError::Validation("Image trop volumineuse (max 25 Mo)".into()));
    }

    // Dossier caché + protégé du propriétaire (réutilisé entre uploads).
    let folder = state
        .files_client
        .ensure_folder_path_ex(owner, &format!("Office/.media/{id}"), true, true, Some("Image"))
        .await
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;

    let name = format!("{}.{}", Uuid::new_v4(), image_ext(&mime));
    let file = state
        .files_client
        .create_file_with_content(
            owner,
            Some(folder.id),
            &name,
            &mime,
            data,
            Some(json!({ "module": "office", "kind": "presentation-asset", "presentation": id })),
            false,
        )
        .await
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;

    Ok(Json(json!({ "file_id": file.id, "ref": format!("kbfile:{}", file.id) })))
}

/// GET /presentations/:id/assets/:file_id → octets de l'image (autorisé par l'accès présentation).
pub async fn get_asset(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((id, file_id)): Path<(Uuid, Uuid)>,
) -> Result<Response> {
    let pres = require_pres_access(&state, id, user.id).await?;
    let (info, bytes) = state
        .files_client
        .get_file_content(pres.owner_id, file_id)
        .await
        .map_err(|_| OfficeError::NotFound(format!("Asset {file_id}")))?;

    let ctype = if info.mime_type.is_empty() { "application/octet-stream".to_string() } else { info.mime_type };
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, ctype),
            (header::CACHE_CONTROL, "private, max-age=31536000, immutable".to_string()),
        ],
        Body::from(bytes),
    )
        .into_response())
}
