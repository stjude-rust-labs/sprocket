//! API error types for v1 endpoints.

use axum::Json;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use serde::Serialize;

use crate::system::v1::db::DatabaseError;
use crate::system::v1::exec::ConfigError;
use crate::system::v1::exec::SelectTargetError;
use crate::system::v1::exec::svc::run_manager::CancelRunError;
use crate::system::v1::exec::svc::run_manager::GetRunError;
use crate::system::v1::exec::svc::run_manager::GetRunOutputsError;
use crate::system::v1::exec::svc::run_manager::GetSessionError;
use crate::system::v1::exec::svc::run_manager::SubmitRunError;

/// The internal server error message.
///
/// This is intentionally vague to discourage leaking information.
const INTERNAL_ERROR_MESSAGE: &str =
    "an internal server error occurred; contact the system administrator for more information";

/// An API error response.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error kind.
    pub kind: String,
    /// Error message.
    pub message: String,
}

/// An API error type.
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

impl From<DatabaseError> for Error {
    fn from(_err: DatabaseError) -> Self {
        Self::Internal
    }
}

impl From<SubmitRunError> for Error {
    fn from(err: SubmitRunError) -> Self {
        match err {
            SubmitRunError::Config(err) => match err {
                ConfigError::FileNotFound(_) => Self::BadRequest(err.to_string()),
                ConfigError::FilePathForbidden(_) => Self::Forbidden(err.to_string()),
                ConfigError::UrlForbidden(_) => Self::Forbidden(err.to_string()),
                ConfigError::FailedToCanonicalize(_) => Self::Internal,
            },
            SubmitRunError::Analysis(err) => Self::BadRequest(err.to_string()),
            SubmitRunError::TargetSelection(err) => match err {
                SelectTargetError::TargetNotFound(_) => Self::NotFound(err.to_string()),
                SelectTargetError::TargetRequired => Self::BadRequest(err.to_string()),
                SelectTargetError::NoExecutableTarget => Self::BadRequest(err.to_string()),
            },
            SubmitRunError::Database(_) => Self::Internal,
            SubmitRunError::Io(_) => Self::Internal,
        }
    }
}

impl From<CancelRunError> for Error {
    fn from(err: CancelRunError) -> Self {
        match err {
            CancelRunError::Database(_) => Self::Internal,
            CancelRunError::NotFound(_) => Self::NotFound(err.to_string()),
            CancelRunError::InvalidStatus { .. } => Self::Conflict(err.to_string()),
        }
    }
}

impl From<GetRunOutputsError> for Error {
    fn from(err: GetRunOutputsError) -> Self {
        match err {
            GetRunOutputsError::Database(_) => Self::Internal,
            GetRunOutputsError::NotFound(_) => Self::NotFound(err.to_string()),
        }
    }
}

impl From<GetRunError> for Error {
    fn from(err: GetRunError) -> Self {
        match err {
            GetRunError::Database(_) => Self::Internal,
            GetRunError::NotFound(_) => Self::NotFound(err.to_string()),
        }
    }
}

impl From<GetSessionError> for Error {
    fn from(err: GetSessionError) -> Self {
        match err {
            GetSessionError::Database(_) => Self::Internal,
            GetSessionError::NotFound(_) => Self::NotFound(err.to_string()),
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
