//! Experimental LSF + Apptainer (aka Singularity) task execution backend.
//!
//! This experimental backend submits each task as an LSF job which invokes
//! Apptainer to provide the appropriate container environment for the WDL
//! command to execute.
//!
//! Due to the proprietary nature of LSF, and limited ability to install
//! Apptainer locally or in CI, this is currently tested by hand; expect (and
//! report) bugs! In follow-up work, we hope to build a limited test suite based
//! on mocking CLI invocations and/or golden testing of generated
//! `bsub`/`apptainer` scripts.

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
use tokio::fs;
use tokio::fs::File;
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

/// The maximum length of an LSF job name.
///
/// See <https://www.ibm.com/docs/en/spectrum-lsf/10.1.0?topic=o-j>.
const LSF_JOB_NAME_MAX_LENGTH: usize = 4094;

/// The experimental LSF + Apptainer backend.
///
/// See the module-level documentation for details.
pub struct LsfApptainerBackend {
    /// The shared engine configuration.
    config: Arc<Config>,
    /// The engine events.
    events: Events,
    /// The evaluation cancellation context.
    cancellation: CancellationContext,
    /// Apptainer state.
    apptainer_state: ApptainerState,
}

impl LsfApptainerBackend {
    /// Create a new backend.
    ///
    /// The `run_root_dir` argument should be a directory that exists for the
    /// duration of the entire top-level evaluation. It is used to store
    /// Apptainer images which should only be created once per container per
    /// run.
    pub fn new(
        config: Arc<Config>,
        run_root_dir: &Path,
        events: Events,
        cancellation: CancellationContext,
    ) -> Result<Self> {
        // Ensure the configured backend is LSF Apptainer
        config
            .backend()?
            .as_lsf_apptainer()
            .context("configured backend is not LSF Apptainer")?;

        Ok(Self {
            config,
            events,
            cancellation,
            apptainer_state: ApptainerState::new(run_root_dir),
        })
    }
}

impl TaskExecutionBackend for LsfApptainerBackend {
    fn constraints(
        &self,
        inputs: &TaskInputs,
        requirements: &HashMap<String, Value>,
        hints: &HashMap<String, Value>,
    ) -> Result<TaskExecutionConstraints> {
        let mut required_cpu = requirements::cpu(inputs, requirements);
        let mut required_memory = ByteSize::b(requirements::memory(inputs, requirements)? as u64);

        let backend_config = self.config.backend()?;
        let backend_config = backend_config
            .as_lsf_apptainer()
            .expect("configured backend is not LSF Apptainer");

        // Determine whether CPU or memory limits are set for this queue, and clamp or
        // deny them as appropriate if the limits are exceeded
        if let Some(queue) = backend_config.lsf_queue_for_task(requirements, hints) {
            if let Some(max_cpu) = queue.max_cpu_per_task()
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

            if let Some(max_memory) = queue.max_memory_per_task()
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
            bail!("LSF Apptainer backend does not support unknown container source `{container:#}`")
        }

        Ok(TaskExecutionConstraints {
            container: Some(container),
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
                .as_lsf_apptainer()
                .expect("configured backend is not LSF Apptainer");

            // Truncate the request ID to fit in the LSF job name length limit.
            let request_id = request.id();
            let name = if request_id.len() > LSF_JOB_NAME_MAX_LENGTH {
                request_id
                    .chars()
                    .take(LSF_JOB_NAME_MAX_LENGTH)
                    .collect::<String>()
            } else {
                request_id.to_string()
            };

            let crankshaft_task_id = crankshaft::events::next_task_id();
            let attempt_dir = request.attempt_dir();

            // Create the host directory that will be mapped to the working directory.
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

            // Create an empty file for the task's stderr
            let stderr_path = request.stderr_path();
            let _ = File::create(&stderr_path).await.with_context(|| {
                format!(
                    "failed to create stderr file `{path}`",
                    path = stderr_path.display()
                )
            })?;

            // Write the evaluated command section to a host file.
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

            // The path for the LSF-level stdout and stderr. This primarily contains the job
            // report, as we redirect Apptainer and WDL output separately.
            let lsf_stdout_path = attempt_dir.join("lsf.stdout");
            let lsf_stderr_path = attempt_dir.join("lsf.stderr");

            let mut bsub_command = Command::new("bsub");

            // If an LSF queue has been configured, specify it. Otherwise, the job will end
            // up on the cluster's default queue.
            if let Some(queue) =
                backend_config.lsf_queue_for_task(request.requirements(), request.hints())
            {
                bsub_command.arg("-q").arg(queue.name());
            }

            // If GPUs are required, pass a basic `-gpu` flag to `bsub`.
            if let Some(n_gpu) = requirements::gpu(inputs, request.requirements(), request.hints())
            {
                bsub_command.arg("-gpu").arg(format!("num={n_gpu}/host"));
            }

            // Add any user-configured extra arguments.
            if let Some(args) = &backend_config.extra_bsub_args {
                bsub_command.args(args);
            }

            bsub_command
                // Pipe stdout and stderr so we can identify when a job begins, and can trace any
                // other output. This should just be the LSF output like `<<Waiting for dispatch
                // ...>>`.
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                // TODO ACF 2025-09-10: make this configurable; hardcode turning off LSF email spam
                // for now though.
                .env("LSB_JOB_REPORT_MAIL", "N")
                // This option makes the `bsub` invocation synchronous, so this command will not
                // exit until the job is complete.
                //
                // If the number of concurrent `bsub` processes becomes a problem, we can switch
                // this to an asynchronous model where we drop this argument, grab the job ID, and
                // poll for it using `bjobs`.
                .arg("-K")
                // Name the LSF job after the task ID, which has already been shortened to fit into
                // the LSF requirements.
                .arg("-J")
                .arg(&name)
                // Send LSF job stdout and stderr streams to these files. Since we redirect the
                // Apptainer invocation's stdio to separate files, this will typically amount to the
                // LSF job report.
                .arg("-oo")
                .arg(lsf_stdout_path)
                .arg("-eo")
                .arg(lsf_stderr_path)
                // CPU request is rounded up to the nearest whole CPU
                .arg("-R")
                .arg(format!(
                    "affinity[cpu({cpu})]",
                    cpu = request.constraints().cpu.ceil() as u64
                ))
                // Memory request is specified per job to avoid ambiguity on clusters which may be
                // configured to interpret memory requests as per-core or per-task. We also use an
                // explicit KB unit which LSF appears to interpret as base-2 kibibytes.
                .arg("-R")
                .arg(format!(
                    "rusage[mem={memory_kb}KB/job]",
                    memory_kb = request.constraints().memory / bytesize::KIB,
                ))
                .arg(apptainer_command_path);

            debug!(?bsub_command, "spawning `bsub` command");

            let mut bsub_child = bsub_command.spawn()?;

            // Create a task-specific cancellation token that is independent of the overall
            // cancellation context
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
            // TODO ACF 2025-09-23: drop the `-K` from `bsub` and poll status instead? Could
            // be less intensive from a resource perspective vs having a process and two
            // loops on the head node per task, but we should wait to observe real-world
            // performance before complicating things.
            let bsub_stdout = bsub_child
                .stdout
                .take()
                .ok_or_else(|| anyhow!("bsub child stdout missing"))?;
            let task_name = name.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(bsub_stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    trace!(stdout = line, task_name);
                }
            });
            let bsub_stderr = bsub_child
                .stderr
                .take()
                .ok_or_else(|| anyhow!("bsub child stderr missing"))?;
            let task_name = name.clone();
            let stderr_crankshaft_events = self.events.crankshaft().clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(bsub_stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.starts_with("<<Starting") {
                        crankshaft::events::send_event!(
                            stderr_crankshaft_events,
                            crankshaft::events::Event::TaskStarted {
                                id: crankshaft_task_id
                            },
                        );
                    }
                    trace!(stderr = line, task_name);
                }
            });

            // Await the result of the `bsub` command, which will only exit on error or once
            // the containerized command has completed.
            let token = self.cancellation.second();
            let bsub_result = tokio::select! {
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
                result = bsub_child.wait() => result.context("failed to wait for `bsub` process")?,
            };

            crankshaft::events::send_event!(
                self.events.crankshaft(),
                crankshaft::events::Event::TaskCompleted {
                    id: crankshaft_task_id,
                    exit_statuses: NonEmpty::new(bsub_result),
                }
            );

            Ok(Some(TaskExecutionResult {
                // Under normal circumstances, the exit code of `bsub -K` is the exit code of its
                // command, and the exit code of `apptainer exec` is likewise the exit code of its
                // command. One potential subtlety/problem here is that if `bsub` or `apptainer`
                // exit due to an error before running the WDL command, we could be erroneously
                // ascribing an exit code to the WDL command.
                exit_code: bsub_result
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
