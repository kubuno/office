use axum::response::IntoResponse;
use axum::http::header;

use crate::script::runtime::api_bridge::KUBUNO_API_TYPES;

/// GET /script/api-types
/// Returns the TypeScript declaration for the Kubuno global namespace.
pub async fn get_types() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        KUBUNO_API_TYPES,
    )
}
