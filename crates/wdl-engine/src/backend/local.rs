//! Implementation of the local backend.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::path::Path;
use std::process::Stdio;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tracing::debug;
use tracing::info;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::v1::TASK_REQUIREMENT_CPU;
use wdl_ast::v1::TASK_REQUIREMENT_MEMORY;

use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use super::TaskSpawnRequest;
use crate::Coercible;
use crate::SYSTEM;
use crate::Value;
use crate::config::LocalBackendConfig;
use crate::convert_unit_string;
use crate::v1::DEFAULT_TASK_REQUIREMENT_CPU;
use crate::v1::DEFAULT_TASK_REQUIREMENT_MEMORY;

/// Gets the `cpu` requirement from a requirements map.
fn cpu(requirements: &HashMap<String, Value>) -> f64 {
    requirements
        .get(TASK_REQUIREMENT_CPU)
        .map(|v| {
            v.coerce(&PrimitiveType::Float.into())
                .expect("type should coerce")
                .unwrap_float()
        })
        .unwrap_or(DEFAULT_TASK_REQUIREMENT_CPU)
}

/// Gets the `memory` requirement from a requirements map.
fn memory(requirements: &HashMap<String, Value>) -> Result<i64> {
    Ok(requirements
        .get(TASK_REQUIREMENT_MEMORY)
        .map(|v| {
            if let Some(v) = v.as_integer() {
                return Ok(v);
            }

            if let Some(s) = v.as_string() {
                return convert_unit_string(s)
                    .and_then(|v| v.try_into().ok())
                    .with_context(|| {
                        format!("task specifies an invalid `memory` requirement `{s}`")
                    });
            }

            unreachable!("value should be an integer or string");
        })
        .transpose()?
        .unwrap_or(DEFAULT_TASK_REQUIREMENT_MEMORY))
}

/// Represents a local task spawn request.
///
/// This request contains the requested cpu and memory reservations for the task
/// as well as the result receiver channel.
#[derive(Debug)]
struct LocalTaskSpawnRequest {
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
    /// The sender to send the response back on.
    tx: oneshot::Sender<Result<i32>>,
}

/// Represents a local task spawn response.
#[derive(Debug)]
struct LocalTaskSpawnResponse {
    /// The result of execution.
    result: Result<i32>,
    /// The requested CPU reservation for the task.
    ///
    /// Note that CPU isn't actually reserved for the task process.
    cpu: f64,
    /// The requested memory reservation for the task.
    ///
    /// Note that memory isn't actually reserved for the task process.
    memory: u64,
    /// The sender to send the response back on.
    tx: oneshot::Sender<Result<i32>>,
}

/// Represents state for the local task execution backend.
struct State {
    /// The amount of available host CPU remaining.
    ///
    /// This doesn't reflect the _actual_ CPU currently available, only what
    /// remains after "reserving" resources for each executing task.
    ///
    /// Initially this is the total CPUs of the host.
    cpu: f64,
    /// The amount of available host memory remaining.
    ///
    /// This doesn't reflect the _actual_ memory currently available, only what
    /// remains after "reserving" resources for each executing task.
    ///
    /// Initially this is the total memory of the host.
    memory: u64,
    /// The set of spawned tasks.
    spawned: JoinSet<LocalTaskSpawnResponse>,
    /// The queue of parked spawn requests.
    ///
    /// A request may be parked if there isn't enough available resources on the
    /// host.
    parked: VecDeque<LocalTaskSpawnRequest>,
}

impl State {
    /// Creates a new state with the given total CPU and memory for the host.
    fn new(cpu: f64, memory: u64) -> Self {
        Self {
            cpu,
            memory,
            spawned: Default::default(),
            parked: Default::default(),
        }
    }
}

/// Represents a task execution backend that locally executes tasks.
///
/// <div class="warning">
/// Warning: the local task execution backend spawns processes on the host
/// directly without the use of a container; only use this backend on trusted
/// WDL. </div>
#[derive(Debug)]
pub struct LocalTaskExecutionBackend {
    /// The total CPU of the host.
    cpu: u64,
    /// The total memory of the host.
    memory: u64,
    /// The sender for new spawn requests.
    tx: mpsc::UnboundedSender<LocalTaskSpawnRequest>,
}

impl LocalTaskExecutionBackend {
    /// Constructs a new local task execution backend with the given
    /// configuration.
    pub fn new(config: &LocalBackendConfig) -> Result<Self> {
        config.validate()?;

        let cpu = config.cpu.unwrap_or_else(|| SYSTEM.cpus().len() as u64);
        let memory = config
            .memory
            .as_ref()
            .map(|s| convert_unit_string(s).expect("value should be valid"))
            .unwrap_or_else(|| SYSTEM.total_memory());

        let (tx, rx) = mpsc::unbounded_channel::<LocalTaskSpawnRequest>();
        tokio::spawn(Self::run_request_queue(rx, cpu as f64, memory));

        Ok(Self { cpu, memory, tx })
    }

    /// Runs the spawn request queue.
    ///
    /// The spawn request queue is responsible for spawning new tasks.
    ///
    /// A task may not immediately run if there aren't enough resources
    /// (CPU or memory) available.
    async fn run_request_queue(
        mut rx: mpsc::UnboundedReceiver<LocalTaskSpawnRequest>,
        total_cpu: f64,
        total_memory: u64,
    ) {
        let mut state = State::new(total_cpu, total_memory);

        loop {
            // If there aren't any spawned tasks, wait for a spawn request only
            if state.spawned.is_empty() {
                assert!(
                    state.parked.is_empty(),
                    "there can't be any parked requests if there are no spawned tasks"
                );
                match rx.recv().await {
                    Some(request) => {
                        Self::handle_spawn_request(&mut state, total_cpu, total_memory, request);
                        continue;
                    }
                    None => break,
                }
            }

            // Otherwise, wait for a spawn request or a completed task
            tokio::select! {
                request = rx.recv() => {
                    match request {
                        Some(request) => {
                            Self::handle_spawn_request(&mut state, total_cpu, total_memory, request);
                        }
                        None => break,
                    }
                }
                Some(Ok(response)) = state.spawned.join_next() => {
                    state.cpu += response.cpu;
                    state.memory += response.memory;
                    response.tx.send(response.result).ok();

                    // Look for tasks to unpark
                    while let Some(pos) = state.parked.iter().position(|r| r.cpu <= state.cpu && r.memory <= state.memory) {
                        let request = state.parked.swap_remove_back(pos).unwrap();

                        debug!(
                            "unparking task with requested CPU {cpu} and memory {memory}",
                            cpu = request.cpu,
                            memory = request.memory,
                        );

                        Self::handle_spawn_request(&mut state, total_cpu, total_memory, request);
                    }
                }
            }
        }
    }

    /// Handles a spawn request by either parking it (not enough resources
    /// currently available) or by spawning it.
    fn handle_spawn_request(
        state: &mut State,
        total_cpu: f64,
        total_memory: u64,
        mut request: LocalTaskSpawnRequest,
    ) {
        // Ensure the request does not exceed the total CPU
        if request.cpu > total_cpu {
            request
                .tx
                .send(Err(anyhow!(
                    "requested task CPU count of {cpu} exceeds the total host CPU count of \
                     {total_cpu}",
                    cpu = request.cpu
                )))
                .ok();
            return;
        }

        // Ensure the request does not exceed the total memory
        if request.memory > total_memory {
            request
                .tx
                .send(Err(anyhow!(
                    "requested task memory of {memory} byte{s} exceeds the total host memory of \
                     {total_memory}",
                    memory = request.memory,
                    s = if request.memory == 1 { "" } else { "s" }
                )))
                .ok();
            return;
        }

        // If the request can't be processed due to resource constraints, park the
        // request for now When a task completes and resources become available,
        // we'll unpark the request
        if request.cpu > state.cpu || request.memory > state.memory {
            debug!(
                "insufficient host resources to spawn a new task (requested {cpu} CPU with \
                 {cpu_remaining} CPU remaining and {memory} memory with {memory_remaining} \
                 remaining); task has been parked",
                cpu = request.cpu,
                memory = request.memory,
                cpu_remaining = state.cpu,
                memory_remaining = state.memory
            );
            state.parked.push_back(request);
            return;
        }

        // Decrement the resource counts and spawn the task
        state.cpu -= request.cpu;
        state.memory -= request.memory;
        debug!(
            "spawning task with {cpu} CPUs and {memory} bytes of memory remaining",
            cpu = state.cpu,
            memory = state.memory
        );

        state.spawned.spawn(async move {
            let spawned = request.inner.spawned.take().unwrap();
            spawned.send(()).ok();

            LocalTaskSpawnResponse {
                result: Self::spawn_task(&request.inner).await,
                cpu: request.cpu,
                memory: request.memory,
                tx: request.tx,
            }
        });
    }

    /// Spawns the requested task.
    ///
    /// Returns the status code of the process when it has exited.
    async fn spawn_task(request: &TaskSpawnRequest) -> Result<i32> {
        // Recreate the working directory
        let work_dir = request.root.work_dir();
        fs::create_dir_all(work_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = work_dir.display()
            )
        })?;

        // Write the evaluated command to disk
        let command_path = request.root.command();
        fs::write(command_path, &request.command).with_context(|| {
            format!(
                "failed to write command contents to `{path}`",
                path = command_path.display()
            )
        })?;

        // Create a file for the stdout
        let stdout_path = request.root.stdout();
        let stdout = File::create(stdout_path).with_context(|| {
            format!(
                "failed to create stdout file `{path}`",
                path = stdout_path.display()
            )
        })?;

        // Create a file for the stderr
        let stderr_path = request.root.stderr();
        let stderr = File::create(stderr_path).with_context(|| {
            format!(
                "failed to create stderr file `{path}`",
                path = stderr_path.display()
            )
        })?;

        // TODO: use the shell from configuration
        let mut command = Command::new("bash");
        command
            .current_dir(work_dir)
            .arg("-C")
            .arg(command_path)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .envs(
                request
                    .env
                    .iter()
                    .map(|(k, v)| (OsStr::new(k), OsStr::new(v))),
            )
            .kill_on_drop(true);

        // Set an environment variable on Windows to get consistent PATH searching
        // See: https://github.com/rust-lang/rust/issues/122660
        #[cfg(windows)]
        command.env("WDL_TASK_EVALUATION", "1");

        let mut child = command.spawn().context("failed to spawn `bash`")?;
        let id = child.id().expect("should have id");
        info!("spawned local `bash` process {id} for task execution");

        let status = child.wait().await.with_context(|| {
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

        let status_code = status.code().expect("process should have exited");
        info!("task process {id} has terminated with status code {status_code}");
        Ok(status_code)
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
            let memory = memory as f64 / (1024.0 * 1024.0 * 1024.0);
            let total_memory = self.memory as f64 / (1024.0 * 1024.0 * 1024.0);

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

    fn container_root_dir(&self) -> Option<&Path> {
        // Local execution does not use a container
        None
    }

    fn spawn(&self, request: TaskSpawnRequest) -> Result<oneshot::Receiver<Result<i32>>> {
        if !request.mounts.is_empty() {
            bail!("cannot spawn a local task with mount points");
        }

        let (tx, rx) = oneshot::channel();
        let cpu = cpu(&request.requirements);
        let memory = memory(&request.requirements)? as u64;

        self.tx
            .send(LocalTaskSpawnRequest {
                inner: request,
                cpu,
                memory,
                tx,
            })
            .ok();

        Ok(rx)
    }
}
