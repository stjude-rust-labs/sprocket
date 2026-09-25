//! Webhook delivery queue and retry policy.

use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::HeaderMap;
use reqwest::header::RETRY_AFTER;
use secrecy::ExposeSecret as _;
use secrecy::SecretString;
use serde_json::Value;
use tokio::select;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::config::NotificationEvent;

/// Timing and retry policy for webhook delivery.
#[derive(Debug, Clone)]
pub(super) struct DeliveryPolicy {
    /// The number of messages that can be queued for a webhook.
    pub(super) queue_capacity: usize,
    /// The minimum time between the start of consecutive sends to a webhook.
    pub(super) min_send_interval: Duration,
    /// The timeout of each request.
    pub(super) request_timeout: Duration,
    /// The maximum number of attempts to deliver a message.
    pub(super) max_attempts: usize,
    /// The delay before the first retry when the server gives none.
    pub(super) backoff_initial: Duration,
    /// The maximum delay between retries when the server gives none.
    pub(super) backoff_max: Duration,
    /// The maximum `Retry-After` delay that is honored.
    pub(super) retry_after_max: Duration,
}

impl Default for DeliveryPolicy {
    fn default() -> Self {
        Self {
            queue_capacity: 100,
            min_send_interval: Duration::from_secs(1),
            request_timeout: Duration::from_secs(10),
            max_attempts: 5,
            backoff_initial: Duration::from_secs(1),
            backoff_max: Duration::from_secs(30),
            retry_after_max: Duration::from_secs(60),
        }
    }
}

#[cfg(test)]
impl DeliveryPolicy {
    /// Gets a policy with short delays for tests.
    pub(super) fn test() -> Self {
        Self {
            queue_capacity: 2,
            min_send_interval: Duration::from_millis(1),
            request_timeout: Duration::from_millis(75),
            max_attempts: 3,
            backoff_initial: Duration::from_millis(10),
            backoff_max: Duration::from_millis(20),
            retry_after_max: Duration::from_millis(50),
        }
    }
}

/// A rendered message to deliver.
#[derive(Debug)]
pub(super) struct DeliveryMessage {
    /// The event of the message, for logging.
    pub(super) event: NotificationEvent,
    /// The JSON payload to post.
    pub(super) payload: Value,
}

/// Signals that stop a webhook delivery worker.
#[derive(Debug, Clone)]
pub(super) struct WorkerSignals {
    /// When task messages stop being delivered, set when shutdown begins.
    ///
    /// After the cutoff, queued task messages are skipped and an in-flight
    /// task message is abandoned so that the remaining time goes to run
    /// messages, including a run's terminal message.
    pub(super) task_cutoff: watch::Receiver<Option<Instant>>,
    /// Canceled once no more messages will be queued; the worker then exits
    /// when its queue is empty.
    pub(super) closing: CancellationToken,
}

/// Delivers queued messages to one webhook, one at a time and in order.
pub(super) async fn run_worker(
    label: String,
    url: SecretString,
    client: reqwest::Client,
    policy: DeliveryPolicy,
    mut queue: mpsc::Receiver<DeliveryMessage>,
    signals: WorkerSignals,
) {
    let mut last_send = None::<Instant>;
    let mut skipped = 0usize;

    loop {
        let message = select! {
            biased;
            Some(message) = queue.recv() => message,
            _ = signals.closing.cancelled() => match queue.try_recv() {
                Ok(message) => message,
                Err(_) => break,
            },
        };

        if let Some(last_send) = last_send {
            tokio::time::sleep_until(last_send + policy.min_send_interval).await;
        }

        let is_task = message.event.is_task();
        if is_task && past_cutoff(&signals.task_cutoff) {
            skipped += 1;
            continue;
        }

        last_send = Some(Instant::now());
        select! {
            _ = deliver(&label, &url, &client, &policy, &message) => {}
            _ = wait_for_cutoff(signals.task_cutoff.clone()), if is_task => skipped += 1,
        }
    }

    if skipped > 0 {
        warn!(
            webhook = %label,
            skipped,
            "skipped task messages to deliver run messages before exiting"
        );
    }
}

/// Returns true if the task message cutoff has passed.
fn past_cutoff(cutoff: &watch::Receiver<Option<Instant>>) -> bool {
    cutoff
        .borrow()
        .is_some_and(|cutoff| Instant::now() >= cutoff)
}

/// Waits until the task message cutoff is set and has passed.
async fn wait_for_cutoff(mut cutoff: watch::Receiver<Option<Instant>>) {
    let Ok(cutoff) = cutoff.wait_for(Option::is_some).await.map(|cutoff| *cutoff) else {
        return std::future::pending().await;
    };

    tokio::time::sleep_until(cutoff.expect("cutoff should be set")).await;
}

/// Delivers a message, retrying transient failures.
async fn deliver(
    label: &str,
    url: &SecretString,
    client: &reqwest::Client,
    policy: &DeliveryPolicy,
    message: &DeliveryMessage,
) {
    let mut backoff = policy.backoff_initial;

    for attempt in 1..=policy.max_attempts {
        let result = client
            .post(url.expose_secret())
            .json(&message.payload)
            .timeout(policy.request_timeout)
            .send()
            .await;

        // The error is stripped of the URL, which contains the webhook secret.
        let (failure, retry_delay) = match result {
            Ok(response) if response.status().is_success() => return,
            Ok(response) => {
                let status = response.status();
                let delay = is_retryable(status)
                    .then(|| retry_after(response.headers(), policy).unwrap_or(backoff));
                (status.to_string(), delay)
            }
            Err(error) => {
                let delay = (error.is_connect() || error.is_timeout()).then_some(backoff);
                (error.without_url().to_string(), delay)
            }
        };

        match retry_delay {
            Some(delay) if attempt < policy.max_attempts => {
                warn!(
                    webhook = %label,
                    event = message.event.as_str(),
                    attempt,
                    "webhook delivery failed, retrying: {failure}"
                );
                tokio::time::sleep(delay).await;
                backoff = backoff.saturating_mul(2).min(policy.backoff_max);
            }
            _ => {
                warn!(
                    webhook = %label,
                    event = message.event.as_str(),
                    attempt,
                    "webhook delivery failed: {failure}"
                );
                return;
            }
        }
    }
}

/// Returns true if a response status indicates a transient failure.
fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// Gets the capped delay from a `Retry-After` header given in seconds.
fn retry_after(headers: &HeaderMap, policy: &DeliveryPolicy) -> Option<Duration> {
    let seconds = headers.get(RETRY_AFTER)?.to_str().ok()?.parse().ok()?;
    Some(Duration::from_secs(seconds).min(policy.retry_after_max))
}
