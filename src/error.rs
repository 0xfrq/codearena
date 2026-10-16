use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized,
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn bad(msg: impl Into<String>) -> Self {
        AppError::BadRequest(msg.into())
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        AppError::Conflict(msg.into())
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        AppError::Forbidden(msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        AppError::NotFound(msg.into())
    }

    pub fn status(&self) -> StatusCode {
        match self {
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn message(&self) -> String {
        match self {
            AppError::BadRequest(m) | AppError::Forbidden(m) | AppError::NotFound(m) | AppError::Conflict(m) => m.clone(),
            AppError::Unauthorized => "Silakan login terlebih dahulu.".into(),
            AppError::Internal(_) => "Terjadi kesalahan server.".into(),
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Internal(m) => write!(f, "internal: {m}"),
            other => write!(f, "{}", other.message()),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let AppError::Internal(m) = &self {
            tracing::error!("internal error: {m}");
        }
        (self.status(), Json(serde_json::json!({ "ok": false, "error": self.message() }))).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &e {
            let msg = db.message().to_string();
            // Integrity rules enforced by triggers / constraints surface as conflicts.
            if msg.contains("immutable")
                || msg.contains("locked")
                || msg.contains("frozen")
                || msg.contains("cannot be deleted")
                || msg.contains("UNIQUE")
                || msg.contains("CHECK")
            {
                return AppError::Conflict(format!("Ditolak oleh aturan integritas data: {msg}"));
            }
        }
        AppError::Internal(e.to_string())
    }
}

impl From<askama::Error> for AppError {
    fn from(e: askama::Error) -> Self {
        AppError::Internal(format!("template: {e}"))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Internal(format!("json: {e}"))
    }
}
