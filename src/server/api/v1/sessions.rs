//! Session API handlers.

use axum::Json;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::extract::rejection::QueryRejection;
use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use utoipa::IntoParams;
use utoipa::ToSchema;
use uuid::Uuid;

use super::AppState;
use super::SprocketCommand;
use super::error::Error;
use super::send_command;
use crate::system::v1::exec::svc::RunManagerCmd;
use crate::system::v1::exec::svc::run_manager::commands;

/// Query parameters for listing sessions.
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct ListSessionsQueryParams {
    /// Number of results to return (default: `100`).
    #[serde(default)]
    pub limit: Option<i64>,
    /// Number of results to skip (default: `0`).
    #[serde(default)]
    pub offset: Option<i64>,
}

/// Session data for API responses.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Session {
    /// Unique identifier.
    pub uuid: Uuid,
    /// The Sprocket subcommand used to create this session.
    pub subcommand: SprocketCommand,
    /// User or system that created this session.
    pub created_by: String,
    /// Timestamp when the session was created.
    pub created_at: DateTime<Utc>,
}

impl From<crate::system::v1::db::Session> for Session {
    fn from(session: crate::system::v1::db::Session) -> Self {
        Self {
            uuid: session.uuid,
            subcommand: session.subcommand,
            created_by: session.created_by,
            created_at: session.created_at,
        }
    }
}

/// The response for a "get session" query.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SessionResponse {
    /// The session data.
    #[serde(flatten)]
    pub session: Session,
}

impl From<commands::SessionResponse> for SessionResponse {
    fn from(response: commands::SessionResponse) -> Self {
        Self {
            session: response.session.into(),
        }
    }
}

/// The response for a "list sessions" query.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListSessionsResponse {
    /// The sessions.
    pub sessions: Vec<Session>,
}

impl From<commands::ListSessionsResponse> for ListSessionsResponse {
    fn from(response: commands::ListSessionsResponse) -> Self {
        Self {
            sessions: response.sessions.into_iter().map(Into::into).collect(),
        }
    }
}

/// List sessions.
#[utoipa::path(
    get,
    path = "/api/v1/sessions",
    params(ListSessionsQueryParams),
    responses(
        (status = 200, description = "Sessions retrieved successfully", body = ListSessionsResponse),
    ),
    tag = "sessions"
)]
pub async fn list_sessions(
    State(state): State<AppState>,
    query: Result<Query<ListSessionsQueryParams>, QueryRejection>,
) -> Result<Json<ListSessionsResponse>, Error> {
    let Query(query) = query.map_err(|rejection| match rejection {
        QueryRejection::FailedToDeserializeQueryString(err) => {
            Error::BadRequest(format!("invalid query parameters: {}", err))
        }
        _ => Error::BadRequest("invalid query parameters".to_string()),
    })?;

    let response = send_command(&state.run_manager_tx, |rx| RunManagerCmd::ListSessions {
        limit: query.limit,
        offset: query.offset,
        rx,
    })
    .await?;

    Ok(Json(response.into()))
}

/// Get session by ID.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}",
    params(
        ("id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 200, description = "Session found", body = SessionResponse),
        (status = 404, description = "Session not found"),
    ),
    tag = "sessions"
)]
pub async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SessionResponse>, Error> {
    let response = send_command(&state.run_manager_tx, |rx| RunManagerCmd::GetSession {
        id,
        rx,
    })
    .await?;
    Ok(Json(response.into()))
}
