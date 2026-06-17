use axum::{
    extract::{Path, State},
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    state::AppState,
};

// Macros « container-bound » : stockées DANS la donnée propre de chaque document
// (elles voyagent avec lui, sont dupliquées/supprimées avec lui). Endpoint générique
// qui dispatch selon le type de document vers le bon stockage. Forme d'une macro :
// { id, name, source }. Le frontend gère le tableau et PUT l'ensemble.

const MAX_MACROS: usize = 200;
const MAX_PAYLOAD_LEN: usize = 2_000_000; // 2 Mo pour l'ensemble des macros/formulaires

// On stocke des objets ARBITRAIRES (modules de code `{id,name,source}` ET formulaires
// `{id,name,kind:'form',controls,source}`) → on ne fige pas le schéma côté serveur :
// chaque entrée doit juste être un objet avec un `id` chaîne. Le reste est préservé.
#[derive(Debug, Deserialize)]
pub struct PutBody {
    pub macros: Vec<Value>,
}

// ── GET /office/doc-macros/:doc_type/:doc_id ──────────────────────────────────

pub async fn get(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((doc_type, doc_id)): Path<(String, Uuid)>,
) -> Result<Json<Value>> {
    let macros: Value = match doc_type.as_str() {
        "spreadsheet" => {
            sqlx::query_scalar::<_, Value>(
                r#"SELECT s.macros FROM spreadsheets s
                   WHERE s.id = $1 AND (s.owner_id = $2 OR EXISTS (
                       SELECT 1 FROM spreadsheet_collaborators c
                       WHERE c.spreadsheet_id = $1 AND c.user_id = $2))"#,
            )
            .bind(doc_id)
            .bind(user.id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| OfficeError::NotFound("Document introuvable".into()))?
        }
        // Autres modules : stockage in-document à venir (rollout) — liste vide pour
        // l'instant afin de ne pas casser leur menu Macros.
        _ => json!([]),
    };
    Ok(Json(json!({ "macros": macros })))
}

// ── PUT /office/doc-macros/:doc_type/:doc_id ──────────────────────────────────

pub async fn put(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((doc_type, doc_id)): Path<(String, Uuid)>,
    Json(body): Json<PutBody>,
) -> Result<Json<Value>> {
    if body.macros.len() > MAX_MACROS {
        return Err(OfficeError::Validation(format!("Trop de macros (max {MAX_MACROS})")));
    }
    for m in &body.macros {
        let id_ok = m.get("id").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false);
        if !m.is_object() || !id_ok {
            return Err(OfficeError::Validation("Macro invalide (id manquant)".into()));
        }
    }
    let macros = Value::Array(body.macros);
    if serde_json::to_string(&macros).map(|s| s.len()).unwrap_or(0) > MAX_PAYLOAD_LEN {
        return Err(OfficeError::Validation("Projet de macros trop volumineux".into()));
    }

    match doc_type.as_str() {
        "spreadsheet" => {
            let affected = sqlx::query(
                r#"UPDATE spreadsheets SET macros = $1
                   WHERE id = $2 AND (owner_id = $3 OR EXISTS (
                       SELECT 1 FROM spreadsheet_collaborators c
                       WHERE c.spreadsheet_id = $2 AND c.user_id = $3))"#,
            )
            .bind(&macros)
            .bind(doc_id)
            .bind(user.id)
            .execute(&state.db)
            .await?
            .rows_affected();
            if affected == 0 {
                return Err(OfficeError::NotFound("Document introuvable".into()));
            }
        }
        other => {
            return Err(OfficeError::Validation(format!(
                "Macros in-document pas encore supportées pour « {other} »"
            )));
        }
    }
    Ok(Json(json!({ "macros": macros })))
}
