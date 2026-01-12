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
use crankshaft::engine::service::runner::backend::TaskRunError;
use crankshaft::engine::service::runner::backend::docker;
use crankshaft::engine::task::Execution;
use crankshaft::engine::task::Input;
use crankshaft::engine::task::Output;
use crankshaft::engine::task::Resources;
use crankshaft::engine::task::input::Contents;
use crankshaft::engine::task::input::Type as InputType;
use crankshaft::engine::task::output::Type as OutputType;
use futures::FutureExt;
use futures::future::BoxFuture;
use indexmap::IndexMap;
use nonempty::NonEmpty;
use tracing::debug;
use tracing::info;
use tracing::warn;
use url::Url;

use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use super::TaskExecutionResult;
use super::TaskSpawnRequest;
use crate::CancellationContext;
use crate::EvaluationPath;
use crate::Events;
use crate::ONE_GIBIBYTE;
use crate::PrimitiveValue;
use crate::Value;
use crate::backend::COMMAND_FILE_NAME;
use crate::backend::INITIAL_EXPECTED_NAMES;
use crate::backend::STDERR_FILE_NAME;
use crate::backend::STDOUT_FILE_NAME;
use crate::backend::WORK_DIR_NAME;
use crate::backend::manager::TaskManager;
use crate::config::Config;
use crate::config::DEFAULT_TASK_SHELL;
use crate::config::TaskResourceLimitBehavior;
use crate::http::Transferer;
use crate::v1::ContainerSource;
use crate::v1::DEFAULT_DISK_MOUNT_POINT;
use crate::v1::container;
use crate::v1::cpu;
use crate::v1::disks;
use crate::v1::gpu;
use crate::v1::max_cpu;
use crate::v1::max_memory;
use crate::v1::memory;

/// The guest working directory.
const GUEST_WORK_DIR: &str = "/mnt/task/work";

/// The guest path for the command file.
const GUEST_COMMAND_PATH: &str = "/mnt/task/command";

/// The path to the container's stdout.
const GUEST_STDOUT_PATH: &str = "/mnt/task/stdout";

/// The path to the container's stderr.
const GUEST_STDERR_PATH: &str = "/mnt/task/stderr";

/// Amount of CPU to request for the cleanup task.
#[cfg(unix)]
const CLEANUP_TASK_CPU: f64 = 0.1;

/// Amount of memory to request for the cleanup task, in bytes.
#[cfg(unix)]
const CLEANUP_TASK_MEMORY: u64 = 64 * 1024;

/// Represents a task that runs with a Docker container.
#[derive(Debug)]
struct DockerTask {
    /// The engine configuration.
    config: Arc<Config>,
    /// The task spawn request.
    request: TaskSpawnRequest,
    /// The underlying Crankshaft backend.
    backend: Arc<docker::Backend>,
    /// The name of the task.
    name: String,
    /// The requested maximum CPU limit for the task.
    max_cpu: Option<f64>,
    /// The requested maximum memory limit for the task, in bytes.
    max_memory: Option<u64>,
    /// The requested GPU count for the task.
    gpu: Option<u64>,
    /// The evaluation cancellation context.
    cancellation: CancellationContext,
}

impl DockerTask {
    /// Runs the docker task.
    ///
    /// Returns `Ok(None)` if the task was canceled.
    async fn run(self) -> Result<Option<TaskExecutionResult>> {
        // Create the working directory
        let work_dir = self.request.attempt_dir().join(WORK_DIR_NAME);
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
        let command_path = self.request.attempt_dir().join(COMMAND_FILE_NAME);
        fs::write(&command_path, self.request.command()).with_context(|| {
            format!(
                "failed to write command contents to `{path}`",
                path = command_path.display()
            )
        })?;

        // Allocate the inputs, which will always be, at most, the number of inputs plus
        // the working directory and command
        let mut inputs = Vec::with_capacity(self.request.inputs().len() + 2);
        for input in self.request.inputs().iter() {
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

        let stdout_path = self.request.attempt_dir().join(STDOUT_FILE_NAME);
        let stderr_path = self.request.attempt_dir().join(STDERR_FILE_NAME);

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

        let volumes = self
            .request
            .constraints()
            .disks
            .keys()
            .filter_map(|mp| {
                // NOTE: the root mount point is already handled by the work
                // directory mount, so we filter it here to avoid duplicate volume
                // mapping.
                if mp == DEFAULT_DISK_MOUNT_POINT {
                    None
                } else {
                    Some(mp.clone())
                }
            })
            .collect::<Vec<_>>();

        if !volumes.is_empty() {
            debug!(
                "disk size constraints cannot be enforced by the Docker backend; mount points \
                 will be created but sizes will not be limited"
            );
        }

        let task = Task::builder()
            .name(&self.name)
            .executions(NonEmpty::new(
                Execution::builder()
                    .image(
                        self.request
                            .constraints()
                            .container
                            .as_ref()
                            .expect("must have container")
                            .to_string(),
                    )
                    .program(
                        self.config
                            .task
                            .shell
                            .as_deref()
                            .unwrap_or(DEFAULT_TASK_SHELL),
                    )
                    .args([GUEST_COMMAND_PATH.to_string()])
                    .work_dir(GUEST_WORK_DIR)
                    .env(self.request.env().clone())
                    .stdout(GUEST_STDOUT_PATH)
                    .stderr(GUEST_STDERR_PATH)
                    .build(),
            ))
            .inputs(inputs)
            .outputs(outputs)
            .resources(
                Resources::builder()
                    .cpu(self.request.constraints().cpu)
                    .maybe_cpu_limit(self.max_cpu)
                    .ram(self.request.constraints().memory as f64 / ONE_GIBIBYTE)
                    .maybe_ram_limit(self.max_memory.map(|m| m as f64 / ONE_GIBIBYTE))
                    .maybe_gpu(self.gpu)
                    .build(),
            )
            .volumes(volumes)
            .build();

        let statuses = match self.backend.run(task, self.cancellation.second())?.await {
            Ok(statuses) => statuses,
            Err(TaskRunError::Canceled) => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        assert_eq!(statuses.len(), 1, "there should only be one exit status");
        let status = statuses.first();

        Ok(Some(TaskExecutionResult {
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
        }))
    }
}

/// Represents a cleanup task that is run upon successful completion of a Docker
/// task.
///
/// On Unix systems, this is used to recursively run `chown` on the work
/// directory so that files created by a container user (e.g. `root`) are
/// changed to be owned by the user performing evaluation.
#[cfg(unix)]
struct CleanupTask {
    /// The name of the task.
    name: String,
    /// The work directory to `chown`.
    work_dir: EvaluationPath,
    /// The underlying Crankshaft backend.
    backend: Arc<docker::Backend>,
    /// The evaluation cancellation context.
    cancellation: CancellationContext,
}

#[cfg(unix)]
impl CleanupTask {
    /// Runs the cleanup task.
    ///
    /// Returns `Ok(None)` if the task was canceled.
    async fn run(self) -> Result<Option<()>> {
        use crankshaft::engine::service::runner::backend::TaskRunError;
        use tracing::debug;

        // SAFETY: the work directory is always local for the Docker backend
        let work_dir = self.work_dir.as_local().expect("path should be local");
        assert!(work_dir.is_absolute(), "work directory should be absolute");

        let (uid, gid) = unsafe { (libc::geteuid(), libc::getegid()) };
        let ownership = format!("{uid}:{gid}");

        let task = Task::builder()
            .name(&self.name)
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
                    .cpu(CLEANUP_TASK_CPU)
                    .ram(CLEANUP_TASK_MEMORY as f64 / ONE_GIBIBYTE)
                    .build(),
            )
            .build();

        debug!(
            "running cleanup task `{name}` to change ownership of `{path}` to `{ownership}`",
            name = self.name,
            path = work_dir.display(),
        );

        match self
            .backend
            .run(task, self.cancellation.second())
            .context("failed to submit cleanup task")?
            .await
        {
            Ok(statuses) => {
                let status = statuses.first();
                if status.success() {
                    Ok(Some(()))
                } else {
                    bail!(
                        "failed to chown task work directory `{path}`",
                        path = work_dir.display()
                    );
                }
            }
            Err(TaskRunError::Canceled) => Ok(None),
            Err(e) => Err(e).context("failed to run cleanup task"),
        }
    }
}

/// Represents the Docker backend.
pub struct DockerBackend {
    /// The engine configuration.
    config: Arc<Config>,
    /// The underlying Crankshaft backend.
    inner: Arc<docker::Backend>,
    /// The evaluation cancellation context.
    cancellation: CancellationContext,
    /// The maximum CPUs for any of one node.
    max_cpu: f64,
    /// The maximum memory for any of one node.
    max_memory: u64,
    /// The task manager for the backend.
    manager: TaskManager,
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
        events: Events,
        cancellation: CancellationContext,
    ) -> Result<Self> {
        info!("initializing Docker backend");

        let names = Arc::new(Mutex::new(GeneratorIterator::new(
            UniqueAlphanumeric::default_with_expected_generations(INITIAL_EXPECTED_NAMES),
            INITIAL_EXPECTED_NAMES,
        )));

        let backend_config = config.backend()?;
        let backend_config = backend_config
            .as_docker()
            .context("configured backend is not Docker")?;

        let backend = docker::Backend::initialize_default_with(
            backend::docker::Config::builder()
                .cleanup(backend_config.cleanup)
                .build(),
            names.clone(),
            events.crankshaft().clone(),
        )
        .await
        .context("failed to initialize Docker backend")?;

        let resources = *backend.resources();
        let cpu = resources.cpu() as f64;
        let max_cpu = resources.max_cpu() as f64;
        let memory = resources.memory();
        let max_memory = resources.max_memory();

        // If a service is being used, then we're going to be spawning into a cluster
        // For the purposes of resource tracking, treat it as unlimited resources and
        // let Docker handle resource allocation
        let manager = if resources.use_service() {
            TaskManager::new_unlimited(max_cpu, max_memory)
        } else {
            TaskManager::new(
                cpu,
                max_cpu,
                memory,
                max_memory,
                events,
                cancellation.clone(),
            )
        };

        Ok(Self {
            config,
            inner: Arc::new(backend),
            cancellation,
            max_cpu,
            max_memory,
            manager,
            names,
        })
    }
}

impl TaskExecutionBackend for DockerBackend {
    fn constraints(
        &self,
        requirements: &HashMap<String, Value>,
        hints: &HashMap<String, Value>,
    ) -> Result<TaskExecutionConstraints> {
        let container: ContainerSource =
            container(requirements, self.config.task.container.as_deref());
        match &container {
            ContainerSource::Docker(_) => {}
            ContainerSource::Library(_) | ContainerSource::Oras(_) => {
                bail!(
                    "Docker backend does not support `{container:#}`; use a Docker registry image \
                     instead"
                )
            }
            ContainerSource::SifFile(_) => {
                bail!(
                    "Docker backend does not support local SIF file `{container:#}`; use a Docker \
                     registry image instead"
                )
            }
            ContainerSource::Unknown(_) => {
                bail!("Docker backend does not support unknown container source `{container:#}`")
            }
        };

        let mut cpu = cpu(requirements);
        if self.max_cpu < cpu {
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
                    cpu = self.max_cpu;
                }
                TaskResourceLimitBehavior::Deny => {
                    bail!(
                        "task requires at least {cpu} CPU{s}{env_specific}",
                        s = if cpu == 1.0 { "" } else { "s" },
                    );
                }
            }
        }

        let mut memory = memory(requirements)? as u64;
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
                    memory = self.max_memory;
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

    fn spawn(
        &self,
        request: TaskSpawnRequest,
        _transferer: Arc<dyn Transferer>,
    ) -> BoxFuture<'_, Result<Option<TaskExecutionResult>>> {
        async move {
            let cpu = request.constraints().cpu;
            let memory = request.constraints().memory;
            // NOTE: in the Docker backend, we clamp `max_cpu` and `max_memory`
            // to what is reported by the backend, as the Docker daemon does not
            // respond gracefully to over-subscribing these.
            let max_cpu = max_cpu(request.hints()).map(|m| m.min(self.max_cpu));
            let max_memory = max_memory(request.hints())?.map(|i| (i as u64).min(self.max_memory));
            let gpu = gpu(request.requirements(), request.hints());

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

            let task = DockerTask {
                config: self.config.clone(),
                request,
                backend: self.inner.clone(),
                name,
                max_cpu,
                max_memory,
                gpu,
                cancellation: self.cancellation.clone(),
            };

            match self.manager.spawn(cpu, memory, task.run()).await? {
                Some(res) => {
                    // The task completed, perform cleanup on unix platforms
                    #[cfg(unix)]
                    {
                        let name = format!(
                            "docker-chown-{id}",
                            id = self
                                .names
                                .lock()
                                .expect("generator should always acquire")
                                .next()
                                .expect("generator should never be exhausted")
                        );

                        let task = CleanupTask {
                            name,
                            work_dir: res.work_dir.clone(),
                            backend: self.inner.clone(),
                            cancellation: self.cancellation.clone(),
                        };

                        if let Err(e) = self
                            .manager
                            .spawn(CLEANUP_TASK_CPU, CLEANUP_TASK_MEMORY, task.run())
                            .await
                        {
                            tracing::error!("Docker backend cleanup failed: {e:#}");
                        }
                    }

                    Ok(Some(res))
                }
                None => Ok(None),
            }
        }
        .boxed()
    }
}
