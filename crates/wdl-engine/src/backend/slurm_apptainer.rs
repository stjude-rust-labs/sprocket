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
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use bytesize::ByteSize;
use futures::FutureExt;
use futures::future::BoxFuture;
use nonempty::NonEmpty;
use tokio::fs::File;
use tokio::fs::{self};
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use tracing::trace;
use tracing::warn;

use super::ApptainerState;
use super::TaskExecutionBackend;
use super::TaskSpawnRequest;
use crate::CancellationContext;
use crate::EvaluationPath;
use crate::Events;
use crate::ONE_GIBIBYTE;
use crate::PrimitiveValue;
use crate::TaskInputs;
use crate::Value;
use crate::backend::TaskExecutionConstraints;
use crate::backend::TaskExecutionResult;
use crate::config::Config;
use crate::config::TaskResourceLimitBehavior;
use crate::http::Transferer;
use crate::v1::requirements;
use crate::v1::requirements::ContainerSource;

/// The name of the file where the Apptainer command invocation will be written.
const APPTAINER_COMMAND_FILE_NAME: &str = "apptainer_command";

/// The maximum length of a Slurm job name.
// TODO ACF 2025-10-13: I worked this out experimentally on the cluster I happen
// to have access to. I do not know whether this translates to other Slurm
// installations, and cannot find documentation about what this limit should be
// or whether it's configurable.
const SLURM_JOB_NAME_MAX_LENGTH: usize = 1024;

/// The experimental Slurm + Apptainer backend.
///
/// See the module-level documentation for details.
pub struct SlurmApptainerBackend {
    /// The shared engine configuration.
    config: Arc<Config>,
    /// The engine events.
    events: Events,
    /// The evaluation cancellation context.
    cancellation: CancellationContext,
    /// Apptainer state.
    apptainer_state: ApptainerState,
}

impl SlurmApptainerBackend {
    /// Create a new backend.
    pub fn new(
        config: Arc<Config>,
        run_root_dir: &Path,
        events: Events,
        cancellation: CancellationContext,
    ) -> Result<Self> {
        // Ensure the configured backend is Slurm Apptainer
        config
            .backend()?
            .as_slurm_apptainer()
            .context("configured backend is not Slurm Apptainer")?;

        Ok(Self {
            config,
            events,
            cancellation,
            apptainer_state: ApptainerState::new(run_root_dir),
        })
    }
}

impl TaskExecutionBackend for SlurmApptainerBackend {
    fn constraints(
        &self,
        inputs: &TaskInputs,
        requirements: &HashMap<String, Value>,
        hints: &HashMap<String, crate::Value>,
    ) -> Result<TaskExecutionConstraints> {
        let mut required_cpu = requirements::cpu(inputs, requirements);
        let mut required_memory = ByteSize::b(requirements::memory(inputs, requirements)? as u64);

        let backend_config = self.config.backend()?;
        let backend_config = backend_config
            .as_slurm_apptainer()
            .expect("configured backend is not Slurm Apptainer");

        // Determine whether CPU or memory limits are set for this partition, and clamp
        // or deny them as appropriate if the limits are exceeded
        //
        // TODO ACF 2025-10-16: refactor so that we're not duplicating logic here (for
        // the in-WDL `task` values) and below in `spawn` (for the actual
        // resource request)
        if let Some(partition) = backend_config.slurm_partition_for_task(requirements, hints) {
            if let Some(max_cpu) = partition.max_cpu_per_task()
                && required_cpu > max_cpu as f64
            {
                let env_specific = if self.config.suppress_env_specific_output {
                    String::new()
                } else {
                    format!(", but the execution backend has a maximum of {max_cpu}",)
                };
                match self.config.task.cpu_limit_behavior {
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
                let env_specific = if self.config.suppress_env_specific_output {
                    String::new()
                } else {
                    format!(
                        ", but the execution backend has a maximum of {max_memory} GiB",
                        max_memory = max_memory.as_u64() as f64 / ONE_GIBIBYTE
                    )
                };
                match self.config.task.memory_limit_behavior {
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

        let container =
            requirements::container(inputs, requirements, self.config.task.container.as_deref());
        if let ContainerSource::Unknown(_) = &container {
            bail!(
                "Slurm Apptainer backend does not support unknown container source `{container:#}`"
            )
        }

        Ok(super::TaskExecutionConstraints {
            container: Some(container),
            // TODO ACF 2025-10-13: populate more meaningful values for these based on the given
            // Slurm partition.
            //
            // sinfo -p <partition> -s --json | jq .sinfo[0].cpus
            // sinfo -p <partition> -s --json | jq .sinfo[0].memory
            cpu: required_cpu,
            memory: required_memory.as_u64(),
            // TODO ACF 2025-10-16: these are almost certainly wrong
            gpu: Default::default(),
            fpga: Default::default(),
            disks: Default::default(),
        })
    }

    fn spawn<'a>(
        &'a self,
        inputs: &'a TaskInputs,
        request: TaskSpawnRequest,
        _transferer: Arc<dyn Transferer>,
    ) -> BoxFuture<'a, Result<Option<TaskExecutionResult>>> {
        async move {
            let backend_config = self.config.backend()?;
            let backend_config = backend_config
                .as_slurm_apptainer()
                .expect("configured backend is not Slurm Apptainer");

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

            let crankshaft_task_id = crankshaft::events::next_task_id();
            let attempt_dir = request.attempt_dir();

            // Create the working directory.
            let work_dir = request.work_dir();
            fs::create_dir_all(&work_dir).await.with_context(|| {
                format!(
                    "failed to create working directory `{path}`",
                    path = work_dir.display()
                )
            })?;

            // Create an empty file for the task's stdout.
            let stdout_path = request.stdout_path();
            let _ = File::create(&stdout_path).await.with_context(|| {
                format!(
                    "failed to create stdout file `{path}`",
                    path = stdout_path.display()
                )
            })?;

            // Create an empty file for the task's stderr.
            let stderr_path = request.stderr_path();
            let _ = File::create(&stderr_path).await.with_context(|| {
                format!(
                    "failed to create stderr file `{path}`",
                    path = stderr_path.display()
                )
            })?;

            // Write the evaluated WDL command section to a host file.
            let command_path = request.command_path();
            fs::write(&command_path, request.command())
                .await
                .with_context(|| {
                    format!(
                        "failed to write command contents to `{path}`",
                        path = command_path.display()
                    )
                })?;

            let apptainer_command = self
                .apptainer_state
                .prepare_apptainer_command(
                    request
                        .constraints()
                        .container
                        .as_ref()
                        .expect("should have container"),
                    self.cancellation.first(),
                    &request,
                    backend_config
                        .apptainer_config
                        .extra_apptainer_exec_args
                        .as_deref()
                        .unwrap_or_default()
                        .iter()
                        .map(String::as_str),
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

            // Ensure the command files are executable
            #[cfg(unix)]
            {
                use std::fs::Permissions;
                use std::os::unix::fs::PermissionsExt;

                fs::set_permissions(&command_path, Permissions::from_mode(0o770)).await?;
                fs::set_permissions(&apptainer_command_path, Permissions::from_mode(0o770)).await?;
            }

            // The path for the Slurm-level stdout and stderr. This primarily contains the
            // job report, as we redirect Apptainer and WDL output separately.
            let slurm_stdout_path = attempt_dir.join("slurm.stdout");
            let slurm_stderr_path = attempt_dir.join("slurm.stderr");

            let mut sbatch_command = Command::new("sbatch");

            // If a Slurm partition has been configured, specify it. Otherwise, the job will
            // end up on the cluster's default partition.
            if let Some(partition) =
                backend_config.slurm_partition_for_task(request.requirements(), request.hints())
            {
                sbatch_command.arg("--partition").arg(partition.name());
            }

            // If GPUs are required, use the gpu helper to determine the count and pass it
            // to `sbatch` via `--gpus-per-task`.
            if let Some(gpu_count) = requirements::gpu(inputs, request.requirements(), request.hints()) {
                sbatch_command.arg(format!("--gpus-per-task={gpu_count}"));
            }

            // Add any user-configured extra arguments.
            if let Some(args) = &backend_config.extra_sbatch_args {
                sbatch_command.args(args);
            }

            sbatch_command
                // Use verbose output that we can check later on
                .arg("-v")
                // Keep `sbatch` running until the job terminates
                .arg("--wait")
                // Pipe stdout and stderr so we can identify when a job begins, and can trace any
                // other output. This should just be the `sbatch` verbose output on stderr.
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                // Name the Slurm job after the task ID, which has already been shortened to fit
                // into the Slurm requirements.
                .arg("--job-name")
                .arg(&name)
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
                    request.constraints().cpu.ceil() as u64
                ))
                // Memory request is specified per node in mebibytes; we round the request up to the
                // next mebibyte.
                //
                // Note that the Slurm documentation says "megabyte" (i.e., the base-10 unit), but
                // the other explanations of the unit suffixes in the first-party documentation show
                // the use of base-2 units, and multiple third-party sources available through
                // online searches back the base-2 interpretation, for example:
                //
                // https://info.nrao.edu/computing/guide/cluster-processing/appendix/memory-options
                // https://wcmscu.atlassian.net/wiki/spaces/WIKI/pages/327731/Using+Slurm
                .arg(format!(
                    "--mem={}M",
                    (request.constraints().memory as f64 / bytesize::MIB as f64).ceil() as u64
                ))
                .arg(apptainer_command_path);

            debug!(?sbatch_command, "spawning `sbatch` command");

            let mut sbatch_child = sbatch_command.spawn()?;

            // Create a task-specific cancellation token that is independent of the overall cancellation context
            let task_token = CancellationToken::new();
            crankshaft::events::send_event!(
                self.events.crankshaft(),
                crankshaft::events::Event::TaskCreated {
                    id: crankshaft_task_id,
                    name: name.clone(),
                    tes_id: None,
                    token: task_token.clone(),
                },
            );

            // Take the stdio pipes from the child process and consume them for event
            // reporting and tracing purposes.
            //
            // TODO ACF 2025-10-13: generate `sbatch`-compatible scripts instead and use a
            // polling mechanism to watch for job status changes? `squeue` can emit json
            // suitable for this.
            let sbatch_stdout = sbatch_child
                .stdout
                .take()
                .ok_or_else(|| anyhow!("sbatch child stdout missing"))?;
            let task_name = name.clone();
            let stdout_crankshaft_events = self.events.crankshaft().clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(sbatch_stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    // TODO ACF 2025-10-14: `sbatch --wait` even on high verbosity doesn't tell us
                    // when a job has actually started, only when it's been submitted.  Unless we
                    // can figure out a way to get that info out directly, we'll have to set up a
                    // separate task to poll job statuses. For the moment, this is potentially
                    // misleading about what work has actually begun computation.
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
            let task_name = name.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(sbatch_stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    trace!(stderr = line, task_name);
                }
            });

            // Await the result of the `sbatch` command, which will only exit on error or
            // once the containerized command has completed.
            let token = self.cancellation.second();
            let sbatch_result = tokio::select! {
                _ = task_token.cancelled() => {
                    crankshaft::events::send_event!(
                        self.events.crankshaft(),
                        crankshaft::events::Event::TaskCanceled {
                            id: crankshaft_task_id
                        },
                    );
                    return Ok(None);
                }
                _ = token.cancelled() => {
                    crankshaft::events::send_event!(
                        self.events.crankshaft(),
                        crankshaft::events::Event::TaskCanceled {
                            id: crankshaft_task_id
                        },
                    );
                    return Ok(None);
                }
                result = sbatch_child.wait() => result.context("failed to wait for `sbatch` process")?,
            };

            crankshaft::events::send_event!(
                self.events.crankshaft(),
                crankshaft::events::Event::TaskCompleted {
                    id: crankshaft_task_id,
                    exit_statuses: NonEmpty::new(sbatch_result),
                }
            );

            Ok(Some(TaskExecutionResult {
                // Under normal circumstances, the exit code of `sbatch --wait` is the exit code of
                // its command, and the exit code of `apptainer exec` is likewise the exit code of
                // its command. One potential subtlety/problem here is that if `sbatch` or
                // `apptainer` exit due to an error before running the WDL command, we could be
                // erroneously ascribing an exit code to the WDL command.
                exit_code: sbatch_result
                    .code()
                    .ok_or(anyhow!("task did not return an exit code"))?,
                work_dir: EvaluationPath::from_local_path(work_dir),
                stdout: PrimitiveValue::new_file(
                    stdout_path
                        .into_os_string()
                        .into_string()
                        .expect("path should be UTF-8"),
                )
                .into(),
                stderr: PrimitiveValue::new_file(
                    stderr_path
                        .into_os_string()
                        .into_string()
                        .expect("path should be UTF-8"),
                )
                .into(),
            }))
        }
        .boxed()
    }
}
