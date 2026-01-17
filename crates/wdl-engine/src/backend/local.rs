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
use futures::FutureExt;
use futures::future::BoxFuture;
use nonempty::NonEmpty;
use tokio::process::Command;
use tokio::select;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing::warn;

use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use super::TaskSpawnRequest;
use crate::CancellationContext;
use crate::EvaluationPath;
use crate::Events;
use crate::ONE_GIBIBYTE;
use crate::PrimitiveValue;
use crate::SYSTEM;
use crate::TaskInputs;
use crate::Value;
use crate::backend::COMMAND_FILE_NAME;
use crate::backend::INITIAL_EXPECTED_NAMES;
use crate::backend::STDERR_FILE_NAME;
use crate::backend::STDOUT_FILE_NAME;
use crate::backend::TaskExecutionResult;
use crate::backend::WORK_DIR_NAME;
use crate::backend::manager::TaskManager;
use crate::config::Config;
use crate::config::DEFAULT_TASK_SHELL;
use crate::config::TaskResourceLimitBehavior;
use crate::convert_unit_string;
use crate::http::Transferer;
use crate::v1::requirements;

/// Represents a local task request.
///
/// This request contains the requested cpu and memory reservations for the task
/// as well as the result receiver channel.
#[derive(Debug)]
struct LocalTask {
    /// The engine configuration.
    config: Arc<Config>,
    /// The task spawn request.
    request: TaskSpawnRequest,
    /// The name of the task.
    name: String,
    /// The sender for events.
    events: Option<broadcast::Sender<Event>>,
    /// The evaluation cancellation context.
    cancellation: CancellationContext,
}

impl LocalTask {
    /// Runs the local task.
    ///
    /// Returns `Ok(None)` if the task was canceled.
    async fn run(self) -> Result<Option<TaskExecutionResult>> {
        let id = next_task_id();
        let work_dir = self.request.attempt_dir().join(WORK_DIR_NAME);
        let stdout_path = self.request.attempt_dir().join(STDOUT_FILE_NAME);
        let stderr_path = self.request.attempt_dir().join(STDERR_FILE_NAME);

        let run = async {
            // Create the working directory
            fs::create_dir_all(&work_dir).with_context(|| {
                format!(
                    "failed to create directory `{path}`",
                    path = work_dir.display()
                )
            })?;

            // Write the evaluated command to disk
            let command_path = self.request.attempt_dir().join(COMMAND_FILE_NAME);
            fs::write(&command_path, self.request.command()).with_context(|| {
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
                    self.request
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
        let task_token = CancellationToken::new();
        send_event!(
            self.events,
            Event::TaskCreated {
                id,
                name: self.name.clone(),
                tes_id: None,
                token: task_token.clone(),
            }
        );

        let token = self.cancellation.second();

        select! {
            // Poll the cancellation tokens before the child future
            biased;
            _ = task_token.cancelled() => {
                send_event!(self.events, Event::TaskCanceled { id });
                Ok(None)
            }
            _ = token.cancelled() => {
                send_event!(self.events, Event::TaskCanceled { id });
                Ok(None)
            }
            result = run => {
                match result {
                    Ok(status) => {
                        send_event!(self.events, Event::TaskCompleted { id, exit_statuses: NonEmpty::new(status) });

                        let exit_code = status.code().expect("process should have exited");
                        info!("process {id} for task `{name}` has terminated with status code {exit_code}", name = self.name);
                        Ok(Some(TaskExecutionResult {
                            exit_code,
                            work_dir: EvaluationPath::from_local_path(work_dir),
                            stdout: PrimitiveValue::new_file(stdout_path.into_os_string().into_string().expect("path should be UTF-8")).into(),
                            stderr: PrimitiveValue::new_file(stderr_path.into_os_string().into_string().expect("path should be UTF-8")).into(),
                        }))
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
    /// The evaluation cancellation context.
    cancellation: CancellationContext,
    /// The total CPU of the host.
    cpu: f64,
    /// The total memory of the host.
    memory: u64,
    /// The underlying task manager.
    manager: TaskManager,
    /// The name generator for tasks.
    names: Arc<Mutex<GeneratorIterator<UniqueAlphanumeric>>>,
    /// The sender for events.
    events: Events,
}

impl LocalBackend {
    /// Constructs a new local task execution backend with the given
    /// configuration.
    ///
    /// The provided configuration is expected to have already been validated.
    pub fn new(
        config: Arc<Config>,
        events: Events,
        cancellation: CancellationContext,
    ) -> Result<Self> {
        info!("initializing local backend");

        let names = Arc::new(Mutex::new(GeneratorIterator::new(
            UniqueAlphanumeric::default_with_expected_generations(INITIAL_EXPECTED_NAMES),
            INITIAL_EXPECTED_NAMES,
        )));

        let backend_config = config.backend()?;
        let backend_config = backend_config
            .as_local()
            .context("configured backend is not local")?;
        let cpu = backend_config
            .cpu
            .map(|v| v as f64)
            .unwrap_or_else(|| SYSTEM.cpus().len() as f64);
        let memory = backend_config
            .memory
            .as_ref()
            .map(|s| convert_unit_string(s).expect("value should be valid"))
            .unwrap_or_else(|| SYSTEM.total_memory());
        let manager = TaskManager::new(
            cpu,
            cpu,
            memory,
            memory,
            events.clone(),
            cancellation.clone(),
        );

        Ok(Self {
            config,
            cancellation,
            cpu,
            memory,
            manager,
            names,
            events,
        })
    }
}

impl TaskExecutionBackend for LocalBackend {
    fn constraints(
        &self,
        inputs: &TaskInputs,
        requirements: &HashMap<String, Value>,
        _: &HashMap<String, Value>,
    ) -> Result<TaskExecutionConstraints> {
        let mut cpu = requirements::cpu(inputs, requirements);
        if self.cpu < cpu {
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
                    cpu = self.cpu;
                }
                TaskResourceLimitBehavior::Deny => {
                    bail!(
                        "task requires at least {cpu} CPU{s}{env_specific}",
                        s = if cpu == 1.0 { "" } else { "s" },
                    );
                }
            }
        }

        let mut memory = requirements::memory(inputs, requirements)? as u64;
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
                    memory = self.memory;
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

    fn guest_inputs_dir(&self) -> Option<&'static str> {
        // Local execution does not use a container
        None
    }

    fn spawn<'a>(
        &'a self,
        _: &'a TaskInputs,
        request: TaskSpawnRequest,
        _transferer: Arc<dyn Transferer>,
    ) -> BoxFuture<'a, Result<Option<TaskExecutionResult>>> {
        async move {
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

            let cpu = request.constraints().cpu;
            let memory = request.constraints().memory;

            let task = LocalTask {
                config: self.config.clone(),
                request,
                name,
                events: self.events.crankshaft().clone(),
                cancellation: self.cancellation.clone(),
            };

            self.manager.spawn(cpu, memory, task.run()).await
        }
        .boxed()
    }
}
