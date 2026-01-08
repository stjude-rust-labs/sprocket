//! Implementation of the Docker backend.

use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use crankshaft::config::backend;
use crankshaft::engine::Task;
use crankshaft::engine::service::name::GeneratorIterator;
use crankshaft::engine::service::name::UniqueAlphanumeric;
use crankshaft::engine::service::runner::Backend;
use crankshaft::engine::service::runner::backend::docker;
use crankshaft::engine::task::Execution;
use crankshaft::engine::task::Input;
use crankshaft::engine::task::Output;
use crankshaft::engine::task::Resources;
use crankshaft::engine::task::input::Contents;
use crankshaft::engine::task::input::Type as InputType;
use crankshaft::engine::task::output::Type as OutputType;
use crankshaft::events::Event;
use indexmap::IndexMap;
use nonempty::NonEmpty;
use tokio::sync::broadcast;
use tokio::sync::oneshot;
use tokio::sync::oneshot::Receiver;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing::warn;
use url::Url;

use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use super::TaskExecutionResult;
use super::TaskManager;
use super::TaskManagerRequest;
use super::TaskSpawnRequest;
use crate::EvaluationPath;
use crate::ONE_GIBIBYTE;
use crate::PrimitiveValue;
use crate::Value;
use crate::backend::COMMAND_FILE_NAME;
use crate::backend::INITIAL_EXPECTED_NAMES;
use crate::backend::STDERR_FILE_NAME;
use crate::backend::STDOUT_FILE_NAME;
use crate::backend::WORK_DIR_NAME;
use crate::config::Config;
use crate::config::DEFAULT_TASK_SHELL;
use crate::config::DockerBackendConfig;
use crate::config::TaskResourceLimitBehavior;
use crate::v1::ContainerSource;
use crate::v1::DEFAULT_DISK_MOUNT_POINT;
use crate::v1::container;
use crate::v1::cpu;
use crate::v1::disks;
use crate::v1::gpu;
use crate::v1::max_cpu;
use crate::v1::max_memory;
use crate::v1::memory;

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

/// This request contains the requested cpu and memory reservations for the task
/// as well as the result receiver channel.
#[derive(Debug)]
struct DockerTaskRequest {
    /// The engine configuration.
    config: Arc<Config>,
    /// The inner task spawn request.
    inner: TaskSpawnRequest,
    /// The underlying Crankshaft backend.
    backend: Arc<docker::Backend>,
    /// The name of the task.
    name: String,
    /// The requested container for the task.
    container: String,
    /// The requested CPU reservation for the task.
    cpu: f64,
    /// The requested memory reservation for the task, in bytes.
    memory: u64,
    /// The requested maximum CPU limit for the task.
    max_cpu: Option<f64>,
    /// The requested maximum memory limit for the task, in bytes.
    max_memory: Option<u64>,
    /// The requested GPU count for the task.
    gpu: Option<u64>,
    /// The disk mount points for the task.
    volumes: Vec<String>,
    /// The cancellation token for the request.
    token: CancellationToken,
}

impl TaskManagerRequest for DockerTaskRequest {
    fn cpu(&self) -> f64 {
        self.cpu
    }

    fn memory(&self) -> u64 {
        self.memory
    }

    async fn run(self) -> Result<TaskExecutionResult> {
        // Create the working directory
        let work_dir = self.inner.attempt_dir().join(WORK_DIR_NAME);
        fs::create_dir_all(&work_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = work_dir.display()
            )
        })?;

        // On Unix, the work directory must be group writable in case the container uses
        // a different user/group; the Crankshaft docker backend will automatically add
        // the current user's egid to the container
        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::fs::set_permissions;
            use std::os::unix::fs::PermissionsExt;
            set_permissions(&work_dir, Permissions::from_mode(0o770)).with_context(|| {
                format!(
                    "failed to set permissions for work directory `{path}`",
                    path = work_dir.display()
                )
            })?;
        }

        // Write the evaluated command to disk
        // This is done even for remote execution so that a copy exists locally
        let command_path = self.inner.attempt_dir().join(COMMAND_FILE_NAME);
        fs::write(&command_path, self.inner.command()).with_context(|| {
            format!(
                "failed to write command contents to `{path}`",
                path = command_path.display()
            )
        })?;

        // Allocate the inputs, which will always be, at most, the number of inputs plus
        // the working directory and command
        let mut inputs = Vec::with_capacity(self.inner.inputs().len() + 2);
        for input in self.inner.inputs().iter() {
            let guest_path = input.guest_path().expect("input should have guest path");
            let local_path = input.local_path().expect("input should be localized");

            // The local path must exist for Docker to mount
            if !local_path.exists() {
                bail!(
                    "cannot mount input `{path}` as it does not exist",
                    path = local_path.display()
                );
            }

            inputs.push(
                Input::builder()
                    .path(guest_path.as_str())
                    .contents(Contents::Path(local_path.into()))
                    .ty(input.kind())
                    .read_only(true)
                    .build(),
            );
        }

        // Add an input for the work directory
        inputs.push(
            Input::builder()
                .path(GUEST_WORK_DIR)
                .contents(Contents::Path(work_dir.to_path_buf()))
                .ty(InputType::Directory)
                .read_only(false)
                .build(),
        );

        // Add an input for the command
        inputs.push(
            Input::builder()
                .path(GUEST_COMMAND_PATH)
                .contents(Contents::Path(command_path.to_path_buf()))
                .ty(InputType::File)
                .read_only(true)
                .build(),
        );

        let stdout_path = self.inner.attempt_dir().join(STDOUT_FILE_NAME);
        let stderr_path = self.inner.attempt_dir().join(STDERR_FILE_NAME);

        let outputs = vec![
            Output::builder()
                .path(GUEST_STDOUT_PATH)
                .url(Url::from_file_path(&stdout_path).expect("path should be absolute"))
                .ty(OutputType::File)
                .build(),
            Output::builder()
                .path(GUEST_STDERR_PATH)
                .url(Url::from_file_path(&stderr_path).expect("path should be absolute"))
                .ty(OutputType::File)
                .build(),
        ];

        let task = Task::builder()
            .name(self.name)
            .executions(NonEmpty::new(
                Execution::builder()
                    .image(self.container)
                    .program(
                        self.config
                            .task
                            .shell
                            .as_deref()
                            .unwrap_or(DEFAULT_TASK_SHELL),
                    )
                    .args([GUEST_COMMAND_PATH.to_string()])
                    .work_dir(GUEST_WORK_DIR)
                    .env(self.inner.env().clone())
                    .stdout(GUEST_STDOUT_PATH)
                    .stderr(GUEST_STDERR_PATH)
                    .build(),
            ))
            .inputs(inputs)
            .outputs(outputs)
            .resources(
                Resources::builder()
                    .cpu(self.cpu)
                    .maybe_cpu_limit(self.max_cpu)
                    .ram(self.memory as f64 / ONE_GIBIBYTE)
                    .maybe_ram_limit(self.max_memory.map(|m| m as f64 / ONE_GIBIBYTE))
                    .maybe_gpu(self.gpu)
                    .build(),
            )
            .volumes(self.volumes)
            .build();

        let statuses = self.backend.run(task, self.token.clone())?.await?;

        assert_eq!(statuses.len(), 1, "there should only be one exit status");
        let status = statuses.first();

        Ok(TaskExecutionResult {
            exit_code: status.code().expect("should have exit code"),
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
        })
    }
}

/// Represents the Docker backend.
pub struct DockerBackend {
    /// The engine configuration.
    config: Arc<Config>,
    /// The underlying Crankshaft backend.
    inner: Arc<docker::Backend>,
    /// The maximum amount of concurrency supported.
    max_concurrency: u64,
    /// The maximum CPUs for any of one node.
    max_cpu: u64,
    /// The maximum memory for any of one node.
    max_memory: u64,
    /// The task manager for the backend.
    manager: TaskManager<DockerTaskRequest>,
    /// The name generator for tasks.
    names: Arc<Mutex<GeneratorIterator<UniqueAlphanumeric>>>,
}

impl DockerBackend {
    /// Constructs a new Docker task execution backend with the given
    /// configuration.
    ///
    /// The provided configuration is expected to have already been validated.
    pub async fn new(
        config: Arc<Config>,
        backend_config: &DockerBackendConfig,
        events: Option<broadcast::Sender<Event>>,
    ) -> Result<Self> {
        info!("initializing Docker backend");

        let names = Arc::new(Mutex::new(GeneratorIterator::new(
            UniqueAlphanumeric::default_with_expected_generations(INITIAL_EXPECTED_NAMES),
            INITIAL_EXPECTED_NAMES,
        )));

        let backend = docker::Backend::initialize_default_with(
            backend::docker::Config::builder()
                .cleanup(backend_config.cleanup)
                .build(),
            names.clone(),
            events,
        )
        .await
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

        Ok(Self {
            config,
            inner: Arc::new(backend),
            max_concurrency: cpu,
            max_cpu,
            max_memory,
            manager,
            names,
        })
    }
}

impl TaskExecutionBackend for DockerBackend {
    fn max_concurrency(&self) -> u64 {
        self.max_concurrency
    }

    fn constraints(
        &self,
        requirements: &HashMap<String, Value>,
        hints: &HashMap<String, Value>,
    ) -> Result<TaskExecutionConstraints> {
        let container_source = container(requirements, self.config.task.container.as_deref());
        let container = match &container_source {
            ContainerSource::Docker(s) => s.clone(),
            ContainerSource::Library(_) | ContainerSource::Oras(_) => {
                bail!(
                    "Docker backend does not support `{container_source:#}`; use a Docker registry \
                     image instead"
                )
            }
            ContainerSource::SifFile(_) => {
                bail!(
                    "Docker backend does not support local SIF files; use a Docker registry image \
                     instead"
                )
            }
            ContainerSource::Unknown(_) => {
                bail!(
                    "Docker backend does not support unknown container source `{container_source:#}`"
                )
            }
        };

        let mut cpu = cpu(requirements);
        if (self.max_cpu as f64) < cpu {
            let env_specific = if self.config.suppress_env_specific_output {
                String::new()
            } else {
                format!(
                    ", but the execution backend has a maximum of {max_cpu}",
                    max_cpu = self.max_cpu,
                )
            };
            match self.config.task.cpu_limit_behavior {
                TaskResourceLimitBehavior::TryWithMax => {
                    warn!(
                        "task requires at least {cpu} CPU{s}{env_specific}",
                        s = if cpu == 1.0 { "" } else { "s" },
                    );
                    // clamp the reported constraint to what's available
                    cpu = self.max_cpu as f64;
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
        if self.max_memory < memory as u64 {
            let env_specific = if self.config.suppress_env_specific_output {
                String::new()
            } else {
                format!(
                    ", but the execution backend has a maximum of {max_memory} GiB",
                    max_memory = self.max_memory as f64 / ONE_GIBIBYTE,
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
                    memory = self.max_memory.try_into().unwrap_or(i64::MAX);
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

        // Generate GPU specification strings in the format "<type>-gpu-<index>".
        // Each string represents one allocated GPU, indexed from 0. The type prefix
        // (e.g., "nvidia", "amd", "intel") identifies the GPU vendor/driver.
        // This is the first backend to populate the gpu field; other backends should
        // follow this format for consistency.
        let gpu = gpu(requirements, hints)
            .map(|count| (0..count).map(|i| format!("nvidia-gpu-{i}")).collect())
            .unwrap_or_default();

        let disks = disks(requirements, hints)?
            .into_iter()
            .map(|(mount_point, disk)| (mount_point.to_string(), disk.size))
            .collect::<IndexMap<_, _>>();

        Ok(TaskExecutionConstraints {
            container: Some(container),
            cpu,
            memory,
            gpu,
            fpga: Default::default(),
            disks,
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
        token: CancellationToken,
    ) -> Result<Receiver<Result<TaskExecutionResult>>> {
        let (completed_tx, completed_rx) = oneshot::channel();

        let requirements = request.requirements();
        let hints = request.hints();

        let container_source = container(requirements, self.config.task.container.as_deref());
        let container = match &container_source {
            ContainerSource::Docker(s) => s.clone(),
            ContainerSource::Library(_) | ContainerSource::Oras(_) => {
                bail!(
                    "Docker backend does not support `{container_source:#}`; use a Docker registry \
                     image instead"
                )
            }
            ContainerSource::SifFile(_) => {
                bail!(
                    "Docker backend does not support local SIF files; use a Docker registry image \
                     instead"
                )
            }
            ContainerSource::Unknown(_) => {
                bail!(
                    "Docker backend does not support unknown container source `{container_source:#}`"
                )
            }
        };
        let mut cpu = cpu(requirements);
        if let TaskResourceLimitBehavior::TryWithMax = self.config.task.cpu_limit_behavior {
            cpu = std::cmp::min(cpu.ceil() as u64, self.max_cpu) as f64;
        }
        let mut memory = memory(requirements)? as u64;
        if let TaskResourceLimitBehavior::TryWithMax = self.config.task.memory_limit_behavior {
            memory = std::cmp::min(memory, self.max_memory);
        }
        // NOTE: in the Docker backend, we clamp the `max_cpu` and `max_memory`
        // to what is reported by the backend, as the Docker daemon does not
        // respond gracefully to over-subscribing these.
        let max_cpu = max_cpu(hints).map(|c| c.min(self.max_cpu as f64));
        let max_memory = max_memory(hints)?.map(|i| (i as u64).min(self.max_memory));
        let gpu = gpu(requirements, hints);
        let volumes = disks(requirements, hints)?
            .into_keys()
            // NOTE: the root mount point is already handled by the work
            // directory mount, so we filter it here to avoid duplicate volume
            // mapping.
            .filter(|mp| *mp != DEFAULT_DISK_MOUNT_POINT)
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        if !volumes.is_empty() {
            warn!(
                "disk size constraints cannot be enforced by the Docker backend; mount points \
                 will be created but sizes will not be limited"
            );
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
            DockerTaskRequest {
                config: self.config.clone(),
                inner: request,
                backend: self.inner.clone(),
                name,
                container,
                cpu,
                memory,
                max_cpu,
                max_memory,
                gpu,
                volumes,
                token,
            },
            completed_tx,
        );

        Ok(completed_rx)
    }

    #[cfg(unix)]
    fn cleanup<'a>(
        &'a self,
        work_dir: &'a EvaluationPath,
        token: CancellationToken,
    ) -> Option<futures::future::BoxFuture<'a, ()>> {
        use futures::FutureExt;
        use tracing::debug;

        /// The guest path for the work directory.
        const GUEST_WORK_DIR: &str = "/mnt/work";
        /// Amount of CPU to reserve for the cleanup task.
        const CLEANUP_CPU: f64 = 0.1;
        /// Amount of memory to reserve for the cleanup task.
        const CLEANUP_MEMORY: f64 = 0.05;

        // SAFETY: the work directory is always local for the Docker backend
        let work_dir = work_dir.as_local().expect("path should be local");
        assert!(work_dir.is_absolute(), "work directory should be absolute");

        let backend = self.inner.clone();
        let names = self.names.clone();

        Some(
            async move {
                let result = async {
                    let (uid, gid) = unsafe { (libc::geteuid(), libc::getegid()) };
                    let ownership = format!("{uid}:{gid}");

                    let name = format!(
                        "docker-backend-cleanup-{id}",
                        id = names
                            .lock()
                            .expect("generator should always acquire")
                            .next()
                            .expect("generator should never be exhausted")
                    );

                    let task = Task::builder()
                        .name(&name)
                        .executions(NonEmpty::new(
                            Execution::builder()
                                .image("alpine:latest")
                                .program("chown")
                                .args([
                                    "-R".to_string(),
                                    ownership.clone(),
                                    GUEST_WORK_DIR.to_string(),
                                ])
                                .build(),
                        ))
                        .inputs([Input::builder()
                            .path(GUEST_WORK_DIR)
                            .contents(Contents::Path(work_dir.to_path_buf()))
                            .ty(InputType::Directory)
                            // need write access to chown
                            .read_only(false)
                            .build()])
                        .resources(
                            Resources::builder()
                                .cpu(CLEANUP_CPU)
                                .ram(CLEANUP_MEMORY)
                                .build(),
                        )
                        .build();

                    debug!(
                        "running cleanup task `{name}` to change ownership of `{path}` to \
                         `{ownership}`",
                        path = work_dir.display(),
                    );

                    let statuses = backend
                        .run(task, token)
                        .context("failed to submit cleanup task")?
                        .await
                        .context("failed to run cleanup task")?;
                    let status = statuses.first();
                    if status.success() {
                        Ok(())
                    } else {
                        bail!(
                            "failed to chown task work directory `{path}`",
                            path = work_dir.display()
                        );
                    }
                }
                .await;

                if let Err(e) = result {
                    tracing::error!("Docker backend cleanup failed: {e:#}");
                }
            }
            .boxed(),
        )
    }
}
