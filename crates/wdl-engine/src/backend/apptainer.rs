#![allow(clippy::missing_docs_in_private_items)]

//! Experimental Apptainer (aka Singularity) task execution backend.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::Context as _;
use anyhow::anyhow;
use anyhow::bail;
use crankshaft::events::Event;
use nonempty::NonEmpty;
use tokio::fs::File;
use tokio::fs::{self};
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::trace;

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

#[derive(Debug)]
struct ApptainerTaskRequest {
    backend_config: Arc<ApptainerBackendConfig>,
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

impl TaskManagerRequest for ApptainerTaskRequest {
    fn cpu(&self) -> f64 {
        self.cpu
    }

    fn memory(&self) -> u64 {
        self.memory
    }

    async fn run(self) -> anyhow::Result<super::TaskExecutionResult> {
        let crankshaft_task_id = crankshaft::events::next_task_id();

        let container_sif = images::sif_for_container(
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
        // filesystem, and ultimately submit it as the command to run via LSF.
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

        let mut apptainer_child = Command::new(&apptainer_command_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        crankshaft::events::send_event!(
            self.crankshaft_events,
            crankshaft::events::Event::TaskCreated {
                id: crankshaft_task_id,
                name: self.name.clone(),
                tes_id: None,
                token: self.cancellation_token.clone(),
            },
        );
        crankshaft::events::send_event!(
            self.crankshaft_events,
            crankshaft::events::Event::TaskStarted {
                id: crankshaft_task_id
            },
        );

        // Take the stdio pipes from the child process and consume them for event
        // reporting and tracing purposes.
        let apptainer_stdout = apptainer_child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("apptainer child stdout missing"))?;
        let task_name = self.name.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(apptainer_stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                trace!(stdout = line, task_name);
            }
        });
        let apptainer_stderr = apptainer_child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("apptainer child stderr missing"))?;
        let task_name = self.name.clone();
        let _stderr_crankshaft_events = self.crankshaft_events.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(apptainer_stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                trace!(stderr = line, task_name);
            }
        });

        // Await the result of the `apptainer` command, which will only exit on error or
        // once the containerized command has completed.
        let apptainer_result = tokio::select! {
            _ = self.cancellation_token.cancelled() => {
                crankshaft::events::send_event!(
                    self.crankshaft_events,
                    crankshaft::events::Event::TaskCanceled {
                        id: crankshaft_task_id
                    },
                );
                Err(anyhow!("task execution cancelled"))
            }
            result = apptainer_child.wait() => result.map_err(Into::into),
        }?;

        crankshaft::events::send_event!(
            self.crankshaft_events,
            crankshaft::events::Event::TaskCompleted {
                id: crankshaft_task_id,
                exit_statuses: NonEmpty::new(apptainer_result),
            }
        );

        Ok(TaskExecutionResult {
            // Under normal circumstances, the exit code `apptainer exec` the exit code of its
            // command. One potential subtlety/problem here is that if `apptainer` exits due to an
            // error before running the WDL command, we could be erroneously ascribing an exit code
            // to the WDL command.
            exit_code: apptainer_result
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

/// The experimental Apptainer backend.
///
/// See the module-level documentation for details.
#[derive(Debug)]
pub struct ApptainerBackend {
    engine_config: Arc<Config>,
    backend_config: Arc<ApptainerBackendConfig>,
    manager: TaskManager<ApptainerTaskRequest>,
    crankshaft_events: Option<broadcast::Sender<Event>>,
}

impl ApptainerBackend {
    /// Create a new backend.
    pub fn new(
        engine_config: Arc<Config>,
        backend_config: Arc<ApptainerBackendConfig>,
        crankshaft_events: Option<broadcast::Sender<Event>>,
    ) -> Self {
        Self {
            engine_config,
            backend_config,
            // TODO ACF 2025-09-29: Should be able to get maxes from the local system when executing
            // locally.
            manager: TaskManager::new_unlimited(u64::MAX, u64::MAX),
            crankshaft_events,
        }
    }
}

impl TaskExecutionBackend for ApptainerBackend {
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
        // TODO ACF 2025-09-29: `apptainer exec` has CPU and memory limits, we should
        // use them when executing locally
        let _max_cpu = v1::max_cpu(hints);
        let _max_memory = v1::max_memory(hints)?.map(|i| i as u64);

        let name = request.id().to_string();

        self.manager.send(
            ApptainerTaskRequest {
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

/// Configuration for the LSF + Apptainer backend.
// TODO ACF 2025-09-23: add a Apptainer/Singularity mode config that switches around executable
// name, env var names, etc.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ApptainerBackendConfig {
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
    /// This should be a location that is accessible by all jobs on the LSF
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

impl Default for ApptainerBackendConfig {
    fn default() -> Self {
        Self {
            max_scatter_concurrency: default_max_scatter_concurrency(),
            apptainer_images_dir: default_apptainer_images_dir(),
            extra_apptainer_exec_args: None,
        }
    }
}

impl ApptainerBackendConfig {
    /// Validate that the backend is appropriately configured.
    pub fn validate(&self, engine_config: &Config) -> Result<(), anyhow::Error> {
        if cfg!(not(unix)) {
            bail!("Apptainer backend is not supported on non-unix platforms");
        }
        if !engine_config.experimental_features_enabled {
            bail!("Apptainer backend requires enabling experimental features");
        }
        Ok(())
    }
}
