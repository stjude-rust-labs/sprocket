//! Notification service for Sprocket run and task events.
//!
//! Run lifecycle messages are driven by persisted run transitions (see
//! [`RunObserver`]), while task messages are driven by the engine and
//! Crankshaft event streams of each run (see [`NotificationSvc::listen`]).
//! Delivery is best-effort: failures are logged and never affect the run.

use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use crankshaft::events::Event as CrankshaftEvent;
use delivery::DeliveryMessage;
use delivery::DeliveryPolicy;
use delivery::WorkerSignals;
use secrecy::ExposeSecret as _;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::error::TryRecvError;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::warn;
use uuid::Uuid;
use wdl::engine::CLEANUP_TASK_NAME_PREFIX;
use wdl::engine::EngineEvent;

use crate::config::NotificationEvent;
use crate::config::NotificationsConfig;
use crate::config::WebhookConfig;
use crate::config::WebhookKind;
use crate::system::v1::db::Run;
use crate::system::v1::db::RunObserver;
use crate::system::v1::db::RunStatus;

mod delivery;
mod slack;
mod teams;

/// The maximum time a terminal run message waits for the run's task events to
/// be processed, so that its suppressed-message counts are complete.
const TERMINAL_WAIT: Duration = Duration::from_secs(5);

/// How long the run messages sent for a run are remembered so that a repeated
/// or late transition does not send another.
const SENT_MARKER_TTL: Duration = Duration::from_secs(60 * 60);

/// The maximum number of characters of a run or task error in a message.
const MAX_ERROR_CHARS: usize = 500;

/// A notification service that renders and delivers configured webhooks.
///
/// Cloning the service is cheap; all clones share the same delivery workers.
#[derive(Clone)]
pub struct NotificationSvc {
    /// The shared service state, or `None` when no webhooks are configured.
    inner: Option<Arc<Inner>>,
}

impl fmt::Debug for NotificationSvc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NotificationSvc")
            .field("enabled", &self.is_enabled())
            .finish_non_exhaustive()
    }
}

impl NotificationSvc {
    /// Creates a notification service from configuration.
    ///
    /// `output_root` is joined with each run's directory to report where the
    /// run's outputs are.
    ///
    /// This spawns a delivery worker per webhook, so it must be called from
    /// within a Tokio runtime.
    pub fn new(config: &NotificationsConfig, output_root: PathBuf) -> Self {
        Self::new_with_policy(config, output_root, DeliveryPolicy::default(), hostname())
    }

    /// Creates a disabled notification service.
    pub fn disabled() -> Self {
        Self { inner: None }
    }

    /// Returns true when at least one webhook is configured.
    pub fn is_enabled(&self) -> bool {
        self.inner.is_some()
    }

    /// Starts handling the task events of a run.
    ///
    /// The returned guard must be kept until the run's evaluation has
    /// finished; call [`RunEventGuard::finish`] to process any remaining
    /// events before the run's terminal message is sent.
    pub fn listen(
        &self,
        run_id: Uuid,
        run_name: String,
        engine: broadcast::Receiver<EngineEvent>,
        crankshaft: broadcast::Receiver<CrankshaftEvent>,
    ) -> RunEventGuard {
        let Some(inner) = &self.inner else {
            return RunEventGuard::disabled();
        };

        inner.register_run(run_id, run_name);
        let shutdown = CancellationToken::new();
        let listener = RunEventListener {
            run_id,
            inner: Arc::clone(inner),
            engine,
            crankshaft,
            shutdown: shutdown.clone(),
            task_names: HashMap::new(),
        };

        RunEventGuard {
            inner: Some(Arc::clone(inner)),
            run_id,
            shutdown,
            join: Some(tokio::spawn(listener.run())),
        }
    }

    /// Waits up to `timeout` for queued messages to be delivered, then stops
    /// the delivery workers.
    ///
    /// Task messages are delivered in order for the first half of `timeout`;
    /// any left after that are skipped so that run messages, including a run's
    /// terminal message, can still be delivered.
    pub async fn shutdown(&self, timeout: Duration) {
        if let Some(inner) = &self.inner {
            inner.shutdown(timeout).await;
        }
    }

    /// Creates a notification service with the given delivery policy and
    /// host name.
    fn new_with_policy(
        config: &NotificationsConfig,
        output_root: PathBuf,
        policy: DeliveryPolicy,
        host: String,
    ) -> Self {
        if config.webhooks.is_empty() {
            return Self::disabled();
        }

        let client = reqwest::Client::new();
        let (task_cutoff, task_cutoff_rx) = watch::channel(None);
        let signals = WorkerSignals {
            task_cutoff: task_cutoff_rx,
            closing: CancellationToken::new(),
        };
        let mut workers = JoinSet::new();
        let webhooks = config
            .webhooks
            .iter()
            .enumerate()
            .map(|(index, config)| {
                let (webhook, worker) = Webhook::new(
                    index,
                    config,
                    client.clone(),
                    policy.clone(),
                    signals.clone(),
                );
                workers.spawn(worker);
                webhook
            })
            .collect();

        Self {
            inner: Some(Arc::new(Inner {
                output_root,
                host,
                webhooks,
                state: Mutex::new(State::default()),
                pending: Mutex::new(JoinSet::new()),
                task_cutoff,
                closing: signals.closing,
                workers: Mutex::new(workers),
            })),
        }
    }
}

impl RunObserver for NotificationSvc {
    fn run_transitioned(&self, run: &Run) {
        if let Some(inner) = &self.inner {
            inner.on_run_transition(run);
        }
    }
}

/// Guard for the task-event listener of one run.
///
/// Dropping the guard stops the listener without processing buffered events.
pub struct RunEventGuard {
    /// The shared service state, or `None` when notifications are disabled.
    inner: Option<Arc<Inner>>,
    /// The ID of the run being listened to.
    run_id: Uuid,
    /// Signals the listener that the run's evaluation has finished.
    shutdown: CancellationToken,
    /// The listener task.
    join: Option<JoinHandle<()>>,
}

impl fmt::Debug for RunEventGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunEventGuard")
            .field("enabled", &self.inner.is_some())
            .field("run_id", &self.run_id)
            .finish_non_exhaustive()
    }
}

impl RunEventGuard {
    /// Creates a guard for a disabled notification service.
    fn disabled() -> Self {
        Self {
            inner: None,
            run_id: Uuid::nil(),
            shutdown: CancellationToken::new(),
            join: None,
        }
    }

    /// Processes the run's buffered task events, then stops the listener.
    ///
    /// Call this once the run's evaluation has finished, so that no further
    /// task events can be emitted.
    pub async fn finish(mut self) {
        self.shutdown.cancel();
        if let Some(join) = self.join.take()
            && let Err(error) = join.await
            && !error.is_cancelled()
        {
            warn!(%error, "notification event listener failed");
        }
    }
}

impl Drop for RunEventGuard {
    fn drop(&mut self) {
        if let Some(join) = self.join.take() {
            join.abort();
        }

        // Release a terminal message that may be waiting on this run.
        if let Some(inner) = &self.inner {
            inner.mark_events_complete(self.run_id);
        }
    }
}

/// The shared state of an enabled notification service.
#[derive(Debug)]
struct Inner {
    /// The root that run directories are relative to.
    output_root: PathBuf,
    /// The host name reported in messages.
    host: String,
    /// The configured webhooks.
    webhooks: Vec<Webhook>,
    /// Per-run notification state.
    state: Mutex<State>,
    /// Tasks that are waiting to enqueue run messages.
    pending: Mutex<JoinSet<()>>,
    /// Sets when the delivery workers stop delivering task messages.
    task_cutoff: watch::Sender<Option<tokio::time::Instant>>,
    /// Signals the delivery workers to exit once their queues are empty.
    closing: CancellationToken,
    /// The delivery workers of the webhooks.
    workers: Mutex<JoinSet<()>>,
}

impl Inner {
    /// Registers a run whose task events are being listened to.
    fn register_run(&self, run_id: Uuid, run_name: String) {
        let mut state = self.state();
        // A run can be canceled before its events are listened to; nothing
        // would remove the state of a run whose terminal message was sent.
        if state.sent.get(&run_id).is_some_and(|sent| sent.terminal) {
            return;
        }

        let (completion, _) = watch::channel(false);
        state.runs.insert(
            run_id,
            RunState {
                run_name,
                target: None,
                completion,
                webhooks: vec![RunWebhookState::default(); self.webhooks.len()],
            },
        );
    }

    /// Locks the per-run notification state.
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("notification state poisoned")
    }

    /// Marks the task events of a run as fully processed.
    fn mark_events_complete(&self, run_id: Uuid) {
        if let Some(run) = self.state().runs.get(&run_id) {
            run.completion.send_replace(true);
        }
    }

    /// Handles a persisted run transition.
    fn on_run_transition(self: &Arc<Self>, run: &Run) {
        let Some(event) = run_event(run.status) else {
            return;
        };

        {
            let mut state = self.state();
            if let Some(run_state) = state.runs.get_mut(&run.uuid) {
                run_state.target.clone_from(&run.target);
            }

            if !state.mark_sent(run.uuid, event) {
                return;
            }
        }

        if !is_terminal(event) {
            // Enqueue synchronously so that the run's first task message
            // cannot be delivered before this one.
            self.enqueue_run_message_now(run, event);
            return;
        }

        // The terminal message reports how many task messages were suppressed,
        // so wait for the run's remaining task events to be processed first.
        let inner = Arc::clone(self);
        let run = run.clone();
        self.spawn_pending(async move {
            inner.wait_for_run_events(run.uuid).await;
            let suppressed = inner.suppressed_counts(run.uuid);
            inner.enqueue_run_message(&run, event, &suppressed).await;
            inner.state().runs.remove(&run.uuid);
        });
    }

    /// Spawns a task that must complete before the service shuts down.
    fn spawn_pending(&self, task: impl Future<Output = ()> + Send + 'static) {
        let mut pending = self.pending.lock().expect("notification tasks poisoned");
        // Reap finished tasks so that a long-lived server does not accumulate
        // them.
        while pending.try_join_next().is_some() {}
        pending.spawn(task);
    }

    /// Waits, up to [`TERMINAL_WAIT`], for the task events of a run to be
    /// processed.
    async fn wait_for_run_events(&self, run_id: Uuid) {
        let Some(mut completion) = self
            .state()
            .runs
            .get(&run_id)
            .map(|run| run.completion.subscribe())
        else {
            return;
        };

        let _ =
            tokio::time::timeout(TERMINAL_WAIT, completion.wait_for(|complete| *complete)).await;
    }

    /// Gets the number of suppressed task messages of a run for each webhook.
    fn suppressed_counts(&self, run_id: Uuid) -> Vec<u64> {
        self.state()
            .runs
            .get(&run_id)
            .map(|run| run.webhooks.iter().map(|state| state.suppressed).collect())
            .unwrap_or_else(|| vec![0; self.webhooks.len()])
    }

    /// Enqueues a task message for each subscribed webhook, subject to the
    /// webhook's per-run cap.
    fn enqueue_task_message(&self, run_id: Uuid, task: &TaskMessage) {
        let mut state = self.state();
        let Some(run_state) = state.runs.get_mut(&run_id) else {
            return;
        };

        for (webhook, webhook_state) in self.webhooks.iter().zip(&mut run_state.webhooks) {
            if !webhook.subscribes(task.event) || webhook.max_task_messages_per_run == 0 {
                continue;
            }

            if webhook_state.sent >= webhook.max_task_messages_per_run {
                webhook_state.suppressed += 1;
                continue;
            }

            let message = NotificationMessage::task(
                task,
                &run_state.run_name,
                run_id,
                run_state.target.clone(),
                self.host.clone(),
            );
            match webhook.try_send(&message) {
                Ok(()) => webhook_state.sent += 1,
                // Task messages are dropped rather than blocking event handling.
                Err(_) => webhook_state.suppressed += 1,
            }
        }
    }

    /// Enqueues a run message without waiting, falling back to a pending task
    /// for any webhook whose queue is full.
    fn enqueue_run_message_now(&self, run: &Run, event: NotificationEvent) {
        let message =
            NotificationMessage::run(run, event, self.output_dir(run), self.host.clone(), 0);
        let mut full = Vec::new();
        for webhook in self.webhooks.iter().filter(|w| w.subscribes(event)) {
            if let Err(TrySendError::Full(command)) = webhook.try_send(&message) {
                full.push((webhook.queue.clone(), webhook.label.clone(), command));
            }
        }

        if full.is_empty() {
            return;
        }

        self.spawn_pending(async move {
            for (sender, label, command) in full {
                if sender.send(command).await.is_err() {
                    warn!(webhook = %label, event = event.as_str(), "webhook delivery queue is closed");
                }
            }
        });
    }

    /// Enqueues a run message for each subscribed webhook, waiting for queue
    /// capacity.
    async fn enqueue_run_message(&self, run: &Run, event: NotificationEvent, suppressed: &[u64]) {
        for (webhook, suppressed) in self.webhooks.iter().zip(suppressed) {
            if !webhook.subscribes(event) {
                continue;
            }

            let message = NotificationMessage::run(
                run,
                event,
                self.output_dir(run),
                self.host.clone(),
                *suppressed,
            );
            if webhook.queue.send(webhook.message(&message)).await.is_err() {
                warn!(webhook = %webhook.label, event = event.as_str(), "webhook delivery queue is closed");
            }
        }
    }

    /// Gets the output directory of a run.
    fn output_dir(&self, run: &Run) -> Option<PathBuf> {
        run.directory
            .as_ref()
            .map(|directory| self.output_root.join(directory))
    }

    /// Waits up to `timeout` for queued messages to be delivered, then stops
    /// the delivery workers.
    async fn shutdown(&self, timeout: Duration) {
        let now = tokio::time::Instant::now();
        let deadline = now + timeout;
        self.task_cutoff.send_replace(Some(now + timeout / 2));

        // Let pending run messages reach the delivery queues.
        let mut pending =
            std::mem::take(&mut *self.pending.lock().expect("notification tasks poisoned"));
        let _ =
            tokio::time::timeout_at(deadline, join_logged(&mut pending, "notification task")).await;

        // Each worker exits once its queue is empty.
        self.closing.cancel();
        let mut workers =
            std::mem::take(&mut *self.workers.lock().expect("notification workers poisoned"));
        let joined = tokio::time::timeout_at(
            deadline,
            join_logged(&mut workers, "webhook delivery worker"),
        )
        .await;

        // Dropping the join set aborts any worker still running at the
        // deadline.
        if joined.is_err() {
            for webhook in &self.webhooks {
                let queued = webhook.queued();
                if queued > 0 {
                    warn!(webhook = %webhook.label, queued, "abandoning undelivered webhook messages");
                }
            }
        }
    }
}

/// Joins every task in a set, logging tasks that panicked.
async fn join_logged(tasks: &mut JoinSet<()>, context: &str) {
    while let Some(result) = tasks.join_next().await {
        if let Err(error) = result
            && !error.is_cancelled()
        {
            warn!(%error, "{context} failed");
        }
    }
}

/// A configured webhook.
#[derive(Debug)]
struct Webhook {
    /// The label used in logs.
    label: String,
    /// The webhook provider.
    kind: WebhookKind,
    /// The events the webhook subscribes to.
    events: HashSet<NotificationEvent>,
    /// The maximum number of task messages sent per run.
    max_task_messages_per_run: u32,
    /// The queue of the webhook's delivery worker.
    queue: mpsc::Sender<DeliveryMessage>,
}

impl Webhook {
    /// Creates a webhook and the future of its delivery worker.
    fn new(
        index: usize,
        config: &WebhookConfig,
        client: reqwest::Client,
        policy: DeliveryPolicy,
        signals: WorkerSignals,
    ) -> (Self, impl Future<Output = ()> + Send + 'static) {
        let label = config.label(index);
        let (queue, receiver) = mpsc::channel(policy.queue_capacity);
        let worker = delivery::run_worker(
            label.clone(),
            config.url.inner().expose_secret().to_string().into(),
            client,
            policy,
            receiver,
            signals,
        );

        let webhook = Self {
            label,
            kind: config.kind,
            events: config.events.iter().copied().collect(),
            max_task_messages_per_run: config.max_task_messages_per_run,
            queue,
        };

        (webhook, worker)
    }

    /// Gets the number of messages waiting in the webhook's queue.
    fn queued(&self) -> usize {
        self.queue.max_capacity() - self.queue.capacity()
    }

    /// Returns true if the webhook subscribes to the event.
    fn subscribes(&self, event: NotificationEvent) -> bool {
        self.events.contains(&event)
    }

    /// Renders a message for this webhook.
    fn message(&self, message: &NotificationMessage) -> DeliveryMessage {
        let payload = match self.kind {
            WebhookKind::Slack => slack::render(message),
            WebhookKind::Teams => teams::render(message),
        };

        DeliveryMessage {
            event: message.event,
            payload,
        }
    }

    /// Enqueues a message without waiting, logging if the queue is closed.
    fn try_send(&self, message: &NotificationMessage) -> Result<(), TrySendError<DeliveryMessage>> {
        let result = self.queue.try_send(self.message(message));
        if let Err(TrySendError::Closed(_)) = &result {
            warn!(webhook = %self.label, event = message.event.as_str(), "webhook delivery queue is closed");
        }
        result
    }
}

/// Notification state shared across runs.
#[derive(Debug, Default)]
struct State {
    /// The runs whose task events are being listened to.
    runs: HashMap<Uuid, RunState>,
    /// The run messages sent for recent runs.
    sent: HashMap<Uuid, SentRunMessages>,
}

impl State {
    /// Records that a run message is being sent.
    ///
    /// Returns false if the message must not be sent because it was already
    /// sent or the run's terminal message was sent.
    fn mark_sent(&mut self, run_id: Uuid, event: NotificationEvent) -> bool {
        let now = Instant::now();
        self.sent
            .retain(|_, sent| now.duration_since(sent.updated_at) < SENT_MARKER_TTL);

        let sent = self.sent.entry(run_id).or_insert(SentRunMessages {
            started: false,
            terminal: false,
            updated_at: now,
        });
        sent.updated_at = now;

        if sent.terminal {
            false
        } else if is_terminal(event) {
            sent.terminal = true;
            true
        } else {
            !std::mem::replace(&mut sent.started, true)
        }
    }
}

/// The run messages sent for a run.
#[derive(Debug)]
struct SentRunMessages {
    /// Whether the run's start message was sent.
    started: bool,
    /// Whether the run's terminal message was sent.
    terminal: bool,
    /// When a message for the run was last attempted.
    updated_at: Instant,
}

/// Notification state of one run.
#[derive(Debug)]
struct RunState {
    /// The name of the run.
    run_name: String,
    /// The run's target, once known from a run transition.
    target: Option<String>,
    /// Set to `true` once the run's task events have been processed.
    completion: watch::Sender<bool>,
    /// Per-webhook task message counts, in webhook order.
    webhooks: Vec<RunWebhookState>,
}

/// Task message counts of one webhook for one run.
#[derive(Debug, Clone, Default)]
struct RunWebhookState {
    /// The number of task messages enqueued.
    sent: u32,
    /// The number of task messages dropped because of the cap or a full
    /// queue.
    suppressed: u64,
}

/// A task event to notify about.
#[derive(Debug)]
struct TaskMessage {
    /// The notification event.
    event: NotificationEvent,
    /// The name of the task.
    task_name: String,
    /// The failed attempt and the maximum number of attempts, if known.
    attempt: Option<(u64, u64)>,
    /// The exit code of the failed attempt, if known.
    exit_code: Option<i32>,
    /// The task's error, if any.
    error: Option<String>,
}

/// A provider-independent notification message.
#[derive(Debug, Clone)]
struct NotificationMessage {
    /// The notification event.
    event: NotificationEvent,
    /// The name of the run.
    run_name: String,
    /// The ID of the run.
    run_id: Uuid,
    /// The run's target, if known.
    target: Option<String>,
    /// The run's status, for run messages.
    status: Option<String>,
    /// The host running Sprocket.
    host: String,
    /// The run's duration, for terminal run messages.
    duration: Option<String>,
    /// The run's output directory, for run messages.
    output_dir: Option<String>,
    /// The name of the task, for task messages.
    task_name: Option<String>,
    /// The failed attempt, for retried task messages.
    attempt: Option<String>,
    /// The exit code of the failed attempt, for retried task messages.
    exit_code: Option<i32>,
    /// The truncated run or task error.
    error: Option<String>,
    /// The number of suppressed task messages, for terminal run messages.
    suppressed: u64,
}

impl NotificationMessage {
    /// Creates a run message.
    fn run(
        run: &Run,
        event: NotificationEvent,
        output_dir: Option<PathBuf>,
        host: String,
        suppressed: u64,
    ) -> Self {
        Self {
            event,
            run_name: run.name.clone(),
            run_id: run.uuid,
            target: run.target.clone(),
            status: Some(run.status.to_string()),
            host,
            duration: duration(run).map(format_duration),
            output_dir: output_dir.map(|path| path.display().to_string()),
            task_name: None,
            attempt: None,
            exit_code: None,
            error: run
                .error
                .as_deref()
                .map(|error| truncate(error, MAX_ERROR_CHARS)),
            suppressed,
        }
    }

    /// Creates a task message.
    fn task(
        task: &TaskMessage,
        run_name: &str,
        run_id: Uuid,
        target: Option<String>,
        host: String,
    ) -> Self {
        Self {
            event: task.event,
            run_name: run_name.to_string(),
            run_id,
            target,
            status: None,
            host,
            duration: None,
            output_dir: None,
            task_name: Some(task.task_name.clone()),
            attempt: task
                .attempt
                .map(|(attempt, max)| format!("{attempt} of {max}")),
            exit_code: task.exit_code,
            error: task
                .error
                .as_deref()
                .map(|error| truncate(error, MAX_ERROR_CHARS)),
            suppressed: 0,
        }
    }

    /// Gets the message title.
    fn title(&self) -> String {
        let summary = match self.event {
            NotificationEvent::RunStarted => "run started",
            NotificationEvent::RunCompleted => "run completed",
            NotificationEvent::RunFailed => "run failed",
            NotificationEvent::RunCanceled => "run canceled",
            NotificationEvent::TaskFailed => "task failed",
            NotificationEvent::TaskRetried => "task retried",
            NotificationEvent::TaskPreempted => "task preempted",
        };

        format!("Sprocket {summary}: {}", self.run_name)
    }

    /// Gets the text noting how many task messages were suppressed.
    fn suppressed_text(&self) -> String {
        format!("{} more task events suppressed", self.suppressed)
    }

    /// Gets the message's labeled fields, in display order.
    fn fields(&self) -> Vec<(&'static str, String)> {
        let mut fields = vec![
            ("Event", self.event.as_str().to_string()),
            ("Run", self.run_name.clone()),
            ("Run ID", self.run_id.to_string()),
            ("Host", self.host.clone()),
        ];

        let optional = [
            ("Target", self.target.clone()),
            ("Status", self.status.clone()),
            ("Duration", self.duration.clone()),
            ("Output", self.output_dir.clone()),
            ("Task", self.task_name.clone()),
            ("Attempt", self.attempt.clone()),
            ("Exit code", self.exit_code.map(|code| code.to_string())),
        ];
        fields.extend(
            optional
                .into_iter()
                .filter_map(|(name, value)| Some((name, value?))),
        );

        fields
    }
}

/// Handles the task events of one run.
#[derive(Debug)]
struct RunEventListener {
    /// The ID of the run.
    run_id: Uuid,
    /// The shared service state.
    inner: Arc<Inner>,
    /// The run's engine events.
    engine: broadcast::Receiver<EngineEvent>,
    /// The run's Crankshaft events.
    crankshaft: broadcast::Receiver<CrankshaftEvent>,
    /// Signals that the run's evaluation has finished.
    shutdown: CancellationToken,
    /// The names of the run's tasks by Crankshaft task ID.
    task_names: HashMap<u64, String>,
}

impl RunEventListener {
    /// Handles events until the streams close or shutdown is signaled, then
    /// marks the run's events as processed.
    async fn run(mut self) {
        let mut crankshaft_open = true;
        let mut engine_open = true;

        while crankshaft_open || engine_open {
            select! {
                _ = self.shutdown.cancelled() => break,
                r = self.crankshaft.recv(), if crankshaft_open => match r {
                    Ok(event) => self.handle_crankshaft(event),
                    Err(RecvError::Lagged(count)) => self.lagged(count),
                    Err(RecvError::Closed) => crankshaft_open = false,
                },
                r = self.engine.recv(), if engine_open => match r {
                    Ok(event) => self.handle_engine(event),
                    Err(RecvError::Lagged(count)) => self.lagged(count),
                    Err(RecvError::Closed) => engine_open = false,
                },
            }
        }

        // Shutdown can be signaled while events are still buffered.
        self.drain();
        self.inner.mark_events_complete(self.run_id);
    }

    /// Handles all buffered events.
    fn drain(&mut self) {
        loop {
            match self.crankshaft.try_recv() {
                Ok(event) => self.handle_crankshaft(event),
                Err(TryRecvError::Lagged(count)) => self.lagged(count),
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            }
        }

        loop {
            match self.engine.try_recv() {
                Ok(event) => self.handle_engine(event),
                Err(TryRecvError::Lagged(count)) => self.lagged(count),
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            }
        }
    }

    /// Logs events that were missed because the listener fell behind.
    fn lagged(&self, count: u64) {
        warn!(count, run_id = %self.run_id, "notification event listener missed events");
    }

    /// Handles an engine event.
    fn handle_engine(&self, event: EngineEvent) {
        let task = match event {
            EngineEvent::TaskRetrying {
                id: _,
                name,
                attempt,
                max_retries,
                exit_code,
            } => TaskMessage {
                event: NotificationEvent::TaskRetried,
                task_name: name,
                attempt: Some((attempt + 1, max_retries + 1)),
                exit_code: Some(exit_code),
                error: None,
            },
            EngineEvent::TaskFailed { id: _, name, error } => TaskMessage {
                event: NotificationEvent::TaskFailed,
                task_name: name,
                attempt: None,
                exit_code: None,
                error: Some(error),
            },
            EngineEvent::TaskInitializing { .. }
            | EngineEvent::TaskLocalizing { .. }
            | EngineEvent::ReusedCachedExecutionResult { .. }
            | EngineEvent::TaskParked
            | EngineEvent::TaskUnparked { .. } => return,
        };

        self.inner.enqueue_task_message(self.run_id, &task);
    }

    /// Handles a Crankshaft event.
    fn handle_crankshaft(&mut self, event: CrankshaftEvent) {
        match event {
            CrankshaftEvent::TaskCreated { id, name, .. } => {
                if !name.starts_with(CLEANUP_TASK_NAME_PREFIX) {
                    self.task_names.insert(id, name);
                }
            }
            CrankshaftEvent::TaskPreempted { id } => {
                if let Some(name) = self.task_names.get(&id) {
                    self.inner.enqueue_task_message(
                        self.run_id,
                        &TaskMessage {
                            event: NotificationEvent::TaskPreempted,
                            task_name: name.clone(),
                            attempt: None,
                            exit_code: None,
                            error: None,
                        },
                    );
                }
            }
            CrankshaftEvent::TaskStarted { .. }
            | CrankshaftEvent::TaskCompleted { .. }
            | CrankshaftEvent::TaskFailed { .. }
            | CrankshaftEvent::TaskCanceled { .. }
            | CrankshaftEvent::TaskContainerCreated { .. }
            | CrankshaftEvent::TaskContainerExited { .. }
            | CrankshaftEvent::TaskStdout { .. }
            | CrankshaftEvent::TaskStderr { .. }
            | CrankshaftEvent::ImagePullStarted { .. }
            | CrankshaftEvent::ImagePullFailed { .. }
            | CrankshaftEvent::ImagePullFinished { .. } => {}
        }
    }
}

/// Gets the notification event for a run status, if there is one.
fn run_event(status: RunStatus) -> Option<NotificationEvent> {
    match status {
        RunStatus::Running => Some(NotificationEvent::RunStarted),
        RunStatus::Completed => Some(NotificationEvent::RunCompleted),
        RunStatus::Failed => Some(NotificationEvent::RunFailed),
        RunStatus::Canceled => Some(NotificationEvent::RunCanceled),
        RunStatus::Queued | RunStatus::Analyzing | RunStatus::Canceling | RunStatus::Orphaned => {
            None
        }
    }
}

/// Returns true if the event ends a run.
fn is_terminal(event: NotificationEvent) -> bool {
    matches!(
        event,
        NotificationEvent::RunCompleted
            | NotificationEvent::RunFailed
            | NotificationEvent::RunCanceled
    )
}

/// Gets the duration of a completed run.
fn duration(run: &Run) -> Option<Duration> {
    let completed_at = run.completed_at?;
    let started_at = run.started_at.unwrap_or(run.created_at);
    completed_at.signed_duration_since(started_at).to_std().ok()
}

/// Formats a duration as hours, minutes, and seconds.
fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    match (hours, minutes) {
        (0, 0) => format!("{seconds}s"),
        (0, _) => format!("{minutes}m {seconds}s"),
        _ => format!("{hours}h {minutes}m {seconds}s"),
    }
}

/// Truncates a string to at most `max` characters, ending truncated strings
/// with an ellipsis.
fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }

    let mut truncated: String = value.chars().take(max.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}

/// Gets the host name reported in messages.
fn hostname() -> String {
    whoami::hostname().unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io;
    use std::net::SocketAddr;
    use std::sync::Mutex as StdMutex;
    use std::time::Instant;

    use axum::Json;
    use axum::Router;
    use axum::extract::State as AxumState;
    use axum::http::HeaderMap;
    use axum::http::HeaderName;
    use axum::http::HeaderValue;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::routing::post;
    use pretty_assertions::assert_eq;
    use serde_json::Value;
    use serde_json::json;
    use tokio::net::TcpListener;
    use tokio_util::sync::CancellationToken;
    use tracing::subscriber::set_default;
    use tracing_subscriber::fmt::MakeWriter;
    use wdl::engine::EngineEvent;

    use super::*;

    fn test_run(status: RunStatus) -> Run {
        let created_at = chrono::DateTime::parse_from_rfc3339("2026-09-25T00:00:00Z")
            .expect("valid time")
            .with_timezone(&chrono::Utc);
        Run {
            uuid: Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").expect("valid uuid"),
            session_uuid: Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb")
                .expect("valid uuid"),
            name: "test-run".to_string(),
            source: "workflow.wdl".to_string(),
            target: Some("workflow.task".to_string()),
            status,
            inputs: "{}".to_string(),
            outputs: None,
            error: None,
            directory: Some("runs/test-run".to_string()),
            index_directory: None,
            started_at: Some(created_at),
            completed_at: Some(created_at + chrono::Duration::seconds(5)),
            created_at,
        }
    }

    #[test]
    fn notifications_render_slack_run_failed_truncates_error() {
        let (message, truncated) = failed_message_with_long_error();

        assert_eq!(
            slack::render(&message),
            json!({
                "text": "Sprocket run failed: test-run",
                "blocks": [
                    {
                        "type": "header",
                        "text": { "type": "plain_text", "text": "Sprocket run failed: test-run" }
                    },
                    {
                        "type": "section",
                        "fields": [
                            { "type": "mrkdwn", "text": "*Event:*\nrun.failed" },
                            { "type": "mrkdwn", "text": "*Run:*\ntest-run" },
                            { "type": "mrkdwn", "text": "*Run ID:*\naaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa" },
                            { "type": "mrkdwn", "text": "*Host:*\ntest-host" },
                            { "type": "mrkdwn", "text": "*Target:*\nworkflow.task" },
                            { "type": "mrkdwn", "text": "*Status:*\nfailed" },
                            { "type": "mrkdwn", "text": "*Duration:*\n5s" },
                            { "type": "mrkdwn", "text": "*Output:*\n/outputs/runs/test-run" }
                        ]
                    },
                    {
                        "type": "section",
                        "text": { "type": "mrkdwn", "text": format!("*Error:*\n```{truncated}```") }
                    }
                ]
            })
        );
    }

    #[test]
    fn notifications_render_teams_run_failed_truncates_error() {
        let (message, truncated) = failed_message_with_long_error();

        assert_eq!(
            teams::render(&message),
            json!({
                "type": "message",
                "attachments": [{
                    "contentType": "application/vnd.microsoft.card.adaptive",
                    "content": {
                        "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
                        "type": "AdaptiveCard",
                        "version": "1.4",
                        "body": [
                            { "type": "TextBlock", "text": "Sprocket run failed: test-run", "weight": "Bolder", "size": "Medium", "wrap": true },
                            { "type": "FactSet", "facts": [
                                { "title": "Event", "value": "run.failed" },
                                { "title": "Run", "value": "test-run" },
                                { "title": "Run ID", "value": "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa" },
                                { "title": "Host", "value": "test-host" },
                                { "title": "Target", "value": "workflow.task" },
                                { "title": "Status", "value": "failed" },
                                { "title": "Duration", "value": "5s" },
                                { "title": "Output", "value": "/outputs/runs/test-run" }
                            ]},
                            { "type": "TextBlock", "text": format!("Error: {truncated}"), "wrap": true }
                        ]
                    }
                }]
            })
        );
    }

    #[test]
    fn notifications_render_task_retried_attempt() {
        let message = NotificationMessage::task(
            &TaskMessage {
                event: NotificationEvent::TaskRetried,
                task_name: "workflow.retry".to_string(),
                attempt: Some((2, 4)),
                exit_code: Some(17),
                error: None,
            },
            "test-run",
            Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").expect("valid uuid"),
            Some("workflow.task".to_string()),
            "test-host".to_string(),
        );

        assert_eq!(
            slack::render(&message),
            json!({
                "text": "Sprocket task retried: test-run",
                "blocks": [
                    { "type": "header", "text": { "type": "plain_text", "text": "Sprocket task retried: test-run" } },
                    { "type": "section", "fields": [
                        { "type": "mrkdwn", "text": "*Event:*\ntask.retried" },
                        { "type": "mrkdwn", "text": "*Run:*\ntest-run" },
                        { "type": "mrkdwn", "text": "*Run ID:*\naaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa" },
                        { "type": "mrkdwn", "text": "*Host:*\ntest-host" },
                        { "type": "mrkdwn", "text": "*Target:*\nworkflow.task" },
                        { "type": "mrkdwn", "text": "*Task:*\nworkflow.retry" },
                        { "type": "mrkdwn", "text": "*Attempt:*\n2 of 4" },
                        { "type": "mrkdwn", "text": "*Exit code:*\n17" }
                    ]}
                ]
            })
        );
    }

    #[test]
    fn notifications_render_run_completed_with_suppressed_count() {
        let mut message = base_message(NotificationEvent::RunCompleted);
        message.status = Some("completed".to_string());
        message.suppressed = 3;

        assert_eq!(
            teams::render(&message),
            json!({
                "type": "message",
                "attachments": [{
                    "contentType": "application/vnd.microsoft.card.adaptive",
                    "content": {
                        "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
                        "type": "AdaptiveCard",
                        "version": "1.4",
                        "body": [
                            { "type": "TextBlock", "text": "Sprocket run completed: test-run", "weight": "Bolder", "size": "Medium", "wrap": true },
                            { "type": "FactSet", "facts": [
                                { "title": "Event", "value": "run.completed" },
                                { "title": "Run", "value": "test-run" },
                                { "title": "Run ID", "value": "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa" },
                                { "title": "Host", "value": "test-host" },
                                { "title": "Target", "value": "workflow.task" },
                                { "title": "Status", "value": "completed" },
                                { "title": "Duration", "value": "5s" },
                                { "title": "Output", "value": "/outputs/runs/test-run" }
                            ]},
                            { "type": "TextBlock", "text": "3 more task events suppressed", "isSubtle": true, "wrap": true }
                        ]
                    }
                }]
            })
        );
    }

    #[tokio::test]
    async fn notifications_delivery_statuses_and_retry_policy() {
        let server = MockServer::start(vec![MockResponse::ok()]).await;
        let svc = service(&server.url(), all_events(), 25, DeliveryPolicy::test());
        svc.run_transitioned(&test_run(RunStatus::Running));
        svc.shutdown(Duration::from_secs(1)).await;
        assert_eq!(server.received().len(), 1);

        let server = MockServer::start(vec![
            MockResponse::status(StatusCode::TOO_MANY_REQUESTS).header("retry-after", "1"),
            MockResponse::ok(),
        ])
        .await;
        let svc = service(&server.url(), all_events(), 25, DeliveryPolicy::test());
        let start = Instant::now();
        svc.run_transitioned(&test_run(RunStatus::Running));
        svc.shutdown(Duration::from_secs(1)).await;
        assert_eq!(server.received().len(), 2);
        assert!(start.elapsed() >= Duration::from_millis(40));

        let server = MockServer::start(vec![
            MockResponse::status(StatusCode::INTERNAL_SERVER_ERROR),
            MockResponse::status(StatusCode::BAD_GATEWAY),
            MockResponse::status(StatusCode::SERVICE_UNAVAILABLE),
        ])
        .await;
        let svc = service(&server.url(), all_events(), 25, DeliveryPolicy::test());
        svc.run_transitioned(&test_run(RunStatus::Running));
        svc.shutdown(Duration::from_secs(1)).await;
        assert_eq!(server.received().len(), 3);

        let server = MockServer::start(vec![MockResponse::status(StatusCode::BAD_REQUEST)]).await;
        let svc = service(&server.url(), all_events(), 25, DeliveryPolicy::test());
        svc.run_transitioned(&test_run(RunStatus::Running));
        svc.shutdown(Duration::from_secs(1)).await;
        assert_eq!(server.received().len(), 1);
    }

    #[tokio::test]
    async fn notifications_event_filtering_skips_unsubscribed_events() {
        let server = MockServer::start(vec![MockResponse::ok()]).await;
        let svc = service(
            &server.url(),
            vec![NotificationEvent::RunFailed],
            25,
            DeliveryPolicy::test(),
        );
        svc.run_transitioned(&test_run(RunStatus::Running));
        svc.shutdown(Duration::from_millis(100)).await;
        assert!(server.received().is_empty());
    }

    #[tokio::test]
    async fn notifications_run_started_precedes_immediate_task_message() {
        let server = MockServer::start(vec![MockResponse::ok(), MockResponse::ok()]).await;
        let svc = service(&server.url(), all_events(), 25, DeliveryPolicy::test());
        let run = test_run(RunStatus::Running);
        let (engine_tx, engine_rx) = broadcast::channel(16);
        let (crank_tx, crank_rx) = broadcast::channel(16);
        let guard = svc.listen(run.uuid, run.name.clone(), engine_rx, crank_rx);

        svc.run_transitioned(&run);
        engine_tx
            .send(EngineEvent::TaskFailed {
                id: "task-1".to_string(),
                name: "workflow.immediate".to_string(),
                error: "boom".to_string(),
            })
            .expect("send immediate task failure");
        drop(engine_tx);
        drop(crank_tx);
        guard.finish().await;
        svc.shutdown(Duration::from_secs(1)).await;

        let received = server.received();
        assert_eq!(received.len(), 2);
        assert_eq!(
            received[0]["blocks"][0]["text"]["text"],
            "Sprocket run started: test-run"
        );
        assert_eq!(
            received[1]["blocks"][0]["text"]["text"],
            "Sprocket task failed: test-run"
        );
    }

    #[tokio::test]
    async fn notifications_cap_suppresses_tasks_and_terminal_waits_for_finish() {
        let server = MockServer::start(vec![
            MockResponse::ok(),
            MockResponse::ok(),
            MockResponse::ok(),
        ])
        .await;
        let svc = service(&server.url(), all_events(), 2, DeliveryPolicy::test());
        let run = test_run(RunStatus::Failed);
        let (engine_tx, engine_rx) = broadcast::channel(16);
        let (crank_tx, crank_rx) = broadcast::channel(16);
        let guard = svc.listen(run.uuid, run.name.clone(), engine_rx, crank_rx);

        for index in 0..5 {
            engine_tx
                .send(EngineEvent::TaskFailed {
                    id: format!("task-{index}"),
                    name: format!("workflow.task_{index}"),
                    error: "boom".to_string(),
                })
                .expect("send task failure");
        }
        svc.run_transitioned(&run);
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(
            server
                .received()
                .iter()
                .all(|payload| payload["text"] != "Sprocket run failed: test-run"),
            "terminal message was sent before the run's task events were finished"
        );

        drop(engine_tx);
        drop(crank_tx);
        guard.finish().await;
        svc.shutdown(Duration::from_secs(1)).await;

        let received = server.received();
        assert_eq!(received.len(), 3);
        assert_eq!(
            received[0]["blocks"][0]["text"]["text"],
            "Sprocket task failed: test-run"
        );
        assert_eq!(
            received[1]["blocks"][0]["text"]["text"],
            "Sprocket task failed: test-run"
        );
        assert_eq!(
            received[2]["blocks"][0]["text"]["text"],
            "Sprocket run failed: test-run"
        );
        assert_eq!(
            received[2]["blocks"][2]["elements"][0]["text"],
            "3 more task events suppressed"
        );
    }

    #[tokio::test]
    async fn notifications_maps_crankshaft_preempted_task_name() {
        let server = MockServer::start(vec![MockResponse::ok()]).await;
        let svc = service(&server.url(), all_events(), 25, DeliveryPolicy::test());
        let run = test_run(RunStatus::Running);
        let (engine_tx, engine_rx) = broadcast::channel(16);
        let (crank_tx, crank_rx) = broadcast::channel(16);
        let guard = svc.listen(run.uuid, run.name.clone(), engine_rx, crank_rx);
        crank_tx
            .send(CrankshaftEvent::TaskCreated {
                id: 7,
                name: "workflow.preempted".to_string(),
                tes_id: None,
                token: CancellationToken::new(),
            })
            .expect("send task created");
        crank_tx
            .send(CrankshaftEvent::TaskPreempted { id: 7 })
            .expect("send preempted");
        drop(engine_tx);
        drop(crank_tx);
        guard.finish().await;
        svc.shutdown(Duration::from_secs(1)).await;

        let received = server.received();
        assert_eq!(received.len(), 1);
        assert_eq!(
            received[0]["blocks"][0]["text"]["text"],
            "Sprocket task preempted: test-run"
        );
        assert_eq!(
            received[0]["blocks"][1]["fields"][4]["text"],
            "*Task:*\nworkflow.preempted"
        );
    }

    #[tokio::test]
    async fn notifications_finish_drains_without_waiting_for_senders() {
        let server = MockServer::start(vec![MockResponse::ok()]).await;
        let svc = service(&server.url(), all_events(), 25, DeliveryPolicy::test());
        let run = test_run(RunStatus::Running);
        let (engine_tx, engine_rx) = broadcast::channel(16);
        let (_crank_tx, crank_rx) = broadcast::channel(16);
        let guard = svc.listen(run.uuid, run.name.clone(), engine_rx, crank_rx);
        engine_tx
            .send(EngineEvent::TaskFailed {
                id: "task-1".to_string(),
                name: "workflow.buffered".to_string(),
                error: "boom".to_string(),
            })
            .expect("send task failure");

        tokio::time::timeout(Duration::from_secs(1), guard.finish())
            .await
            .expect("finish should not wait for event senders to be dropped");
        svc.shutdown(Duration::from_secs(1)).await;

        let received = server.received();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0]["text"], "Sprocket task failed: test-run");
    }

    #[test]
    fn notifications_render_slack_escapes_control_sequences() {
        let mut message = base_message(NotificationEvent::TaskFailed);
        message.task_name = Some("workflow.<task>".to_string());
        message.error = Some("<!channel> usage: tool <input> & more".to_string());

        let payload = slack::render(&message);
        assert_eq!(
            payload["blocks"][2]["text"]["text"],
            "*Error:*\n```&lt;!channel&gt; usage: tool &lt;input&gt; &amp; more```"
        );
        let fields = payload["blocks"][1]["fields"].as_array().unwrap();
        assert!(
            fields
                .iter()
                .any(|field| field["text"] == "*Task:*\nworkflow.&lt;task&gt;"),
            "task name was not escaped: {fields:?}"
        );
    }

    #[tokio::test]
    async fn notifications_terminal_message_is_delivered_ahead_of_task_backlog() {
        let server = MockServer::start(Vec::new()).await;
        let policy = DeliveryPolicy {
            queue_capacity: 10,
            min_send_interval: Duration::from_millis(100),
            ..DeliveryPolicy::test()
        };
        let svc = service(&server.url(), all_events(), 25, policy);
        let run = test_run(RunStatus::Failed);
        let (engine_tx, engine_rx) = broadcast::channel(16);
        let (_crank_tx, crank_rx) = broadcast::channel(16);
        let guard = svc.listen(run.uuid, run.name.clone(), engine_rx, crank_rx);
        for index in 0..5 {
            engine_tx
                .send(EngineEvent::TaskFailed {
                    id: format!("task-{index}"),
                    name: format!("workflow.task_{index}"),
                    error: "boom".to_string(),
                })
                .expect("send task failure");
        }

        svc.run_transitioned(&run);
        guard.finish().await;
        // Too short to deliver the whole backlog at the send interval.
        svc.shutdown(Duration::from_millis(400)).await;

        let received = server.received();
        assert!(
            received
                .iter()
                .any(|payload| payload["text"] == "Sprocket run failed: test-run"),
            "terminal message was not delivered: {received:?}"
        );
        assert!(received.len() < 6, "the whole backlog was delivered");
    }

    #[tokio::test]
    async fn notifications_messages_after_terminal_are_suppressed() {
        let server = MockServer::start(Vec::new()).await;
        let svc = service(&server.url(), all_events(), 25, DeliveryPolicy::test());
        let mut run = test_run(RunStatus::Canceled);
        svc.run_transitioned(&run);

        // A run canceled while analyzing can still be started and listened to.
        let (engine_tx, engine_rx) = broadcast::channel(16);
        let (_crank_tx, crank_rx) = broadcast::channel(16);
        let guard = svc.listen(run.uuid, run.name.clone(), engine_rx, crank_rx);
        run.status = RunStatus::Running;
        svc.run_transitioned(&run);
        engine_tx
            .send(EngineEvent::TaskFailed {
                id: "task-1".to_string(),
                name: "workflow.task".to_string(),
                error: "boom".to_string(),
            })
            .expect("send task failure");
        guard.finish().await;
        run.status = RunStatus::Canceled;
        svc.run_transitioned(&run);
        svc.shutdown(Duration::from_secs(1)).await;

        let received = server.received();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0]["text"], "Sprocket run canceled: test-run");
        let inner = svc.inner.as_ref().unwrap();
        assert!(inner.state().runs.is_empty(), "run state was leaked");
    }

    #[tokio::test]
    async fn notifications_dropping_guard_releases_terminal_message() {
        let server = MockServer::start(vec![MockResponse::ok()]).await;
        let svc = service(&server.url(), all_events(), 25, DeliveryPolicy::test());
        let run = test_run(RunStatus::Failed);
        let (_engine_tx, engine_rx) = broadcast::channel(16);
        let (_crank_tx, crank_rx) = broadcast::channel(16);
        let guard = svc.listen(run.uuid, run.name.clone(), engine_rx, crank_rx);
        svc.run_transitioned(&run);
        drop(guard);
        svc.shutdown(Duration::from_secs(1)).await;
        assert_eq!(server.received().len(), 1);
    }

    #[tokio::test]
    async fn notifications_shutdown_after_terminal_still_delivers() {
        let server = MockServer::start(vec![MockResponse::ok()]).await;
        let svc = service(&server.url(), all_events(), 25, DeliveryPolicy::test());
        svc.run_transitioned(&test_run(RunStatus::Failed));
        svc.shutdown(Duration::from_secs(1)).await;
        assert_eq!(server.received().len(), 1);
    }

    #[tokio::test]
    async fn notifications_shutdown_returns_by_timeout_when_endpoint_hangs() {
        let server = MockServer::start(vec![MockResponse::Hang]).await;
        let svc = service(&server.url(), all_events(), 25, DeliveryPolicy::test());
        svc.run_transitioned(&test_run(RunStatus::Running));
        let start = Instant::now();
        svc.shutdown(Duration::from_millis(50)).await;
        assert!(start.elapsed() < Duration::from_millis(250));
    }

    #[tokio::test]
    async fn notifications_repeated_concurrent_failed_transitions_send_once() {
        let server = MockServer::start(vec![MockResponse::ok()]).await;
        let svc = service(&server.url(), all_events(), 25, DeliveryPolicy::test());
        let run = test_run(RunStatus::Failed);
        let mut handles = Vec::new();
        for _ in 0..10 {
            let svc = svc.clone();
            let run = run.clone();
            handles.push(tokio::spawn(async move {
                svc.run_transitioned(&run);
            }));
        }
        for handle in handles {
            handle.await.expect("transition task should complete");
        }
        svc.shutdown(Duration::from_secs(1)).await;
        assert_eq!(server.received().len(), 1);
    }

    #[tokio::test]
    async fn notifications_logs_do_not_include_webhook_url() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind closed port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);
        let url = format!("http://127.0.0.1:{port}/SECRET-SENTINEL");
        let writer = BufferWriter::default();
        let buffer = writer.buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(writer)
            .with_ansi(false)
            .finish();
        let _guard = set_default(subscriber);

        let svc = service(&url, all_events(), 25, DeliveryPolicy::test());
        svc.run_transitioned(&test_run(RunStatus::Running));
        svc.shutdown(Duration::from_secs(1)).await;

        let logs =
            String::from_utf8(buffer.lock().expect("buffer lock").clone()).expect("utf8 logs");
        assert!(
            logs.contains("webhook delivery failed"),
            "expected delivery failures to be logged: {logs}"
        );
        assert!(!logs.contains("SECRET-SENTINEL"), "logs leaked URL: {logs}");
    }

    #[tokio::test]
    async fn notifications_disabled_service_is_noop() {
        let svc = NotificationSvc::disabled();
        assert!(!svc.is_enabled());
        let (_engine_tx, engine_rx) = broadcast::channel(1);
        let (_crank_tx, crank_rx) = broadcast::channel(1);
        let guard = svc.listen(Uuid::new_v4(), "run".to_string(), engine_rx, crank_rx);
        svc.run_transitioned(&test_run(RunStatus::Running));
        guard.finish().await;
        svc.shutdown(Duration::from_millis(1)).await;
    }

    fn failed_message_with_long_error() -> (NotificationMessage, String) {
        let mut run = test_run(RunStatus::Failed);
        run.error = Some("x".repeat(501));
        let message = NotificationMessage::run(
            &run,
            NotificationEvent::RunFailed,
            Some(PathBuf::from("/outputs/runs/test-run")),
            "test-host".to_string(),
            0,
        );
        (message, format!("{}…", "x".repeat(MAX_ERROR_CHARS - 1)))
    }

    fn base_message(event: NotificationEvent) -> NotificationMessage {
        NotificationMessage {
            event,
            run_name: "test-run".to_string(),
            run_id: Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").expect("valid uuid"),
            target: Some("workflow.task".to_string()),
            status: Some("failed".to_string()),
            host: "test-host".to_string(),
            duration: Some("5s".to_string()),
            output_dir: Some("/outputs/runs/test-run".to_string()),
            task_name: None,
            attempt: None,
            exit_code: None,
            error: None,
            suppressed: 0,
        }
    }

    fn service(
        url: &str,
        events: Vec<NotificationEvent>,
        cap: u32,
        policy: DeliveryPolicy,
    ) -> NotificationSvc {
        NotificationSvc::new_with_policy(
            &NotificationsConfig {
                webhooks: vec![WebhookConfig {
                    name: Some("test webhook".to_string()),
                    kind: WebhookKind::Slack,
                    url: url.to_string().into(),
                    events,
                    max_task_messages_per_run: cap,
                }],
            },
            PathBuf::from("/outputs"),
            policy,
            "test-host".to_string(),
        )
    }

    fn all_events() -> Vec<NotificationEvent> {
        NotificationEvent::ALL.to_vec()
    }

    /// A webhook endpoint that records payloads and replies with scripted
    /// responses.
    struct MockServer {
        addr: SocketAddr,
        state: Arc<MockState>,
        handle: JoinHandle<()>,
    }

    impl MockServer {
        async fn start(responses: Vec<MockResponse>) -> Self {
            let state = Arc::new(MockState {
                received: StdMutex::new(Vec::new()),
                responses: StdMutex::new(VecDeque::from(responses)),
            });
            let app = Router::new()
                .route("/", post(mock_handler))
                .route("/SECRET-SENTINEL", post(mock_handler))
                .with_state(Arc::clone(&state));
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock");
            let addr = listener.local_addr().expect("mock addr");
            let handle = tokio::spawn(async move {
                axum::serve(listener, app).await.expect("mock server");
            });
            Self {
                addr,
                state,
                handle,
            }
        }

        fn url(&self) -> String {
            format!("http://{}", self.addr)
        }

        fn received(&self) -> Vec<Value> {
            self.state.received.lock().expect("received lock").clone()
        }
    }

    impl Drop for MockServer {
        fn drop(&mut self) {
            self.handle.abort();
        }
    }

    struct MockState {
        received: StdMutex<Vec<Value>>,
        responses: StdMutex<VecDeque<MockResponse>>,
    }

    #[derive(Clone)]
    enum MockResponse {
        Status(StatusCode, Vec<(HeaderName, HeaderValue)>),
        Hang,
    }

    impl MockResponse {
        fn ok() -> Self {
            Self::status(StatusCode::OK)
        }

        fn status(status: StatusCode) -> Self {
            Self::Status(status, Vec::new())
        }

        fn header(mut self, name: &'static str, value: &'static str) -> Self {
            if let Self::Status(_, headers) = &mut self {
                headers.push((
                    HeaderName::from_static(name),
                    HeaderValue::from_static(value),
                ));
            }
            self
        }
    }

    async fn mock_handler(
        AxumState(state): AxumState<Arc<MockState>>,
        Json(body): Json<Value>,
    ) -> axum::response::Response {
        state.received.lock().expect("received lock").push(body);
        let response = state
            .responses
            .lock()
            .expect("responses lock")
            .pop_front()
            .unwrap_or_else(MockResponse::ok);
        match response {
            MockResponse::Status(status, headers) => {
                let mut header_map = HeaderMap::new();
                for (name, value) in headers {
                    header_map.insert(name, value);
                }
                (status, header_map, "").into_response()
            }
            MockResponse::Hang => std::future::pending::<axum::response::Response>().await,
        }
    }

    #[derive(Clone, Default)]
    struct BufferWriter {
        buffer: Arc<StdMutex<Vec<u8>>>,
    }

    impl<'a> MakeWriter<'a> for BufferWriter {
        type Writer = Buffer;

        fn make_writer(&'a self) -> Self::Writer {
            Buffer {
                buffer: Arc::clone(&self.buffer),
            }
        }
    }

    struct Buffer {
        buffer: Arc<StdMutex<Vec<u8>>>,
    }

    impl io::Write for Buffer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.buffer
                .lock()
                .expect("buffer lock")
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
