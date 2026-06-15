use axum::{
    body::Bytes,
    extract::{Path, State},
    Extension, Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    state::AppState,
};

pub async fn upload_thumbnail(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    body: Bytes,
) -> Result<Json<Value>> {
    // Vérifier que le board appartient à l'utilisateur
    let exists: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM office_wb.boards WHERE id = $1 AND owner_id = $2",
    )
    .bind(id).bind(user.id)
    .fetch_optional(&state.db).await?;

    if exists.is_none() {
        return Err(OfficeError::NotFound(format!("Tableau {id} introuvable")));
    }

    // Stocker la miniature — on la stocke en base comme BYTEA pour simplifier
    // (les thumbnails sont petites, <50KB typiquement)
    let thumbnail_data = body.to_vec();
    let thumbnail_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &thumbnail_data);
    let path = format!("data:image/png;base64,{thumbnail_b64}");

    sqlx::query(
        "UPDATE office_wb.boards SET thumbnail_path = $2 WHERE id = $1",
    )
    .bind(id).bind(&path)
    .execute(&state.db).await?;

    Ok(Json(json!({ "thumbnail_path": path })))
}
