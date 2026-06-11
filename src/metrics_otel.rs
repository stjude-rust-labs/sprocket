//! OpenTelemetry metrics for WDL workflow/task execution.
//!
//! Enabled by the `metrics` cargo feature and activated at runtime by the
//! `run --metrics-addr` flag. Rides `wdl-engine`'s existing broadcast event
//! channels as a sibling of the run progress consumer (no execution-path
//! changes) and exposes a Prometheus `/metrics` endpoint.

use std::collections::HashMap;
use std::io::Read as _;
use std::io::Write as _;
use std::net::TcpListener;
use std::time::Instant;

use anyhow::Context as _;
use anyhow::Result;
use crankshaft::events::Event as CrankshaftEvent;
use opentelemetry::KeyValue;
use opentelemetry::metrics::Counter;
use opentelemetry::metrics::Histogram;
use opentelemetry::metrics::MeterProvider as _;
use opentelemetry::metrics::UpDownCounter;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use prometheus::Encoder as _;
use prometheus::Registry;
use prometheus::TextEncoder;
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;
use wdl::engine::EngineEvent;

/// OpenTelemetry instruments for WDL execution. Cheap to clone (instruments are
/// `Arc`-backed); the owning provider is held to keep them alive.
#[derive(Clone)]
pub struct WdlMetrics {
    /// Owns the metric pipeline; held to keep the instruments alive.
    _provider: SdkMeterProvider,
    /// Tasks reaching a terminal state, by workflow/task/state/kind.
    tasks: Counter<u64>,
    /// Wall-clock duration of a task attempt.
    task_duration: Histogram<f64>,
    /// Tasks currently executing, by workflow/task.
    in_flight: UpDownCounter<i64>,
    /// Task executions served from the call cache.
    cache_hits: Counter<u64>,
    /// Tasks parked waiting on local resources.
    parked: UpDownCounter<i64>,
    /// Completed workflow runs, by workflow/status.
    workflow_runs: Counter<u64>,
    /// Wall-clock duration of a whole workflow run.
    workflow_duration: Histogram<f64>,
}

impl std::fmt::Debug for WdlMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WdlMetrics")
    }
}

impl WdlMetrics {
    /// Initializes metrics and starts the Prometheus `/metrics` server at `addr`.
    pub fn init(addr: &str) -> Result<Self> {
        let registry = Registry::new();
        let exporter = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            .build()
            .context("failed to build the Prometheus exporter")?;
        let provider = SdkMeterProvider::builder()
            .with_reader(exporter)
            .with_resource(Resource::new(vec![KeyValue::new("service.name", "sprocket")]))
            .build();
        let meter = provider.meter("sprocket");

        let metrics = Self {
            tasks: meter
                .u64_counter("sprocket_wdl_tasks")
                .with_description("WDL tasks reaching a terminal state")
                .build(),
            task_duration: meter
                .f64_histogram("sprocket_wdl_task_duration_seconds")
                .with_description("Wall-clock duration of a WDL task attempt")
                .build(),
            in_flight: meter
                .i64_up_down_counter("sprocket_wdl_tasks_in_flight")
                .with_description("WDL tasks currently executing")
                .build(),
            cache_hits: meter
                .u64_counter("sprocket_wdl_cache_hits")
                .with_description("Task executions served from the call cache")
                .build(),
            parked: meter
                .i64_up_down_counter("sprocket_wdl_tasks_parked")
                .with_description("Tasks parked waiting on local resources")
                .build(),
            workflow_runs: meter
                .u64_counter("sprocket_wdl_workflow_runs")
                .with_description("Completed workflow runs, by workflow and status")
                .build(),
            workflow_duration: meter
                .f64_histogram("sprocket_wdl_workflow_duration_seconds")
                .with_description("Wall-clock duration of a whole workflow run")
                .build(),
            _provider: provider,
        };

        start_metrics_server(addr, registry)?;
        tracing::info!("WDL metrics exposed at http://{addr}/metrics");
        Ok(metrics)
    }

    /// Records the result of a whole workflow run (timed at the run.rs call site,
    /// since the engine emits no workflow-level lifecycle event).
    pub fn record_workflow(&self, workflow: &str, status: &str, duration_secs: f64) {
        self.workflow_runs.add(1, &[
            KeyValue::new("workflow", workflow.to_string()),
            KeyValue::new("status", status.to_string()),
        ]);
        self.workflow_duration
            .record(duration_secs, &[KeyValue::new("workflow", workflow.to_string())]);
    }

    /// Spawns the event subscriber (a sibling of `run::progress`). `workflow`
    /// labels every task metric (low cardinality).
    pub fn spawn_subscriber(
        &self,
        workflow: String,
        mut crankshaft: Receiver<CrankshaftEvent>,
        mut engine: Receiver<EngineEvent>,
    ) {
        let m = self.clone();
        tokio::spawn(async move {
            // crankshaft task id -> (start instant, task_name, kind)
            let mut running: HashMap<u64, (Instant, String, &'static str)> = HashMap::new();
            let mut names: HashMap<u64, (String, &'static str)> = HashMap::new();
            let mut dropped: u64 = 0;

            loop {
                tokio::select! {
                    r = crankshaft.recv() => match r {
                        Ok(ev) => m.on_crankshaft(ev, &workflow, &mut names, &mut running),
                        Err(RecvError::Closed) => break,
                        Err(RecvError::Lagged(n)) => {
                            dropped += n;
                            tracing::warn!("WDL metrics subscriber lagged; dropped {n} crankshaft events ({dropped} total)");
                        }
                    },
                    r = engine.recv() => match r {
                        Ok(ev) => m.on_engine(ev, &workflow),
                        Err(RecvError::Closed) => {}
                        Err(RecvError::Lagged(n)) => {
                            dropped += n;
                            tracing::warn!("WDL metrics subscriber lagged; dropped {n} engine events ({dropped} total)");
                        }
                    },
                }
            }
        });
    }

    /// Updates instruments from a crankshaft task lifecycle event.
    fn on_crankshaft(
        &self,
        ev: CrankshaftEvent,
        workflow: &str,
        names: &mut HashMap<u64, (String, &'static str)>,
        running: &mut HashMap<u64, (Instant, String, &'static str)>,
    ) {
        match ev {
            CrankshaftEvent::TaskCreated { id, name, .. } => {
                let (task, _shard) = split_shard(&name.to_string());
                let kind = classify_kind(&task);
                names.insert(id, (task, kind));
            }
            CrankshaftEvent::TaskStarted { id } => {
                let (task, kind) = names
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| ("unknown".into(), "wdl"));
                self.in_flight.add(1, &[
                    KeyValue::new("workflow", workflow.to_string()),
                    KeyValue::new("task", task.clone()),
                ]);
                running.insert(id, (Instant::now(), task, kind));
            }
            CrankshaftEvent::TaskCompleted { id, .. } => self.terminal(id, "completed", workflow, running),
            CrankshaftEvent::TaskFailed { id, .. } => self.terminal(id, "failed", workflow, running),
            CrankshaftEvent::TaskPreempted { id } => self.terminal(id, "preempted", workflow, running),
            CrankshaftEvent::TaskCanceled { id } => self.terminal(id, "canceled", workflow, running),
            _ => {}
        }
    }

    /// Updates instruments from a wdl-engine event (cache hits, parking).
    fn on_engine(&self, ev: EngineEvent, workflow: &str) {
        match ev {
            EngineEvent::ReusedCachedExecutionResult { id } => {
                let (task, _shard) = split_shard(&id);
                self.cache_hits.add(1, &[
                    KeyValue::new("workflow", workflow.to_string()),
                    KeyValue::new("task", task),
                ]);
            }
            EngineEvent::TaskParked => {
                self.parked.add(1, &[KeyValue::new("workflow", workflow.to_string())]);
            }
            EngineEvent::TaskUnparked { .. } => {
                self.parked.add(-1, &[KeyValue::new("workflow", workflow.to_string())]);
            }
        }
    }

    /// Records a task reaching a terminal `state`: decrement in-flight, bump the
    /// counter, and record the attempt duration.
    fn terminal(
        &self,
        id: u64,
        state: &str,
        workflow: &str,
        running: &mut HashMap<u64, (Instant, String, &'static str)>,
    ) {
        let (start, task, kind) = running
            .remove(&id)
            .unwrap_or_else(|| (Instant::now(), "unknown".into(), "wdl"));
        self.in_flight.add(-1, &[
            KeyValue::new("workflow", workflow.to_string()),
            KeyValue::new("task", task.clone()),
        ]);
        let attrs = [
            KeyValue::new("workflow", workflow.to_string()),
            KeyValue::new("task", task),
            KeyValue::new("state", state.to_string()),
            KeyValue::new("kind", kind),
        ];
        self.tasks.add(1, &attrs);
        self.task_duration.record(start.elapsed().as_secs_f64(), &attrs);
    }
}

/// Serves the Prometheus registry on `addr` from a dedicated OS thread.
fn start_metrics_server(addr: &str, registry: Registry) -> Result<()> {
    let listener = TcpListener::bind(addr).with_context(|| format!("failed to bind {addr}"))?;
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let mut body = Vec::new();
            let _ = TextEncoder::new().encode(&registry.gather(), &mut body);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    Ok(())
}

/// Splits a crankshaft task id into a stable `task_name` and an optional shard.
///
/// The live id format is `{task}-[{scatter_index}-...]{nonce}` (e.g.
/// `align-0-Urx5FjZomn5B`): a trailing uniqueness nonce, with scatter index
/// segments before it. We drop the nonce, then strip trailing purely-numeric
/// shard segments, so scatter shards aggregate under one `task_name`.
///
/// NOTE: interim — this string-munging is brittle; the production fix is
/// structured identity on the engine event (see the implementation spec).
fn split_shard(name: &str) -> (String, Option<String>) {
    let mut parts: Vec<&str> = name.split('-').collect();
    // Drop the trailing crankshaft uniqueness nonce (always the last segment).
    if parts.len() > 1 {
        parts.pop();
    }
    // Strip trailing purely-numeric scatter-index segments.
    let mut split = parts.len();
    while split > 1
        && !parts[split - 1].is_empty()
        && parts[split - 1].bytes().all(|b| b.is_ascii_digit())
    {
        split -= 1;
    }
    let task = parts[..split].join("-");
    let shard = if split < parts.len() {
        Some(parts[split..].join("-"))
    } else {
        None
    };
    (task, shard)
}

/// Classifies a task as a WDL task (`wdl`) or a backend-injected helper
/// (`internal`, e.g. `docker-chown`), for the metric `kind` label.
fn classify_kind(task_name: &str) -> &'static str {
    // Interim heuristic: backend-injected helper tasks use a `docker-` prefix.
    // The robust signal will come from structured engine identity (spec PR3).
    if task_name.starts_with("docker-") {
        "internal"
    } else {
        "wdl"
    }
}

#[cfg(test)]
mod tests {
    use super::classify_kind;
    use super::split_shard;

    #[test]
    fn split_shard_extracts_task_and_shard_from_real_ids() {
        // real formats observed against live sprocket
        assert_eq!(split_shard("align-0-Urx5FjZomn5B"), ("align".into(), Some("0".into())));
        assert_eq!(
            split_shard("call_variants-3-mJnJUlEMv8J7"),
            ("call_variants".into(), Some("3".into()))
        );
        // internal helper whose base name contains a hyphen, no scatter
        assert_eq!(split_shard("docker-chown-5jC5YIQHhXIK"), ("docker-chown".into(), None));
        // nested scatter: drop nonce, strip both numeric levels
        assert_eq!(split_shard("inner-0-1-NONCEabcd1234"), ("inner".into(), Some("0-1".into())));
    }

    #[test]
    fn classify_kind_distinguishes_internal_helpers() {
        assert_eq!(classify_kind("align"), "wdl");
        assert_eq!(classify_kind("haplotype_caller"), "wdl");
        assert_eq!(classify_kind("docker-chown"), "internal");
    }
}
