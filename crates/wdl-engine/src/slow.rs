//! Utilities for surfacing operations that are taking longer than expected.
//!
//! Operations on shared file systems (e.g., CephFS) or HPC scheduler commands
//! can stall indefinitely without producing an error; these utilities ensure
//! such stalls are reported to the user rather than appearing as a silent hang.

use std::fmt;
use std::time::Duration;

use tokio::time::Instant;
use tokio::time::MissedTickBehavior;
use tracing::warn;

/// The interval at which warnings are emitted for slow operations.
const SLOW_OPERATION_WARNING_INTERVAL: Duration = Duration::from_secs(60);

/// Awaits the given future, periodically emitting a warning while it has not
/// yet completed.
///
/// The `description` describes the operation being performed, e.g. "acquiring
/// exclusive lock on file `foo`".
pub(crate) async fn warn_if_slow<F, D>(future: F, description: impl Fn() -> D) -> F::Output
where
    F: Future,
    D: fmt::Display,
{
    warn_if_slow_with_interval(future, SLOW_OPERATION_WARNING_INTERVAL, description).await
}

/// Implementation of [`warn_if_slow`] with a configurable interval.
async fn warn_if_slow_with_interval<F, D>(
    future: F,
    interval: Duration,
    description: impl Fn() -> D,
) -> F::Output
where
    F: Future,
    D: fmt::Display,
{
    let start = Instant::now();
    let mut timer = tokio::time::interval_at(start + interval, interval);
    timer.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut future = std::pin::pin!(future);
    loop {
        tokio::select! {
            output = &mut future => return output,
            _ = timer.tick() => {
                warn!(
                    "{description} is taking longer than expected ({elapsed} seconds elapsed)",
                    description = description(),
                    elapsed = start.elapsed().as_secs()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn completes_with_output() {
        let output = warn_if_slow_with_interval(
            async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                42
            },
            Duration::from_millis(10),
            || "test operation",
        )
        .await;
        assert_eq!(output, 42);
    }
}
