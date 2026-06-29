use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    maths::models::formula::{CreateFormulaDto, Formula, ListFormulasQuery, UpdateFormulaDto},
    services::content_files,
    state::AppState,
};

const DEFAULT_LATEX: &str = "";

// ── List ──────────────────────────────────────────────────────────────────────

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Query(q): Query<ListFormulasQuery>,
) -> Result<Json<Value>> {
    let trashed = q.trashed.unwrap_or(false);
    let limit   = q.limit.unwrap_or(50).min(200);
    let offset  = q.offset.unwrap_or(0);

    let formulas: Vec<Formula> = if let Some(ref search) = q.search {
        sqlx::query_as::<_, Formula>(
            r#"SELECT id, owner_id, name, description, file_id, is_starred, is_trashed, created_at, updated_at
               FROM office_maths.formulas
               WHERE owner_id = $1 AND is_trashed = $2 AND name ILIKE $3
               ORDER BY updated_at DESC LIMIT $4 OFFSET $5"#,
        )
        .bind(user.id).bind(trashed).bind(format!("%{search}%")).bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    } else {
        sqlx::query_as::<_, Formula>(
            r#"SELECT id, owner_id, name, description, file_id, is_starred, is_trashed, created_at, updated_at
               FROM office_maths.formulas
               WHERE owner_id = $1 AND is_trashed = $2
               ORDER BY updated_at DESC LIMIT $3 OFFSET $4"#,
        )
        .bind(user.id).bind(trashed).bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    };

    Ok(Json(json!({ "formulas": formulas, "total": formulas.len() })))
}

// ── Create ────────────────────────────────────────────────────────────────────

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<CreateFormulaDto>,
) -> Result<Json<Value>> {
    let name  = dto.name.unwrap_or_else(|| "Nouvelle formule".to_string());
    let latex = dto.latex.unwrap_or_else(|| DEFAULT_LATEX.to_string());

    // Contenu (LaTeX) → fichier .kbmath.
    let file_id = content_files::create_maths_file(&state, user.id, &name, &latex).await?;

    let mut formula: Formula = sqlx::query_as::<_, Formula>(
        r#"INSERT INTO office_maths.formulas (owner_id, name, description, file_id)
           VALUES ($1, $2, $3, $4)
           RETURNING id, owner_id, name, description, file_id, is_starred, is_trashed, created_at, updated_at"#,
    )
    .bind(user.id).bind(&name).bind(&dto.description).bind(file_id)
    .fetch_one(&state.db).await?;

    formula.latex = latex;
    Ok(Json(json!({ "formula": formula })))
}

// ── Get ───────────────────────────────────────────────────────────────────────

pub async fn get(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let formula = fetch_formula(&state, id, user.id).await?;
    Ok(Json(json!({ "formula": formula })))
}

// ── Update ────────────────────────────────────────────────────────────────────

pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateFormulaDto>,
) -> Result<Json<Value>> {
    let existing = fetch_formula(&state, id, user.id).await?;

    let name        = dto.name.as_deref().unwrap_or(&existing.name).to_string();
    let latex       = dto.latex.as_deref().unwrap_or(&existing.latex).to_string();
    let description = dto.description.or(existing.description);

    // LaTeX modifié → écrit dans le fichier .kbmath (créé si absent).
    let file_id = match existing.file_id {
        Some(fid) => {
            if dto.latex.is_some() {
                content_files::write_maths_formula(&state, user.id, fid, &latex).await?;
            }
            fid
        }
        None => content_files::create_maths_file(&state, user.id, &name, &latex).await?,
    };

    let mut formula: Formula = sqlx::query_as::<_, Formula>(
        r#"UPDATE office_maths.formulas
           SET name = $3, description = $4, file_id = $5,
               is_starred = COALESCE($6, is_starred)
           WHERE id = $1 AND owner_id = $2
           RETURNING id, owner_id, name, description, file_id, is_starred, is_trashed, created_at, updated_at"#,
    )
    .bind(id).bind(user.id).bind(&name).bind(&description).bind(file_id).bind(dto.is_starred)
    .fetch_one(&state.db).await?;

    // Nom modifié → renommer le fichier .kbmath (nom = nom du fichier). Best-effort.
    if let (true, Some(fid)) = (dto.name.is_some(), formula.file_id) {
        if !name.trim().is_empty() {
            if let Err(e) = content_files::rename_content_file(&state, user.id, fid, &name, "kbmath").await {
                tracing::warn!(error = %e, formula = %id, "rename .kbmath (nom) échoué");
            }
        }
    }

    formula.latex = latex;
    Ok(Json(json!({ "formula": formula })))
}

// ── Delete ────────────────────────────────────────────────────────────────────

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let affected = sqlx::query("DELETE FROM office_maths.formulas WHERE id = $1 AND owner_id = $2")
        .bind(id).bind(user.id).execute(&state.db).await?.rows_affected();
    if affected == 0 {
        return Err(OfficeError::NotFound(format!("Formule {id} introuvable")));
    }
    Ok(Json(json!({ "deleted": true })))
}

// ── Trash / Restore ─────────────────────────────────────────────────────────────

pub async fn trash(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let affected = sqlx::query("UPDATE office_maths.formulas SET is_trashed = TRUE WHERE id = $1 AND owner_id = $2")
        .bind(id).bind(user.id).execute(&state.db).await?.rows_affected();
    if affected == 0 {
        return Err(OfficeError::NotFound(format!("Formule {id} introuvable")));
    }
    Ok(Json(json!({ "trashed": true })))
}

pub async fn restore(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query("UPDATE office_maths.formulas SET is_trashed = FALSE WHERE id = $1 AND owner_id = $2")
        .bind(id).bind(user.id).execute(&state.db).await?;
    Ok(Json(json!({ "restored": true })))
}

// ── Duplicate ─────────────────────────────────────────────────────────────────

pub async fn duplicate(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let existing = fetch_formula(&state, id, user.id).await?;

    let new_name = format!("{} (copie)", existing.name);
    let new_file_id = content_files::create_maths_file(&state, user.id, &new_name, &existing.latex).await?;

    let mut formula: Formula = sqlx::query_as::<_, Formula>(
        r#"INSERT INTO office_maths.formulas (owner_id, name, description, file_id)
           VALUES ($1, $2, $3, $4)
           RETURNING id, owner_id, name, description, file_id, is_starred, is_trashed, created_at, updated_at"#,
    )
    .bind(user.id).bind(&new_name).bind(&existing.description).bind(new_file_id)
    .fetch_one(&state.db).await?;

    formula.latex = existing.latex;
    Ok(Json(json!({ "formula": formula })))
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
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id FROM office_maths.formulas
           WHERE file_id = $1 AND owner_id = $2 AND is_trashed = FALSE"#,
    )
    .bind(dto.file_id).bind(user.id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Aucune formule liée au fichier {}", dto.file_id)))?;

    let formula = fetch_formula(&state, id, user.id).await?;
    Ok(Json(json!({ "formula": formula })))
}

// ── Helper ────────────────────────────────────────────────────────────────────

pub async fn fetch_formula(state: &AppState, id: Uuid, owner_id: Uuid) -> Result<Formula> {
    let mut formula = sqlx::query_as::<_, Formula>(
        r#"SELECT id, owner_id, name, description, file_id, is_starred, is_trashed, created_at, updated_at
           FROM office_maths.formulas WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id).bind(owner_id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Formule {id} introuvable")))?;

    // LaTeX lu depuis le fichier .kbmath.
    if let Some(fid) = formula.file_id {
        formula.latex = content_files::read_maths_formula(state, owner_id, fid).await
            .unwrap_or_default();
        // Nom = nom du fichier .kbmath (sans extension) ; self-heal si renommé ailleurs.
        if let Some(fname) = content_files::file_name(state, owner_id, fid).await {
            let stem = content_files::strip_ext(&fname);
            if !stem.is_empty() && stem != formula.name {
                sqlx::query("UPDATE office_maths.formulas SET name = $2 WHERE id = $1")
                    .bind(id).bind(&stem).execute(&state.db).await?;
                formula.name = stem;
            }
        }
    }
    Ok(formula)
}
