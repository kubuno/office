use axum::{Extension, extract::State, Json};
use serde_json::{Value, json};
use crate::{errors::OfficeError, middleware::OfficeUser, state::AppState};

/// Dossiers Office (chemin, icône Lucide) — dossiers de module/sous-modules.
const OFFICE_FOLDERS: &[(&str, &str)] = &[
    ("Office",                "Briefcase"),
    ("Office/Documents",      "FileText"),
    ("Office/Spreadsheets",   "Table"),
    ("Office/Presentations",  "Presentation"),
    ("Office/Diagrams",       "Shapes"),
    ("Office/Projects",       "SquareKanban"),
    ("Office/Scripts",        "FileCode"),
];

/// POST /office/ensure-folders
/// Crée (idempotent) toute la hiérarchie de dossiers Office dans Files pour l'utilisateur
/// authentifié, et les marque comme protégés (non-supprimables, non-renommables) avec icône.
pub async fn ensure_user_folders(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
) -> Result<Json<Value>, OfficeError> {
    for (path, icon) in OFFICE_FOLDERS {
        if let Err(e) = state.files_client.ensure_folder_path(user.id, path, true, Some(icon)).await {
            tracing::warn!(user_id = %user.id, path, "ensure_user_folders: {e}");
        }
    }
    Ok(Json(json!({ "ok": true })))
}
