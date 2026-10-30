//! API models and handlers.

use std::sync::Arc;

use axum::Router;
use bon::Builder;
use tokio::sync::mpsc;

use crate::server::api::v1::info::ServerFailureMode;
use crate::system::v1::db::Database;
use crate::system::v1::exec::svc::run_manager::RunManagerCmd;

pub mod v1;

/// A sender for run manager commands.
type RunManagerTx = mpsc::Sender<RunManagerCmd>;

/// Application state.
#[derive(Builder, Clone)]
pub struct AppState {
    /// The run manager command transmitter.
    run_manager_tx: RunManagerTx,
    /// The database used for read-only API requests.
    database: Arc<dyn Database>,
    /// The cancellation failure mode the server is configured to use.
    ///
    /// Surfaced via the [`info`](crate::server::api::v1::info) endpoint so
    /// clients (e.g. the `dev server cancel` CLI) can adapt their behavior.
    failure_mode: ServerFailureMode,
    /// The server's output directory, rendered as a string.
    ///
    /// Populated from `config.server.output_dir` after shell expansion.
    /// Surfaced via the [`info`](crate::server::api::v1::info) endpoint so
    /// clients (e.g. `dev server inspect`) can display absolute output paths.
    output_dir: String,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("run_manager_tx", &self.run_manager_tx)
            .field("database", &"<dyn Database>")
            .field("failure_mode", &self.failure_mode)
            .field("output_dir", &self.output_dir)
            .finish()
    }
}

impl AppState {
    /// Gets the run manager tx channel.
    pub fn run_manager_tx(&self) -> &RunManagerTx {
        &self.run_manager_tx
    }

    /// Gets the database.
    pub fn database(&self) -> &Arc<dyn Database> {
        &self.database
    }
}

/// Create the API router with all versions.
pub fn create_router(state: AppState) -> Router {
    Router::new().nest("/v1", v1::create_router(state))
}

#[cfg(test)]
pub(crate) async fn test_database() -> Arc<dyn Database> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    Arc::new(
        crate::system::v1::db::SqliteDatabase::from_pool(pool)
            .await
            .unwrap(),
    )
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use axum::http::StatusCode;
    use tokio::sync::mpsc;
    use tower::ServiceExt;

    use super::*;
    #[tokio::test]
    async fn app_state_returns_run_manager_sender() {
        let (run_manager_tx, _run_manager_rx) = mpsc::channel::<RunManagerCmd>(1);
        let state = AppState::builder()
            .run_manager_tx(run_manager_tx.clone())
            .database(super::test_database().await)
            .failure_mode(ServerFailureMode::Slow)
            .output_dir(String::new())
            .build();

        assert!(!state.run_manager_tx().is_closed());
        assert_eq!(
            state.run_manager_tx().max_capacity(),
            run_manager_tx.max_capacity()
        );
    }

    #[tokio::test]
    async fn router_nests_v1_routes() -> anyhow::Result<()> {
        let (run_manager_tx, _run_manager_rx) = mpsc::channel::<RunManagerCmd>(1);
        let state = AppState::builder()
            .run_manager_tx(run_manager_tx)
            .database(super::test_database().await)
            .failure_mode(ServerFailureMode::Slow)
            .output_dir(String::new())
            .build();
        let app = create_router(state);

        let request = Request::builder().uri("/missing").body(Body::empty())?;
        let response = app.oneshot(request).await?;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        Ok(())
    }
}
