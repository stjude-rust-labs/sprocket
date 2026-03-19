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
use super::pagination::decode_token;
use super::pagination::encode_token;
use super::send_command;
use crate::system::v1::db::SessionCursor;
use crate::system::v1::exec::svc::RunManagerCmd;
use crate::system::v1::exec::svc::run_manager::commands;

/// Cursor payload used for session pagination tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionsCursorToken {
    /// Creation time of the last session on the previous page.
    created_at: DateTime<Utc>,
    /// UUID of the last session on the previous page.
    uuid: Uuid,
}

/// Query parameters for listing sessions.
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct ListSessionsQueryParams {
    /// Number of results to return (default: `100`).
    #[serde(default)]
    pub limit: Option<i64>,
    /// Token for pagination. It is expected that clients pass the value from a
    /// previous response to retrieve the next page. Legacy numeric tokens are
    /// not supported.
    #[serde(default)]
    pub next_token: Option<String>,
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
    /// Total count before applying cursor pagination.
    ///
    /// This is retained for compatibility with existing clients.
    pub total: i64,
    /// Next token for pagination. Pass this value as `next_token` in the next
    /// request to retrieve the next page of results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_token: Option<String>,
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

    let cursor = match query.next_token.as_deref() {
        Some(token) => {
            let parsed: SessionsCursorToken = decode_token(token).map_err(Error::BadRequest)?;
            Some(SessionCursor {
                created_at: parsed.created_at,
                uuid: parsed.uuid,
            })
        }
        None => None,
    };

    let limit = query.limit.unwrap_or(100);
    let db_limit = if limit > 0 {
        Some(limit.saturating_add(1))
    } else {
        Some(limit)
    };

    let mut response = send_command(&state.run_manager_tx, |rx| RunManagerCmd::ListSessions {
        limit: db_limit,
        cursor,
        rx,
    })
    .await?;

    let has_more = if limit > 0 && response.sessions.len() > limit as usize {
        response.sessions.truncate(limit as usize);
        true
    } else {
        false
    };

    let next_token = if has_more {
        response.sessions.last().and_then(|session| {
            encode_token(&SessionsCursorToken {
                created_at: session.created_at,
                uuid: session.uuid,
            })
            .ok()
        })
    } else {
        None
    };

    Ok(Json(ListSessionsResponse {
        sessions: response.sessions.into_iter().map(Into::into).collect(),
        total: response.total,
        next_token,
    }))
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
