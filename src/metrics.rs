//! Collection of execution metrics from engine and Crankshaft events.
//!
//! The dev server persists metrics to its database through the task monitor;
//! local runs have no database, so [`collect_metrics`] accumulates the same
//! information directly from the event streams and [`MetricsCollector`]
//! renders it into the same [`RunMetricsResponse`] shape the server's
//! `/api/v1/runs/{id}/metrics` endpoint reports. Local runs write that
//! response to a `metrics.json` file in the run directory.

use std::collections::HashMap;

use chrono::DateTime;
use chrono::Utc;
use cloud_copy::Location;
use cloud_copy::TransferEvent;
use crankshaft_events::Event as CrankshaftEvent;
use indexmap::IndexMap;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;
use wdl::engine::CLEANUP_TASK_NAME_PREFIX;
use wdl::engine::EngineEvent;

use crate::server::RunMetricsResponse;
use crate::server::RunMetricsRun;
use crate::server::Task;
use crate::server::TransferTotals;
use crate::server::build_run_metrics;
use crate::system::v1::db::TaskStatus;

/// A single execution attempt observed from the event streams.
#[derive(Debug, Clone)]
struct AttemptRecord {
    /// The stable WDL call path, when the engine announced it.
    call_id: Option<String>,
    /// The 0-based attempt number within the call.
    attempt: i64,
    /// The current status of the attempt.
    status: TaskStatus,
    /// The exit status, when the attempt completed execution.
    exit_status: Option<i32>,
    /// The resolved execution constraints, when the attempt reached a backend.
    constraints: Option<serde_json::Value>,
    /// Why the attempt was retried, when it was.
    retry_cause: Option<serde_json::Value>,
    /// The resource utilization observed for the attempt, when the backend
    /// reported it.
    utilization: Option<serde_json::Value>,
    /// When the attempt was first observed.
    created_at: DateTime<Utc>,
    /// When the attempt was submitted to an execution backend.
    submitted_at: Option<DateTime<Utc>>,
    /// When the attempt started executing.
    started_at: Option<DateTime<Utc>>,
    /// When the attempt reached a terminal status.
    completed_at: Option<DateTime<Utc>>,
}

impl AttemptRecord {
    /// Creates a record for an attempt first observed now.
    fn new(call_id: Option<String>, attempt: i64, status: TaskStatus) -> Self {
        Self {
            call_id,
            attempt,
            status,
            exit_status: None,
            constraints: None,
            retry_cause: None,
            utilization: None,
            created_at: Utc::now(),
            submitted_at: None,
            started_at: None,
            completed_at: None,
        }
    }
}

/// Returns the rank of a status in the task lifecycle.
///
/// The engine and Crankshaft event channels are independent and carry no
/// ordering relative to one another, so transitions are kept monotonic: a
/// status only ever advances.
fn status_rank(status: TaskStatus) -> u8 {
    match status {
        TaskStatus::Initializing => 0,
        TaskStatus::Localizing => 1,
        TaskStatus::Pending => 2,
        TaskStatus::Running => 3,
        TaskStatus::Completed
        | TaskStatus::Cached
        | TaskStatus::Failed
        | TaskStatus::Canceled
        | TaskStatus::Preempted
        | TaskStatus::Orphaned => 4,
    }
}

/// Accumulates transfer byte totals from the transfer event stream.
///
/// Totals count successful transfers whose size was known when the transfer
/// started; direction is classified by the destination (a URL destination is
/// an upload, a local path destination is a download).
#[derive(Debug, Default)]
pub struct TransferAccumulator {
    /// In-flight transfers: id to (is upload, size in bytes).
    pending: HashMap<u64, (bool, u64)>,
    /// The accumulated totals.
    totals: TransferTotals,
    /// Whether any transfer was observed at all.
    observed: bool,
}

impl TransferAccumulator {
    /// Handles a transfer event.
    pub fn handle_event(&mut self, event: &TransferEvent) {
        match event {
            TransferEvent::TransferStarted {
                id,
                destination,
                size,
                ..
            } => {
                self.observed = true;
                if let Some(size) = size {
                    let upload = matches!(destination, Location::Url(_));
                    self.pending.insert(*id, (upload, *size));
                }
            }
            TransferEvent::TransferCompleted { id, failed } => {
                if let Some((upload, size)) = self.pending.remove(id)
                    && !failed
                {
                    if upload {
                        self.totals.uploaded_bytes += size;
                    } else {
                        self.totals.downloaded_bytes += size;
                    }
                }
            }
            _ => {}
        }
    }

    /// Returns the accumulated totals, or `None` if no transfer was observed.
    pub fn totals(&self) -> Option<TransferTotals> {
        self.observed.then_some(self.totals)
    }
}

/// Accumulates execution metrics from the engine and Crankshaft event
/// streams.
#[derive(Debug, Default)]
pub struct MetricsCollector {
    /// Attempts observed so far, keyed by attempt name.
    attempts: IndexMap<String, AttemptRecord>,
    /// Maps Crankshaft task ids to attempt names.
    task_names: HashMap<u64, String>,
    /// Accumulates transfer byte totals.
    transfers: TransferAccumulator,
}

impl MetricsCollector {
    /// Gets an attempt record, creating it if this is the first observation.
    fn attempt(
        &mut self,
        name: &str,
        call_id: Option<&str>,
        attempt: i64,
        status: TaskStatus,
    ) -> &mut AttemptRecord {
        let record = self
            .attempts
            .entry(name.to_string())
            .or_insert_with(|| AttemptRecord::new(call_id.map(str::to_string), attempt, status));

        // Whichever channel observes the attempt first creates the record;
        // only the engine's announcement carries the call id and attempt
        // number, so fill them in when they arrive late.
        if record.call_id.is_none() {
            record.call_id = call_id.map(str::to_string);
        }
        record.attempt = record.attempt.max(attempt);
        record
    }

    /// Advances an attempt's status, keeping transitions monotonic.
    fn advance(&mut self, name: &str, status: TaskStatus) {
        let record = self.attempt(name, None, 0, status);
        if status_rank(status) > status_rank(record.status) {
            record.status = status;
        }
    }

    /// Handles an engine event.
    fn handle_engine_event(&mut self, event: EngineEvent) {
        match event {
            EngineEvent::TaskInitializing { id, name, attempt } => {
                self.attempt(
                    &name,
                    Some(&id),
                    attempt.try_into().unwrap_or(i64::MAX),
                    TaskStatus::Initializing,
                );
            }
            EngineEvent::TaskLocalizing { name } => {
                self.advance(&name, TaskStatus::Localizing);
            }
            EngineEvent::TaskExecuting { name, constraints } => {
                let constraints = serde_json::to_value(&constraints).ok();
                self.attempt(&name, None, 0, TaskStatus::Initializing)
                    .constraints = constraints;
            }
            EngineEvent::TaskRetrying {
                prior_name,
                next_name,
                cause,
            } => {
                // Record the cause on the attempt that failed and create the
                // successor, inheriting the call id and attempt number. An
                // evaluator retry is followed by a `TaskInitializing` event
                // that raises the attempt number; a backend-local
                // resubmission is not, and remains part of the same attempt.
                let cause = serde_json::to_value(cause).ok();
                let (call_id, attempt, constraints) = {
                    let record = self.attempt(&prior_name, None, 0, TaskStatus::Initializing);
                    record.retry_cause = cause;
                    (
                        record.call_id.clone(),
                        record.attempt,
                        record.constraints.clone(),
                    )
                };
                self.attempt(
                    &next_name,
                    call_id.as_deref(),
                    attempt,
                    TaskStatus::Initializing,
                )
                .constraints = constraints;
            }
            EngineEvent::ReusedCachedExecutionResult { id, name } => {
                let record = self.attempt(&name, Some(&id), 0, TaskStatus::Initializing);
                record.status = TaskStatus::Cached;
                record.completed_at = Some(Utc::now());
            }
            EngineEvent::TaskDiskUsage { name, disk_used } => {
                // Merge the engine-measured disk usage into whatever the
                // backend may have reported.
                let record = self.attempt(&name, None, 0, TaskStatus::Initializing);
                let utilization = record
                    .utilization
                    .get_or_insert_with(|| serde_json::json!({}));
                if let Some(map) = utilization.as_object_mut() {
                    map.insert("disk_used".to_string(), disk_used.into());
                }
            }
            EngineEvent::TaskParked | EngineEvent::TaskUnparked { .. } => {}
            _ => {}
        }
    }

    /// Handles a Crankshaft event.
    fn handle_crankshaft_event(&mut self, event: CrankshaftEvent) {
        match event {
            CrankshaftEvent::TaskCreated { id, name, .. } => {
                // Work a backend runs on its own behalf is not a task of the
                // workflow; see `CLEANUP_TASK_NAME_PREFIX`.
                if name.starts_with(CLEANUP_TASK_NAME_PREFIX) {
                    return;
                }

                self.task_names.insert(id, name.clone());
                self.advance(&name, TaskStatus::Pending);
                let record = self.attempt(&name, None, 0, TaskStatus::Pending);
                if record.submitted_at.is_none() {
                    record.submitted_at = Some(Utc::now());
                }
            }
            CrankshaftEvent::TaskStarted { id } => {
                if let Some(name) = self.task_names.get(&id).cloned() {
                    self.advance(&name, TaskStatus::Running);
                    let record = self.attempt(&name, None, 0, TaskStatus::Running);
                    if record.started_at.is_none() {
                        record.started_at = Some(Utc::now());
                    }
                }
            }
            CrankshaftEvent::TaskCompleted { id, exit_statuses } => {
                if let Some(name) = self.task_names.get(&id).cloned() {
                    let exit_status = exit_statuses.last().code();
                    self.finish(&name, TaskStatus::Completed, exit_status);
                }
            }
            CrankshaftEvent::TaskFailed { id, .. } => {
                if let Some(name) = self.task_names.get(&id).cloned() {
                    self.finish(&name, TaskStatus::Failed, None);
                }
            }
            CrankshaftEvent::TaskCanceled { id } => {
                if let Some(name) = self.task_names.get(&id).cloned() {
                    self.finish(&name, TaskStatus::Canceled, None);
                }
            }
            CrankshaftEvent::TaskPreempted { id } => {
                if let Some(name) = self.task_names.get(&id).cloned() {
                    self.finish(&name, TaskStatus::Preempted, None);
                }
            }
            CrankshaftEvent::TaskResourceUsage { id, usage } => {
                // Utilization is a cumulative snapshot: the last received is
                // authoritative.
                if let Some(name) = self.task_names.get(&id).cloned() {
                    let usage = serde_json::to_value(&usage).ok();
                    self.attempt(&name, None, 0, TaskStatus::Initializing)
                        .utilization = usage;
                }
            }
            _ => {}
        }
    }

    /// Records an attempt reaching a terminal status.
    fn finish(&mut self, name: &str, status: TaskStatus, exit_status: Option<i32>) {
        let record = self.attempt(name, None, 0, status);
        if status_rank(record.status) < status_rank(status) || record.completed_at.is_none() {
            record.status = status;
            record.exit_status = exit_status;
            record.completed_at = Some(Utc::now());
        }
    }

    /// Returns the accumulated transfer totals, or `None` if no transfer
    /// was observed.
    pub fn transfer_totals(&self) -> Option<TransferTotals> {
        self.transfers.totals()
    }

    /// Renders the collected metrics into the shared response shape.
    ///
    /// Log references are best-effort relative paths within the run
    /// directory, pointing at each attempt's directory.
    pub fn into_response(self, run: RunMetricsRun) -> RunMetricsResponse {
        let run_uuid = run.uuid;
        let tasks: Vec<Task> = self
            .attempts
            .into_iter()
            .map(|(name, record)| Task {
                name,
                run_uuid,
                status: record.status,
                exit_status: record.exit_status,
                error: None,
                call_id: record.call_id,
                attempt: record.attempt,
                constraints: record.constraints,
                retry_cause: record.retry_cause,
                utilization: record.utilization,
                created_at: record.created_at,
                submitted_at: record.submitted_at,
                started_at: record.started_at,
                completed_at: record.completed_at,
            })
            .collect();

        build_run_metrics(run, tasks, |name| name.to_string())
    }
}

/// Collects execution metrics from the event streams until both close.
///
/// This is intended to be spawned alongside evaluation; once evaluation
/// returns and the event senders drop, the collector drains and returns.
pub async fn collect_metrics(
    mut crankshaft: broadcast::Receiver<CrankshaftEvent>,
    mut engine: broadcast::Receiver<EngineEvent>,
    mut transfer: broadcast::Receiver<TransferEvent>,
) -> MetricsCollector {
    let mut collector = MetricsCollector::default();
    let mut crankshaft_open = true;
    let mut engine_open = true;
    let mut transfer_open = true;

    while crankshaft_open || engine_open || transfer_open {
        select! {
            r = crankshaft.recv(), if crankshaft_open => match r {
                Ok(event) => collector.handle_crankshaft_event(event),
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => crankshaft_open = false,
            },
            r = engine.recv(), if engine_open => match r {
                Ok(event) => collector.handle_engine_event(event),
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => engine_open = false,
            },
            r = transfer.recv(), if transfer_open => match r {
                Ok(event) => collector.transfers.handle_event(&event),
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => transfer_open = false,
            },
        }
    }

    collector
}

/// Builds the run summary for a locally executed run.
pub fn local_run_summary(
    run_id: Uuid,
    name: &str,
    status: crate::system::v1::db::RunStatus,
    started_at: DateTime<Utc>,
    backend: Option<String>,
    transfer: Option<TransferTotals>,
) -> RunMetricsRun {
    RunMetricsRun {
        uuid: run_id,
        name: name.to_string(),
        status,
        wall_time_ms: Some((Utc::now() - started_at).num_milliseconds()),
        backend,
        sprocket_version: env!("CARGO_PKG_VERSION").to_string(),
        transfer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_events_accumulate_attempts_constraints_and_retries() {
        let mut collector = MetricsCollector::default();

        collector.handle_engine_event(EngineEvent::TaskInitializing {
            id: "wf-t".to_string(),
            name: "wf-t-x1".to_string(),
            attempt: 0,
        });
        let constraints = serde_json::from_value(serde_json::json!({
            "container": "docker://ubuntu:latest",
            "cpu": 2.0,
            "memory": 1024,
            "gpu": [],
            "fpga": [],
            "disks": {},
            "max_retries": 1,
            "preemptible": 0,
            "max_cpu": null,
            "max_memory": null
        }))
        .expect("constraints should deserialize");
        collector.handle_engine_event(EngineEvent::TaskExecuting {
            name: "wf-t-x1".to_string(),
            constraints,
        });
        collector.handle_engine_event(EngineEvent::TaskRetrying {
            prior_name: "wf-t-x1".to_string(),
            next_name: "wf-t-x2".to_string(),
            cause: wdl::engine::RetryCause::UnacceptableExitCode { code: 137 },
        });
        collector.handle_engine_event(EngineEvent::TaskInitializing {
            id: "wf-t".to_string(),
            name: "wf-t-x2".to_string(),
            attempt: 1,
        });

        let run = RunMetricsRun {
            uuid: Uuid::nil(),
            name: "test".to_string(),
            status: crate::system::v1::db::RunStatus::Completed,
            wall_time_ms: None,
            backend: None,
            sprocket_version: "0.0.0-test".to_string(),
            transfer: None,
        };
        let response = collector.into_response(run);

        assert_eq!(response.totals.attempts, 2);
        assert_eq!(response.totals.retries, 1);
        assert_eq!(response.calls.len(), 1);

        let call = &response.calls[0];
        assert!(
            call.attempts
                .iter()
                .all(|attempt| attempt.constraints.is_some())
        );
        assert_eq!(call.call_id, "wf-t");
        assert_eq!(call.attempts.len(), 2);
        assert_eq!(call.attempts[0].name, "wf-t-x1");
        assert_eq!(call.attempts[0].attempt, 0);
        assert_eq!(
            call.attempts[0].constraints.as_ref().unwrap()["cpu"]
                .as_f64()
                .unwrap(),
            2.0
        );
        assert_eq!(
            call.attempts[0].retry_cause.as_ref().unwrap()["kind"],
            "unacceptable_exit_code"
        );
        // The successor inherits the call id; its attempt number comes from
        // its own `TaskInitializing` announcement.
        assert_eq!(call.attempts[1].name, "wf-t-x2");
        assert_eq!(call.attempts[1].attempt, 1);
        assert!(call.attempts[1].retry_cause.is_none());
    }

    #[test]
    fn statuses_only_advance() {
        let mut collector = MetricsCollector::default();

        collector.handle_engine_event(EngineEvent::TaskInitializing {
            id: "wf-t".to_string(),
            name: "wf-t-x1".to_string(),
            attempt: 0,
        });
        collector.advance("wf-t-x1", TaskStatus::Pending);
        // A late localizing event must not drag the attempt backwards.
        collector.handle_engine_event(EngineEvent::TaskLocalizing {
            name: "wf-t-x1".to_string(),
        });

        assert_eq!(collector.attempts["wf-t-x1"].status, TaskStatus::Pending);
    }

    #[test]
    fn cached_results_are_terminal() {
        let mut collector = MetricsCollector::default();

        collector.handle_engine_event(EngineEvent::ReusedCachedExecutionResult {
            id: "wf-t".to_string(),
            name: "wf-t-x1".to_string(),
        });

        let record = &collector.attempts["wf-t-x1"];
        assert_eq!(record.status, TaskStatus::Cached);
        assert!(record.completed_at.is_some());
    }

    #[test]
    fn utilization_is_recorded_on_the_attempt() {
        let mut collector = MetricsCollector::default();

        collector.handle_engine_event(EngineEvent::TaskInitializing {
            id: "wf-t".to_string(),
            name: "wf-t-x1".to_string(),
            attempt: 0,
        });
        // The Crankshaft channel announces the task and then reports its
        // resource usage by task id.
        collector.handle_crankshaft_event(CrankshaftEvent::TaskCreated {
            id: 7,
            name: "wf-t-x1".to_string(),
            tes_id: None,
            token: Default::default(),
        });
        let mut usage = crankshaft_events::TaskResourceUsage::default();
        usage.max_memory = Some(241_172_480);
        usage.cpu_time_ms = Some(324_000);
        collector.handle_crankshaft_event(CrankshaftEvent::TaskResourceUsage { id: 7, usage });

        let utilization = collector.attempts["wf-t-x1"]
            .utilization
            .as_ref()
            .expect("utilization should be recorded");
        assert_eq!(utilization["max_memory"], 241_172_480u64);
        assert_eq!(utilization["cpu_time_ms"], 324_000);
    }

    #[test]
    fn transfers_total_successful_sized_transfers_by_direction() {
        use std::path::PathBuf;

        let mut acc = TransferAccumulator::default();
        assert_eq!(acc.totals(), None);

        // A successful download.
        acc.handle_event(&TransferEvent::TransferStarted {
            id: 1,
            source: Location::Url("https://example.com/in".parse().unwrap()),
            destination: Location::Path(PathBuf::from("/tmp/in")),
            blocks: 1,
            size: Some(1024),
        });
        acc.handle_event(&TransferEvent::TransferCompleted {
            id: 1,
            failed: false,
        });

        // A successful upload.
        acc.handle_event(&TransferEvent::TransferStarted {
            id: 2,
            source: Location::Path(PathBuf::from("/tmp/out")),
            destination: Location::Url("https://example.com/out".parse().unwrap()),
            blocks: 1,
            size: Some(2048),
        });
        acc.handle_event(&TransferEvent::TransferCompleted {
            id: 2,
            failed: false,
        });

        // A failed transfer contributes nothing.
        acc.handle_event(&TransferEvent::TransferStarted {
            id: 3,
            source: Location::Url("https://example.com/bad".parse().unwrap()),
            destination: Location::Path(PathBuf::from("/tmp/bad")),
            blocks: 1,
            size: Some(4096),
        });
        acc.handle_event(&TransferEvent::TransferCompleted {
            id: 3,
            failed: true,
        });

        let totals = acc.totals().expect("transfers were observed");
        assert_eq!(totals.downloaded_bytes, 1024);
        assert_eq!(totals.uploaded_bytes, 2048);
    }

    #[test]
    fn disk_usage_merges_into_utilization() {
        let mut collector = MetricsCollector::default();

        collector.handle_engine_event(EngineEvent::TaskInitializing {
            id: "wf-t".to_string(),
            name: "wf-t-x1".to_string(),
            attempt: 0,
        });
        collector.handle_engine_event(EngineEvent::TaskDiskUsage {
            name: "wf-t-x1".to_string(),
            disk_used: 4096,
        });

        let utilization = collector.attempts["wf-t-x1"]
            .utilization
            .as_ref()
            .expect("utilization should be recorded");
        assert_eq!(utilization["disk_used"], 4096);
    }
}
