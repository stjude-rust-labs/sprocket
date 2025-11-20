//! API error types.

use axum::Json;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use serde::Serialize;

use crate::execution::ConfigError;
use crate::execution::ManagerError;

/// Internal server error message.
const INTERNAL_ERROR_MESSAGE: &str =
    "an internal server error occurred; contact the system administrator for more information";

/// API error response.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error kind.
    pub kind: String,
    /// Error message.
    pub message: String,
}

/// API error type.
#[derive(Debug)]
pub enum Error {
    /// A "bad request" error (`400`).
    BadRequest(String),

    /// A "forbidden" error (`403`).
    Forbidden(String),

    /// A "not found" error (`404`).
    NotFound(String),

    /// A "conflict" error (`409`).
    Conflict(String),

    /// An "internal server" error (`500`).
    Internal,
}

impl From<ManagerError> for Error {
    fn from(err: ManagerError) -> Self {
        match err {
            ManagerError::RunNotFound => Self::NotFound(err.to_string()),
            ManagerError::TargetNotFound(_) => Self::NotFound(err.to_string()),
            ManagerError::TargetRequired => Self::BadRequest(err.to_string()),
            ManagerError::NoExecutableTarget => Self::BadRequest(err.to_string()),
            ManagerError::Analysis(_) => Self::BadRequest(err.to_string()),
            ManagerError::Config(config_err) => match config_err {
                ConfigError::FileNotFound(_) => Self::BadRequest(config_err.to_string()),
                ConfigError::FilePathForbidden(_) => Self::Forbidden(config_err.to_string()),
                ConfigError::UrlForbidden(_) => Self::Forbidden(config_err.to_string()),
                ConfigError::FailedToCanonicalize(_) => Self::Internal,
                ConfigError::InvalidUtf8(_) => Self::BadRequest(config_err.to_string()),
            },
            ManagerError::CannotCancel(_) => Self::Conflict(err.to_string()),
            ManagerError::Database(_) => Self::Internal,
            ManagerError::Io(_) => Self::Internal,
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, kind, message) = match self {
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BadRequest", msg),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, "Forbidden", msg),
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, "NotFound", msg),
            Self::Conflict(msg) => (StatusCode::CONFLICT, "Conflict", msg),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal",
                String::from(INTERNAL_ERROR_MESSAGE),
            ),
        };

        let body = Json(ErrorResponse {
            kind: kind.to_string(),
            message,
        });

        (status, body).into_response()
    }
}
