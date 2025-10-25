//! API error types.

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::Json;
use serde::Serialize;

/// Internal server error message.
const INTERNAL_ERROR_MESSAGE: &str = "an internal server error occurred; contact the system administrator for more information";

/// API error response.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error message.
    pub error: String,
}

/// API error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Workflow not found.
    #[error("workflow not found")]
    WorkflowNotFound,

    /// Invalid request.
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// File sources not allowed.
    #[error("file sources are not allowed")]
    FileSourcesNotAllowed,

    /// File path not allowed.
    #[error("file path is not in allowed paths")]
    FilePathNotAllowed,

    /// File not found.
    #[error("file does not exist: {0}")]
    FileNotFound(String),

    /// Workflow cannot be cancelled.
    #[error("workflow cannot be cancelled (status: {0})")]
    CannotCancel(String),

    /// Internal server error.
    #[error("internal server error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::WorkflowNotFound => (StatusCode::NOT_FOUND, self.to_string()),
            Self::InvalidRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            Self::FileSourcesNotAllowed => (StatusCode::FORBIDDEN, self.to_string()),
            Self::FilePathNotAllowed => (StatusCode::FORBIDDEN, self.to_string()),
            Self::FileNotFound(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            Self::CannotCancel(_) => (StatusCode::CONFLICT, self.to_string()),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, String::from(INTERNAL_ERROR_MESSAGE)),
        };

        let body = Json(ErrorResponse { error: message });
        (status, body).into_response()
    }
}
