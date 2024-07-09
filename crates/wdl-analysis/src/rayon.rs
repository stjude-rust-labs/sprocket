//! Helper module for integration of rayon tasks with Tokio.

use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

use tokio::sync::oneshot;
use tokio::sync::oneshot::Receiver;

/// Represents a handle from spawning a task on the rayon thread pool.
///
/// Awaiting the handle returns the result of the spawned task.
#[must_use]
#[derive(Debug)]
pub struct RayonHandle<T> {
    /// The receiver that is notified when the rayon task completes.
    rx: Receiver<T>,
}

impl<T> RayonHandle<T>
where
    T: Send + 'static,
{
    /// Spawns a task on the rayon thread pool.
    ///
    /// The provided function will run on a rayon thread.
    ///
    /// The return handle must be awaited.
    pub fn spawn<F: FnOnce() -> T + Send + 'static>(func: F) -> Self {
        let (tx, rx) = oneshot::channel();
        rayon::spawn(move || {
            tx.send(func()).ok();
        });

        Self { rx }
    }
}

impl<T> Future for RayonHandle<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let rx = Pin::new(&mut self.rx);
        rx.poll(cx)
            .map(|result| result.expect("failed to receive from oneshot channel"))
    }
}
