use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;
use axum::Extension;

use crate::{
    converters::ods::import_ods,
    converters::xlsx::import_xlsx,
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    models::spreadsheet::*,
    services::content_files as cf,
    state::AppState,
};

// ── Spreadsheets CRUD ─────────────────────────────────────────────────────────

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Query(q): Query<ListSpreadsheetsQuery>,
) -> Result<Json<Value>> {
    let limit   = q.limit.unwrap_or(50).min(200);
    let offset  = q.offset.unwrap_or(0);
    let trashed = q.trashed.unwrap_or(false);

    let rows: Vec<Spreadsheet> = if let Some(ref search) = q.search {
        sqlx::query_as::<_, Spreadsheet>(
            r#"SELECT id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at
               FROM spreadsheets WHERE owner_id = $1 AND is_trashed = $2 AND title ILIKE $3
               ORDER BY updated_at DESC LIMIT $4 OFFSET $5"#,
        ).bind(user.id).bind(trashed).bind(format!("%{search}%")).bind(limit).bind(offset)
         .fetch_all(&state.db).await?
    } else if q.starred.unwrap_or(false) {
        sqlx::query_as::<_, Spreadsheet>(
            r#"SELECT id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at
               FROM spreadsheets WHERE owner_id = $1 AND is_starred = TRUE AND is_trashed = FALSE
               ORDER BY updated_at DESC LIMIT $2 OFFSET $3"#,
        ).bind(user.id).bind(limit).bind(offset).fetch_all(&state.db).await?
    } else if q.recent.unwrap_or(false) {
        sqlx::query_as::<_, Spreadsheet>(
            r#"SELECT id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at
               FROM spreadsheets WHERE owner_id = $1 AND is_trashed = FALSE
               ORDER BY updated_at DESC LIMIT $2 OFFSET $3"#,
        ).bind(user.id).bind(limit.min(20)).bind(offset).fetch_all(&state.db).await?
    } else if q.shared.unwrap_or(false) {
        // Tableurs partagés AVEC moi (collaborateur, pas propriétaire).
        sqlx::query_as::<_, Spreadsheet>(
            r#"SELECT s.id, s.owner_id, s.title, s.file_id, s.draft_file_id, s.is_starred, s.is_trashed, s.trashed_at, s.created_at, s.updated_at
               FROM spreadsheets s
               JOIN spreadsheet_collaborators c ON c.spreadsheet_id = s.id
               WHERE c.user_id = $1 AND s.is_trashed = FALSE
               ORDER BY s.updated_at DESC LIMIT $2 OFFSET $3"#,
        ).bind(user.id).bind(limit).bind(offset).fetch_all(&state.db).await?
    } else {
        sqlx::query_as::<_, Spreadsheet>(
            r#"SELECT id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at
               FROM spreadsheets WHERE owner_id = $1 AND is_trashed = $2
               ORDER BY updated_at DESC LIMIT $3 OFFSET $4"#,
        ).bind(user.id).bind(trashed).bind(limit).bind(offset).fetch_all(&state.db).await?
    };

    Ok(Json(json!({ "spreadsheets": rows, "total": rows.len() })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<CreateSpreadsheetDto>,
) -> Result<Json<Value>> {
    let title = dto.title.unwrap_or_else(|| "Sans titre".to_string());

    let mut tx = state.db.begin().await?;

    let ss: Spreadsheet = sqlx::query_as::<_, Spreadsheet>(
        r#"INSERT INTO spreadsheets (owner_id, title)
           VALUES ($1, $2)
           RETURNING id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at"#,
    )
    .bind(user.id).bind(&title).fetch_one(&mut *tx).await?;

    let sheet: SpreadsheetSheet = sqlx::query_as::<_, SpreadsheetSheet>(
        r#"INSERT INTO spreadsheet_sheets (spreadsheet_id, name, position)
           VALUES ($1, 'Feuille 1', 0)
           RETURNING id, spreadsheet_id, name, position, created_at, updated_at"#,
    )
    .bind(ss.id).fetch_one(&mut *tx).await?;

    tx.commit().await?;

    let file_id = cf::create_spreadsheet_content_file(&state, user.id, &title, sheet.id).await?;
    sqlx::query("UPDATE spreadsheets SET file_id = $1 WHERE id = $2")
        .bind(file_id).bind(ss.id).execute(&state.db).await?;

    let ss = sqlx::query_as::<_, Spreadsheet>(
        "SELECT id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at FROM spreadsheets WHERE id = $1"
    ).bind(ss.id).fetch_one(&state.db).await?;

    Ok(Json(json!({ "spreadsheet": ss, "sheets": [sheet] })))
}

pub async fn get(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let ss: Spreadsheet = sqlx::query_as::<_, Spreadsheet>(
        r#"SELECT id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at
           FROM spreadsheets WHERE id = $1 AND is_trashed = FALSE AND (owner_id = $2 OR EXISTS (
               SELECT 1 FROM spreadsheet_collaborators c WHERE c.spreadsheet_id = $1 AND c.user_id = $2
           ))"#,
    )
    .bind(id).bind(user.id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Tableur introuvable".into()))?;

    let sheets: Vec<SpreadsheetSheet> = sqlx::query_as::<_, SpreadsheetSheet>(
        r#"SELECT id, spreadsheet_id, name, position, created_at, updated_at
           FROM spreadsheet_sheets WHERE spreadsheet_id = $1 ORDER BY position ASC"#,
    )
    .bind(id).fetch_all(&state.db).await?;

    // Titre = nom du fichier .kbcal (sans extension) ; self-heal de la colonne si
    // le fichier a été renommé depuis l'explorateur.
    let mut ss = ss;
    if let Some(fid) = ss.file_id {
        if let Some(name) = cf::file_name(&state, ss.owner_id, fid).await {
            let stem = cf::strip_ext(&name);
            if !stem.is_empty() && stem != ss.title {
                sqlx::query("UPDATE spreadsheets SET title = $2 WHERE id = $1")
                    .bind(id).bind(&stem).execute(&state.db).await?;
                ss.title = stem;
            }
        }
    }

    Ok(Json(json!({ "spreadsheet": ss, "sheets": sheets })))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateSpreadsheetDto>,
) -> Result<Json<Value>> {
    let ss: Spreadsheet = sqlx::query_as::<_, Spreadsheet>(
        "SELECT id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at FROM spreadsheets WHERE id = $1 AND owner_id = $2",
    )
    .bind(id).bind(user.id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Tableur introuvable".into()))?;

    let title_changed = dto.title.is_some();
    let title      = dto.title.unwrap_or(ss.title);
    let is_starred = dto.is_starred.unwrap_or(ss.is_starred);

    let updated: Spreadsheet = sqlx::query_as::<_, Spreadsheet>(
        r#"UPDATE spreadsheets SET title = $1, is_starred = $2
           WHERE id = $3
           RETURNING id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at"#,
    )
    .bind(&title).bind(is_starred).bind(id).fetch_one(&state.db).await?;

    // Titre modifié → renommer le fichier .kbcal (titre = nom). Best-effort.
    if title_changed && !title.trim().is_empty() {
        if let Some(fid) = updated.file_id {
            if let Err(e) = cf::rename_content_file(&state, updated.owner_id, fid, &title, "kbcal").await {
                tracing::warn!(error = %e, ss = %id, "rename .kbcal (titre) échoué");
            }
        }
    }

    Ok(Json(json!({ "spreadsheet": updated })))
}

pub async fn trash(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query("UPDATE spreadsheets SET is_trashed = TRUE, trashed_at = NOW() WHERE id = $1 AND owner_id = $2")
        .bind(id).bind(user.id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn restore(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query("UPDATE spreadsheets SET is_trashed = FALSE, trashed_at = NULL WHERE id = $1 AND owner_id = $2")
        .bind(id).bind(user.id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query("DELETE FROM spreadsheets WHERE id = $1 AND owner_id = $2")
        .bind(id).bind(user.id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── Sheets ────────────────────────────────────────────────────────────────────

pub async fn get_sheet(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((ss_id, sheet_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    let ss = require_ss_access(&state, ss_id, user.id).await?;

    let sheet: SpreadsheetSheet = sqlx::query_as::<_, SpreadsheetSheet>(
        "SELECT id, spreadsheet_id, name, position, created_at, updated_at FROM spreadsheet_sheets WHERE id = $1 AND spreadsheet_id = $2",
    )
    .bind(sheet_id).bind(ss_id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Feuille introuvable".into()))?;

    let content_file_id = active_content_file_id(&ss)
        .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Spreadsheet has no content file")))?;
    let file_content = cf::read_content(&state, ss.owner_id, content_file_id).await?;
    let sheet_data   = cf::get_sheet_data(&file_content, sheet_id);
    let names = file_content.get("names").cloned().unwrap_or_else(|| json!({}));

    Ok(Json(json!({ "sheet": sheet, "data": sheet_data, "names": names })))
}

pub async fn update_sheet(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((ss_id, sheet_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateSheetDto>,
) -> Result<Json<Value>> {
    let ss = require_ss_access(&state, ss_id, user.id).await?;

    let mut meta_changed = false;
    let sheet: SpreadsheetSheet = sqlx::query_as::<_, SpreadsheetSheet>(
        "SELECT id, spreadsheet_id, name, position, created_at, updated_at FROM spreadsheet_sheets WHERE id = $1 AND spreadsheet_id = $2",
    )
    .bind(sheet_id).bind(ss_id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Feuille introuvable".into()))?;

    let content_file_id = ensure_content_file(&state, &ss, user.id, ss_id, "spreadsheet").await?;

    // Update content in file
    let has_content_update = dto.data.is_some() || dto.col_widths.is_some()
        || dto.row_heights.is_some() || dto.frozen_rows.is_some() || dto.frozen_cols.is_some() || dto.merges.is_some() || dto.gridlines.is_some() || dto.images.is_some() || dto.equations.is_some() || dto.charts.is_some();

    let out_data = if has_content_update {
        let mut file_content = cf::read_content(&state, ss.owner_id, content_file_id).await?;
        let mut sheet_data   = cf::get_sheet_data(&file_content, sheet_id);

        // `dto.data` arrive sous la forme SheetData `{ cells: {...} }` — on en
        // extrait la map pour éviter un double-emboîtement `cells.cells`.
        // Les styles ligne/colonne (formatage « colonne entière ») voyagent dans
        // le même blob : on les extrait avant de déplacer `d` dans `cells`.
        if let Some(d)  = dto.data        {
            if let Some(cs) = d.get("colStyles") { sheet_data["col_styles"] = cs.clone(); }
            if let Some(rs) = d.get("rowStyles") { sheet_data["row_styles"] = rs.clone(); }
            sheet_data["cells"] = d.get("cells").cloned().unwrap_or(d);
        }
        if let Some(cw) = dto.col_widths  { sheet_data["col_widths"]  = cw; }
        if let Some(rh) = dto.row_heights { sheet_data["row_heights"] = rh; }
        if let Some(fr) = dto.frozen_rows { sheet_data["frozen_rows"] = json!(fr); }
        if let Some(fc) = dto.frozen_cols { sheet_data["frozen_cols"] = json!(fc); }
        if let Some(mg) = dto.merges      { sheet_data["merges"]      = mg; }
        if let Some(gl) = dto.gridlines   { sheet_data["gridlines"]   = json!(gl); }
        if let Some(im) = dto.images      { sheet_data["images"]      = im; }
        if let Some(eq) = dto.equations   { sheet_data["equations"]   = eq; }
        if let Some(ch) = dto.charts      { sheet_data["charts"]      = ch; }

        cf::set_sheet_data(&mut file_content, sheet_id, sheet_data.clone());
        cf::write_content_mirrored(&state, ss.owner_id, content_file_id, ss.file_id, &file_content).await?;

        state.sheet_hub.publish(sheet_id, crate::state::SheetMessage::SheetUpdated {
            user_id:  user.id,
            sheet_id,
            data:     sheet_data.clone(),
        }).await;

        sheet_data
    } else {
        let file_content = cf::read_content(&state, ss.owner_id, content_file_id).await?;
        cf::get_sheet_data(&file_content, sheet_id)
    };

    // Update sheet metadata if needed
    let updated_sheet = if let Some(ref name) = dto.name {
        meta_changed = true;
        sqlx::query_as::<_, SpreadsheetSheet>(
            "UPDATE spreadsheet_sheets SET name = $1 WHERE id = $2 AND spreadsheet_id = $3
             RETURNING id, spreadsheet_id, name, position, created_at, updated_at",
        )
        .bind(name).bind(sheet_id).bind(ss_id).fetch_one(&state.db).await?
    } else if let Some(pos) = dto.position {
        meta_changed = true;
        sqlx::query_as::<_, SpreadsheetSheet>(
            "UPDATE spreadsheet_sheets SET position = $1 WHERE id = $2 AND spreadsheet_id = $3
             RETURNING id, spreadsheet_id, name, position, created_at, updated_at",
        )
        .bind(pos).bind(sheet_id).bind(ss_id).fetch_one(&state.db).await?
    } else {
        sheet
    };

    let _ = meta_changed;
    Ok(Json(json!({ "sheet": updated_sheet, "data": out_data })))
}

pub async fn create_sheet(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(ss_id): Path<Uuid>,
    Json(dto): Json<CreateSheetDto>,
) -> Result<Json<Value>> {
    let ss = require_ss_access(&state, ss_id, user.id).await?;

    // `position` is INT4, so MAX(position) comes back as INT4 — decode as i32.
    let (max_pos,): (Option<i32>,) = sqlx::query_as::<_, (Option<i32>,)>(
        "SELECT MAX(position) FROM spreadsheet_sheets WHERE spreadsheet_id = $1",
    )
    .bind(ss_id).fetch_one(&state.db).await?;
    let position    = max_pos.unwrap_or(-1) + 1;
    let sheet_count = max_pos.unwrap_or(-1) + 2;
    let name        = dto.name.unwrap_or_else(|| format!("Feuille {sheet_count}"));

    let sheet: SpreadsheetSheet = sqlx::query_as::<_, SpreadsheetSheet>(
        r#"INSERT INTO spreadsheet_sheets (spreadsheet_id, name, position)
           VALUES ($1, $2, $3)
           RETURNING id, spreadsheet_id, name, position, created_at, updated_at"#,
    )
    .bind(ss_id).bind(&name).bind(position).fetch_one(&state.db).await?;

    let content_file_id = ensure_content_file(&state, &ss, user.id, ss_id, "spreadsheet").await?;
    let mut file_content = cf::read_content(&state, ss.owner_id, content_file_id).await?;
    cf::set_sheet_data(&mut file_content, sheet.id, cf::empty_sheet_data());
    cf::write_content_mirrored(&state, ss.owner_id, content_file_id, ss.file_id, &file_content).await?;

    Ok(Json(json!({ "sheet": sheet, "data": cf::empty_sheet_data() })))
}

pub async fn delete_sheet(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((ss_id, sheet_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    let ss = require_ss_access(&state, ss_id, user.id).await?;

    let (count,): (i64,) = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM spreadsheet_sheets WHERE spreadsheet_id = $1",
    )
    .bind(ss_id).fetch_one(&state.db).await?;
    if count <= 1 {
        return Err(OfficeError::Validation("Impossible de supprimer la dernière feuille".into()));
    }

    sqlx::query("DELETE FROM spreadsheet_sheets WHERE id = $1 AND spreadsheet_id = $2")
        .bind(sheet_id).bind(ss_id).execute(&state.db).await?;

    // Remove sheet data from content file
    let content_file_id = active_content_file_id(&ss);
    if let Some(fid) = content_file_id {
        if let Ok(mut fc) = cf::read_content(&state, ss.owner_id, fid).await {
            cf::remove_sheet_data(&mut fc, sheet_id);
            let _ = cf::write_content(&state, ss.owner_id, fid, &fc).await;
        }
    }

    Ok(Json(json!({ "ok": true })))
}

// ── Versions ─────────────────────────────────────────────────────────────────

pub async fn create_version(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(ss_id): Path<Uuid>,
    Json(dto): Json<CreateVersionDto>,
) -> Result<Json<Value>> {
    let ss = require_ss_access(&state, ss_id, user.id).await?;

    let content_file_id = active_content_file_id(&ss)
        .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Spreadsheet has no content file")))?;
    let file_content = cf::read_content(&state, ss.owner_id, content_file_id).await?;

    let version: SpreadsheetVersion = sqlx::query_as::<_, SpreadsheetVersion>(
        r#"INSERT INTO spreadsheet_versions (spreadsheet_id, author_id, snapshot, label)
           VALUES ($1, $2, $3, $4)
           RETURNING id, spreadsheet_id, author_id, snapshot, label, created_at"#,
    )
    .bind(ss_id).bind(user.id).bind(&file_content).bind(&dto.label)
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "version": version })))
}

pub async fn list_versions(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(ss_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_ss_access(&state, ss_id, user.id).await?;

    let versions: Vec<SpreadsheetVersion> = sqlx::query_as::<_, SpreadsheetVersion>(
        "SELECT id, spreadsheet_id, author_id, snapshot, label, created_at FROM spreadsheet_versions WHERE spreadsheet_id = $1 ORDER BY created_at DESC LIMIT 50",
    )
    .bind(ss_id).fetch_all(&state.db).await?;

    Ok(Json(json!({ "versions": versions })))
}

pub async fn duplicate(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let source: Spreadsheet = sqlx::query_as::<_, Spreadsheet>(
        "SELECT id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at FROM spreadsheets WHERE id = $1 AND owner_id = $2 AND is_trashed = FALSE",
    )
    .bind(id).bind(user.id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Tableur introuvable".into()))?;

    let sheets: Vec<SpreadsheetSheet> = sqlx::query_as::<_, SpreadsheetSheet>(
        "SELECT id, spreadsheet_id, name, position, created_at, updated_at FROM spreadsheet_sheets WHERE spreadsheet_id = $1 ORDER BY position",
    )
    .bind(id).fetch_all(&state.db).await?;

    // Read source content
    let src_content_id = source.file_id
        .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Source spreadsheet has no content file")))?;
    let src_file_content = cf::read_content(&state, source.owner_id, src_content_id).await?;

    let new_title = format!("{} (copie)", source.title);
    // Copie aussi les macros « container-bound » → elles voyagent avec le classeur.
    let new_id: Uuid = sqlx::query_scalar(
        "INSERT INTO spreadsheets (owner_id, title, macros)
         SELECT $1, $2, macros FROM spreadsheets WHERE id = $3
         RETURNING id",
    )
    .bind(user.id).bind(&new_title).bind(id).fetch_one(&state.db).await?;

    // Insert new sheets with remapped IDs
    let mut new_file_content = json!({ "version": 1, "sheets": {} });
    let mut first_sheet_id = None;

    for sheet in &sheets {
        let new_sheet: SpreadsheetSheet = sqlx::query_as::<_, SpreadsheetSheet>(
            r#"INSERT INTO spreadsheet_sheets (spreadsheet_id, name, position)
               VALUES ($1, $2, $3)
               RETURNING id, spreadsheet_id, name, position, created_at, updated_at"#,
        )
        .bind(new_id).bind(&sheet.name).bind(sheet.position)
        .fetch_one(&state.db).await?;

        let sheet_data = cf::get_sheet_data(&src_file_content, sheet.id);
        cf::set_sheet_data(&mut new_file_content, new_sheet.id, sheet_data);
        if first_sheet_id.is_none() { first_sheet_id = Some(new_sheet.id); }
    }

    // Create content file for the duplicate
    let folder = state.files_client
        .ensure_folder_path(user.id, "Office/Spreadsheets", true, Some("Table")).await
        .map_err(|e| OfficeError::Internal(e))?;
    let raw = serde_json::to_vec(&new_file_content)
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;
    let file = state.files_client.create_file_with_content(
        user.id, Some(folder.id),
        &cf::kb_file_name(&new_title, "kbcal"),
        "application/vnd.kubuno.spreadsheet+json",
        bytes::Bytes::from(cf::gzip(&raw)?),
        Some(json!({ "module": "office", "subtype": "spreadsheet" })),
        false,
    ).await.map_err(|e| OfficeError::Internal(e))?;

    sqlx::query("UPDATE spreadsheets SET file_id = $1 WHERE id = $2")
        .bind(file.id).bind(new_id).execute(&state.db).await?;

    Ok(Json(json!({ "id": new_id })))
}

// ── open-by-file ──────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct OpenByFileDto {
    pub file_id: Uuid,
}

pub async fn open_by_file(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<OpenByFileDto>,
) -> Result<Json<Value>> {
    if let Some(ss) = sqlx::query_as::<_, Spreadsheet>(
        "SELECT id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at FROM spreadsheets WHERE file_id = $1 AND owner_id = $2 AND is_trashed = FALSE",
    )
    .bind(dto.file_id).bind(user.id).fetch_optional(&state.db).await? {
        return Ok(Json(json!({ "spreadsheet": ss })));
    }

    let (file_info, content_bytes) = state.files_client
        .get_file_content(user.id, dto.file_id).await.map_err(anyhow::Error::from)?;

    let name_lower = file_info.name.to_lowercase();
    let is_xlsx = file_info.mime_type.contains("spreadsheetml.sheet") || name_lower.ends_with(".xlsx");
    let is_ods  = file_info.mime_type.contains("opendocument.spreadsheet") || name_lower.ends_with(".ods");

    // Normalise every supported format to a common shape:
    //   sheets: [(name, cells, col_widths, row_heights, merges)] + workbook names.
    struct SheetImport { name: String, cells: Value, col_widths: Value, row_heights: Value, merges: Value, cf: Value, gridlines: bool, default_row_height: Value, default_col_width: Value, images: Value, charts: Value }
    let (ext, sheets, names): (&str, Vec<SheetImport>, Vec<(String, String)>) = if is_xlsx {
        let wb = import_xlsx(&content_bytes).map_err(OfficeError::Internal)?;
        let sheets = wb.sheets.into_iter().map(|s| SheetImport {
            name:        s.name,
            cells:       json!(s.cells),
            col_widths:  json!(s.col_widths),
            row_heights: json!(s.row_heights),
            merges:      json!(s.merges),
            cf:          json!(s.cond_formats),
            gridlines:   s.show_gridlines,
            default_row_height: json!(s.default_row_height),
            default_col_width: json!(s.default_col_width),
            images:      json!(s.images),
            charts:      json!(s.charts),
        }).collect();
        (".xlsx", sheets, wb.defined_names)
    } else if is_ods {
        let sheets = import_ods(&content_bytes)
            .map_err(|e| OfficeError::Internal(e.into()))?
            .into_iter().map(|(name, cells)| SheetImport {
                name, cells: json!(cells), col_widths: json!({}), row_heights: json!({}), merges: json!([]), cf: json!([]), gridlines: true, default_row_height: json!(null), default_col_width: json!(null), images: json!([]), charts: json!([]),
            }).collect();
        (".ods", sheets, Vec::new())
    } else {
        return Err(OfficeError::Validation("Format non supporté (attendu : .xlsx ou .ods)".into()));
    };

    let base  = file_info.name.trim_end_matches(ext);
    let title = if base.is_empty() { "Tableur importé".to_string() } else { base.to_string() };

    // Workbook-level defined names (uppercased keys, matching the engine).
    let names_obj: serde_json::Map<String, Value> = names.into_iter()
        .map(|(n, def)| (n.to_uppercase(), Value::String(def))).collect();

    let mut tx = state.db.begin().await?;
    let ss: Spreadsheet = sqlx::query_as::<_, Spreadsheet>(
        "INSERT INTO spreadsheets (owner_id, title) VALUES ($1, $2) RETURNING id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at",
    )
    .bind(user.id).bind(&title).fetch_one(&mut *tx).await?;

    let mut file_content = json!({ "version": 1, "sheets": {}, "names": Value::Object(names_obj) });
    for (pos, s) in sheets.iter().enumerate() {
        let sheet: SpreadsheetSheet = sqlx::query_as::<_, SpreadsheetSheet>(
            "INSERT INTO spreadsheet_sheets (spreadsheet_id, name, position) VALUES ($1, $2, $3) RETURNING id, spreadsheet_id, name, position, created_at, updated_at",
        )
        .bind(ss.id).bind(&s.name).bind(pos as i32).fetch_one(&mut *tx).await?;

        cf::set_sheet_data(&mut file_content, sheet.id, json!({
            "cells":       s.cells,
            "col_widths":  s.col_widths,
            "row_heights": s.row_heights,
            "frozen_rows": 0,
            "frozen_cols": 0,
            "merges":      s.merges,
            "cf":          s.cf,
            "gridlines":   s.gridlines,
            "default_row_height": s.default_row_height,
            "default_col_width": s.default_col_width,
            "images":      s.images,
            "charts":      s.charts,
        }));
    }
    tx.commit().await?;

    let folder = state.files_client
        .ensure_folder_path(user.id, "Office/Spreadsheets", true, Some("Table")).await
        .map_err(|e| OfficeError::Internal(e))?;
    let raw = serde_json::to_vec(&file_content)
        .map_err(|e| OfficeError::Internal(anyhow::anyhow!(e)))?;
    let file = state.files_client.create_file_with_content(
        user.id, Some(folder.id), &cf::kb_file_name(&title, "kbcal"),
        "application/vnd.kubuno.spreadsheet+json", bytes::Bytes::from(cf::gzip(&raw)?),
        Some(json!({ "module": "office", "subtype": "spreadsheet" })),
        false,
    ).await.map_err(|e| OfficeError::Internal(e))?;

    sqlx::query("UPDATE spreadsheets SET file_id = $1 WHERE id = $2")
        .bind(file.id).bind(ss.id).execute(&state.db).await?;

    let ss = sqlx::query_as::<_, Spreadsheet>(
        "SELECT id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at FROM spreadsheets WHERE id = $1",
    ).bind(ss.id).fetch_one(&state.db).await?;

    Ok(Json(json!({ "spreadsheet": ss })))
}

// ── Editing session (draft + presence) ───────────────────────────────────────

pub async fn join_editing(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let ss = sqlx::query_as::<_, Spreadsheet>(
        "SELECT id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at FROM spreadsheets WHERE id = $1 AND owner_id = $2",
    ).bind(id).bind(user.id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Tableur introuvable".into()))?;

    let draft_id = ensure_content_file(&state, &ss, user.id, id, "spreadsheet").await?;

    sqlx::query(
        r#"INSERT INTO editing_sessions (entity_type, entity_id, user_id, display_name)
           VALUES ('spreadsheet', $1, $2, $3)
           ON CONFLICT (entity_type, entity_id, user_id) DO UPDATE SET last_ping_at = NOW()"#,
    )
    .bind(id).bind(user.id).bind(format!("{}", user.id))
    .execute(&state.db).await?;

    state.sheet_hub.publish(id, crate::state::SheetMessage::PresenceChange {
        user_id: user.id,
        action:  "join".to_string(),
    }).await;

    let file_content = cf::read_content(&state, ss.owner_id, draft_id).await?;
    let editors = get_editing_sessions(&state, "spreadsheet", id).await?;
    Ok(Json(json!({ "content": file_content, "editors": editors })))
}

pub async fn save_editing(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let ss = sqlx::query_as::<_, Spreadsheet>(
        "SELECT id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at FROM spreadsheets WHERE id = $1 AND owner_id = $2",
    ).bind(id).bind(user.id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Tableur introuvable".into()))?;

    let draft_id = match ss.draft_file_id { Some(d) => d, None => return Ok(Json(json!({ "ok": true }))) };
    let main_id  = ss.file_id.ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("no main file")))?;
    let content  = cf::read_content(&state, ss.owner_id, draft_id).await?;
    cf::write_content(&state, ss.owner_id, main_id, &content).await?;

    Ok(Json(json!({ "ok": true })))
}

pub async fn leave_editing(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let _ = save_ss_draft_internal(&state, user.id, id).await;

    sqlx::query("DELETE FROM editing_sessions WHERE entity_type = 'spreadsheet' AND entity_id = $1 AND user_id = $2")
        .bind(id).bind(user.id).execute(&state.db).await?;

    state.sheet_hub.publish(id, crate::state::SheetMessage::PresenceChange {
        user_id: user.id,
        action:  "leave".to_string(),
    }).await;

    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM editing_sessions WHERE entity_type = 'spreadsheet' AND entity_id = $1",
    ).bind(id).fetch_one(&state.db).await?;

    if remaining == 0 {
        cleanup_ss_draft(&state, user.id, id).await;
    }

    Ok(Json(json!({ "ok": true })))
}

pub async fn ping_editing(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query("UPDATE editing_sessions SET last_ping_at = NOW() WHERE entity_type = 'spreadsheet' AND entity_id = $1 AND user_id = $2")
        .bind(id).bind(user.id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn active_content_file_id(ss: &Spreadsheet) -> Option<Uuid> {
    ss.draft_file_id.or(ss.file_id)
}

async fn require_ss_owner(state: &AppState, ss_id: Uuid, user_id: Uuid) -> Result<Spreadsheet> {
    sqlx::query_as::<_, Spreadsheet>(
        "SELECT id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at FROM spreadsheets WHERE id = $1 AND owner_id = $2",
    )
    .bind(ss_id).bind(user_id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Tableur introuvable".into()))
}

/// Accès au tableur : propriétaire OU collaborateur (partage user-à-user).
async fn require_ss_access(state: &AppState, ss_id: Uuid, user_id: Uuid) -> Result<Spreadsheet> {
    sqlx::query_as::<_, Spreadsheet>(
        r#"SELECT s.id, s.owner_id, s.title, s.file_id, s.draft_file_id, s.is_starred, s.is_trashed,
                  s.trashed_at, s.created_at, s.updated_at
           FROM spreadsheets s
           WHERE s.id = $1 AND (s.owner_id = $2 OR EXISTS (
               SELECT 1 FROM spreadsheet_collaborators c WHERE c.spreadsheet_id = $1 AND c.user_id = $2
           ))"#,
    )
    .bind(ss_id).bind(user_id).fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Tableur introuvable".into()))
}

async fn ensure_content_file(
    state:       &AppState,
    ss:          &Spreadsheet,
    user_id:     Uuid,
    ss_id:       Uuid,
    entity_type: &str,
) -> Result<Uuid> {
    if let Some(d) = ss.draft_file_id { return Ok(d); }
    // No draft yet — create one from main
    let main_id = ss.file_id.ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("Spreadsheet has no content file")))?;
    let content = cf::read_content(state, ss.owner_id, main_id).await?;
    let draft_id = cf::create_draft_file(state, user_id, entity_type, ss_id, &content).await?;
    sqlx::query("UPDATE spreadsheets SET draft_file_id = $1 WHERE id = $2")
        .bind(draft_id).bind(ss_id).execute(&state.db).await?;
    Ok(draft_id)
}

async fn save_ss_draft_internal(state: &AppState, user_id: Uuid, ss_id: Uuid) -> std::result::Result<(), ()> {
    let ss = sqlx::query_as::<_, Spreadsheet>(
        "SELECT id, owner_id, title, file_id, draft_file_id, is_starred, is_trashed, trashed_at, created_at, updated_at FROM spreadsheets WHERE id = $1 AND owner_id = $2",
    ).bind(ss_id).bind(user_id).fetch_optional(&state.db).await.ok().flatten().ok_or(())?;
    let draft_id = ss.draft_file_id.ok_or(())?;
    let main_id  = ss.file_id.ok_or(())?;
    let content  = cf::read_content(state, ss.owner_id, draft_id).await.map_err(|_| ())?;
    cf::write_content(state, ss.owner_id, main_id, &content).await.map_err(|_| ())?;
    Ok(())
}

async fn cleanup_ss_draft(state: &AppState, user_id: Uuid, ss_id: Uuid) {
    if let Ok(Some(Some(fid))) = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT draft_file_id FROM spreadsheets WHERE id = $1 AND owner_id = $2",
    ).bind(ss_id).bind(user_id).fetch_optional(&state.db).await {
        let _ = state.files_client.delete_file(user_id, fid).await;
        let _ = sqlx::query("UPDATE spreadsheets SET draft_file_id = NULL WHERE id = $1")
            .bind(ss_id).execute(&state.db).await;
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

// Suppress unused import warning
#[allow(unused_imports)]
use crate::converters::ods::export_ods;
