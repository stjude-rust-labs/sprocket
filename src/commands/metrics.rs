//! Implementation of the `metrics` subcommand.

use anyhow::Context;
use clap::Parser;
use colored::Colorize as _;

use crate::commands::CommandResult;
use crate::commands::client::ServerConnectionArgs;
use crate::commands::client::get_json;
use crate::commands::client::resolve_run_id;
use crate::commands::inspect::status_color;
use crate::commands::inspect::task_status_color;
use crate::config::Config;
use crate::server::CallMetrics;
use crate::server::RunMetricsResponse;
use crate::server::TaskAttemptMetrics;
use crate::server::paths;

/// Arguments for the `metrics` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The run to report metrics for.
    ///
    /// May be a UUID or the human-readable generated name of the run (e.g.
    /// `happy-dolphin-42`).
    #[clap(value_name = "RUN")]
    run_id: String,

    /// Only report metrics for calls whose call id contains this string.
    #[clap(long, value_name = "CALL")]
    call: Option<String>,

    /// Output the raw JSON response instead of the formatted summary.
    #[clap(long)]
    json: bool,

    /// The connection arguments for the Sprocket server.
    #[command(flatten)]
    client_args: ServerConnectionArgs,
}

/// Handles the `metrics` subcommand.
///
/// Fetches the run's execution metrics from the server and reports them,
/// either as a formatted summary or as raw JSON.
pub async fn metrics(args: Args, config: Config, colorize: bool) -> CommandResult<()> {
    let base_url = args.client_args.base_url(&config);
    let uuid = resolve_run_id(&args.run_id, &base_url).await?;

    let url = format!("{base_url}{path}", path = paths::run_metrics(uuid));
    let mut body: RunMetricsResponse = get_json(&url, "run metrics").await?;

    if let Some(call) = &args.call {
        body.calls.retain(|c| c.call_id.contains(call.as_str()));
    }

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).context("failed to pretty-print response")?
        );
        return Ok(());
    }

    print!("{}", render_metrics(&body, colorize));
    Ok(())
}

/// Formats a byte count for display, choosing the largest unit (B, KiB,
/// MiB, or GiB) for which the value is at least 1.
///
/// A fixed unit (e.g. always GiB) can silently round a real, nonzero
/// transfer down to `0.0`, which is visually indistinguishable from no
/// transfer having happened at all. Choosing the unit by magnitude avoids
/// that: the displayed value is always `>= 1.0` in its chosen unit unless
/// the byte count is genuinely zero.
fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;

    let bytes_f = bytes as f64;
    if bytes_f < KIB {
        format!("{bytes} B")
    } else if bytes_f < MIB {
        format!("{:.1} KiB", bytes_f / KIB)
    } else if bytes_f < GIB {
        format!("{:.1} MiB", bytes_f / MIB)
    } else {
        format!("{:.1} GiB", bytes_f / GIB)
    }
}

/// Formats a millisecond duration for display.
fn format_ms(ms: Option<i64>) -> String {
    match ms {
        None => "-".to_string(),
        Some(ms) if ms < 1_000 => format!("{ms}ms"),
        Some(ms) if ms < 60_000 => format!("{:.1}s", ms as f64 / 1_000.0),
        Some(ms) => {
            let seconds = ms / 1_000;
            format!("{m}m{s:02}s", m = seconds / 60, s = seconds % 60)
        }
    }
}

/// Formats an attempt's constraints for display.
///
/// Reports cpu, memory, and any GPUs; the full detail is available in
/// `--json` mode.
fn format_constraints(constraints: Option<&serde_json::Value>) -> String {
    let Some(constraints) = constraints else {
        return "-".to_string();
    };

    let mut parts = Vec::new();
    if let Some(cpu) = constraints.get("cpu").and_then(|v| v.as_f64()) {
        parts.push(format!("{cpu} cpu"));
    }
    if let Some(memory) = constraints.get("memory").and_then(|v| v.as_i64()) {
        parts.push(format!(
            "{:.1} GiB",
            memory as f64 / (1024.0 * 1024.0 * 1024.0)
        ));
    }
    if let Some(gpu) = constraints.get("gpu").and_then(|v| v.as_array())
        && !gpu.is_empty()
    {
        parts.push(format!(
            "{count} gpu{s}",
            count = gpu.len(),
            s = if gpu.len() == 1 { "" } else { "s" }
        ));
    }

    if parts.is_empty() {
        "-".to_string()
    } else {
        parts.join(", ")
    }
}

/// Formats a retry cause for display.
fn format_retry_cause(cause: &serde_json::Value) -> String {
    match cause.get("kind").and_then(|k| k.as_str()) {
        Some("unacceptable_exit_code") => match cause.get("code").and_then(|c| c.as_i64()) {
            Some(code) => format!("retried: unacceptable exit code {code}"),
            None => "retried: unacceptable exit code".to_string(),
        },
        Some("preempted") => "retried: preempted".to_string(),
        Some(kind) => format!("retried: {kind}"),
        None => format!("retried: {cause}"),
    }
}

/// Formats an attempt's observed resource utilization for display.
///
/// Reports peak resident memory and total CPU time; the full detail is
/// available in `--json` mode.
fn format_utilization(utilization: Option<&serde_json::Value>) -> Option<String> {
    let utilization = utilization?;

    let mut parts = Vec::new();
    if let Some(max_memory) = utilization.get("max_memory").and_then(|v| v.as_i64()) {
        parts.push(format!(
            "peak {:.1} GiB",
            max_memory as f64 / (1024.0 * 1024.0 * 1024.0)
        ));
    }
    if let Some(cpu_time_ms) = utilization.get("cpu_time_ms").and_then(|v| v.as_i64()) {
        parts.push(format!("cpu {}", format_ms(Some(cpu_time_ms))));
    }

    (!parts.is_empty()).then(|| parts.join(", "))
}

/// Renders one attempt line.
fn attempt_line(attempt: &TaskAttemptMetrics, colorize: bool) -> String {
    let status = attempt.status.to_string();
    let status = if colorize {
        status.color(task_status_color(attempt.status)).to_string()
    } else {
        status
    };

    let mut line = format!(
        "    #{n:<3} {status:<12} wall {wall:>8}  queued {queued:>8}  {constraints}",
        n = attempt.attempt,
        wall = format_ms(attempt.wall_time_ms),
        queued = format_ms(attempt.queued_ms),
        constraints = format_constraints(attempt.constraints.as_ref()),
    );

    if let Some(exit_status) = attempt.exit_status
        && exit_status != 0
    {
        line.push_str(&format!("  exit {exit_status}"));
    }

    if let Some(utilization) = format_utilization(attempt.utilization.as_ref()) {
        line.push_str("  ");
        line.push_str(&utilization);
    }

    if let Some(cause) = &attempt.retry_cause {
        let cause = format_retry_cause(cause);
        let cause = if colorize {
            cause.yellow().to_string()
        } else {
            cause
        };
        line.push_str("  ");
        line.push_str(&cause);
    }

    line
}

/// Renders one call section.
fn call_section(call: &CallMetrics, colorize: bool) -> String {
    let mut out = String::new();

    let header = if colorize {
        call.name.bold().to_string()
    } else {
        call.name.clone()
    };
    let qualifier = if call.name != call.call_id {
        let full = format!("({id}) ", id = call.call_id);
        if colorize {
            full.dimmed().to_string()
        } else {
            full
        }
    } else {
        String::new()
    };

    let attempts = call.attempts.len();
    out.push_str(&format!(
        "  {header} {qualifier}({attempts} attempt{s})\n",
        s = if attempts == 1 { "" } else { "s" }
    ));

    for attempt in &call.attempts {
        out.push_str(&attempt_line(attempt, colorize));
        out.push('\n');
    }

    out
}

/// Renders the full metrics report.
fn render_metrics(body: &RunMetricsResponse, colorize: bool) -> String {
    let mut out = String::new();

    let status = body.run.status.to_string();
    let status = if colorize {
        status
            .color(status_color(&body.run.status))
            .bold()
            .to_string()
    } else {
        status
    };
    out.push_str(&format!(
        "Run `{name}` ({uuid}): {status}, wall {wall}\n\n",
        name = body.run.name,
        uuid = body.run.uuid,
        wall = format_ms(body.run.wall_time_ms),
    ));

    for call in &body.calls {
        out.push_str(&call_section(call, colorize));
    }

    let retries = format!("{} retried", body.totals.retries);
    let retries = if colorize && body.totals.retries > 0 {
        retries.yellow().to_string()
    } else {
        retries
    };
    out.push_str(&format!(
        "\n{attempts} attempt{s} total: {retries}, {cached} cached, {preempted} preempted\n",
        attempts = body.totals.attempts,
        s = if body.totals.attempts == 1 { "" } else { "s" },
        cached = body.totals.cached,
        preempted = body.totals.preempted,
    ));

    if body.totals.allocated_cpu_time_ms > 0 {
        out.push_str(&format!(
            "allocated cpu {}\n",
            format_ms(Some(body.totals.allocated_cpu_time_ms))
        ));
    }
    if body.totals.preemption_wasted_ms > 0 {
        out.push_str(&format!(
            "wasted to preemption {}\n",
            format_ms(Some(body.totals.preemption_wasted_ms))
        ));
    }
    if let Some(transfer) = &body.run.transfer {
        out.push_str(&format!(
            "transferred {downloaded} down, {uploaded} up\n",
            downloaded = format_bytes(transfer.downloaded_bytes),
            uploaded = format_bytes(transfer.uploaded_bytes),
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::server::RunMetricsRun;
    use crate::server::RunMetricsTotals;
    use crate::server::TransferTotals;
    use crate::system::v1::db::RunStatus;
    use crate::system::v1::db::TaskStatus;

    fn body() -> RunMetricsResponse {
        RunMetricsResponse {
            run: RunMetricsRun {
                uuid: Uuid::nil(),
                name: "happy-dolphin-42".to_string(),
                status: RunStatus::Completed,
                wall_time_ms: Some(83_000),
                backend: Some("lsf_apptainer".to_string()),
                sprocket_version: "0.0.0-test".to_string(),
                transfer: None,
            },
            calls: vec![
                CallMetrics {
                    call_id: "wf-align".to_string(),
                    name: "wf-align".to_string(),
                    attempts: vec![
                        TaskAttemptMetrics {
                            name: "wf-align-x1".to_string(),
                            attempt: 0,
                            status: TaskStatus::Completed,
                            exit_status: Some(137),
                            wall_time_ms: Some(45_500),
                            queued_ms: Some(300),
                            pending_ms: Some(120),
                            allocated_cpu_time_ms: Some(182_000),
                            constraints: Some(serde_json::json!({
                                "cpu": 4.0,
                                "memory": 8589934592i64,
                                "gpu": ["nvidia-tesla-t4"],
                            })),
                            retry_cause: Some(serde_json::json!({
                                "kind": "unacceptable_exit_code",
                                "code": 137,
                            })),
                            utilization: None,
                            logs: "/api/v1/tasks/wf-align-x1/logs".to_string(),
                        },
                        TaskAttemptMetrics {
                            name: "wf-align-x2".to_string(),
                            attempt: 1,
                            status: TaskStatus::Completed,
                            exit_status: Some(0),
                            wall_time_ms: Some(37_000),
                            queued_ms: Some(150),
                            pending_ms: Some(80),
                            allocated_cpu_time_ms: Some(148_000),
                            constraints: None,
                            retry_cause: None,
                            utilization: Some(serde_json::json!({
                                "max_memory": 12025908428i64,
                                "avg_memory": 8589934592i64,
                                "cpu_time_ms": 324_000,
                            })),
                            logs: "/api/v1/tasks/wf-align-x2/logs".to_string(),
                        },
                    ],
                },
                // A call made inside a subworkflow: its identifier carries a
                // call path and displays under its short name.
                CallMetrics {
                    call_id: "sub--wf-align".to_string(),
                    name: "wf-align".to_string(),
                    attempts: vec![TaskAttemptMetrics {
                        name: "sub--wf-align-x1".to_string(),
                        attempt: 0,
                        status: TaskStatus::Completed,
                        exit_status: Some(0),
                        wall_time_ms: Some(10_000),
                        queued_ms: Some(100),
                        pending_ms: Some(50),
                        allocated_cpu_time_ms: Some(10_000),
                        constraints: None,
                        retry_cause: None,
                        utilization: None,
                        logs: "/api/v1/tasks/sub--wf-align-x1/logs".to_string(),
                    }],
                },
            ],
            totals: RunMetricsTotals {
                attempts: 3,
                retries: 1,
                cached: 0,
                preempted: 0,
                allocated_cpu_time_ms: 340_000,
                preemption_wasted_ms: 0,
            },
        }
    }

    #[test]
    fn durations_format_across_magnitudes() {
        assert_eq!(format_ms(None), "-");
        assert_eq!(format_ms(Some(250)), "250ms");
        assert_eq!(format_ms(Some(45_500)), "45.5s");
        assert_eq!(format_ms(Some(83_000)), "1m23s");
    }

    #[test]
    fn constraints_summarize_cpu_memory_and_gpus() {
        assert_eq!(format_constraints(None), "-");
        assert_eq!(
            format_constraints(Some(&serde_json::json!({
                "cpu": 4.0,
                "memory": 8589934592i64,
                "gpu": ["nvidia-tesla-t4"],
            }))),
            "4 cpu, 8.0 GiB, 1 gpu"
        );
        assert_eq!(
            format_constraints(Some(&serde_json::json!({ "cpu": 2.0, "gpu": [] }))),
            "2 cpu"
        );
    }

    #[test]
    fn utilization_summarizes_peak_memory_and_cpu_time() {
        assert_eq!(format_utilization(None), None);
        assert_eq!(
            format_utilization(Some(&serde_json::json!({
                "max_memory": 12025908428i64,
                "avg_memory": 8589934592i64,
                "cpu_time_ms": 324_000,
            }))),
            Some("peak 11.2 GiB, cpu 5m24s".to_string())
        );
        // A snapshot with only CPU time reports just that.
        assert_eq!(
            format_utilization(Some(&serde_json::json!({ "cpu_time_ms": 500 }))),
            Some("cpu 500ms".to_string())
        );
        // An empty object yields nothing.
        assert_eq!(format_utilization(Some(&serde_json::json!({}))), None);
    }

    #[test]
    fn report_includes_run_calls_attempts_and_totals() {
        let report = render_metrics(&body(), false);
        assert!(report.contains("Run `happy-dolphin-42`"));
        assert!(report.contains("completed, wall 1m23s"));
        // A top-level call displays its identifier without a qualifier.
        assert!(report.contains("wf-align (2 attempts)"));
        assert!(report.contains("#0"));
        assert!(report.contains("exit 137"));
        assert!(report.contains("retried: unacceptable exit code 137"));
        assert!(report.contains("#1"));
        assert!(report.contains("peak 11.2 GiB, cpu 5m24s"));
        // A call inside a subworkflow displays its short name with the full
        // identifier as a qualifier.
        assert!(report.contains("wf-align (sub--wf-align) (1 attempt)"));
        assert!(report.contains("3 attempts total: 1 retried, 0 cached, 0 preempted"));
        assert!(report.contains("allocated cpu 5m40s"));
    }

    /// Transferred bytes are reported for a failed run just like a
    /// successful one: transfer accounting reflects data actually moved
    /// during localization/delocalization, independent of the run's final
    /// status.
    #[test]
    fn report_includes_transfer_for_failed_run() {
        let mut failed = body();
        failed.run.status = RunStatus::Failed;
        failed.run.transfer = Some(TransferTotals {
            downloaded_bytes: 3 * 1024 * 1024 * 1024,
            uploaded_bytes: 1024 * 1024 * 1024,
        });

        let report = render_metrics(&failed, false);
        assert!(report.contains("failed, wall 1m23s"));
        assert!(report.contains("transferred 3.0 GiB down, 1.0 GiB up"));
    }

    /// A transfer well under 1 GiB (e.g. a single small-to-medium input
    /// staged to a remote TES backend) must not render as `0.0 GiB`, which
    /// is visually indistinguishable from "no data transferred" and has
    /// been reported as looking like a bug even though the underlying bytes
    /// were correctly recorded.
    #[test]
    fn report_does_not_round_small_transfers_to_zero() {
        let mut small = body();
        small.run.transfer = Some(TransferTotals {
            downloaded_bytes: 20_000_000, // ~19.1 MiB
            uploaded_bytes: 512,
        });

        let report = render_metrics(&small, false);
        assert!(!report.contains("0.0 GiB down"));
        assert!(report.contains("transferred 19.1 MiB down, 512 B up"));
    }
}
