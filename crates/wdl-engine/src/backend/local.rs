//! Implementation of the local backend.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use crankshaft::engine::service::name::GeneratorIterator;
use crankshaft::engine::service::name::UniqueAlphanumeric;
use crankshaft::events::Event;
use crankshaft::events::next_task_id;
use crankshaft::events::send_event;
use nonempty::NonEmpty;
use tokio::process::Command;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::oneshot;
use tokio::sync::oneshot::Receiver;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing::warn;
use wdl_ast::Diagnostic;
use wdl_ast::v1::TaskDefinition;

use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use super::TaskManager;
use super::TaskManagerRequest;
use super::TaskSpawnRequest;
use crate::COMMAND_FILE_NAME;
use crate::ONE_GIBIBYTE;
use crate::PrimitiveValue;
use crate::STDERR_FILE_NAME;
use crate::STDOUT_FILE_NAME;
use crate::SYSTEM;
use crate::TaskExecutionResult;
use crate::Value;
use crate::WORK_DIR_NAME;
use crate::backend::INITIAL_EXPECTED_NAMES;
use crate::config::Config;
use crate::config::DEFAULT_TASK_SHELL;
use crate::config::LocalBackendConfig;
use crate::config::TaskResourceLimitBehavior;
use crate::convert_unit_string;
use crate::path::EvaluationPath;
use crate::tree::SyntaxNode;
use crate::v1::cpu;
use crate::v1::cpu_from_map;
use crate::v1::memory;
use crate::v1::memory_from_map;

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
    /// The name of the task.
    name: String,
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
    /// The sender for events.
    events: Option<broadcast::Sender<Event>>,
}

impl TaskManagerRequest for LocalTaskRequest {
    fn cpu(&self) -> f64 {
        self.cpu
    }

    fn memory(&self) -> u64 {
        self.memory
    }

    async fn run(self) -> Result<TaskExecutionResult> {
        let id = next_task_id();
        let work_dir = self.inner.attempt_dir().join(WORK_DIR_NAME);
        let stdout_path = self.inner.attempt_dir().join(STDOUT_FILE_NAME);
        let stderr_path = self.inner.attempt_dir().join(STDERR_FILE_NAME);

        let run = async {
            // Create the working directory
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
            let stdout = File::create(&stdout_path).with_context(|| {
                format!(
                    "failed to create stdout file `{path}`",
                    path = stdout_path.display()
                )
            })?;

            // Create a file for the stderr
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
            send_event!(self.events, Event::TaskStarted { id });

            let id = child.id().expect("should have id");
            info!(
                "spawned local shell process {id} for execution of task `{name}`",
                name = self.name
            );

            let status = child.wait().await.with_context(|| {
                format!("failed to wait for termination of task child process {id}")
            })?;

            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(signal) = status.signal() {
                    tracing::warn!("task process {id} has terminated with signal {signal}");

                    bail!(
                        "task child process {id} has terminated with signal {signal}; see stderr \
                         file `{path}` for more details",
                        path = stderr_path.display()
                    );
                }
            }

            Ok(status)
        };

        // Send the created event
        send_event!(
            self.events,
            Event::TaskCreated {
                id,
                name: self.name.clone(),
                tes_id: None,
                token: self.token.clone(),
            }
        );

        select! {
            // Poll the cancellation token before the child future
            biased;

            _ = self.token.cancelled() => {
                send_event!(self.events, Event::TaskCanceled { id });
                bail!("task was cancelled");
            }
            result = run => {
                match result {
                    Ok(status) => {
                        send_event!(self.events, Event::TaskCompleted { id, exit_statuses: NonEmpty::new(status) });

                        let exit_code = status.code().expect("process should have exited");
                        info!("process {id} for task `{name}` has terminated with status code {exit_code}", name = self.name);
                        Ok(TaskExecutionResult {
                            exit_code,
                            work_dir: EvaluationPath::Local(work_dir),
                            stdout: PrimitiveValue::new_file(stdout_path.into_os_string().into_string().expect("path should be UTF-8")).into(),
                            stderr: PrimitiveValue::new_file(stderr_path.into_os_string().into_string().expect("path should be UTF-8")).into(),
                        })
                    }
                    Err(e) => {
                        send_event!(self.events, Event::TaskFailed { id, message: format!("{e:#}") });
                        Err(e)
                    }
                }
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
    /// The name generator for tasks.
    names: Arc<Mutex<GeneratorIterator<UniqueAlphanumeric>>>,
    /// The sender for events.
    events: Option<broadcast::Sender<Event>>,
}

impl LocalBackend {
    /// Constructs a new local task execution backend with the given
    /// configuration.
    ///
    /// The provided configuration is expected to have already been validated.
    pub fn new(
        config: Arc<Config>,
        backend_config: &LocalBackendConfig,
        events: Option<broadcast::Sender<Event>>,
    ) -> Result<Self> {
        info!("initializing local backend");

        let names = Arc::new(Mutex::new(GeneratorIterator::new(
            UniqueAlphanumeric::default_with_expected_generations(INITIAL_EXPECTED_NAMES),
            INITIAL_EXPECTED_NAMES,
        )));

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
            names,
            events,
        })
    }
}

impl TaskExecutionBackend for LocalBackend {
    fn max_concurrency(&self) -> u64 {
        self.cpu
    }

    fn constraints(
        &self,
        task: &TaskDefinition<SyntaxNode>,
        requirements: &HashMap<String, Value>,
        _: &HashMap<String, Value>,
    ) -> Result<TaskExecutionConstraints, Diagnostic> {
        let mut required_cpu = cpu(task, requirements);
        if (self.cpu as f64) < required_cpu.value {
            let span = required_cpu.span;
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
                        cpu = required_cpu.value,
                        s = if required_cpu.value == 1.0 { "" } else { "s" },
                    );
                    // clamp the reported constraint to what's available
                    required_cpu.value = self.cpu as f64;
                }
                TaskResourceLimitBehavior::Deny => {
                    let msg = format!(
                        "task requires at least {cpu} CPU{s}{env_specific}",
                        cpu = required_cpu.value,
                        s = if required_cpu.value == 1.0 { "" } else { "s" },
                    );
                    return Err(Diagnostic::error(msg)
                        .with_label("this requirement exceeds the available CPUs", span));
                }
            }
        }

        let mut required_memory = memory(task, requirements)?;
        if self.memory < required_memory.value as u64 {
            let span = required_memory.span;
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
                        memory = required_memory.value as f64 / ONE_GIBIBYTE,
                    );
                    // clamp the reported constraint to what's available
                    required_memory.value = self.memory.try_into().unwrap_or(i64::MAX);
                }
                TaskResourceLimitBehavior::Deny => {
                    let msg = format!(
                        "task requires at least {memory} GiB of memory{env_specific}",
                        // Display the error in GiB, as it is the most common unit for memory
                        memory = required_memory.value as f64 / ONE_GIBIBYTE,
                    );
                    return Err(Diagnostic::error(msg)
                        .with_label("this requirement exceeds the available memory", span));
                }
            }
        }

        Ok(TaskExecutionConstraints {
            container: None,
            cpu: required_cpu.value,
            memory: required_memory.value,
            gpu: Default::default(),
            fpga: Default::default(),
            disks: Default::default(),
        })
    }

    fn guest_inputs_dir(&self) -> Option<&'static str> {
        // Local execution does not use a container
        None
    }

    fn needs_local_inputs(&self) -> bool {
        true
    }

    fn spawn(
        &self,
        request: TaskSpawnRequest,
        token: CancellationToken,
    ) -> Result<Receiver<Result<TaskExecutionResult>>> {
        let (completed_tx, completed_rx) = oneshot::channel();

        let requirements = request.requirements();
        let mut cpu = cpu_from_map(requirements);
        if let TaskResourceLimitBehavior::TryWithMax = self.config.task.cpu_limit_behavior {
            cpu = std::cmp::min(cpu.ceil() as u64, self.cpu) as f64;
        }
        let mut memory = memory_from_map(requirements)? as u64;
        if let TaskResourceLimitBehavior::TryWithMax = self.config.task.memory_limit_behavior {
            memory = std::cmp::min(memory, self.memory);
        }

        let name = format!(
            "{id}-{generated}",
            id = request.id(),
            generated = self
                .names
                .lock()
                .expect("generator should always acquire")
                .next()
                .expect("generator should never be exhausted")
        );

        self.manager.send(
            LocalTaskRequest {
                config: self.config.clone(),
                inner: request,
                name,
                cpu,
                memory,
                token,
                events: self.events.clone(),
            },
            completed_tx,
        );

        Ok(completed_rx)
    }
}
