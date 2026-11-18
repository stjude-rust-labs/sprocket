//! Graceful shutdown for the server.

use std::time::Duration;

use tokio::task::JoinHandle;
use tracing::warn;
use wdl::engine::CancellationContext;
use wdl::engine::CancellationContextState;
use wdl::engine::config::FailureMode;

/// What the server does after a shutdown signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShutdownStep {
    /// Stop taking runs and wait for running tasks to finish.
    Drain,
    /// Cancel running tasks.
    Cancel,
    /// Exit now without waiting.
    Exit,
}

/// Moves the server through shutdown, one signal at a time.
///
/// Every run's cancellation context is a child of [`Self::context`], so each
/// step applies to every run.
#[derive(Debug, Clone)]
pub(crate) struct ShutdownController {
    /// The server's root cancellation context.
    context: CancellationContext,
}

impl ShutdownController {
    /// Creates a controller for the server's failure mode.
    pub(crate) fn new(mode: FailureMode) -> Self {
        Self {
            context: CancellationContext::new(mode),
        }
    }

    /// The root cancellation context.
    pub(crate) fn context(&self) -> &CancellationContext {
        &self.context
    }

    /// Handles a shutdown signal and returns the step taken.
    ///
    /// Slow mode: drain, then cancel, then exit. Fast mode: cancel, then exit.
    pub(crate) fn signal(&self) -> ShutdownStep {
        if self.context.state() == CancellationContextState::Canceling {
            return ShutdownStep::Exit;
        }

        match self.context.cancel() {
            CancellationContextState::NotCanceled => unreachable!("`cancel()` always cancels"),
            CancellationContextState::Waiting => ShutdownStep::Drain,
            CancellationContextState::Canceling => ShutdownStep::Cancel,
        }
    }

    /// Cancels running tasks after `timeout` if the server is still draining.
    pub(crate) fn spawn_deadline(&self, timeout: Duration) -> JoinHandle<()> {
        let context = self.context.clone();
        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            if context.state() == CancellationContextState::Waiting {
                warn!("shutdown timeout passed: canceling running tasks");
                let _ = context.cancel();
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_mode_drains_then_cancels_then_exits() {
        let shutdown = ShutdownController::new(FailureMode::Slow);
        assert_eq!(shutdown.signal(), ShutdownStep::Drain);
        assert_eq!(
            shutdown.context().state(),
            CancellationContextState::Waiting
        );
        assert_eq!(shutdown.signal(), ShutdownStep::Cancel);
        assert_eq!(
            shutdown.context().state(),
            CancellationContextState::Canceling
        );
        assert_eq!(shutdown.signal(), ShutdownStep::Exit);
    }

    #[test]
    fn fast_mode_cancels_then_exits() {
        let shutdown = ShutdownController::new(FailureMode::Fast);
        assert_eq!(shutdown.signal(), ShutdownStep::Cancel);
        assert_eq!(
            shutdown.context().state(),
            CancellationContextState::Canceling
        );
        assert_eq!(shutdown.signal(), ShutdownStep::Exit);
    }

    #[tokio::test]
    async fn deadline_cancels_while_draining() {
        let shutdown = ShutdownController::new(FailureMode::Slow);
        assert_eq!(shutdown.signal(), ShutdownStep::Drain);

        shutdown
            .spawn_deadline(Duration::from_millis(10))
            .await
            .unwrap();
        assert_eq!(
            shutdown.context().state(),
            CancellationContextState::Canceling
        );
        assert!(shutdown.context().user_canceled());
    }
}
