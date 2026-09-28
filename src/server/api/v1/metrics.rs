//! Run execution metrics endpoint.

use axum::Json;
use axum::extract::Path;
use axum::extract::State;
use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use super::AppState;
use super::error::Error;
use super::send_command;
use super::tasks::Task;
use crate::server::api::v1::RunStatus;
use crate::server::api::v1::TaskStatus;
use crate::system::v1::exec::svc::run_manager::RunManagerCmd;

/// The page size used when collecting every task of a run.
const METRICS_PAGE_SIZE: i64 = 500;

/// Summary of the run a metrics report describes.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RunMetricsRun {
    /// Unique identifier of the run.
    pub uuid: Uuid,
    /// Name of the run.
    pub name: String,
    /// Current status of the run.
    pub status: RunStatus,
    /// Wall time of the run in milliseconds, from start to completion.
    ///
    /// `null` until the run has both started and reached a terminal status.
    pub wall_time_ms: Option<i64>,
    /// The name of the execution backend the run executed on.
    ///
    /// `null` for runs recorded before backends were tracked or that never
    /// began execution.
    pub backend: Option<String>,
    /// The version of Sprocket that produced this report.
    pub sprocket_version: String,
    /// The bytes transferred while localizing inputs and delocalizing
    /// outputs.
    ///
    /// `null` when no transfers were recorded.
    pub transfer: Option<TransferTotals>,
}

/// Metrics for a single execution attempt of a call.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TaskAttemptMetrics {
    /// The unique name of the attempt.
    pub name: String,
    /// The 0-based attempt number within the call.
    ///
    /// Backend-local resubmissions (names carrying a `~{n}` suffix) share the
    /// attempt number of the execution they resubmit.
    pub attempt: i64,
    /// The status of the attempt.
    pub status: TaskStatus,
    /// The exit status of the attempt, if it completed execution.
    pub exit_status: Option<i32>,
    /// Wall time of the attempt in milliseconds, from execution start to
    /// completion.
    ///
    /// `null` until the attempt has both started and reached a terminal
    /// status.
    pub wall_time_ms: Option<i64>,
    /// Time spent between the attempt's creation and the start of its
    /// execution, in milliseconds.
    ///
    /// This covers evaluation, input localization, and backend queuing.
    /// `null` until the attempt starts executing.
    pub queued_ms: Option<i64>,
    /// Time spent waiting in the backend's scheduler queue, in milliseconds:
    /// from submission to the backend until execution starts.
    ///
    /// `null` until the attempt starts executing, or for attempts recorded
    /// before submission times were tracked.
    pub pending_ms: Option<i64>,
    /// The allocated CPU time for the attempt, in core-milliseconds.
    ///
    /// This is the allocated CPU count multiplied by the attempt's execution
    /// wall time, reflecting what was reserved rather than what was consumed.
    /// `null` until the attempt completes or when its constraints were not
    /// recorded.
    pub allocated_cpu_time_ms: Option<i64>,
    /// The resolved execution constraints for the attempt (container, cpu,
    /// memory, gpu, fpga, disks).
    ///
    /// `null` when the attempt never reached a backend (e.g. served from the
    /// call cache).
    #[schema(value_type = Option<Object>)]
    pub constraints: Option<serde_json::Value>,
    /// Why the attempt was retried.
    ///
    /// `null` when the attempt was not retried.
    #[schema(value_type = Option<Object>)]
    pub retry_cause: Option<serde_json::Value>,
    /// The resource utilization observed for the attempt (resident memory,
    /// CPU time).
    ///
    /// Recorded at the attempt's termination by backends whose scheduler
    /// reports utilization (currently LSF and Slurm); `null` for other
    /// backends.
    #[schema(value_type = Option<Object>)]
    pub utilization: Option<serde_json::Value>,
    /// A reference to the attempt's logs.
    pub logs: String,
}

/// Metrics for every execution attempt of a single call.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CallMetrics {
    /// The stable WDL call path shared by every attempt of this call.
    ///
    /// This is unique within the run: calls made inside subworkflows are
    /// qualified by their call path, with levels joined by `--`. Falls back
    /// to the attempt name for tasks recorded without attempt attribution.
    pub call_id: String,
    /// The short display form of the call's identifier: the final level of
    /// the call path.
    ///
    /// Unlike [`call_id`](Self::call_id), this is not necessarily unique
    /// within the run.
    pub name: String,
    /// The execution attempts of this call, ordered by attempt number and
    /// creation time.
    pub attempts: Vec<TaskAttemptMetrics>,
}

/// Derives the short display form of a call identifier: the final level of
/// the `--`-joined call path.
///
/// Identifiers without a level separator (top-level calls and rows recorded
/// before call paths were qualified) are their own short form.
fn call_short_name(call_id: &str) -> String {
    call_id
        .rsplit_once("--")
        .map(|(_, name)| name)
        .unwrap_or(call_id)
        .to_string()
}

/// Totals across every execution attempt of a run.
#[derive(Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct RunMetricsTotals {
    /// The total number of execution attempts.
    pub attempts: i64,
    /// The number of attempts that were retried.
    pub retries: i64,
    /// The number of attempts served from the call cache.
    pub cached: i64,
    /// The number of attempts that were preempted.
    pub preempted: i64,
    /// The total allocated CPU time across all attempts, in
    /// core-milliseconds.
    ///
    /// This is the sum over attempts of the allocated CPU count multiplied by
    /// the attempt's execution wall time, including retried and failed
    /// attempts, reflecting what was reserved rather than what was consumed.
    pub allocated_cpu_time_ms: i64,
    /// The total execution wall time of preempted attempts, in milliseconds.
    ///
    /// This is the time wasted to preemption: work that was performed and
    /// then lost.
    pub preemption_wasted_ms: i64,
}

/// The bytes transferred while localizing inputs and delocalizing outputs.
///
/// This is a proxy for data movement (e.g. reviewing that egress stays at
/// zero), not a billing figure: it counts successful transfers whose size was
/// known when the transfer started.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TransferTotals {
    /// The total bytes downloaded, in bytes.
    pub downloaded_bytes: u64,
    /// The total bytes uploaded, in bytes.
    pub uploaded_bytes: u64,
}

/// The response for a run's execution metrics.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RunMetricsResponse {
    /// Summary of the run.
    pub run: RunMetricsRun,
    /// Per-call execution metrics.
    pub calls: Vec<CallMetrics>,
    /// Totals across all attempts.
    pub totals: RunMetricsTotals,
}

/// Computes the number of milliseconds between two optional timestamps.
fn duration_ms(start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) -> Option<i64> {
    Some((end? - start?).num_milliseconds())
}

/// Builds a metrics response from a run summary and its task rows.
///
/// Tasks are grouped by call id (falling back to the task name for rows
/// recorded without one), calls are ordered by the creation time of their
/// first attempt, and attempts within a call are ordered by attempt number and
/// creation time.
pub fn build_run_metrics(
    run: RunMetricsRun,
    mut tasks: Vec<Task>,
    logs_for: impl Fn(&str) -> String,
) -> RunMetricsResponse {
    tasks.sort_by(|a, b| {
        (a.attempt, a.created_at, a.name.as_str()).cmp(&(b.attempt, b.created_at, b.name.as_str()))
    });

    let mut totals = RunMetricsTotals {
        attempts: tasks.len() as i64,
        ..Default::default()
    };

    // Group attempts by call id, preserving first-attempt order.
    let mut calls: Vec<CallMetrics> = Vec::new();
    for task in tasks {
        let wall_time_ms = duration_ms(task.started_at, task.completed_at);

        match task.status {
            TaskStatus::Cached => totals.cached += 1,
            TaskStatus::Preempted => {
                totals.preempted += 1;
                // Time wasted to preemption: work performed and then lost.
                totals.preemption_wasted_ms += wall_time_ms.unwrap_or(0);
            }
            _ => {}
        }
        if task.retry_cause.is_some() {
            totals.retries += 1;
        }

        // Allocated CPU time reflects what was reserved rather than what was
        // consumed: the allocated CPU count times the execution wall time.
        let allocated_cpu_time_ms = match (
            task.constraints
                .as_ref()
                .and_then(|c| c.get("cpu"))
                .and_then(|c| c.as_f64()),
            wall_time_ms,
        ) {
            (Some(cpu), Some(wall)) => Some((cpu * wall as f64) as i64),
            _ => None,
        };
        totals.allocated_cpu_time_ms += allocated_cpu_time_ms.unwrap_or(0);

        let call_id = task.call_id.clone().unwrap_or_else(|| task.name.clone());
        let attempt = TaskAttemptMetrics {
            logs: logs_for(&task.name),
            name: task.name,
            attempt: task.attempt,
            status: task.status,
            exit_status: task.exit_status,
            wall_time_ms,
            queued_ms: duration_ms(Some(task.created_at), task.started_at),
            pending_ms: duration_ms(task.submitted_at, task.started_at),
            allocated_cpu_time_ms,
            constraints: task.constraints,
            retry_cause: task.retry_cause,
            utilization: task.utilization,
        };

        match calls.iter_mut().find(|c| c.call_id == call_id) {
            Some(call) => call.attempts.push(attempt),
            None => calls.push(CallMetrics {
                name: call_short_name(&call_id),
                call_id,
                attempts: vec![attempt],
            }),
        }
    }

    // Order calls by the creation order of their first attempt, which the
    // per-task sort above has already established within each call.
    RunMetricsResponse { run, calls, totals }
}

/// Get the execution metrics for a specific run.
///
/// Reports wall and queue times, execution attempts grouped by call, resolved
/// execution constraints, and retry causes.
#[utoipa::path(
    get,
    path = super::paths::RUN_METRICS,
    params(
        ("id" = String, Path, description = "Run ID")
    ),
    responses(
        (status = 200, description = "Metrics retrieved", body = RunMetricsResponse),
        (status = 404, description = "Run not found"),
    ),
    tag = "runs"
)]
pub async fn get_run_metrics(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<RunMetricsResponse>, Error> {
    let run = send_command(&state.run_manager_tx, |rx| RunManagerCmd::GetStatus {
        id,
        rx,
    })
    .await?
    .run;

    // Collect every task of the run, paging through the results.
    let mut tasks: Vec<Task> = Vec::new();
    loop {
        let response = send_command(&state.run_manager_tx, |rx| RunManagerCmd::ListTasks {
            run_id: Some(id),
            status: None,
            limit: Some(METRICS_PAGE_SIZE),
            offset: Some(tasks.len() as i64),
            rx,
        })
        .await?;

        let total = response.total;
        tasks.extend(response.tasks.into_iter().map(Task::from));
        if tasks.len() as i64 >= total || total == 0 {
            break;
        }
    }

    let run = RunMetricsRun {
        uuid: run.uuid,
        name: run.name,
        status: run.status,
        wall_time_ms: duration_ms(run.started_at, run.completed_at),
        backend: run.backend,
        sprocket_version: env!("CARGO_PKG_VERSION").to_string(),
        transfer: run
            .transfer_totals
            .as_deref()
            .and_then(|t| serde_json::from_str(t).ok()),
    };

    Ok(Json(build_run_metrics(run, tasks, |name| {
        super::paths::get_task_logs(name)
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_names_are_the_final_call_path_level() {
        // Top-level calls (and rows recorded before call paths were
        // qualified) are their own short form.
        assert_eq!(call_short_name("t"), "t");
        assert_eq!(call_short_name("ns-t-alias-3"), "ns-t-alias-3");

        // Qualified calls display the final level, which may itself contain
        // single `-` separators.
        assert_eq!(call_short_name("other-sub--t"), "t");
        assert_eq!(call_short_name("a--b--ns-t-alias-3"), "ns-t-alias-3");
    }
}
