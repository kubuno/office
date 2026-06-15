use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum OfficeError {
    #[error("Non authentifié")]
    Unauthorized,

    #[error("Accès refusé")]
    Forbidden,

    #[error("Ressource introuvable: {0}")]
    NotFound(String),

    #[error("Données invalides: {0}")]
    Validation(String),

    #[error("Conflit: {0}")]
    Conflict(String),

    #[error("Conversion échouée: {0}")]
    Conversion(String),

    #[error("Erreur base de données")]
    Database(#[from] sqlx::Error),

    #[error("Erreur interne")]
    Internal(#[from] anyhow::Error),
}

impl From<zip::result::ZipError> for OfficeError {
    fn from(e: zip::result::ZipError) -> Self {
        OfficeError::Conversion(e.to_string())
    }
}

impl From<std::io::Error> for OfficeError {
    fn from(e: std::io::Error) -> Self {
        OfficeError::Conversion(e.to_string())
    }
}

impl IntoResponse for OfficeError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            OfficeError::Unauthorized  => (StatusCode::UNAUTHORIZED,         "UNAUTHORIZED", self.to_string()),
            OfficeError::Forbidden     => (StatusCode::FORBIDDEN,            "FORBIDDEN",    self.to_string()),
            OfficeError::NotFound(_)   => (StatusCode::NOT_FOUND,            "NOT_FOUND",    self.to_string()),
            OfficeError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION",   self.to_string()),
            OfficeError::Conflict(_)    => (StatusCode::CONFLICT,             "CONFLICT",     self.to_string()),
            OfficeError::Conversion(_)  => (StatusCode::UNPROCESSABLE_ENTITY, "CONVERSION_ERROR", self.to_string()),
            OfficeError::Database(e) => {
                tracing::error!(error = %e, "Database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR", "Erreur base de données".to_string())
            }
            OfficeError::Internal(e) => {
                tracing::error!(error = %e, "Internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", "Erreur interne".to_string())
            }
        };
        (status, Json(json!({ "error": code, "message": message }))).into_response()
    }
}

pub type Result<T> = std::result::Result<T, OfficeError>;
