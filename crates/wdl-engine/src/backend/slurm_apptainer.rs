//! Experimental Slurm + Apptainer (aka Singularity) task execution backend.
//!
//! This experimental backend submits each task as an Slurm job which invokes
//! Apptainer to provide the appropriate container environment for the WDL
//! command to execute.
//!
//! Due to the difficulty of getting a Slurm test cluster spun up, and limited
//! ability to install Apptainer locally or in CI, this is currently tested by
//! hand; expect (and report) bugs! In follow-up work, we hope to build a
//! limited test suite based on mocking CLI invocations and/or golden testing of
//! generated `srun`/`apptainer` scripts.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::Context as _;
use anyhow::anyhow;
use anyhow::bail;
use anyhow::ensure;
use bytesize::ByteSize;
use crankshaft::events::Event;
use nonempty::NonEmpty;
use tokio::fs::File;
use tokio::fs::{self};
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use tracing::trace;
use tracing::warn;

use super::ApptainerConfig;
use super::TaskExecutionBackend;
use super::TaskManager;
use super::TaskManagerRequest;
use super::TaskSpawnRequest;
use crate::ONE_GIBIBYTE;
use crate::PrimitiveValue;
use crate::TaskExecutionResult;
use crate::Value;
use crate::config::Config;
use crate::config::TaskResourceLimitBehavior;
use crate::path::EvaluationPath;
use crate::v1;

/// The name of the file where the Apptainer command invocation will be written.
const APPTAINER_COMMAND_FILE_NAME: &str = "apptainer_command";

/// The root guest path for inputs.
const GUEST_INPUTS_DIR: &str = "/mnt/task/inputs/";

/// The maximum length of a Slurm job name.
// TODO ACF 2025-10-13: I worked this out experimentally on the cluster I happen
// to have access to. I do not know whether this translates to other Slurm
// installations, and cannot find documentation about what this limit should be
// or whether it's configurable.
const SLURM_JOB_NAME_MAX_LENGTH: usize = 1024;

/// A request to execute a task on a Slurm + Apptainer backend.
#[derive(Debug)]
struct SlurmApptainerTaskRequest {
    /// The desired configuration of the backend.
    backend_config: Arc<SlurmApptainerBackendConfig>,
    /// The name of the task, potentially truncated to fit within the Slurm job
    /// name length limit.
    name: String,
    /// The task spawn request.
    spawn_request: TaskSpawnRequest,
    /// The requested container for the task.
    container: String,
    /// The requested CPU reservation for the task.
    required_cpu: f64,
    /// The requested memory reservation for the task.
    required_memory: ByteSize,
    /// The broadcast channel to update interested parties with the status of
    /// executing tasks.
    ///
    /// This backend does not yet take advantage of the full Crankshaft
    /// machinery, but we send rudimentary messages on this channel which helps
    /// with UI presentation.
    crankshaft_events: Option<broadcast::Sender<Event>>,
    /// The cancellation token for this task execution request.
    cancellation_token: CancellationToken,
}

impl TaskManagerRequest for SlurmApptainerTaskRequest {
    fn cpu(&self) -> f64 {
        self.required_cpu
    }

    fn memory(&self) -> u64 {
        self.required_memory.as_u64()
    }

    async fn run(self) -> anyhow::Result<super::TaskExecutionResult> {
        let crankshaft_task_id = crankshaft::events::next_task_id();

        let attempt_dir = self.spawn_request.attempt_dir();

        // Create the host directory that will be mapped to the WDL working directory.
        let wdl_work_dir = self.spawn_request.wdl_work_dir_host_path();
        fs::create_dir_all(&wdl_work_dir).await.with_context(|| {
            format!(
                "failed to create WDL working directory `{path}`",
                path = wdl_work_dir.display()
            )
        })?;

        // Create an empty file for the WDL command's stdout.
        let wdl_stdout_path = self.spawn_request.wdl_stdout_host_path();
        let _ = File::create(&wdl_stdout_path).await.with_context(|| {
            format!(
                "failed to create WDL stdout file `{path}`",
                path = wdl_stdout_path.display()
            )
        })?;

        // Create an empty file for the WDL command's stderr.
        let wdl_stderr_path = self.spawn_request.wdl_stderr_host_path();
        let _ = File::create(&wdl_stderr_path).await.with_context(|| {
            format!(
                "failed to create WDL stderr file `{path}`",
                path = wdl_stderr_path.display()
            )
        })?;

        // Write the evaluated WDL command section to a host file.
        let wdl_command_path = self.spawn_request.wdl_command_host_path();
        fs::write(&wdl_command_path, self.spawn_request.command())
            .await
            .with_context(|| {
                format!(
                    "failed to write WDL command contents to `{path}`",
                    path = wdl_command_path.display()
                )
            })?;
        #[cfg(unix)]
        tokio::fs::set_permissions(
            &wdl_command_path,
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o770),
        )
        .await?;

        let apptainer_command = self
            .backend_config
            .apptainer_config
            .prepare_apptainer_command(
                &self.container,
                self.cancellation_token.clone(),
                &self.spawn_request,
            )
            .await?;

        let apptainer_command_path = attempt_dir.join(APPTAINER_COMMAND_FILE_NAME);
        fs::write(&apptainer_command_path, apptainer_command)
            .await
            .with_context(|| {
                format!(
                    "failed to write Apptainer command file `{}`",
                    apptainer_command_path.display()
                )
            })?;
        #[cfg(unix)]
        tokio::fs::set_permissions(
            &apptainer_command_path,
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o770),
        )
        .await?;

        // The path for the Slurm-level stdout and stderr. This primarily contains the
        // job report, as we redirect Apptainer and WDL output separately.
        let slurm_stdout_path = attempt_dir.join("slurm.stdout");
        let slurm_stderr_path = attempt_dir.join("slurm.stderr");

        let mut sbatch_command = Command::new("sbatch");

        // If a Slurm partition has been configured, specify it. Otherwise, the job will
        // end up on the cluster's default partition.
        if let Some(partition) = self.backend_config.slurm_partition_for_task(
            self.spawn_request.requirements(),
            self.spawn_request.hints(),
        ) {
            sbatch_command.arg("--partition").arg(partition.name());
        }

        // If GPUs are required, pass a basic `--gpus-per-task` flag to `sbatch`. If
        // this is a bare `requirements { gpu: true }`, we request 1 GPU per
        // node. If there is also an integer `hints: { gpu: n }`, we request `n`
        // GPUs per node.
        if let Some(true) = self
            .spawn_request
            .requirements()
            .get(wdl_ast::v1::TASK_REQUIREMENT_GPU)
            .and_then(Value::as_boolean)
        {
            match self.spawn_request.hints().get(wdl_ast::v1::TASK_HINT_GPU) {
                Some(Value::Primitive(PrimitiveValue::Integer(n))) => {
                    sbatch_command.arg(format!("--gpus-per-task={n}"));
                }
                Some(Value::Primitive(PrimitiveValue::String(hint))) => {
                    warn!(
                        %hint,
                        "string hints for GPU are not supported; falling back to 1 GPU per host"
                    );
                    sbatch_command.arg("--gpus-per-task=1");
                }
                // Other hint value types should be rejected already, so the remaining valid case is
                // a GPU requirement with no hints
                _ => {
                    sbatch_command.arg("--gpus-per-task=1");
                }
            }
        }

        // Add any user-configured extra arguments.
        if let Some(args) = &self.backend_config.extra_sbatch_args {
            sbatch_command.args(args);
        }

        sbatch_command
            // Use verbose output that we can check later on
            .arg("-v")
            // Keep `sbatch` running until the job terminates
            .arg("--wait")
            // Pipe stdout and stderr so we can identify when a job begins, and can trace any other
            // output. This should just be the `sbatch` verbose output on stderr.
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Name the Slurm job after the task ID, which has already been shortened to fit into
            // the Slurm requirements.
            .arg("--job-name")
            .arg(&self.name)
            // Send Slurm job stdout and stderr streams to these files. Since we redirect the
            // Apptainer invocation's stdio to separate files, this will typically not contain
            // anything, but can be useful for debugging if the scripts get modified.
            .arg("-o")
            .arg(slurm_stdout_path)
            .arg("-e")
            .arg(slurm_stderr_path)
            // An explicit task count is required for some options
            .arg("--ntasks=1")
            // CPU request is rounded up to the nearest whole CPU
            .arg(format!(
                "--cpus-per-task={}",
                self.required_cpu.ceil() as u64
            ))
            // Memory request is specified per node in mebibytes; we round the request up to the
            // next mebibyte.
            //
            // Note that the Slurm documentation says "megabyte" (i.e., the base-10 unit), but the
            // other explanations of the unit suffixes in the first-party documentation show the use
            // of base-2 units, and multiple third-party sources available through online searches
            // back the base-2 interpretation, for example:
            //
            // https://info.nrao.edu/computing/guide/cluster-processing/appendix/memory-options
            // https://wcmscu.atlassian.net/wiki/spaces/WIKI/pages/327731/Using+Slurm
            .arg(format!(
                "--mem={}M",
                (self.required_memory.as_u64() as f64 / bytesize::MIB as f64).ceil() as u64
            ))
            .arg(apptainer_command_path);

        debug!(?sbatch_command, "spawning `sbatch` command");

        let mut sbatch_child = sbatch_command.spawn()?;

        crankshaft::events::send_event!(
            self.crankshaft_events,
            crankshaft::events::Event::TaskCreated {
                id: crankshaft_task_id,
                name: self.name.clone(),
                tes_id: None,
                token: self.cancellation_token.clone(),
            },
        );

        // Take the stdio pipes from the child process and consume them for event
        // reporting and tracing purposes.
        //
        // TODO ACF 2025-10-13: generate `sbatch`-compatible scripts instead and use a
        // polling mechanism to watch for job status changes? `squeue` can emit
        // json suitable for this.
        let sbatch_stdout = sbatch_child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("sbatch child stdout missing"))?;
        let task_name = self.name.clone();
        let stdout_crankshaft_events = self.crankshaft_events.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(sbatch_stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                // TODO ACF 2025-10-14: `sbatch --wait` even on high verbosity doesn't tell us
                // when a job has actually started, only when it's been
                // submitted.  Unless we can figure out a way to get that info
                // out directly, we'll have to set up a separate task to
                // poll job statuses. For the moment, this is potentially misleading about what
                // work has actually begun computation.
                if line.starts_with("Submitted batch job") {
                    crankshaft::events::send_event!(
                        stdout_crankshaft_events,
                        crankshaft::events::Event::TaskStarted {
                            id: crankshaft_task_id
                        },
                    );
                }
                trace!(stdout = line, task_name);
            }
        });
        let sbatch_stderr = sbatch_child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("sbatch child stderr missing"))?;
        let task_name = self.name.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(sbatch_stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                trace!(stderr = line, task_name);
            }
        });

        // Await the result of the `sbatch` command, which will only exit on error or
        // once the containerized command has completed.
        let sbatch_result = tokio::select! {
            _ = self.cancellation_token.cancelled() => {
                crankshaft::events::send_event!(
                    self.crankshaft_events,
                    crankshaft::events::Event::TaskCanceled {
                        id: crankshaft_task_id
                    },
                );
                Err(anyhow!("task execution cancelled"))
            }
            result = sbatch_child.wait() => result.map_err(Into::into),
        }?;

        crankshaft::events::send_event!(
            self.crankshaft_events,
            crankshaft::events::Event::TaskCompleted {
                id: crankshaft_task_id,
                exit_statuses: NonEmpty::new(sbatch_result),
            }
        );

        Ok(TaskExecutionResult {
            // Under normal circumstances, the exit code of `sbatch --wait` is the exit code of its
            // command, and the exit code of `apptainer exec` is likewise the exit code of its
            // command. One potential subtlety/problem here is that if `sbatch` or `apptainer` exit
            // due to an error before running the WDL command, we could be erroneously ascribing an
            // exit code to the WDL command.
            exit_code: sbatch_result
                .code()
                .ok_or(anyhow!("task did not return an exit code"))?,
            work_dir: EvaluationPath::Local(wdl_work_dir),
            stdout: PrimitiveValue::new_file(
                wdl_stdout_path
                    .into_os_string()
                    .into_string()
                    .expect("path should be UTF-8"),
            )
            .into(),
            stderr: PrimitiveValue::new_file(
                wdl_stderr_path
                    .into_os_string()
                    .into_string()
                    .expect("path should be UTF-8"),
            )
            .into(),
        })
    }
}

/// The experimental Slurm + Apptainer backend.
///
/// See the module-level documentation for details.
#[derive(Debug)]
pub struct SlurmApptainerBackend {
    /// The configuration of the overall engine being executed.
    engine_config: Arc<Config>,
    /// The configuration of this backend.
    backend_config: Arc<SlurmApptainerBackendConfig>,
    /// The task manager for the backend.
    manager: TaskManager<SlurmApptainerTaskRequest>,
    /// Sender for crankshaft events.
    crankshaft_events: Option<broadcast::Sender<Event>>,
}

impl SlurmApptainerBackend {
    /// Create a new backend.
    pub fn new(
        engine_config: Arc<Config>,
        backend_config: Arc<SlurmApptainerBackendConfig>,
        crankshaft_events: Option<broadcast::Sender<Event>>,
    ) -> Self {
        Self {
            engine_config,
            backend_config,
            // TODO ACF 2025-10-13: the `MAX` values here mean that in addition to not limiting the
            // overall number of CPU and memory used, we don't limit per-task consumption. There is
            // potentially a path to pulling partition limits from Slurm for these, but for now we
            // just throw jobs at the cluster.
            manager: TaskManager::new_unlimited(u64::MAX, u64::MAX),
            crankshaft_events,
        }
    }
}

impl TaskExecutionBackend for SlurmApptainerBackend {
    fn max_concurrency(&self) -> u64 {
        self.backend_config.max_scatter_concurrency
    }

    fn constraints(
        &self,
        requirements: &std::collections::HashMap<String, crate::Value>,
        hints: &std::collections::HashMap<String, crate::Value>,
    ) -> anyhow::Result<super::TaskExecutionConstraints> {
        let mut required_cpu = v1::cpu(requirements);
        let mut required_memory = ByteSize::b(v1::memory(requirements)? as u64);

        // Determine whether CPU or memory limits are set for this partition, and clamp
        // or deny them as appropriate if the limits are exceeded
        //
        // TODO ACF 2025-10-16: refactor so that we're not duplicating logic here (for
        // the in-WDL `task` values) and below in `spawn` (for the actual
        // resource request)
        if let Some(partition) = self
            .backend_config
            .slurm_partition_for_task(requirements, hints)
        {
            if let Some(max_cpu) = partition.max_cpu_per_task()
                && required_cpu > max_cpu as f64
            {
                let env_specific = if self.engine_config.suppress_env_specific_output {
                    String::new()
                } else {
                    format!(", but the execution backend has a maximum of {max_cpu}",)
                };
                match self.engine_config.task.cpu_limit_behavior {
                    TaskResourceLimitBehavior::TryWithMax => {
                        warn!(
                            "task requires at least {required_cpu} CPU{s}{env_specific}",
                            s = if required_cpu == 1.0 { "" } else { "s" },
                        );
                        // clamp the reported constraint to what's available
                        required_cpu = max_cpu as f64;
                    }
                    TaskResourceLimitBehavior::Deny => {
                        bail!(
                            "task requires at least {required_cpu} CPU{s}{env_specific}",
                            s = if required_cpu == 1.0 { "" } else { "s" },
                        );
                    }
                }
            }
            if let Some(max_memory) = partition.max_memory_per_task()
                && required_memory > max_memory
            {
                let env_specific = if self.engine_config.suppress_env_specific_output {
                    String::new()
                } else {
                    format!(
                        ", but the execution backend has a maximum of {max_memory} GiB",
                        max_memory = max_memory.as_u64() as f64 / ONE_GIBIBYTE
                    )
                };
                match self.engine_config.task.memory_limit_behavior {
                    TaskResourceLimitBehavior::TryWithMax => {
                        warn!(
                            "task requires at least {required_memory} GiB of memory{env_specific}",
                            required_memory = required_memory.as_u64() as f64 / ONE_GIBIBYTE
                        );
                        // clamp the reported constraint to what's available
                        required_memory = max_memory;
                    }
                    TaskResourceLimitBehavior::Deny => {
                        bail!(
                            "task requires at least {required_memory} GiB of memory{env_specific}",
                            required_memory = required_memory.as_u64() as f64 / ONE_GIBIBYTE
                        );
                    }
                }
            }
        }
        Ok(super::TaskExecutionConstraints {
            container: Some(
                v1::container(requirements, self.engine_config.task.container.as_deref())
                    .into_owned(),
            ),
            // TODO ACF 2025-10-13: populate more meaningful values for these based on the given
            // Slurm partition.
            //
            // sinfo -p <partition> -s --json | jq .sinfo[0].cpus
            // sinfo -p <partition> -s --json | jq .sinfo[0].memory
            cpu: required_cpu,
            memory: required_memory.as_u64().try_into().unwrap_or(i64::MAX),
            // TODO ACF 2025-10-16: these are almost certainly wrong
            gpu: Default::default(),
            fpga: Default::default(),
            disks: Default::default(),
        })
    }

    fn guest_inputs_dir(&self) -> Option<&'static str> {
        Some(GUEST_INPUTS_DIR)
    }

    fn needs_local_inputs(&self) -> bool {
        true
    }

    fn spawn(
        &self,
        request: TaskSpawnRequest,
        cancellation_token: CancellationToken,
    ) -> anyhow::Result<tokio::sync::oneshot::Receiver<anyhow::Result<TaskExecutionResult>>> {
        let (completed_tx, completed_rx) = tokio::sync::oneshot::channel();

        let requirements = request.requirements();
        let hints = request.hints();

        let container =
            v1::container(requirements, self.engine_config.task.container.as_deref()).into_owned();

        let mut required_cpu = v1::cpu(requirements);
        let mut required_memory = ByteSize::b(v1::memory(requirements)? as u64);

        // Determine whether CPU or memory limits are set for this partition, and clamp
        // or deny them as appropriate if the limits are exceeded
        //
        // TODO ACF 2025-10-16: refactor so that we're not duplicating logic here (for
        // the in-WDL `task` values) and below in `spawn` (for the actual
        // resource request)
        if let Some(partition) = self
            .backend_config
            .slurm_partition_for_task(requirements, hints)
        {
            if let Some(max_cpu) = partition.max_cpu_per_task()
                && required_cpu > max_cpu as f64
            {
                let env_specific = if self.engine_config.suppress_env_specific_output {
                    String::new()
                } else {
                    format!(", but the execution backend has a maximum of {max_cpu}",)
                };
                match self.engine_config.task.cpu_limit_behavior {
                    TaskResourceLimitBehavior::TryWithMax => {
                        warn!(
                            "task requires at least {required_cpu} CPU{s}{env_specific}",
                            s = if required_cpu == 1.0 { "" } else { "s" },
                        );
                        // clamp the reported constraint to what's available
                        required_cpu = max_cpu as f64;
                    }
                    TaskResourceLimitBehavior::Deny => {
                        bail!(
                            "task requires at least {required_cpu} CPU{s}{env_specific}",
                            s = if required_cpu == 1.0 { "" } else { "s" },
                        );
                    }
                }
            }
            if let Some(max_memory) = partition.max_memory_per_task()
                && required_memory > max_memory
            {
                let env_specific = if self.engine_config.suppress_env_specific_output {
                    String::new()
                } else {
                    format!(
                        ", but the execution backend has a maximum of {max_memory} GiB",
                        max_memory = max_memory.as_u64() as f64 / ONE_GIBIBYTE
                    )
                };
                match self.engine_config.task.memory_limit_behavior {
                    TaskResourceLimitBehavior::TryWithMax => {
                        warn!(
                            "task requires at least {required_memory} GiB of memory{env_specific}",
                            required_memory = required_memory.as_u64() as f64 / ONE_GIBIBYTE
                        );
                        // clamp the reported constraint to what's available
                        required_memory = max_memory;
                    }
                    TaskResourceLimitBehavior::Deny => {
                        bail!(
                            "task requires at least {required_memory} GiB of memory{env_specific}",
                            required_memory = required_memory.as_u64() as f64 / ONE_GIBIBYTE
                        );
                    }
                }
            }
        }

        // TODO ACF 2025-10-23: investigate whether Slurm offers hard vs soft limits for
        // CPU and memory
        let _max_cpu = v1::max_cpu(hints);
        let _max_memory = v1::max_memory(hints)?.map(|i| i as u64);

        // Truncate the request ID to fit in the Slurm job name length limit.
        let request_id = request.id();
        let name = if request_id.len() > SLURM_JOB_NAME_MAX_LENGTH {
            request_id
                .chars()
                .take(SLURM_JOB_NAME_MAX_LENGTH)
                .collect::<String>()
        } else {
            request_id.to_string()
        };

        self.manager.send(
            SlurmApptainerTaskRequest {
                backend_config: self.backend_config.clone(),
                spawn_request: request,
                name,
                container,
                required_cpu,
                required_memory,
                crankshaft_events: self.crankshaft_events.clone(),
                cancellation_token,
            },
            completed_tx,
        );

        Ok(completed_rx)
    }

    fn cleanup<'a>(
        &'a self,
        _work_dir: &'a EvaluationPath,
        _token: CancellationToken,
    ) -> Option<futures::future::BoxFuture<'a, ()>> {
        // TODO ACF 2025-09-11: determine whether we need cleanup logic here;
        // Apptainer's security model is fairly different from Docker so
        // uid/gids on files shouldn't be as much of an issue, and using only
        // `apptainer exec` means no longer-running containers to tear down
        None
    }
}

/// Configuration for a Slurm partition.
///
/// Each partition can optionally have per-task CPU and memory limits set so
/// that tasks which are too large to be scheduled on that partition will fail
/// immediately instead of pending indefinitely. In the future, these limits may
/// be populated or validated by live information from the cluster, but
/// for now they must be manually based on the user's understanding of the
/// cluster configuration.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SlurmPartitionConfig {
    /// The name of the partition; this is the string passed to `sbatch
    /// --partition=<partition_name>`.
    name: String,
    /// The maximum number of CPUs this partition can provision for a single
    /// task.
    max_cpu_per_task: Option<u64>,
    /// The maximum memory this partition can provision for a single task.
    max_memory_per_task: Option<ByteSize>,
}

impl SlurmPartitionConfig {
    /// Create a [`SlurmPartitionConfig`].
    pub fn new(
        name: String,
        max_cpu_per_task: Option<u64>,
        max_memory_per_task: Option<ByteSize>,
    ) -> Self {
        Self {
            name,
            max_cpu_per_task,
            max_memory_per_task,
        }
    }

    /// The name of the partition; this is the string passed to `sbatch
    /// --partition=<partition_name>`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The maximum number of CPUs this partition can provision for a single
    /// task.
    pub fn max_cpu_per_task(&self) -> Option<u64> {
        self.max_cpu_per_task
    }

    /// The maximum memory this partition can provision for a single task.
    pub fn max_memory_per_task(&self) -> Option<ByteSize> {
        self.max_memory_per_task
    }

    /// Validate that this Slurm partition exists according to the local
    /// `sinfo`.
    async fn validate(&self, name: &str) -> Result<(), anyhow::Error> {
        let partition = self.name();
        ensure!(
            !partition.is_empty(),
            "{name}_slurm_partition name cannot be empty"
        );
        if let Some(max_cpu_per_task) = self.max_cpu_per_task() {
            ensure!(
                max_cpu_per_task > 0,
                "{name}_slurm_partition `{partition}` must allow at least 1 CPU to be provisioned"
            );
        }
        if let Some(max_memory_per_task) = self.max_memory_per_task() {
            ensure!(
                max_memory_per_task.as_u64() > 0,
                "{name}_slurm_partition `{partition}` must allow at least some memory to be \
                 provisioned"
            );
        }
        match tokio::time::timeout(
            // 10 seconds is rather arbitrary; `scontrol` ordinarily returns extremely quickly, but
            // we don't want things to run away on a misconfigured system
            std::time::Duration::from_secs(10),
            Command::new("scontrol")
                .arg("show")
                .arg("partition")
                .arg(partition)
                .output(),
        )
        .await
        {
            Ok(output) => {
                let output = output.context("validating Slurm partition")?;
                if !output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    error!(%stdout, %stderr, %partition, "failed to validate {name}_slurm_partition");
                    Err(anyhow!(
                        "failed to validate {name}_slurm_partition `{partition}`"
                    ))
                } else {
                    Ok(())
                }
            }
            Err(_) => Err(anyhow!(
                "timed out trying to validate {name}_slurm_partition `{partition}`"
            )),
        }
    }
}

/// Configuration for the Slurm + Apptainer backend.
// TODO ACF 2025-09-23: add a Apptainer/Singularity mode config that switches around executable
// name, env var names, etc.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SlurmApptainerBackendConfig {
    /// Which partition, if any, to specify when submitting normal jobs to
    /// Slurm.
    ///
    /// This may be superseded by
    /// [`short_task_slurm_partition`][Self::short_task_slurm_partition],
    /// [`gpu_slurm_partition`][Self::gpu_slurm_partition], or
    /// [`fpga_slurm_partition`][Self::fpga_slurm_partition] for corresponding
    /// tasks.
    pub default_slurm_partition: Option<SlurmPartitionConfig>,
    /// Which partition, if any, to specify when submitting [short
    /// tasks](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#short_task) to Slurm.
    ///
    /// This may be superseded by
    /// [`gpu_slurm_partition`][Self::gpu_slurm_partition] or
    /// [`fpga_slurm_partition`][Self::fpga_slurm_partition] for tasks which
    /// require specialized hardware.
    pub short_task_slurm_partition: Option<SlurmPartitionConfig>,
    /// Which partition, if any, to specify when submitting [tasks which require
    /// a GPU](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#hardware-accelerators-gpu-and--fpga)
    /// to Slurm.
    pub gpu_slurm_partition: Option<SlurmPartitionConfig>,
    /// Which partition, if any, to specify when submitting [tasks which require
    /// an FPGA](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#hardware-accelerators-gpu-and--fpga)
    /// to Slurm.
    pub fpga_slurm_partition: Option<SlurmPartitionConfig>,
    /// Additional command-line arguments to pass to `sbatch` when submitting
    /// jobs to Slurm.
    pub extra_sbatch_args: Option<Vec<String>>,
    /// The maximum number of scatter subtasks that can be evaluated
    /// concurrently.
    ///
    /// By default, this is 200.
    #[serde(default = "default_max_scatter_concurrency")]
    pub max_scatter_concurrency: u64,
    /// The configuration of Apptainer, which is used as the container runtime
    /// on the compute nodes where Slurm dispatches tasks.
    ///
    /// Note that this will likely be replaced by an abstraction over multiple
    /// container execution runtimes in the future, rather than being
    /// hardcoded to Apptainer.
    #[serde(default)]
    // TODO ACF 2025-10-16: temporarily flatten this into the overall config so that it doesn't
    // break existing serialized configs. We'll save breaking the config file format for when we
    // actually have meaningful composition of in-place runtimes.
    #[serde(flatten)]
    pub apptainer_config: ApptainerConfig,
}

fn default_max_scatter_concurrency() -> u64 {
    200
}

impl Default for SlurmApptainerBackendConfig {
    fn default() -> Self {
        Self {
            default_slurm_partition: None,
            short_task_slurm_partition: None,
            gpu_slurm_partition: None,
            fpga_slurm_partition: None,
            extra_sbatch_args: None,
            max_scatter_concurrency: default_max_scatter_concurrency(),
            apptainer_config: ApptainerConfig::default(),
        }
    }
}

impl SlurmApptainerBackendConfig {
    /// Validate that the backend is appropriately configured.
    pub async fn validate(&self, engine_config: &Config) -> Result<(), anyhow::Error> {
        if cfg!(not(unix)) {
            bail!("Slurm + Apptainer backend is not supported on non-unix platforms");
        }
        if !engine_config.experimental_features_enabled {
            bail!("Slurm + Apptainer backend requires enabling experimental features");
        }

        // Do what we can to validate options that are dependent on the dynamic
        // environment. These are a bit fraught, particularly if the behavior of
        // the external tools changes based on where a job gets dispatched, but
        // querying from the perspective of the current node allows
        // us to get better error messages in circumstances typical to a cluster.
        if let Some(partition) = &self.default_slurm_partition {
            partition.validate("default").await?;
        }
        if let Some(partition) = &self.short_task_slurm_partition {
            partition.validate("short_task").await?;
        }
        if let Some(partition) = &self.gpu_slurm_partition {
            partition.validate("gpu").await?;
        }
        if let Some(partition) = &self.fpga_slurm_partition {
            partition.validate("fpga").await?;
        }
        Ok(())
    }

    /// Get the appropriate Slurm partition for a task under this configuration.
    ///
    /// Specialized hardware requirements are prioritized over other
    /// characteristics, with FPGA taking precedence over GPU.
    fn slurm_partition_for_task(
        &self,
        requirements: &HashMap<String, Value>,
        hints: &HashMap<String, Value>,
    ) -> Option<&SlurmPartitionConfig> {
        // TODO ACF 2025-09-26: what's the relationship between this code and
        // `TaskExecutionConstraints`? Should this be there instead, or be pulling
        // values from that instead of directly from `requirements` and `hints`?

        // Specialized hardware gets priority.
        if let Some(partition) = self.fpga_slurm_partition.as_ref()
            && let Some(true) = requirements
                .get(wdl_ast::v1::TASK_REQUIREMENT_FPGA)
                .and_then(Value::as_boolean)
        {
            return Some(partition);
        }

        if let Some(partition) = self.gpu_slurm_partition.as_ref()
            && let Some(true) = requirements
                .get(wdl_ast::v1::TASK_REQUIREMENT_GPU)
                .and_then(Value::as_boolean)
        {
            return Some(partition);
        }

        // Then short tasks.
        if let Some(partition) = self.short_task_slurm_partition.as_ref()
            && let Some(true) = hints
                .get(wdl_ast::v1::TASK_HINT_SHORT_TASK)
                .and_then(Value::as_boolean)
        {
            return Some(partition);
        }

        // Finally the default partition. If this is `None`, `sbatch` gets run without a
        // partition argument and the cluster's default is used.
        self.default_slurm_partition.as_ref()
    }
}
