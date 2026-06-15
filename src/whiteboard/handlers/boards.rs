use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    state::AppState,
    whiteboard::models::board::*,
};

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Query(q): Query<ListBoardsQuery>,
) -> Result<Json<Value>> {
    let trashed = q.trashed.unwrap_or(false);
    let limit   = q.limit.unwrap_or(50).min(200);
    let offset  = q.offset.unwrap_or(0);

    let rows: Vec<Board> = if let Some(ref search) = q.search {
        sqlx::query_as::<_, Board>(
            r#"SELECT id, owner_id, title, description, thumbnail_path, share_token,
                      is_public, background, collaborators, element_count, frame_count,
                      is_trashed, trashed_at, last_edited_at, last_edited_by, is_starred,
                      created_at, updated_at
               FROM office_wb.boards
               WHERE owner_id = $1 AND is_trashed = $2 AND title ILIKE $3
               ORDER BY updated_at DESC LIMIT $4 OFFSET $5"#,
        )
        .bind(user.id).bind(trashed).bind(format!("%{search}%")).bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    } else if q.starred.unwrap_or(false) {
        sqlx::query_as::<_, Board>(
            r#"SELECT id, owner_id, title, description, thumbnail_path, share_token,
                      is_public, background, collaborators, element_count, frame_count,
                      is_trashed, trashed_at, last_edited_at, last_edited_by, is_starred,
                      created_at, updated_at
               FROM office_wb.boards
               WHERE owner_id = $1 AND is_starred = TRUE AND is_trashed = FALSE
               ORDER BY updated_at DESC LIMIT $2 OFFSET $3"#,
        )
        .bind(user.id).bind(limit).bind(offset).fetch_all(&state.db).await?
    } else {
        sqlx::query_as::<_, Board>(
            r#"SELECT id, owner_id, title, description, thumbnail_path, share_token,
                      is_public, background, collaborators, element_count, frame_count,
                      is_trashed, trashed_at, last_edited_at, last_edited_by, is_starred,
                      created_at, updated_at
               FROM office_wb.boards
               WHERE owner_id = $1 AND is_trashed = $2
               ORDER BY updated_at DESC LIMIT $3 OFFSET $4"#,
        )
        .bind(user.id).bind(trashed).bind(limit).bind(offset).fetch_all(&state.db).await?
    };

    Ok(Json(json!({ "boards": rows, "total": rows.len() })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<CreateBoardDto>,
) -> Result<Json<Value>> {
    let title      = dto.title.unwrap_or_else(|| "Nouveau tableau".to_string());
    let background = dto.background.as_deref().unwrap_or("dots");

    let board: Board = sqlx::query_as::<_, Board>(
        r#"INSERT INTO office_wb.boards (owner_id, title, description, background)
           VALUES ($1, $2, $3, $4)
           RETURNING id, owner_id, title, description, thumbnail_path, share_token,
                     is_public, background, collaborators, element_count, frame_count,
                     is_trashed, trashed_at, last_edited_at, last_edited_by, is_starred,
                     created_at, updated_at"#,
    )
    .bind(user.id)
    .bind(&title)
    .bind(&dto.description)
    .bind(background)
    .fetch_one(&state.db)
    .await?;

    // Fichier document Yjs (.kbwbd) — créé d'emblée, lié au board.
    if let Ok(file_id) = crate::services::content_files::create_whiteboard_file(&state, user.id, &title).await {
        let _ = sqlx::query("UPDATE office_wb.boards SET file_id = $1 WHERE id = $2")
            .bind(file_id).bind(board.id)
            .execute(&state.db).await;
    }

    Ok(Json(json!({ "board": board })))
}

pub async fn get(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let mut board = fetch_board(&state.db, id, user.id).await?;
    // Titre = nom du fichier .kbwbd (sans extension) ; self-heal si renommé ailleurs.
    if let Some(fid) = board_file_id(&state.db, id, user.id).await {
        if let Some(fname) = crate::services::content_files::file_name(&state, user.id, fid).await {
            let stem = crate::services::content_files::strip_ext(&fname);
            if !stem.is_empty() && stem != board.title {
                sqlx::query("UPDATE office_wb.boards SET title = $2 WHERE id = $1")
                    .bind(id).bind(&stem).execute(&state.db).await?;
                board.title = stem;
            }
        }
    }
    Ok(Json(json!({ "board": board })))
}

#[derive(serde::Deserialize)]
pub struct OpenByFileDto {
    pub file_id: Uuid,
}

pub async fn open_by_file(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<OpenByFileDto>,
) -> Result<Json<Value>> {
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id FROM office_wb.boards
           WHERE file_id = $1 AND owner_id = $2 AND is_trashed = FALSE"#,
    )
    .bind(dto.file_id).bind(user.id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Aucun tableau lié au fichier {}", dto.file_id)))?;

    let board = fetch_board(&state.db, id, user.id).await?;
    Ok(Json(json!({ "board": board })))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateBoardDto>,
) -> Result<Json<Value>> {
    let existing = fetch_board(&state.db, id, user.id).await?;

    let title      = dto.title.as_deref().unwrap_or(&existing.title).to_string();
    let background = dto.background.as_deref().unwrap_or(&existing.background).to_string();
    let is_starred = dto.is_starred.unwrap_or(existing.is_starred);
    let is_public  = dto.is_public.unwrap_or(existing.is_public);
    let element_count = dto.element_count.unwrap_or(existing.element_count);
    let frame_count   = dto.frame_count.unwrap_or(existing.frame_count);

    let board: Board = sqlx::query_as::<_, Board>(
        r#"UPDATE office_wb.boards
           SET title = $3, description = $4, background = $5, is_starred = $6,
               is_public = $7, element_count = $8, frame_count = $9,
               last_edited_at = NOW(), last_edited_by = $10
           WHERE id = $1 AND owner_id = $2
           RETURNING id, owner_id, title, description, thumbnail_path, share_token,
                     is_public, background, collaborators, element_count, frame_count,
                     is_trashed, trashed_at, last_edited_at, last_edited_by, is_starred,
                     created_at, updated_at"#,
    )
    .bind(id).bind(user.id)
    .bind(&title)
    .bind(dto.description.as_deref().or(existing.description.as_deref()))
    .bind(&background)
    .bind(is_starred).bind(is_public)
    .bind(element_count).bind(frame_count)
    .bind(dto.last_edited_by.or(Some(user.id)))
    .fetch_one(&state.db)
    .await?;

    // Titre modifié → renommer le fichier .kbwbd (titre = nom). Best-effort.
    if dto.title.is_some() && !title.trim().is_empty() {
        if let Some(fid) = board_file_id(&state.db, id, user.id).await {
            if let Err(e) = crate::services::content_files::rename_content_file(&state, user.id, fid, &title, "kbwbd").await {
                tracing::warn!(error = %e, board = %id, "rename .kbwbd (titre) échoué");
            }
        }
    }

    Ok(Json(json!({ "board": board })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let affected = sqlx::query(
        "DELETE FROM office_wb.boards WHERE id = $1 AND owner_id = $2",
    )
    .bind(id).bind(user.id)
    .execute(&state.db).await?.rows_affected();

    if affected == 0 {
        return Err(OfficeError::NotFound(format!("Tableau {id} introuvable")));
    }
    Ok(Json(json!({ "deleted": true })))
}

pub async fn trash(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query(
        "UPDATE office_wb.boards SET is_trashed = TRUE, trashed_at = NOW() WHERE id = $1 AND owner_id = $2",
    )
    .bind(id).bind(user.id).execute(&state.db).await?;
    Ok(Json(json!({ "trashed": true })))
}

pub async fn restore(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query(
        "UPDATE office_wb.boards SET is_trashed = FALSE, trashed_at = NULL WHERE id = $1 AND owner_id = $2",
    )
    .bind(id).bind(user.id).execute(&state.db).await?;
    Ok(Json(json!({ "restored": true })))
}

pub async fn duplicate(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let existing = fetch_board(&state.db, id, user.id).await?;
    let new_title = format!("{} (copie)", existing.title);

    let board: Board = sqlx::query_as::<_, Board>(
        r#"INSERT INTO office_wb.boards (owner_id, title, description, background)
           VALUES ($1, $2, $3, $4)
           RETURNING id, owner_id, title, description, thumbnail_path, share_token,
                     is_public, background, collaborators, element_count, frame_count,
                     is_trashed, trashed_at, last_edited_at, last_edited_by, is_starred,
                     created_at, updated_at"#,
    )
    .bind(user.id).bind(&new_title).bind(&existing.description).bind(&existing.background)
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "board": board })))
}

async fn fetch_board(pool: &sqlx::PgPool, id: Uuid, owner_id: Uuid) -> Result<Board> {
    sqlx::query_as::<_, Board>(
        r#"SELECT id, owner_id, title, description, thumbnail_path, share_token,
                  is_public, background, collaborators, element_count, frame_count,
                  is_trashed, trashed_at, last_edited_at, last_edited_by, is_starred,
                  created_at, updated_at
           FROM office_wb.boards WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id).bind(owner_id)
    .fetch_optional(pool).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Tableau {id} introuvable")))
}

/// Identifiant du fichier .kbwbd lié au board (la colonne `file_id` n'est pas dans `Board`).
async fn board_file_id(pool: &sqlx::PgPool, id: Uuid, owner_id: Uuid) -> Option<Uuid> {
    sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT file_id FROM office_wb.boards WHERE id = $1 AND owner_id = $2",
    ).bind(id).bind(owner_id).fetch_optional(pool).await.ok().flatten().flatten()
}
