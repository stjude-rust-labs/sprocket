//! Exposes the server's configuration and whether it is shutting down.
//!
//! Most values are configured at startup and do not change. The `state` field
//! reports whether the server is running or shutting down. Clients use this
//! endpoint to adapt their behavior — for example, the `dev server cancel`
//! CLI command queries the failure mode to decide whether to print a
//! "slow-cancel" advisory.

use std::fmt;

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde::Serialize;
use utoipa::ToSchema;
use wdl::engine::CancellationContextState;

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

/// Whether the server is taking runs or shutting down.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ServerState {
    /// Taking new runs.
    #[default]
    Running,
    /// Shutting down: not taking runs, and waiting for running tasks to finish.
    Draining,
    /// Shutting down: canceling running tasks.
    Canceling,
}

impl From<CancellationContextState> for ServerState {
    fn from(state: CancellationContextState) -> Self {
        match state {
            CancellationContextState::NotCanceled => Self::Running,
            CancellationContextState::Waiting => Self::Draining,
            CancellationContextState::Canceling => Self::Canceling,
        }
    }
}

impl fmt::Display for ServerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Running => "running",
            Self::Draining => "draining",
            Self::Canceling => "canceling",
        })
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
    /// Whether the server is taking runs or shutting down.
    #[serde(default)]
    pub state: ServerState,
}

/// Get metadata about the running server.
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
        state: ServerState::from(state.shutdown.state()),
    })
}

#[cfg(test)]
mod tests {
    use axum::extract::State;
    use wdl::engine::CancellationContext;
    use wdl::engine::config::FailureMode;

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
            state: ServerState::Running,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"failure_mode":"slow","output_dir":"/var/sprocket/runs","state":"running"}"#
        );

        let json = serde_json::to_string(&ServerInfoResponse {
            failure_mode: ServerFailureMode::Fast,
            output_dir: "/var/sprocket/runs".to_string(),
            state: ServerState::Running,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"failure_mode":"fast","output_dir":"/var/sprocket/runs","state":"running"}"#
        );
    }

    #[tokio::test]
    async fn info_reports_shutdown_state() {
        let shutdown = CancellationContext::new(FailureMode::Slow);
        let (run_manager_tx, _run_manager_rx) = tokio::sync::mpsc::channel(1);
        let state = AppState::builder()
            .run_manager_tx(run_manager_tx)
            .database(crate::server::api::test_database().await)
            .failure_mode(ServerFailureMode::Slow)
            .output_dir(String::new())
            .shutdown(shutdown.clone())
            .build();

        let Json(info) = get_server_info(State(state.clone())).await;
        assert_eq!(info.state, ServerState::Running);

        let _ = shutdown.cancel();
        let Json(info) = get_server_info(State(state.clone())).await;
        assert_eq!(info.state, ServerState::Draining);

        let _ = shutdown.cancel();
        let Json(info) = get_server_info(State(state)).await;
        assert_eq!(info.state, ServerState::Canceling);
    }

    #[test]
    fn missing_state_reads_as_running() {
        let info: ServerInfoResponse =
            serde_json::from_str(r#"{"failure_mode":"slow","output_dir":"/x"}"#).unwrap();
        assert_eq!(info.state, ServerState::Running);
    }
}
