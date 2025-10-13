#![allow(clippy::missing_docs_in_private_items)]

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
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::Context as _;
use anyhow::anyhow;
use anyhow::bail;
use crankshaft::events::Event;
use images::sif_for_container;
use nonempty::NonEmpty;
use tokio::fs::File;
use tokio::fs::{self};
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::error;
use tracing::trace;
use tracing::warn;

use super::COMMAND_FILE_NAME;
use super::TaskExecutionBackend;
use super::TaskManager;
use super::TaskManagerRequest;
use super::TaskSpawnRequest;
use super::WORK_DIR_NAME;
use crate::PrimitiveValue;
use crate::STDERR_FILE_NAME;
use crate::STDOUT_FILE_NAME;
use crate::TaskExecutionResult;
use crate::Value;
use crate::config::Config;
use crate::path::EvaluationPath;
use crate::v1;

mod images;

/// The name of the file where the Apptainer command invocation will be written.
const APPTAINER_COMMAND_FILE_NAME: &str = "apptainer_command";

/// The root guest path for inputs.
const GUEST_INPUTS_DIR: &str = "/mnt/task/inputs/";

/// The guest working directory.
const GUEST_WORK_DIR: &str = "/mnt/task/work";

/// The guest path for the command file.
const GUEST_COMMAND_PATH: &str = "/mnt/task/command";

/// The path to the container's stdout.
const GUEST_STDOUT_PATH: &str = "/mnt/task/stdout";

/// The path to the container's stderr.
const GUEST_STDERR_PATH: &str = "/mnt/task/stderr";

/// The maximum length of a Slurm job name.
// TODO ACF 2025-10-13: I worked this out experimentally on the cluster I happen
// to have access to. I do not know whether this translates to other Slurm
// installations, and cannot find documentation about what this limit should be
// or whether it's configurable.
const SLURM_JOB_NAME_MAX_LENGTH: usize = 1024;

#[derive(Debug)]
struct SlurmApptainerTaskRequest {
    backend_config: Arc<SlurmApptainerBackendConfig>,
    name: String,
    spawn_request: TaskSpawnRequest,
    /// The requested container for the task.
    container: String,
    /// The requested CPU reservation for the task.
    cpu: f64,
    /// The requested memory reservation for the task, in bytes.
    memory: u64,
    /// The broadcast channel to update interested parties with the status of
    /// executing tasks.
    ///
    /// This backend does not yet take advantage of the full Crankshaft
    /// machinery, but we send rudimentary messages on this channel which helps
    /// with UI presentation.
    crankshaft_events: Option<broadcast::Sender<Event>>,
    cancellation_token: CancellationToken,
}

impl TaskManagerRequest for SlurmApptainerTaskRequest {
    fn cpu(&self) -> f64 {
        self.cpu
    }

    fn memory(&self) -> u64 {
        self.memory
    }

    async fn run(self) -> anyhow::Result<super::TaskExecutionResult> {
        let crankshaft_task_id = crankshaft::events::next_task_id();

        let container_sif = sif_for_container(
            &self.backend_config,
            &self.container,
            self.cancellation_token.clone(),
        )
        .await?;

        let attempt_dir = self.spawn_request.attempt_dir();

        // Create the host directory that will be mapped to the WDL working directory.
        let wdl_work_dir = attempt_dir.join(WORK_DIR_NAME);
        fs::create_dir_all(&wdl_work_dir).await.with_context(|| {
            format!(
                "failed to create WDL working directory `{path}`",
                path = wdl_work_dir.display()
            )
        })?;

        // Write the evaluated WDL command section to a host file.
        let wdl_command_path = attempt_dir.join(COMMAND_FILE_NAME);
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

        // Create an empty file for the WDL command's stdout.
        let wdl_stdout_path = attempt_dir.join(STDOUT_FILE_NAME);
        let _ = File::create(&wdl_stdout_path).await.with_context(|| {
            format!(
                "failed to create WDL stdout file `{path}`",
                path = wdl_stdout_path.display()
            )
        })?;

        // Create an empty file for the WDL command's stderr.
        let wdl_stderr_path = attempt_dir.join(STDERR_FILE_NAME);
        let _ = File::create(&wdl_stderr_path).await.with_context(|| {
            format!(
                "failed to create WDL stderr file `{path}`",
                path = wdl_stderr_path.display()
            )
        })?;

        // Create a temp dir for the container's execution within the attempt dir
        // hierarchy. On many HPC systems, `/tmp` is mapped to a relatively
        // small, local scratch disk that can fill up easily. Mapping the
        // container's `/tmp` and `/var/tmp` paths to the filesystem we're using
        // for other inputs and outputs prevents this from being a capacity problem,
        // though potentially at the expense of execution speed if the
        // non-`/tmp` filesystem is significantly slower.
        let container_tmp_path = self
            .spawn_request
            .temp_dir()
            .join("container_tmp")
            .to_path_buf();
        tokio::fs::DirBuilder::new()
            .recursive(true)
            .create(&container_tmp_path)
            .await
            .with_context(|| {
                format!(
                    "failed to create container /tmp directory at `{path}`",
                    path = container_tmp_path.display()
                )
            })?;
        let container_var_tmp_path = self
            .spawn_request
            .temp_dir()
            .join("container_var_tmp")
            .to_path_buf();
        tokio::fs::DirBuilder::new()
            .recursive(true)
            .create(&container_var_tmp_path)
            .await
            .with_context(|| {
                format!(
                    "failed to create container /var/tmp directory at `{path}`",
                    path = container_var_tmp_path.display()
                )
            })?;

        // Assemble the Apptainer invocation. We'll write out this command to the host
        // filesystem, and ultimately submit it as the command to run via Slurm.
        let apptainer_command_path = attempt_dir.join(APPTAINER_COMMAND_FILE_NAME);
        let mut apptainer_command = String::new();
        writeln!(&mut apptainer_command, "#!/bin/env bash")?;

        // Set up any WDL-specified guest environment variables, using the
        // `APPTAINERENV_` prefix approach (ref:
        // https://apptainer.org/docs/user/1.3/environment_and_metadata.html#apptainerenv-prefix) to
        // avoid command line argument limits.
        for (k, v) in self.spawn_request.env().iter() {
            writeln!(&mut apptainer_command, "export APPTAINERENV_{k}={v}")?;
        }

        // Begin writing the `apptainer` command itself. We're using the synchronous
        // `exec` command which keeps running until the containerized command is
        // finished.
        write!(&mut apptainer_command, "apptainer -v exec ")?;
        write!(&mut apptainer_command, "--cwd {GUEST_WORK_DIR} ")?;
        // These options make the Apptainer sandbox behave more like default Docker
        // behavior, e.g. by not auto-mounting the user's home directory and
        // inheriting all environment variables.
        write!(&mut apptainer_command, "--containall --cleanenv ")?;

        for input in self.spawn_request.inputs() {
            write!(
                &mut apptainer_command,
                "--mount type=bind,src={host_path},dst={guest_path},ro ",
                host_path = input
                    .local_path()
                    .ok_or_else(|| anyhow!("input not localized: {input:?}"))?
                    .display(),
                guest_path = input
                    .guest_path()
                    .ok_or_else(|| anyhow!("guest path missing: {input:?}"))?,
            )?;
        }

        // Mount the instantiated WDL command as read-only.
        write!(
            &mut apptainer_command,
            "--mount type=bind,src={},dst={GUEST_COMMAND_PATH},ro ",
            wdl_command_path.display()
        )?;
        // Mount the working dir, temp dirs, and stdio files as read/write (no `,ro` on
        // the end like for the inputs).
        write!(
            &mut apptainer_command,
            "--mount type=bind,src={},dst={GUEST_WORK_DIR} ",
            wdl_work_dir.display()
        )?;
        write!(
            &mut apptainer_command,
            "--mount type=bind,src={},dst=/tmp ",
            container_tmp_path.display()
        )?;
        write!(
            &mut apptainer_command,
            "--mount type=bind,src={},dst=/var/tmp ",
            container_var_tmp_path.display()
        )?;
        write!(
            &mut apptainer_command,
            "--mount type=bind,src={},dst={GUEST_STDOUT_PATH} ",
            wdl_stdout_path.display()
        )?;
        write!(
            &mut apptainer_command,
            "--mount type=bind,src={},dst={GUEST_STDERR_PATH} ",
            wdl_stderr_path.display()
        )?;
        // Add the `--nv` argument if a GPU is required by the task.
        if let Some(true) = self
            .spawn_request
            .requirements()
            .get(wdl_ast::v1::TASK_REQUIREMENT_GPU)
            .and_then(Value::as_boolean)
        {
            write!(&mut apptainer_command, "--nv ")?;
        }

        // Add any user-configured extra arguments.
        if let Some(args) = &self.backend_config.extra_apptainer_exec_args {
            for arg in args {
                write!(&mut apptainer_command, "{arg} ")?;
            }
        }
        // Specify the container sif file as a positional argument.
        write!(&mut apptainer_command, "{} ", container_sif.display())?;
        // Provide the instantiated WDL command, with its stdio handles redirected to
        // their respective guest paths.
        write!(
            &mut apptainer_command,
            "bash -c \"{GUEST_COMMAND_PATH} > {GUEST_STDOUT_PATH} 2> {GUEST_STDERR_PATH}\" "
        )?;
        // The path for the Apptainer-level stdout and stderr.
        let apptainer_stdout_path = attempt_dir.join("apptainer.stdout");
        let apptainer_stderr_path = attempt_dir.join("apptainer.stderr");
        // Redirect the output of Apptainer itself to these files. We run Apptainer with
        // verbosity cranked up, so these should be helpful diagnosing failures.
        writeln!(
            &mut apptainer_command,
            "> {stdout} 2> {stderr}",
            stdout = apptainer_stdout_path.display(),
            stderr = apptainer_stderr_path.display()
        )?;

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

        let mut srun_command = Command::new("srun");

        // If a Slurm partition has been configured, specify it. Otherwise, the job will
        // end up on the cluster's default partition.
        if let Some(partition) = self.backend_config.slurm_partition_for_task(
            self.spawn_request.requirements(),
            self.spawn_request.hints(),
        ) {
            srun_command.arg("--partition").arg(partition);
        }

        // If GPUs are required, pass a basic `--gpus-per-node` flag to `srun`. If this
        // is a bare `requirements { gpu: true }`, we request 1 GPU per node. If
        // there is also an integer `hints: { gpu: n }`, we request `n` GPUs per
        // node.
        if let Some(true) = self
            .spawn_request
            .requirements()
            .get(wdl_ast::v1::TASK_REQUIREMENT_GPU)
            .and_then(Value::as_boolean)
        {
            match self.spawn_request.hints().get(wdl_ast::v1::TASK_HINT_GPU) {
                Some(Value::Primitive(PrimitiveValue::Integer(n))) => {
                    srun_command.arg(format!("--gpus-per-node={n}"));
                }
                Some(Value::Primitive(PrimitiveValue::String(hint))) => {
                    warn!(
                        %hint,
                        "string hints for GPU are not supported; falling back to 1 GPU per host"
                    );
                    srun_command.arg("--gpus-per-node=1");
                }
                // Other hint value types should be rejected already, so the remaining valid case is
                // a GPU requirement with no hints
                _ => {
                    srun_command.arg("--gpus-per-node=1");
                }
            }
        }

        // Add any user-configured extra arguments.
        if let Some(args) = &self.backend_config.extra_srun_args {
            srun_command.args(args);
        }

        srun_command
            // Use verbose output that we can check later on
            .arg("-v")
            // Pipe stdout and stderr so we can identify when a job begins, and can trace any other
            // output. This should just be the `srun` verbose output on stderr.
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
            // CPU request is rounded up to the nearest whole CPU
            .arg(format!("--cpus-per-task={}", self.cpu.ceil() as u64))
            // Memory request is specified per node in megabytes
            .arg(format!(
                "--mem={}",
                (self.memory as f64 / (1024.0 * 1024.0)).ceil() as u64
            ))
            .arg(apptainer_command_path);

        let mut srun_child = srun_command.spawn()?;

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
        let s4run_stdout = srun_child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("srun child stdout missing"))?;
        let task_name = self.name.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(s4run_stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                trace!(stdout = line, task_name);
            }
        });
        let srun_stderr = srun_child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("srun child stderr missing"))?;
        let task_name = self.name.clone();
        let stderr_crankshaft_events = self.crankshaft_events.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(srun_stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                // TODO ACF 2025-10-13: this could probably be made more robust, but while we're
                // only submitting one job at a time with `srun` we know that if any tasks
                // start, it's the one we're monitoring
                if line.starts_with("srun:") && line.ends_with("tasks started") {
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

        // Await the result of the `srun` command, which will only exit on error or once
        // the containerized command has completed.
        let srun_result = tokio::select! {
            _ = self.cancellation_token.cancelled() => {
                crankshaft::events::send_event!(
                    self.crankshaft_events,
                    crankshaft::events::Event::TaskCanceled {
                        id: crankshaft_task_id
                    },
                );
                Err(anyhow!("task execution cancelled"))
            }
            result = srun_child.wait() => result.map_err(Into::into),
        }?;

        crankshaft::events::send_event!(
            self.crankshaft_events,
            crankshaft::events::Event::TaskCompleted {
                id: crankshaft_task_id,
                exit_statuses: NonEmpty::new(srun_result),
            }
        );

        Ok(TaskExecutionResult {
            // Under normal circumstances, the exit code of `srun` is the exit code of its
            // command, and the exit code of `apptainer exec` is likewise the exit code of its
            // command. One potential subtlety/problem here is that if `srun` or `apptainer` exit
            // due to an error before running the WDL command, we could be erroneously ascribing an
            // exit code to the WDL command.
            exit_code: srun_result
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
    engine_config: Arc<Config>,
    backend_config: Arc<SlurmApptainerBackendConfig>,
    manager: TaskManager<SlurmApptainerTaskRequest>,
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
        _hints: &std::collections::HashMap<String, crate::Value>,
    ) -> anyhow::Result<super::TaskExecutionConstraints> {
        Ok(super::TaskExecutionConstraints {
            container: Some(
                v1::container(requirements, self.engine_config.task.container.as_deref())
                    .into_owned(),
            ),
            // TODO ACF 2025-10-13: populate more meaningful values for these based on the given
            // Slurm partition.
            cpu: f64::MAX,
            memory: i64::MAX,
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
        let cpu = v1::cpu(requirements);
        let memory = v1::memory(requirements)? as u64;
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
                cpu,
                memory,
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
    pub default_slurm_partition: Option<String>,
    /// Which partition, if any, to specify when submitting [short
    /// tasks](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#short_task) to Slurm.
    ///
    /// This may be superseded by
    /// [`gpu_slurm_partition`][Self::gpu_slurm_partition] or
    /// [`fpga_slurm_partition`][Self::fpga_slurm_partition] for tasks which
    /// require specialized hardware.
    pub short_task_slurm_partition: Option<String>,
    /// Which partition, if any, to specify when submitting [tasks which require
    /// a GPU](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#hardware-accelerators-gpu-and--fpga)
    /// to Slurm.
    pub gpu_slurm_partition: Option<String>,
    /// Which partition, if any, to specify when submitting [tasks which require
    /// a GPU](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#hardware-accelerators-gpu-and--fpga)
    /// to Slurm.
    pub fpga_slurm_partition: Option<String>,
    /// Additional command-line arguments to pass to `srun` when submitting jobs
    /// to Slurm.
    pub extra_srun_args: Option<Vec<String>>,
    /// The maximum number of scatter subtasks that can be evaluated
    /// concurrently.
    ///
    /// By default, this is 200.
    #[serde(default = "default_max_scatter_concurrency")]
    pub max_scatter_concurrency: u64,
    /// Additional command-line arguments to pass to `apptainer exec` when
    /// executing tasks.
    pub extra_apptainer_exec_args: Option<Vec<String>>,
    /// The directory in which temporary directories will be created containing
    /// Apptainer `.sif` files.
    ///
    /// This should be a location that is accessible by all jobs on the Slurm
    /// cluster.
    ///
    /// By default, this is `$HOME/.cache/sprocket-apptainer-images`, or
    /// `/tmp/sprocket-apptainer-images` if the home directory cannot be
    /// determined.
    #[serde(default = "default_apptainer_images_dir")]
    pub apptainer_images_dir: PathBuf,
}

fn default_max_scatter_concurrency() -> u64 {
    200
}

fn default_apptainer_images_dir() -> PathBuf {
    if let Some(cache) = dirs::cache_dir() {
        cache.join("sprocket-apptainer-images").to_path_buf()
    } else {
        std::env::temp_dir()
            .join("sprocket-apptainer-images")
            .to_path_buf()
    }
}

impl Default for SlurmApptainerBackendConfig {
    fn default() -> Self {
        Self {
            default_slurm_partition: None,
            short_task_slurm_partition: None,
            gpu_slurm_partition: None,
            fpga_slurm_partition: None,
            extra_srun_args: None,
            max_scatter_concurrency: default_max_scatter_concurrency(),
            apptainer_images_dir: default_apptainer_images_dir(),
            extra_apptainer_exec_args: None,
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
            validate_slurm_partition("default", partition).await?;
        }
        if let Some(partition) = &self.short_task_slurm_partition {
            validate_slurm_partition("short_task", partition).await?;
        }
        if let Some(partition) = &self.gpu_slurm_partition {
            validate_slurm_partition("gpu", partition).await?;
        }
        if let Some(partition) = &self.fpga_slurm_partition {
            validate_slurm_partition("fpga", partition).await?;
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
    ) -> Option<&str> {
        // TODO ACF 2025-09-26: what's the relationship between this code and
        // `TaskExecutionConstraints`? Should this be there instead, or be pulling
        // values from that instead of directly from `requirements` and `hints`?

        // Specialized hardware gets priority.
        if let Some(partition) = self.fpga_slurm_partition.as_deref()
            && let Some(true) = requirements
                .get(wdl_ast::v1::TASK_REQUIREMENT_FPGA)
                .and_then(Value::as_boolean)
        {
            return Some(partition);
        }

        if let Some(partition) = self.gpu_slurm_partition.as_deref()
            && let Some(true) = requirements
                .get(wdl_ast::v1::TASK_REQUIREMENT_GPU)
                .and_then(Value::as_boolean)
        {
            return Some(partition);
        }

        // Then short tasks.
        if let Some(partition) = self.short_task_slurm_partition.as_deref()
            && let Some(true) = hints
                .get(wdl_ast::v1::TASK_HINT_SHORT_TASK)
                .and_then(Value::as_boolean)
        {
            return Some(partition);
        }

        // Finally the default partition. If this is `None`, `srun` gets run without a
        // partition argument and the cluster's default is used.
        self.default_slurm_partition.as_deref()
    }
}

async fn validate_slurm_partition(name: &str, partition: &str) -> Result<(), anyhow::Error> {
    match tokio::time::timeout(
        // 10 seconds is rather arbitrary; `sinfo` ordinarily returns extremely quickly, but we
        // don't want things to run away on a misconfigured system
        std::time::Duration::from_secs(10),
        Command::new("sinfo")
            .arg(format!("--partition={partition}"))
            // TODO ACF 2025-10-13: this is a fairly crude way to validate, but I couldn't
            // quickly figure out a way to get `sinfo` or `squeue` to give me a non-zero exit
            // code for an invalid partition name, so instead this arranges for stdout to be empty
            // if the given name doesn't match a partition.
            .arg("--noheader")
            .output(),
    )
    .await
    {
        Ok(output) => {
            let output = output.context("validating Slurm partition")?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            // If there is nothing but whitespace in stdout, the partition is not valid
            if !output.status.success() || stdout.chars().all(char::is_whitespace) {
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
