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
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::Context as _;
use anyhow::anyhow;
use anyhow::bail;
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
use tracing::debug;
use tracing::trace;
use tracing::warn;
use wdl_ast::Diagnostic;

use super::ApptainerState;
use super::TaskExecutionBackend;
use super::TaskManager;
use super::TaskManagerRequest;
use super::TaskSpawnRequest;
use crate::EvaluationPath;
use crate::ONE_GIBIBYTE;
use crate::PrimitiveValue;
use crate::Value;
use crate::backend::TaskExecutionResult;
use crate::config::Config;
use crate::config::SlurmApptainerBackendConfig;
use crate::config::TaskResourceLimitBehavior;
use crate::tree::SyntaxNode;
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
    /// The Apptainer state for the backend,
    apptainer_state: Arc<ApptainerState>,
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
            .apptainer_state
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

        // If GPUs are required, use the gpu helper to determine the count and pass
        // it to `sbatch` via `--gpus-per-task`.
        if let Some(gpu_count) = v1::gpu(
            self.spawn_request.requirements(),
            self.spawn_request.hints(),
        ) {
            sbatch_command.arg(format!("--gpus-per-task={gpu_count}"));
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
            work_dir: EvaluationPath::from_local_path(wdl_work_dir),
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
    /// Apptainer state.
    apptainer_state: Arc<ApptainerState>,
}

impl SlurmApptainerBackend {
    /// Create a new backend.
    pub fn new(
        run_root_dir: &Path,
        engine_config: Arc<Config>,
        backend_config: Arc<SlurmApptainerBackendConfig>,
        crankshaft_events: Option<broadcast::Sender<Event>>,
    ) -> Self {
        let apptainer_state =
            ApptainerState::new(&backend_config.apptainer_config, run_root_dir).into();
        Self {
            engine_config,
            backend_config,
            // TODO ACF 2025-10-13: the `MAX` values here mean that in addition to not limiting the
            // overall number of CPU and memory used, we don't limit per-task consumption. There is
            // potentially a path to pulling partition limits from Slurm for these, but for now we
            // just throw jobs at the cluster.
            manager: TaskManager::new_unlimited(u64::MAX, u64::MAX),
            crankshaft_events,
            apptainer_state,
        }
    }
}

impl TaskExecutionBackend for SlurmApptainerBackend {
    fn max_concurrency(&self) -> u64 {
        self.backend_config.max_scatter_concurrency
    }

    fn constraints(
        &self,
        task: &wdl_ast::v1::TaskDefinition<SyntaxNode>,
        requirements: &HashMap<String, Value>,
        hints: &HashMap<String, crate::Value>,
    ) -> anyhow::Result<super::TaskExecutionConstraints, Diagnostic> {
        let mut required_cpu = v1::cpu(task, requirements);
        let required_memory = v1::memory(task, requirements)?;
        let (mut required_memory, required_memory_span) = (
            ByteSize::b(required_memory.value as u64),
            required_memory.span,
        );

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
                && required_cpu.value > max_cpu as f64
            {
                let span = required_cpu.span;
                let env_specific = if self.engine_config.suppress_env_specific_output {
                    String::new()
                } else {
                    format!(", but the execution backend has a maximum of {max_cpu}",)
                };
                match self.engine_config.task.cpu_limit_behavior {
                    TaskResourceLimitBehavior::TryWithMax => {
                        warn!(
                            "task requires at least {required_cpu} CPU{s}{env_specific}",
                            required_cpu = required_cpu.value,
                            s = if required_cpu.value == 1.0 { "" } else { "s" },
                        );
                        // clamp the reported constraint to what's available
                        required_cpu.value = max_cpu as f64;
                    }
                    TaskResourceLimitBehavior::Deny => {
                        let msg = format!(
                            "task requires at least {required_cpu} CPU{s}{env_specific}",
                            required_cpu = required_cpu.value,
                            s = if required_cpu.value == 1.0 { "" } else { "s" },
                        );
                        return Err(Diagnostic::error(msg)
                            .with_label("this requirement exceeds the available CPUs", span));
                    }
                }
            }
            if let Some(max_memory) = partition.max_memory_per_task()
                && required_memory > max_memory
            {
                let span = required_memory_span;
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
                        let msg = format!(
                            "task requires at least {required_memory} GiB of memory{env_specific}",
                            required_memory = required_memory.as_u64() as f64 / ONE_GIBIBYTE
                        );
                        return Err(Diagnostic::error(msg)
                            .with_label("this requirement exceeds the available memory", span));
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
            cpu: required_cpu.value,
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

        let mut required_cpu = v1::cpu_from_values(requirements);
        let mut required_memory = ByteSize::b(v1::memory_from_values(requirements)? as u64);

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
        let _max_cpu = v1::max_cpu_from_values(hints);
        let _max_memory = v1::max_memory_from_values(hints)?.map(|i| i as u64);

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
                apptainer_state: self.apptainer_state.clone(),
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
