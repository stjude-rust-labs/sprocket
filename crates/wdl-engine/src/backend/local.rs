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
use anyhow::bail;
use futures::FutureExt;
use futures::future::BoxFuture;
use tokio::process::Command;
use tokio::select;
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing::warn;

use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use super::TaskExecutionEvents;
use super::TaskManager;
use super::TaskManagerRequest;
use super::TaskSpawnRequest;
use crate::COMMAND_FILE_NAME;
use crate::Input;
use crate::ONE_GIBIBYTE;
use crate::PrimitiveValue;
use crate::STDERR_FILE_NAME;
use crate::STDOUT_FILE_NAME;
use crate::SYSTEM;
use crate::TaskExecutionResult;
use crate::Value;
use crate::WORK_DIR_NAME;
use crate::config::Config;
use crate::config::DEFAULT_TASK_SHELL;
use crate::config::LocalBackendConfig;
use crate::config::TaskResourceLimitBehavior;
use crate::convert_unit_string;
use crate::http::Downloader;
use crate::http::HttpDownloader;
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
    /// The engine configuration.
    config: Arc<Config>,
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
        let work_dir = self.inner.attempt_dir().join(WORK_DIR_NAME);
        fs::create_dir_all(&work_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = work_dir.display()
            )
        })?;

        // Write the evaluated command to disk
        let command_path = self.inner.attempt_dir().join(COMMAND_FILE_NAME);
        fs::write(&command_path, self.inner.command()).with_context(|| {
            format!(
                "failed to write command contents to `{path}`",
                path = command_path.display()
            )
        })?;

        // Create a file for the stdout
        let stdout_path = self.inner.attempt_dir().join(STDOUT_FILE_NAME);
        let stdout = File::create(&stdout_path).with_context(|| {
            format!(
                "failed to create stdout file `{path}`",
                path = stdout_path.display()
            )
        })?;

        // Create a file for the stderr
        let stderr_path = self.inner.attempt_dir().join(STDERR_FILE_NAME);
        let stderr = File::create(&stderr_path).with_context(|| {
            format!(
                "failed to create stderr file `{path}`",
                path = stderr_path.display()
            )
        })?;

        let mut command = Command::new(
            self.config
                .task
                .shell
                .as_deref()
                .unwrap_or(DEFAULT_TASK_SHELL),
        );
        command
            .current_dir(&work_dir)
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

        // Set the PATH variable for the child on Windows to get consistent PATH
        // searching. See: https://github.com/rust-lang/rust/issues/122660
        #[cfg(windows)]
        if let Ok(path) = std::env::var("PATH") {
            command.env("PATH", path);
        }

        let mut child = command.spawn().context("failed to spawn shell")?;

        // Notify that the process has spawned
        spawned.send(()).ok();

        let id = child.id().expect("should have id");
        info!("spawned local shell process {id} for task execution");

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
                    inputs: self.inner.info.inputs,
                    exit_code,
                    work_dir: EvaluationPath::Local(work_dir),
                    stdout: PrimitiveValue::new_file(stdout_path.into_os_string().into_string().expect("path should be UTF-8")).into(),
                    stderr: PrimitiveValue::new_file(stderr_path.into_os_string().into_string().expect("path should be UTF-8")).into(),
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
pub struct LocalBackend {
    /// The engine configuration.
    config: Arc<Config>,
    /// The total CPU of the host.
    cpu: u64,
    /// The total memory of the host.
    memory: u64,
    /// The underlying task manager.
    manager: TaskManager<LocalTaskRequest>,
}

impl LocalBackend {
    /// Constructs a new local task execution backend with the given
    /// configuration.
    ///
    /// The provided configuration is expected to have already been validated.
    pub fn new(config: Arc<Config>, backend_config: &LocalBackendConfig) -> Result<Self> {
        info!("initializing local backend");

        let cpu = backend_config
            .cpu
            .unwrap_or_else(|| SYSTEM.cpus().len() as u64);
        let memory = backend_config
            .memory
            .as_ref()
            .map(|s| convert_unit_string(s).expect("value should be valid"))
            .unwrap_or_else(|| SYSTEM.total_memory());
        let manager = TaskManager::new(cpu, cpu, memory, memory);

        Ok(Self {
            config,
            cpu,
            memory,
            manager,
        })
    }
}

impl TaskExecutionBackend for LocalBackend {
    fn max_concurrency(&self) -> u64 {
        self.cpu
    }

    fn constraints(
        &self,
        requirements: &HashMap<String, Value>,
        _: &HashMap<String, Value>,
    ) -> Result<TaskExecutionConstraints> {
        let mut cpu = cpu(requirements);
        if (self.cpu as f64) < cpu {
            let env_specific = if self.config.suppress_env_specific_output {
                String::new()
            } else {
                format!(
                    ", but the host only has {total_cpu} available",
                    total_cpu = self.cpu
                )
            };
            match self.config.task.cpu_limit_behavior {
                TaskResourceLimitBehavior::TryWithMax => {
                    warn!(
                        "task requires at least {cpu} CPU{s}{env_specific}",
                        s = if cpu == 1.0 { "" } else { "s" },
                    );
                    // clamp the reported constraint to what's available
                    cpu = self.cpu as f64;
                }
                TaskResourceLimitBehavior::Deny => {
                    bail!(
                        "task requires at least {cpu} CPU{s}{env_specific}",
                        s = if cpu == 1.0 { "" } else { "s" },
                    );
                }
            }
        }

        let mut memory = memory(requirements)?;
        if self.memory < memory as u64 {
            let env_specific = if self.config.suppress_env_specific_output {
                String::new()
            } else {
                format!(
                    ", but the host only has {total_memory} GiB available",
                    total_memory = self.memory as f64 / ONE_GIBIBYTE,
                )
            };
            match self.config.task.memory_limit_behavior {
                TaskResourceLimitBehavior::TryWithMax => {
                    warn!(
                        "task requires at least {memory} GiB of memory{env_specific}",
                        // Display the error in GiB, as it is the most common unit for memory
                        memory = memory as f64 / ONE_GIBIBYTE,
                    );
                    // clamp the reported constraint to what's available
                    memory = self.memory.try_into().unwrap_or(i64::MAX);
                }
                TaskResourceLimitBehavior::Deny => {
                    bail!(
                        "task requires at least {memory} GiB of memory{env_specific}",
                        // Display the error in GiB, as it is the most common unit for memory
                        memory = memory as f64 / ONE_GIBIBYTE,
                    );
                }
            }
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
        downloader: &'b HttpDownloader,
        inputs: &'c mut [Input],
    ) -> BoxFuture<'d, Result<()>>
    where
        'a: 'd,
        'b: 'd,
        'c: 'd,
        Self: 'd,
    {
        async move {
            let mut downloads = JoinSet::new();

            for (idx, input) in inputs.iter_mut().enumerate() {
                match input.path() {
                    EvaluationPath::Local(path) => {
                        let location = Location::Path(path.clone().into());
                        let guest_path = location
                            .to_str()
                            .with_context(|| {
                                format!("path `{path}` is not UTF-8", path = path.display())
                            })?
                            .to_string();
                        input.set_location(location.into_owned());
                        input.set_guest_path(guest_path);
                    }
                    EvaluationPath::Remote(url) => {
                        let downloader = downloader.clone();
                        let url = url.clone();
                        downloads.spawn(async move {
                            let location_result = downloader.download(&url).await;

                            match location_result {
                                Ok(location) => Ok((idx, location.into_owned())),
                                Err(e) => bail!("failed to localize `{url}`: {e:?}"),
                            }
                        });
                    }
                }
            }

            while let Some(result) = downloads.join_next().await {
                match result {
                    Ok(Ok((idx, location))) => {
                        let guest_path = location
                            .to_str()
                            .with_context(|| {
                                format!(
                                    "downloaded path `{path}` is not UTF-8",
                                    path = location.display()
                                )
                            })?
                            .to_string();

                        let input = inputs.get_mut(idx).expect("index should be valid");
                        input.set_location(location);
                        input.set_guest_path(guest_path);
                    }
                    Ok(Err(e)) => {
                        // Futures are aborted when the `JoinSet` is dropped.
                        bail!(e);
                    }
                    Err(e) => {
                        // Futures are aborted when the `JoinSet` is dropped.
                        bail!("download task failed: {e}");
                    }
                }
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
        let mut cpu = cpu(requirements);
        if let TaskResourceLimitBehavior::TryWithMax = self.config.task.cpu_limit_behavior {
            cpu = std::cmp::min(cpu.ceil() as u64, self.cpu) as f64;
        }
        let mut memory = memory(requirements)? as u64;
        if let TaskResourceLimitBehavior::TryWithMax = self.config.task.memory_limit_behavior {
            memory = std::cmp::min(memory, self.memory);
        }

        self.manager.send(
            LocalTaskRequest {
                config: self.config.clone(),
                inner: request,
                cpu,
                memory,
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
