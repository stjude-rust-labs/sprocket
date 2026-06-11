//! OpenTelemetry metrics for WDL workflow/task execution.
//!
//! Enabled by the `metrics` cargo feature and activated at runtime by the
//! `run --metrics-addr` flag. Subscribes to `wdl-engine`'s structured event
//! channel as a sibling of the run progress consumer (no execution-path
//! changes) and exposes a Prometheus `/metrics` endpoint.
//!
//! Task identity comes from the engine's structured `WdlTask*` events (the
//! un-mangled WDL task name) — no parsing of backend task ids.

use std::collections::HashMap;
use std::io::Read as _;
use std::io::Write as _;
use std::net::TcpListener;
use std::time::Instant;

use anyhow::Context as _;
use anyhow::Result;
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

/// Maps a task attempt's outcome to the `state` metric label.
fn task_state(exit_code: Option<i32>, canceled: bool) -> &'static str {
    if canceled {
        "canceled"
    } else if exit_code == Some(0) {
        "completed"
    } else {
        "failed"
    }
}

/// OpenTelemetry instruments for WDL execution. Cheap to clone (instruments are
/// `Arc`-backed); the owning provider is held to keep them alive.
#[derive(Clone)]
pub struct WdlMetrics {
    /// Owns the metric pipeline; held to keep the instruments alive.
    _provider: SdkMeterProvider,
    /// Task attempts reaching a terminal state, by workflow/task/state.
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
                .with_description("WDL task attempts reaching a terminal state")
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
    pub fn record_workflow(&self, workflow: &str, workflow_id: &str, status: &str, duration_secs: f64) {
        self.workflow_runs.add(1, &[
            KeyValue::new("workflow", workflow.to_string()),
            KeyValue::new("workflow_id", workflow_id.to_string()),
            KeyValue::new("status", status.to_string()),
        ]);
        self.workflow_duration.record(duration_secs, &[
            KeyValue::new("workflow", workflow.to_string()),
            KeyValue::new("workflow_id", workflow_id.to_string()),
        ]);
    }

    /// Spawns the engine-event subscriber (a sibling of `run::progress`).
    /// `workflow`/`workflow_id` label every task metric; `workflow_id` is
    /// high-cardinality by design (per-run drilldown).
    pub fn spawn_subscriber(
        &self,
        workflow: String,
        workflow_id: String,
        mut engine: Receiver<EngineEvent>,
    ) {
        let m = self.clone();
        tokio::spawn(async move {
            // task attempt id -> (start instant, task name)
            let mut running: HashMap<String, (Instant, String)> = HashMap::new();
            let mut dropped: u64 = 0;
            loop {
                match engine.recv().await {
                    Ok(ev) => m.on_engine(ev, &workflow, &workflow_id, &mut running),
                    Err(RecvError::Closed) => break,
                    Err(RecvError::Lagged(n)) => {
                        dropped += n;
                        tracing::warn!("WDL metrics subscriber lagged; dropped {n} events ({dropped} total)");
                    }
                }
            }
        });
    }

    /// Updates instruments from a structured wdl-engine event.
    fn on_engine(
        &self,
        ev: EngineEvent,
        workflow: &str,
        workflow_id: &str,
        running: &mut HashMap<String, (Instant, String)>,
    ) {
        let base = |task: &str| {
            vec![
                KeyValue::new("workflow", workflow.to_string()),
                KeyValue::new("workflow_id", workflow_id.to_string()),
                KeyValue::new("task", task.to_string()),
            ]
        };
        match ev {
            EngineEvent::WdlTaskStarted { id, name } => {
                self.in_flight.add(1, &base(&name));
                running.insert(id, (Instant::now(), name));
            }
            EngineEvent::WdlTaskCompleted { id, name, exit_code, canceled } => {
                let start = running.remove(&id).map(|(s, _)| s).unwrap_or_else(Instant::now);
                self.in_flight.add(-1, &base(&name));
                let mut attrs = base(&name);
                attrs.push(KeyValue::new("state", task_state(exit_code, canceled)));
                self.tasks.add(1, &attrs);
                self.task_duration.record(start.elapsed().as_secs_f64(), &attrs);
            }
            EngineEvent::ReusedCachedExecutionResult { name, .. } => {
                self.cache_hits.add(1, &base(&name));
            }
            EngineEvent::TaskParked => {
                self.parked.add(1, &[
                    KeyValue::new("workflow", workflow.to_string()),
                    KeyValue::new("workflow_id", workflow_id.to_string()),
                ]);
            }
            EngineEvent::TaskUnparked { .. } => {
                self.parked.add(-1, &[
                    KeyValue::new("workflow", workflow.to_string()),
                    KeyValue::new("workflow_id", workflow_id.to_string()),
                ]);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::task_state;

    #[test]
    fn task_state_maps_outcomes() {
        assert_eq!(task_state(Some(0), false), "completed");
        assert_eq!(task_state(Some(1), false), "failed");
        assert_eq!(task_state(None, false), "failed");
        assert_eq!(task_state(None, true), "canceled");
        assert_eq!(task_state(Some(0), true), "canceled");
    }
}
