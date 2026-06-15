use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;
use axum::Extension;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    models::diagram::*,
    services::content_files as cf,
    state::AppState,
};

// ── Diagrams CRUD ─────────────────────────────────────────────────────────────

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Query(q): Query<ListDiagramsQuery>,
) -> Result<Json<Value>> {
    let limit   = q.limit.unwrap_or(50).min(200);
    let offset  = q.offset.unwrap_or(0);
    let trashed = q.trashed.unwrap_or(false);

    let rows: Vec<Diagram> = if let Some(ref search) = q.search {
        sqlx::query_as::<_, Diagram>(
            r#"SELECT id, owner_id, title, file_id, draft_file_id, diagram_type, settings, is_starred, is_trashed,
                      trashed_at, last_edited_by, created_at, updated_at
               FROM diagrams
               WHERE owner_id = $1 AND is_trashed = $2 AND title ILIKE $3
               ORDER BY updated_at DESC LIMIT $4 OFFSET $5"#,
        )
        .bind(user.id).bind(trashed).bind(format!("%{search}%"))
        .bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    } else if q.starred.unwrap_or(false) {
        sqlx::query_as::<_, Diagram>(
            r#"SELECT id, owner_id, title, file_id, draft_file_id, diagram_type, settings, is_starred, is_trashed,
                      trashed_at, last_edited_by, created_at, updated_at
               FROM diagrams
               WHERE owner_id = $1 AND is_starred = TRUE AND is_trashed = FALSE
               ORDER BY updated_at DESC LIMIT $2 OFFSET $3"#,
        )
        .bind(user.id).bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    } else if q.recent.unwrap_or(false) {
        sqlx::query_as::<_, Diagram>(
            r#"SELECT id, owner_id, title, file_id, draft_file_id, diagram_type, settings, is_starred, is_trashed,
                      trashed_at, last_edited_by, created_at, updated_at
               FROM diagrams
               WHERE owner_id = $1 AND is_trashed = FALSE
               ORDER BY updated_at DESC LIMIT $2 OFFSET $3"#,
        )
        .bind(user.id).bind(limit.min(20)).bind(offset)
        .fetch_all(&state.db).await?
    } else {
        sqlx::query_as::<_, Diagram>(
            r#"SELECT id, owner_id, title, file_id, draft_file_id, diagram_type, settings, is_starred, is_trashed,
                      trashed_at, last_edited_by, created_at, updated_at
               FROM diagrams
               WHERE owner_id = $1 AND is_trashed = $2
               ORDER BY updated_at DESC LIMIT $3 OFFSET $4"#,
        )
        .bind(user.id).bind(trashed).bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    };

    Ok(Json(json!({ "diagrams": rows, "total": rows.len() })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    body: Option<Json<CreateDiagramDto>>,
) -> Result<Json<Value>> {
    let dto = body.map(|Json(d)| d).unwrap_or(CreateDiagramDto {
        title: None, diagram_type: None,
    });
    let title        = dto.title.unwrap_or_else(|| "Diagramme sans titre".to_string());
    let diagram_type = dto.diagram_type.as_deref().unwrap_or("freeform");

    let mut tx = state.db.begin().await?;

    let diagram = sqlx::query_as::<_, Diagram>(
        r#"INSERT INTO diagrams (owner_id, title, diagram_type)
           VALUES ($1, $2, $3)
           RETURNING id, owner_id, title, file_id, draft_file_id, diagram_type, settings, is_starred, is_trashed,
                     trashed_at, last_edited_by, created_at, updated_at"#,
    )
    .bind(user.id).bind(&title).bind(diagram_type)
    .fetch_one(&mut *tx).await?;

    let page = sqlx::query_as::<_, DiagramPage>(
        r#"INSERT INTO diagram_pages (diagram_id, name, position)
           VALUES ($1, 'Page 1', 0)
           RETURNING id, diagram_id, name, position, bg_color, width, height,
                     is_hidden, created_at, updated_at"#,
    )
    .bind(diagram.id)
    .fetch_one(&mut *tx).await?;

    tx.commit().await?;

    let file_id = cf::create_diagram_content_file(&state, user.id, &title, page.id).await?;
    sqlx::query("UPDATE diagrams SET file_id = $1 WHERE id = $2")
        .bind(file_id).bind(diagram.id).execute(&state.db).await?;

    let diagram = sqlx::query_as::<_, Diagram>(
        "SELECT id, owner_id, title, file_id, draft_file_id, diagram_type, settings, is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at FROM diagrams WHERE id = $1",
    ).bind(diagram.id).fetch_one(&state.db).await?;

    Ok(Json(json!({ "diagram": diagram, "first_page": page })))
}

pub async fn get(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let diagram = sqlx::query_as::<_, Diagram>(
        r#"SELECT id, owner_id, title, file_id, draft_file_id, diagram_type, settings, is_starred, is_trashed,
                  trashed_at, last_edited_by, created_at, updated_at
           FROM diagrams WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id).bind(user.id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Diagramme introuvable".into()))?;

    let pages = sqlx::query_as::<_, DiagramPage>(
        r#"SELECT id, diagram_id, name, position, bg_color, width, height,
                  is_hidden, created_at, updated_at
           FROM diagram_pages WHERE diagram_id = $1 ORDER BY position ASC"#,
    )
    .bind(id)
    .fetch_all(&state.db).await?;

    // Titre = nom du fichier .kbdia (sans extension) ; self-heal si renommé ailleurs.
    let mut diagram = diagram;
    if let Some(fid) = diagram.file_id {
        if let Some(name) = cf::file_name(&state, diagram.owner_id, fid).await {
            let stem = cf::strip_ext(&name);
            if !stem.is_empty() && stem != diagram.title {
                sqlx::query("UPDATE diagrams SET title = $2 WHERE id = $1")
                    .bind(id).bind(&stem).execute(&state.db).await?;
                diagram.title = stem;
            }
        }
    }

    Ok(Json(json!({ "diagram": diagram, "pages": pages })))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateDiagramDto>,
) -> Result<Json<Value>> {
    let diagram = sqlx::query_as::<_, Diagram>(
        r#"UPDATE diagrams SET
             title          = COALESCE($3, title),
             diagram_type   = COALESCE($4, diagram_type),
             settings       = COALESCE($5, settings),
             is_starred     = COALESCE($6, is_starred),
             last_edited_by = $7
           WHERE id = $1 AND owner_id = $2
           RETURNING id, owner_id, title, file_id, draft_file_id, diagram_type, settings, is_starred, is_trashed,
                     trashed_at, last_edited_by, created_at, updated_at"#,
    )
    .bind(id).bind(user.id)
    .bind(dto.title.as_deref())
    .bind(dto.diagram_type.as_deref())
    .bind(dto.settings.as_ref())
    .bind(dto.is_starred)
    .bind(user.id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Diagramme introuvable".into()))?;

    // Titre modifié → renommer le fichier .kbdia (titre = nom). Best-effort.
    if let (Some(t), Some(fid)) = (dto.title.as_deref(), diagram.file_id) {
        if !t.trim().is_empty() {
            if let Err(e) = cf::rename_content_file(&state, diagram.owner_id, fid, t, "kbdia").await {
                tracing::warn!(error = %e, diagram = %id, "rename .kbdia (titre) échoué");
            }
        }
    }

    Ok(Json(json!({ "diagram": diagram })))
}

pub async fn trash(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query("UPDATE diagrams SET is_trashed = TRUE, trashed_at = NOW() WHERE id = $1 AND owner_id = $2")
        .bind(id).bind(user.id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn restore(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query("UPDATE diagrams SET is_trashed = FALSE, trashed_at = NULL WHERE id = $1 AND owner_id = $2")
        .bind(id).bind(user.id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query("DELETE FROM diagrams WHERE id = $1 AND owner_id = $2 AND is_trashed = TRUE")
        .bind(id).bind(user.id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn duplicate(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let src = require_diagram_owner(&state, id, user.id).await?;

    let src_content_id = active_diagram_content_file_id(&src)
        .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Diagram has no content file")))?;
    let src_content = cf::read_content(&state, src.owner_id, src_content_id).await?;

    let src_pages = sqlx::query_as::<_, DiagramPage>(
        "SELECT id, diagram_id, name, position, bg_color, width, height, is_hidden, created_at, updated_at FROM diagram_pages WHERE diagram_id = $1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(&state.db).await?;

    let mut tx = state.db.begin().await?;

    let new_diag = sqlx::query_as::<_, Diagram>(
        r#"INSERT INTO diagrams (owner_id, title, diagram_type, settings)
           VALUES ($1, $2, $3, $4)
           RETURNING id, owner_id, title, file_id, draft_file_id, diagram_type, settings, is_starred, is_trashed,
                     trashed_at, last_edited_by, created_at, updated_at"#,
    )
    .bind(user.id)
    .bind(format!("{} (copie)", src.title))
    .bind(&src.diagram_type)
    .bind(&src.settings)
    .fetch_one(&mut *tx).await?;

    // Insert pages with new IDs, keeping a remap for content migration
    let mut page_remap: Vec<(Uuid, Uuid)> = Vec::new(); // (old_id, new_id)
    for p in &src_pages {
        let new_page = sqlx::query_as::<_, DiagramPage>(
            r#"INSERT INTO diagram_pages (diagram_id, name, position, bg_color, width, height, is_hidden)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING id, diagram_id, name, position, bg_color, width, height, is_hidden, created_at, updated_at"#,
        )
        .bind(new_diag.id).bind(&p.name).bind(p.position)
        .bind(&p.bg_color).bind(p.width).bind(p.height).bind(p.is_hidden)
        .fetch_one(&mut *tx).await?;
        page_remap.push((p.id, new_page.id));
    }

    tx.commit().await?;

    // Build new content with remapped page IDs
    let new_content = remap_diagram_content(&src_content, &page_remap);
    let file_id = cf::create_diagram_content_file(&state, user.id, &new_diag.title,
        page_remap.first().map(|(_, nid)| *nid).unwrap_or_else(Uuid::new_v4)).await?;

    // Write actual content (create_diagram_content_file creates empty content, overwrite it)
    cf::write_content(&state, user.id, file_id, &new_content).await?;

    sqlx::query("UPDATE diagrams SET file_id = $1 WHERE id = $2")
        .bind(file_id).bind(new_diag.id).execute(&state.db).await?;

    let new_diag = sqlx::query_as::<_, Diagram>(
        "SELECT id, owner_id, title, file_id, draft_file_id, diagram_type, settings, is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at FROM diagrams WHERE id = $1",
    ).bind(new_diag.id).fetch_one(&state.db).await?;

    Ok(Json(json!({ "diagram": new_diag })))
}

// ── Pages ─────────────────────────────────────────────────────────────────────

pub async fn list_pages(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(diagram_id): Path<Uuid>,
) -> Result<Json<Value>> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM diagrams WHERE id = $1 AND owner_id = $2)")
        .bind(diagram_id).bind(user.id)
        .fetch_one(&state.db).await?;
    if !exists { return Err(OfficeError::NotFound("Diagramme introuvable".into())); }

    let pages = sqlx::query_as::<_, DiagramPage>(
        r#"SELECT id, diagram_id, name, position, bg_color, width, height,
                  is_hidden, created_at, updated_at
           FROM diagram_pages WHERE diagram_id = $1 ORDER BY position ASC"#,
    )
    .bind(diagram_id)
    .fetch_all(&state.db).await?;

    Ok(Json(json!({ "pages": pages })))
}

pub async fn create_page(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(diagram_id): Path<Uuid>,
    body: Option<Json<CreatePageDto>>,
) -> Result<Json<Value>> {
    let diag = require_diagram_owner(&state, diagram_id, user.id).await?;
    let dto  = body.map(|Json(d)| d).unwrap_or(CreatePageDto { name: None, bg_color: None });
    let name = dto.name.unwrap_or_else(|| "Nouvelle page".to_string());

    let page = sqlx::query_as::<_, DiagramPage>(
        r#"INSERT INTO diagram_pages (diagram_id, name, bg_color, position)
           VALUES ($1, $2, COALESCE($3, '#ffffff'),
                   COALESCE((SELECT MAX(position)+1 FROM diagram_pages WHERE diagram_id = $1), 0))
           RETURNING id, diagram_id, name, position, bg_color, width, height,
                     is_hidden, created_at, updated_at"#,
    )
    .bind(diagram_id).bind(&name).bind(dto.bg_color.as_deref())
    .fetch_one(&state.db).await?;

    // Add empty page data in content file
    let content_id = ensure_diag_content_file(&state, &diag, user.id, diagram_id).await?;
    let mut fc = cf::read_content(&state, diag.owner_id, content_id).await?;
    cf::set_page_data(&mut fc, page.id, cf::empty_page_data());
    cf::write_content_mirrored(&state, diag.owner_id, content_id, diag.file_id, &fc).await?;

    Ok(Json(json!({ "page": page })))
}

pub async fn get_page(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((diagram_id, page_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    let diag = require_diagram_owner(&state, diagram_id, user.id).await?;

    let page = sqlx::query_as::<_, DiagramPage>(
        r#"SELECT id, diagram_id, name, position, bg_color, width, height,
                  is_hidden, created_at, updated_at
           FROM diagram_pages WHERE id = $1 AND diagram_id = $2"#,
    )
    .bind(page_id).bind(diagram_id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Page introuvable".into()))?;

    let content_id = active_diagram_content_file_id(&diag)
        .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Diagram has no content file")))?;
    let fc   = cf::read_content(&state, diag.owner_id, content_id).await?;
    let data = cf::get_page_data(&fc, page_id);

    Ok(Json(json!({ "page": page, "data": data })))
}

pub async fn update_page_data(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((diagram_id, page_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdatePageDataDto>,
) -> Result<Json<Value>> {
    let diag = require_diagram_owner(&state, diagram_id, user.id).await?;

    // Ensure page exists
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM diagram_pages WHERE id = $1 AND diagram_id = $2)")
        .bind(page_id).bind(diagram_id).fetch_one(&state.db).await?;
    if !exists { return Err(OfficeError::NotFound("Page introuvable".into())); }

    let content_id = ensure_diag_content_file(&state, &diag, user.id, diagram_id).await?;
    let mut fc = cf::read_content(&state, diag.owner_id, content_id).await?;
    cf::set_page_data(&mut fc, page_id, dto.data.clone());
    cf::write_content_mirrored(&state, diag.owner_id, content_id, diag.file_id, &fc).await?;

    sqlx::query("UPDATE diagrams SET last_edited_by = $1 WHERE id = $2")
        .bind(user.id).bind(diagram_id).execute(&state.db).await?;

    state.diagram_hub.publish(page_id, crate::state::DiagramMessage::PageUpdated {
        user_id: user.id,
        data:    dto.data,
    }).await;

    Ok(Json(json!({ "ok": true })))
}

pub async fn update_page_meta(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((diagram_id, page_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdatePageMetaDto>,
) -> Result<Json<Value>> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM diagrams WHERE id = $1 AND owner_id = $2)")
        .bind(diagram_id).bind(user.id)
        .fetch_one(&state.db).await?;
    if !exists { return Err(OfficeError::NotFound("Diagramme introuvable".into())); }

    let page = sqlx::query_as::<_, DiagramPage>(
        r#"UPDATE diagram_pages SET
             name      = COALESCE($3, name),
             bg_color  = COALESCE($4, bg_color),
             is_hidden = COALESCE($5, is_hidden),
             position  = COALESCE($6, position)
           WHERE id = $1 AND diagram_id = $2
           RETURNING id, diagram_id, name, position, bg_color, width, height,
                     is_hidden, created_at, updated_at"#,
    )
    .bind(page_id).bind(diagram_id)
    .bind(dto.name.as_deref())
    .bind(dto.bg_color.as_deref())
    .bind(dto.is_hidden)
    .bind(dto.position)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Page introuvable".into()))?;

    Ok(Json(json!({ "page": page })))
}

pub async fn delete_page(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((diagram_id, page_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM diagram_pages WHERE diagram_id = $1"
    )
    .bind(diagram_id)
    .fetch_one(&state.db).await?;

    if count <= 1 {
        return Err(OfficeError::Validation("Impossible de supprimer la dernière page".into()));
    }

    let diag = require_diagram_owner(&state, diagram_id, user.id).await?;

    // Remove page data from content file
    if let Some(content_id) = active_diagram_content_file_id(&diag) {
        if let Ok(mut fc) = cf::read_content(&state, diag.owner_id, content_id).await {
            cf::remove_page_data(&mut fc, page_id);
            let _ = cf::write_content(&state, diag.owner_id, content_id, &fc).await;
        }
    }

    sqlx::query("DELETE FROM diagram_pages WHERE id = $1 AND diagram_id = $2")
        .bind(page_id).bind(diagram_id)
        .execute(&state.db).await?;

    Ok(Json(json!({ "ok": true })))
}

pub async fn reorder_pages(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(diagram_id): Path<Uuid>,
    Json(dto): Json<ReorderPagesDto>,
) -> Result<Json<Value>> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM diagrams WHERE id = $1 AND owner_id = $2)")
        .bind(diagram_id).bind(user.id)
        .fetch_one(&state.db).await?;
    if !exists { return Err(OfficeError::NotFound("Diagramme introuvable".into())); }

    let mut tx = state.db.begin().await?;
    for p in &dto.pages {
        sqlx::query("UPDATE diagram_pages SET position = $1 WHERE id = $2 AND diagram_id = $3")
            .bind(p.position).bind(p.id).bind(diagram_id)
            .execute(&mut *tx).await?;
    }
    tx.commit().await?;

    Ok(Json(json!({ "ok": true })))
}

// ── Custom Shapes ─────────────────────────────────────────────────────────────

pub async fn list_custom_shapes(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
) -> Result<Json<Value>> {
    let shapes = sqlx::query_as::<_, CustomShape>(
        "SELECT id, owner_id, name, category, shape_def, thumbnail, created_at FROM custom_shapes WHERE owner_id = $1 ORDER BY created_at DESC"
    )
    .bind(user.id)
    .fetch_all(&state.db).await?;

    Ok(Json(json!({ "shapes": shapes })))
}

pub async fn create_custom_shape(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<CreateCustomShapeDto>,
) -> Result<Json<Value>> {
    let shape = sqlx::query_as::<_, CustomShape>(
        r#"INSERT INTO custom_shapes (owner_id, name, category, shape_def, thumbnail)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, owner_id, name, category, shape_def, thumbnail, created_at"#,
    )
    .bind(user.id).bind(&dto.name)
    .bind(dto.category.as_deref().unwrap_or("Mes formes"))
    .bind(&dto.shape_def)
    .bind(dto.thumbnail.as_deref())
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "shape": shape })))
}

pub async fn delete_custom_shape(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(sid): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query("DELETE FROM custom_shapes WHERE id = $1 AND owner_id = $2")
        .bind(sid).bind(user.id)
        .execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── Export JSON ───────────────────────────────────────────────────────────────

pub async fn export_json(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let diagram = require_diagram_owner(&state, id, user.id).await?;

    let pages = sqlx::query_as::<_, DiagramPage>(
        "SELECT id, diagram_id, name, position, bg_color, width, height, is_hidden, created_at, updated_at FROM diagram_pages WHERE diagram_id = $1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(&state.db).await?;

    let content_id = active_diagram_content_file_id(&diagram)
        .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Diagram has no content file")))?;
    let fc = cf::read_content(&state, diagram.owner_id, content_id).await?;

    let pages_with_data: Vec<Value> = pages.iter().map(|p| {
        let data = cf::get_page_data(&fc, p.id);
        json!({
            "id": p.id, "name": p.name, "position": p.position,
            "bg_color": p.bg_color, "width": p.width, "height": p.height,
            "is_hidden": p.is_hidden,
            "data": data,
        })
    }).collect();

    Ok(Json(json!({
        "format":  "kubuno-diagram/v1",
        "diagram": diagram,
        "pages":   pages_with_data,
    })))
}

// ── Open by file ──────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct OpenByFileDto {
    pub file_id: Uuid,
}

pub async fn open_by_file(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<OpenByFileDto>,
) -> Result<Json<Value>> {
    let diagram = sqlx::query_as::<_, Diagram>(
        r#"SELECT id, owner_id, title, file_id, draft_file_id, diagram_type, settings, is_starred, is_trashed,
                  trashed_at, last_edited_by, created_at, updated_at
           FROM diagrams
           WHERE file_id = $1 AND owner_id = $2 AND is_trashed = FALSE"#,
    )
    .bind(dto.file_id).bind(user.id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Aucun diagramme lié au fichier {}", dto.file_id)))?;

    Ok(Json(json!({ "diagram": diagram })))
}

// ── Editing session ───────────────────────────────────────────────────────────

pub async fn join_editing(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let diag     = require_diagram_owner(&state, id, user.id).await?;
    let draft_id = ensure_diag_content_file(&state, &diag, user.id, id).await?;

    sqlx::query(
        r#"INSERT INTO editing_sessions (entity_type, entity_id, user_id, display_name)
           VALUES ('diagram', $1, $2, $3)
           ON CONFLICT (entity_type, entity_id, user_id) DO UPDATE SET last_ping_at = NOW()"#,
    )
    .bind(id).bind(user.id).bind(format!("{}", user.id))
    .execute(&state.db).await?;

    // Notify all active page hubs
    let pages = sqlx::query_scalar::<_, Uuid>("SELECT id FROM diagram_pages WHERE diagram_id = $1")
        .bind(id).fetch_all(&state.db).await?;
    for page_id in pages {
        state.diagram_hub.publish(page_id, crate::state::DiagramMessage::PresenceChange {
            user_id: user.id,
            action:  "join".to_string(),
        }).await;
    }

    let file_content = cf::read_content(&state, diag.owner_id, draft_id).await?;
    let editors = get_editing_sessions(&state, "diagram", id).await?;
    Ok(Json(json!({ "content": file_content, "editors": editors })))
}

pub async fn leave_editing(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let _ = save_diag_draft_internal(&state, user.id, id).await;

    sqlx::query("DELETE FROM editing_sessions WHERE entity_type = 'diagram' AND entity_id = $1 AND user_id = $2")
        .bind(id).bind(user.id).execute(&state.db).await?;

    // Notify all active page hubs
    let pages = sqlx::query_scalar::<_, Uuid>("SELECT id FROM diagram_pages WHERE diagram_id = $1")
        .bind(id).fetch_all(&state.db).await?;
    for page_id in pages {
        state.diagram_hub.publish(page_id, crate::state::DiagramMessage::PresenceChange {
            user_id: user.id,
            action:  "leave".to_string(),
        }).await;
    }

    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM editing_sessions WHERE entity_type = 'diagram' AND entity_id = $1",
    ).bind(id).fetch_one(&state.db).await?;

    if remaining == 0 {
        cleanup_diag_draft(&state, user.id, id).await;
    }

    Ok(Json(json!({ "ok": true })))
}

pub async fn ping_editing(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query("UPDATE editing_sessions SET last_ping_at = NOW() WHERE entity_type = 'diagram' AND entity_id = $1 AND user_id = $2")
        .bind(id).bind(user.id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn active_diagram_content_file_id(diagram: &Diagram) -> Option<Uuid> {
    diagram.draft_file_id.or(diagram.file_id)
}

async fn require_diagram_owner(state: &AppState, diagram_id: Uuid, user_id: Uuid) -> Result<Diagram> {
    sqlx::query_as::<_, Diagram>(
        "SELECT id, owner_id, title, file_id, draft_file_id, diagram_type, settings, is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at FROM diagrams WHERE id = $1 AND owner_id = $2",
    ).bind(diagram_id).bind(user_id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Diagramme {diagram_id}")))
}

/// Ensures a draft content file exists. Creates one from the main file if not.
/// Returns the draft file ID (or main file ID if the diagram was just created without a draft).
async fn ensure_diag_content_file(
    state:      &AppState,
    diag:       &Diagram,
    user_id:    Uuid,
    diagram_id: Uuid,
) -> Result<Uuid> {
    if let Some(d) = diag.draft_file_id { return Ok(d); }
    let main_id = diag.file_id.ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Diagram has no content file")))?;
    let content  = cf::read_content(state, diag.owner_id, main_id).await?;
    let draft_id = cf::create_draft_file(state, user_id, "diagram", diagram_id, &content).await?;
    sqlx::query("UPDATE diagrams SET draft_file_id = $1 WHERE id = $2")
        .bind(draft_id).bind(diagram_id).execute(&state.db).await?;
    Ok(draft_id)
}

async fn save_diag_draft_internal(state: &AppState, user_id: Uuid, diagram_id: Uuid) -> std::result::Result<(), ()> {
    let diag     = require_diagram_owner(state, diagram_id, user_id).await.map_err(|_| ())?;
    let draft_id = diag.draft_file_id.ok_or(())?;
    let main_id  = diag.file_id.ok_or(())?;
    let content  = cf::read_content(state, diag.owner_id, draft_id).await.map_err(|_| ())?;
    cf::write_content(state, diag.owner_id, main_id, &content).await.map_err(|_| ())?;
    Ok(())
}

async fn cleanup_diag_draft(state: &AppState, user_id: Uuid, diagram_id: Uuid) {
    if let Ok(Some(Some(fid))) = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT draft_file_id FROM diagrams WHERE id = $1 AND owner_id = $2",
    ).bind(diagram_id).bind(user_id).fetch_optional(&state.db).await {
        let _ = state.files_client.delete_file(user_id, fid).await;
        let _ = sqlx::query("UPDATE diagrams SET draft_file_id = NULL WHERE id = $1")
            .bind(diagram_id).execute(&state.db).await;
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

/// Remaps page IDs in a diagram content JSON from old UUIDs to new UUIDs.
fn remap_diagram_content(content: &Value, remap: &[(Uuid, Uuid)]) -> Value {
    let pages_obj = match content.get("pages").and_then(|p| p.as_object()) {
        Some(o) => o,
        None    => return content.clone(),
    };

    let mut new_pages = serde_json::Map::new();
    for (old_id, new_id) in remap {
        if let Some(page_data) = pages_obj.get(&old_id.to_string()) {
            new_pages.insert(new_id.to_string(), page_data.clone());
        }
    }

    json!({ "version": content.get("version").cloned().unwrap_or(json!(1)), "pages": new_pages })
}
