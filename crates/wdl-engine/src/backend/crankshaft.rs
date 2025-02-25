//! Implementation of the crankshaft backend.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use crankshaft::engine::Task;
use crankshaft::engine::service::name::GeneratorIterator;
use crankshaft::engine::service::name::UniqueAlphanumeric;
use crankshaft::engine::service::runner::Backend;
use crankshaft::engine::service::runner::backend::docker;
use crankshaft::engine::task::Execution;
use crankshaft::engine::task::Input;
use crankshaft::engine::task::Resources;
use crankshaft::engine::task::input::Contents;
use crankshaft::engine::task::input::Type;
use nonempty::NonEmpty;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use super::TaskManager;
use super::TaskManagerRequest;
use super::TaskSpawnRequest;
use crate::ONE_GIBIBYTE;
use crate::Value;
use crate::config::CrankshaftBackendConfig;
use crate::config::CrankshaftBackendKind;
use crate::config::DEFAULT_TASK_SHELL;
use crate::config::TaskConfig;
use crate::v1::container;
use crate::v1::cpu;
use crate::v1::max_cpu;
use crate::v1::max_memory;
use crate::v1::memory;

/// The number of initial expected task names.
///
/// This controls the initial size of the bloom filter and how many names are
/// prepopulated into the name generator.
const INITIAL_EXPECTED_NAMES: usize = 1000;

/// Represents a crankshaft task request.
///
/// This request contains the requested cpu and memory reservations for the task
/// as well as the result receiver channel.
#[derive(Debug)]
struct CrankshaftTaskRequest {
    /// The inner task spawn request.
    inner: TaskSpawnRequest,
    /// The Crankshaft backend to use.
    backend: Arc<dyn Backend>,
    /// The name of the task.
    name: String,
    /// The requested container for the task.
    container: String,
    /// The requested shell to use for the task.
    shell: Option<Arc<String>>,
    /// The requested CPU reservation for the task.
    cpu: f64,
    /// The requested memory reservation for the task, in bytes.
    memory: u64,
    /// The requested maximum CPU limit for the task.
    max_cpu: Option<f64>,
    /// The requested maximum memory limit for the task, in bytes.
    max_memory: Option<u64>,
    /// The cancellation token for the request.
    token: CancellationToken,
}

impl TaskManagerRequest for CrankshaftTaskRequest {
    fn cpu(&self) -> f64 {
        self.cpu
    }

    fn memory(&self) -> u64 {
        self.memory
    }

    async fn run(self, spawned: oneshot::Sender<()>) -> Result<i32> {
        // Create the working directory
        let work_dir = self.inner.root.work_dir();
        fs::create_dir_all(work_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = work_dir.display()
            )
        })?;

        // Write the evaluated command to disk
        let command_path = self.inner.root.command();
        fs::write(command_path, &self.inner.command).with_context(|| {
            format!(
                "failed to write command contents to `{path}`",
                path = command_path.display()
            )
        })?;

        let inputs = self
            .inner
            .mounts
            .iter()
            .filter(|m| m.host.exists())
            .map(|m| {
                Ok(Arc::new(
                    Input::builder()
                        .path(
                            m.guest
                                .as_os_str()
                                .to_str()
                                .context("task input path is not UTF-8")?,
                        )
                        .contents(Contents::Path(m.host.clone()))
                        .ty(if m.host.is_dir() {
                            Type::Directory
                        } else {
                            Type::File
                        })
                        .read_only(m.read_only)
                        .build(),
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        let task = Task::builder()
            .name(self.name)
            .executions(NonEmpty::new(
                Execution::builder()
                    .image(&self.container)
                    .program(
                        self.shell
                            .as_ref()
                            .map(|s| s.as_str())
                            .unwrap_or(DEFAULT_TASK_SHELL),
                    )
                    .args([
                        "-C".to_string(),
                        self.inner
                            .mounts
                            .guest(command_path)
                            .expect("should have guest path")
                            .as_os_str()
                            .to_str()
                            .context("command path is not UTF-8")?
                            .to_string(),
                    ])
                    .work_dir(
                        self.inner
                            .mounts
                            .guest(work_dir)
                            .expect("should have guest path")
                            .as_os_str()
                            .to_str()
                            .context("working directory path is not UTF-8")?,
                    )
                    .env(self.inner.env.as_ref().clone())
                    .build(),
            ))
            .inputs(inputs)
            .resources(
                Resources::builder()
                    .cpu(self.cpu)
                    .maybe_cpu_limit(self.max_cpu)
                    .ram(self.memory as f64 / ONE_GIBIBYTE)
                    .maybe_ram_limit(self.max_memory.map(|m| m as f64 / ONE_GIBIBYTE))
                    .build(),
            )
            .build();

        let outputs = self
            .backend
            .run(task, Some(spawned), self.token.clone())
            .map_err(|e| anyhow!("{e:#}"))?
            .await
            .map_err(|e| anyhow!("{e:#}"))?;

        assert_eq!(outputs.len(), 1, "there should only be one output");
        let output = outputs.first();

        // TODO: in the future it would be nice if Crankshaft wrote the output directly
        // to the files rather than buffering it in memory
        fs::write(self.inner.root.stdout(), &output.stdout).with_context(|| {
            format!(
                "failed to write to stdout file `{path}`",
                path = self.inner.root.stdout().display()
            )
        })?;
        fs::write(self.inner.root.stderr(), &output.stderr).with_context(|| {
            format!(
                "failed to write to stderr file `{path}`",
                path = self.inner.root.stderr().display()
            )
        })?;
        Ok(output.status.code().expect("should have exit code"))
    }
}

/// Represents the crankshaft backend.
pub struct CrankshaftBackend {
    /// The underlying Crankshaft backend.
    inner: Arc<dyn Backend>,
    /// The default container to use for tasks.
    container: Option<String>,
    /// The default shell to use for tasks.
    shell: Option<Arc<String>>,
    /// The maximum amount of concurrency supported.
    max_concurrency: u64,
    /// The maximum CPUs for any of one node.
    max_cpu: u64,
    /// The maximum memory for any of one node.
    max_memory: u64,
    /// The task manager for the backend.
    manager: TaskManager<CrankshaftTaskRequest>,
    /// The name generator for tasks.
    generator: Arc<Mutex<GeneratorIterator<UniqueAlphanumeric>>>,
}

impl CrankshaftBackend {
    /// Constructs a new crankshaft task execution backend with the given
    /// configuration.
    pub async fn new(task: &TaskConfig, config: &CrankshaftBackendConfig) -> Result<Self> {
        task.validate()?;
        config.validate()?;

        let (inner, max_concurrency, manager, max_cpu, max_memory) = match config.default {
            CrankshaftBackendKind::Docker => {
                info!("initializing Docker backend");

                let backend = docker::Backend::initialize_default_with(config.docker.clone())
                    .await
                    .map_err(|e| anyhow!("{e:#}"))
                    .context("failed to initialize Docker backend")?;

                let resources = *backend.resources();
                let cpu = resources.cpu();
                let max_cpu = resources.max_cpu();
                let memory = resources.memory();
                let max_memory = resources.max_memory();

                // If a service is being used, then we're going to be spawning into a cluster
                // For the purposes of resource tracking, treat it as unlimited resources and
                // let Docker handle resource allocation
                let manager = if resources.use_service() {
                    TaskManager::new_unlimited(max_cpu, max_memory)
                } else {
                    TaskManager::new(cpu, max_cpu, memory, max_memory)
                };

                (Arc::new(backend), cpu, manager, max_cpu, max_memory)
            }
        };

        Ok(Self {
            inner,
            container: task.container.clone(),
            shell: task.shell.clone().map(Into::into),
            max_concurrency,
            max_cpu,
            max_memory,
            manager,
            generator: Arc::new(Mutex::new(GeneratorIterator::new(
                UniqueAlphanumeric::default_with_expected_generations(INITIAL_EXPECTED_NAMES),
                INITIAL_EXPECTED_NAMES,
            ))),
        })
    }
}

impl TaskExecutionBackend for CrankshaftBackend {
    fn max_concurrency(&self) -> u64 {
        self.max_concurrency
    }

    fn constraints(
        &self,
        requirements: &HashMap<String, Value>,
        _: &HashMap<String, Value>,
    ) -> Result<TaskExecutionConstraints> {
        let container = container(requirements, self.container.as_deref());

        let cpu = cpu(requirements);
        if (self.max_cpu as f64) < cpu {
            bail!(
                "task requires at least {cpu} CPU{s}, but the execution backend has a maximum of \
                 {max_cpu}",
                s = if cpu == 1.0 { "" } else { "s" },
                max_cpu = self.max_cpu,
            );
        }

        let memory = memory(requirements)?;
        if self.max_memory < memory as u64 {
            // Display the error in GiB, as it is the most common unit for memory
            let memory = memory as f64 / ONE_GIBIBYTE;
            let max_memory = self.max_memory as f64 / ONE_GIBIBYTE;

            bail!(
                "task requires at least {memory} GiB of memory, but the execution backend has a \
                 maximum of {max_memory} GiB",
            );
        }

        Ok(TaskExecutionConstraints {
            container: Some(container.into_owned()),
            cpu,
            memory,
            gpu: Default::default(),
            fpga: Default::default(),
            disks: Default::default(),
        })
    }

    fn container_root_dir(&self) -> Option<&Path> {
        Some(Path::new("/mnt/task"))
    }

    fn spawn(
        &self,
        request: TaskSpawnRequest,
        token: CancellationToken,
    ) -> Result<(oneshot::Receiver<()>, oneshot::Receiver<Result<i32>>)> {
        let (spawned_tx, spawned_rx) = oneshot::channel();
        let (completed_tx, completed_rx) = oneshot::channel();

        let container = container(&request.requirements, self.container.as_deref()).into_owned();
        let cpu = cpu(&request.requirements);
        let memory = memory(&request.requirements)? as u64;
        let max_cpu = max_cpu(&request.hints);
        let max_memory = max_memory(&request.hints)?.map(|i| i as u64);

        let name = format!(
            "{id}-{generated}",
            id = request.id(),
            generated = self
                .generator
                .lock()
                .expect("generator should always acquire")
                .next()
                .expect("generator should never be exhausted")
        );
        self.manager.send(
            CrankshaftTaskRequest {
                inner: request,
                backend: self.inner.clone(),
                name,
                container,
                shell: self.shell.clone(),
                cpu,
                memory,
                max_cpu,
                max_memory,
                token,
            },
            spawned_tx,
            completed_tx,
        );

        Ok((spawned_rx, completed_rx))
    }
}
