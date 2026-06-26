//! API models and handlers.

use axum::Router;
use bon::Builder;
use tokio::sync::mpsc;

use crate::system::v1::exec::svc::run_manager::RunManagerCmd;

pub mod v1;

/// A sender for run manager commands.
type RunManagerTx = mpsc::Sender<RunManagerCmd>;

/// Application state.
#[derive(Builder, Clone, Debug)]
pub struct AppState {
    /// The run manager command transmitter.
    run_manager_tx: RunManagerTx,
}

impl AppState {
    /// Gets the run manager tx channel.
    pub fn run_manager_tx(&self) -> &RunManagerTx {
        &self.run_manager_tx
    }
}

/// Create the API router with all versions.
pub fn create_router(state: AppState) -> Router {
    Router::new().nest("/v1", v1::create_router(state))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use axum::http::StatusCode;
    use tokio::sync::mpsc;
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn app_state_returns_run_manager_sender() {
        let (run_manager_tx, _run_manager_rx) = mpsc::channel::<RunManagerCmd>(1);
        let state = AppState::builder()
            .run_manager_tx(run_manager_tx.clone())
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
        let state = AppState::builder().run_manager_tx(run_manager_tx).build();
        let app = create_router(state);

        let request = Request::builder().uri("/missing").body(Body::empty())?;
        let response = app.oneshot(request).await?;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        Ok(())
    }
}
