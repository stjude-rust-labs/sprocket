//! Server metadata API handlers.
//!
//! Exposes static information about the running server (the values that are
//! configured at startup and do not change for the server's lifetime). Clients
//! use this endpoint to adapt their behavior to the server's configuration —
//! for example, the `dev server cancel` CLI command queries the failure mode
//! to decide whether to print a "slow-cancel" advisory.

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde::Serialize;
use utoipa::ToSchema;

use super::AppState;

/// The cancellation behavior the server is configured to use.
///
/// Mirrors [`wdl::engine::config::FailureMode`] but is defined separately so
/// the server controls its own wire format and the engine crate's surface
/// stays untouched.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ServerFailureMode {
    /// Cancellation waits for outstanding tasks to complete before marking the
    /// run as canceled.
    Slow,
    /// Cancellation immediately attempts to cancel outstanding tasks.
    Fast,
}

impl From<wdl::engine::config::FailureMode> for ServerFailureMode {
    fn from(mode: wdl::engine::config::FailureMode) -> Self {
        match mode {
            wdl::engine::config::FailureMode::Slow => Self::Slow,
            wdl::engine::config::FailureMode::Fast => Self::Fast,
        }
    }
}

/// The response for a "server info" query.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServerInfoResponse {
    /// The cancellation failure mode the server is configured to use.
    pub failure_mode: ServerFailureMode,
    /// The absolute path to the server's output directory, after shell
    /// expansion.
    ///
    /// Clients join run-relative paths (from `Run.directory`) against this to
    /// produce user-visible absolute paths.
    pub output_dir: String,
}

/// Get static metadata about the running server.
#[utoipa::path(
    get,
    path = super::paths::SERVER_INFO,
    responses(
        (status = 200, description = "Server info retrieved", body = ServerInfoResponse),
    ),
    tag = "server"
)]
pub async fn get_server_info(State(state): State<AppState>) -> Json<ServerInfoResponse> {
    Json(ServerInfoResponse {
        failure_mode: state.failure_mode,
        output_dir: state.output_dir.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_engine_failure_mode_maps_each_variant() {
        assert_eq!(
            ServerFailureMode::from(wdl::engine::config::FailureMode::Slow),
            ServerFailureMode::Slow
        );
        assert_eq!(
            ServerFailureMode::from(wdl::engine::config::FailureMode::Fast),
            ServerFailureMode::Fast
        );
    }

    #[test]
    fn serializes_with_lowercase_tag() {
        let json = serde_json::to_string(&ServerInfoResponse {
            failure_mode: ServerFailureMode::Slow,
            output_dir: "/var/sprocket/runs".to_string(),
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"failure_mode":"slow","output_dir":"/var/sprocket/runs"}"#
        );

        let json = serde_json::to_string(&ServerInfoResponse {
            failure_mode: ServerFailureMode::Fast,
            output_dir: "/var/sprocket/runs".to_string(),
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"failure_mode":"fast","output_dir":"/var/sprocket/runs"}"#
        );
    }
}
