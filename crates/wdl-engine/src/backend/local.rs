//! Implementation of the local backend.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use futures::FutureExt;
use futures::future::BoxFuture;
use tokio::process::Command;
use tokio::select;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use super::TaskExecutionEvents;
use super::TaskManager;
use super::TaskManagerRequest;
use super::TaskSpawnRequest;
use crate::Input;
use crate::ONE_GIBIBYTE;
use crate::SYSTEM;
use crate::TaskExecutionResult;
use crate::Value;
use crate::WORK_DIR_NAME;
use crate::config::DEFAULT_TASK_SHELL;
use crate::config::LocalBackendConfig;
use crate::config::TaskConfig;
use crate::convert_unit_string;
use crate::http::Downloader;
use crate::http::Location;
use crate::path::EvaluationPath;
use crate::v1::cpu;
use crate::v1::memory;

/// Represents a local task request.
///
/// This request contains the requested cpu and memory reservations for the task
/// as well as the result receiver channel.
#[derive(Debug)]
struct LocalTaskRequest {
    /// The inner task spawn request.
    inner: TaskSpawnRequest,
    /// The requested CPU reservation for the task.
    ///
    /// Note that CPU isn't actually reserved for the task process.
    cpu: f64,
    /// The requested memory reservation for the task.
    ///
    /// Note that memory isn't actually reserved for the task process.
    memory: u64,
    /// The shell to use for spawning the task.
    shell: Option<Arc<String>>,
    /// The cancellation token for the request.
    token: CancellationToken,
}

impl TaskManagerRequest for LocalTaskRequest {
    fn cpu(&self) -> f64 {
        self.cpu
    }

    fn memory(&self) -> u64 {
        self.memory
    }

    async fn run(self, spawned: oneshot::Sender<()>) -> Result<TaskExecutionResult> {
        // Create the working directory
        let work_dir = self.inner.root.attempt_dir().join(WORK_DIR_NAME);
        fs::create_dir_all(&work_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = work_dir.display()
            )
        })?;

        // Write the evaluated command to disk
        let command_path = self.inner.root.command();
        fs::write(command_path, self.inner.command()).with_context(|| {
            format!(
                "failed to write command contents to `{path}`",
                path = command_path.display()
            )
        })?;

        // Create a file for the stdout
        let stdout_path = self.inner.root.stdout();
        let stdout = File::create(stdout_path).with_context(|| {
            format!(
                "failed to create stdout file `{path}`",
                path = stdout_path.display()
            )
        })?;

        // Create a file for the stderr
        let stderr_path = self.inner.root.stderr();
        let stderr = File::create(stderr_path).with_context(|| {
            format!(
                "failed to create stderr file `{path}`",
                path = stderr_path.display()
            )
        })?;

        let mut command = Command::new(
            self.shell
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or(DEFAULT_TASK_SHELL),
        );
        command
            .current_dir(&work_dir)
            .arg("-C")
            .arg(command_path)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .envs(
                self.inner
                    .env()
                    .iter()
                    .map(|(k, v)| (OsStr::new(k), OsStr::new(v))),
            )
            .kill_on_drop(true);

        // Set an environment variable on Windows to get consistent PATH searching
        // See: https://github.com/rust-lang/rust/issues/122660
        #[cfg(windows)]
        command.env("WDL_TASK_EVALUATION", "1");

        let mut child = command.spawn().context("failed to spawn `bash`")?;

        // Notify that the process has spawned
        spawned.send(()).ok();

        let id = child.id().expect("should have id");
        info!("spawned local `bash` process {id} for task execution");

        select! {
            // Poll the cancellation token before the child future
            biased;

            _ = self.token.cancelled() => {
                bail!("task was cancelled");
            }
            status = child.wait() => {
                let status = status.with_context(|| {
                    format!("failed to wait for termination of task child process {id}")
                })?;

                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    if let Some(signal) = status.signal() {
                        tracing::warn!("task process {id} has terminated with signal {signal}");

                        bail!(
                            "task child process {id} has terminated with signal {signal}; see stderr file \
                            `{path}` for more details",
                            path = stderr_path.display()
                        );
                    }
                }

                let exit_code = status.code().expect("process should have exited");
                info!("task process {id} has terminated with status code {exit_code}");
                Ok(TaskExecutionResult {
                    exit_code,
                    work_dir: EvaluationPath::Local(work_dir),
                })
            }
        }
    }
}

/// Represents a task execution backend that locally executes tasks.
///
/// <div class="warning">
/// Warning: the local task execution backend spawns processes on the host
/// directly without the use of a container; only use this backend on trusted
/// WDL. </div>
pub struct LocalTaskExecutionBackend {
    /// The total CPU of the host.
    cpu: u64,
    /// The total memory of the host.
    memory: u64,
    /// The default shell to use for running tasks.
    shell: Option<Arc<String>>,
    /// The underlying task manager.
    manager: TaskManager<LocalTaskRequest>,
}

impl LocalTaskExecutionBackend {
    /// Constructs a new local task execution backend with the given
    /// configuration.
    pub fn new(task: &TaskConfig, config: &LocalBackendConfig) -> Result<Self> {
        task.validate()?;
        config.validate()?;

        let cpu = config.cpu.unwrap_or_else(|| SYSTEM.cpus().len() as u64);
        let memory = config
            .memory
            .as_ref()
            .map(|s| convert_unit_string(s).expect("value should be valid"))
            .unwrap_or_else(|| SYSTEM.total_memory());
        let manager = TaskManager::new(cpu, cpu, memory, memory);

        Ok(Self {
            cpu,
            memory,
            shell: task.shell.as_ref().map(|s| Arc::new(s.clone())),
            manager,
        })
    }
}

impl TaskExecutionBackend for LocalTaskExecutionBackend {
    fn max_concurrency(&self) -> u64 {
        self.cpu
    }

    fn constraints(
        &self,
        requirements: &HashMap<String, Value>,
        _: &HashMap<String, Value>,
    ) -> Result<TaskExecutionConstraints> {
        let cpu = cpu(requirements);
        if (self.cpu as f64) < cpu {
            bail!(
                "task requires at least {cpu} CPU{s}, but the host only has {total_cpu} available",
                s = if cpu == 1.0 { "" } else { "s" },
                total_cpu = self.cpu,
            );
        }

        let memory = memory(requirements)?;
        if self.memory < memory as u64 {
            // Display the error in GiB, as it is the most common unit for memory
            let memory = memory as f64 / ONE_GIBIBYTE;
            let total_memory = self.memory as f64 / ONE_GIBIBYTE;

            bail!(
                "task requires at least {memory} GiB of memory, but the host only has \
                 {total_memory} GiB available",
            );
        }

        Ok(TaskExecutionConstraints {
            container: None,
            cpu,
            memory,
            gpu: Default::default(),
            fpga: Default::default(),
            disks: Default::default(),
        })
    }

    fn guest_work_dir(&self) -> Option<&Path> {
        // Local execution does not use a container
        None
    }

    fn localize_inputs<'a, 'b, 'c, 'd>(
        &'a self,
        downloader: &'b dyn Downloader,
        inputs: &'c mut [Input],
    ) -> BoxFuture<'d, Result<()>>
    where
        'a: 'd,
        'b: 'd,
        'c: 'd,
        Self: 'd,
    {
        async {
            for input in inputs {
                // TODO: parallelize the downloads
                let location = match input.path() {
                    EvaluationPath::Local(path) => Location::Path(path.into()),
                    EvaluationPath::Remote(url) => downloader
                        .download(url)
                        .await
                        .map_err(|e| anyhow!("failed to localize `{url}`: {e:?}"))?,
                };

                let guest_path = location
                    .to_str()
                    .with_context(|| {
                        format!("path `{path}` is not UTF-8", path = location.display())
                    })?
                    .to_string();

                // Set the guest path to the download location for path translation
                let location = location.into_owned();
                input.set_guest_path(guest_path);
                input.set_location(location);
            }

            Ok(())
        }
        .boxed()
    }

    fn spawn(
        &self,
        request: TaskSpawnRequest,
        token: CancellationToken,
    ) -> Result<TaskExecutionEvents> {
        let (spawned_tx, spawned_rx) = oneshot::channel();
        let (completed_tx, completed_rx) = oneshot::channel();

        let requirements = request.requirements();
        let cpu = cpu(requirements);
        let memory = memory(requirements)? as u64;

        self.manager.send(
            LocalTaskRequest {
                inner: request,
                cpu,
                memory,
                shell: self.shell.clone(),
                token,
            },
            spawned_tx,
            completed_tx,
        );

        Ok(TaskExecutionEvents {
            spawned: spawned_rx,
            completed: completed_rx,
        })
    }
}
