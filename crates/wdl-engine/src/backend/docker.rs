//! Implementation of the Docker backend.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
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
use nonempty::NonEmpty;
#[cfg(unix)]
use tokio_util::sync::CancellationToken;
use tracing::debug;
use tracing::info;
use tracing::warn;
use url::Url;

use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use super::TaskExecutionResult;
#[cfg(unix)]
use crate::CLEANUP_TASK_NAME_PREFIX;
use crate::EvaluationPath;
use crate::INITIAL_EXPECTED_NAMES;
use crate::ONE_GIBIBYTE;
use crate::Object;
use crate::PrimitiveValue;
use crate::TaskInputs;
use crate::backend::ExecuteTaskRequest;
use crate::backend::PullResults;
use crate::backend::manager::ManagedTask;
use crate::backend::manager::TaskManager;
use crate::config::Config;
use crate::config::TaskResourceLimitBehavior;
use crate::v1::DEFAULT_DISK_MOUNT_POINT;
use crate::v1::hints;
use crate::v1::requirements;
use crate::v1::requirements::ImageSource;

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
///
/// The Docker daemon requires memory values to be at least 4MiB.
#[cfg(unix)]
const CLEANUP_TASK_MEMORY: u64 = 4096 * 1024;

/// The message Docker uses when it cannot find the source of a bind mount.
const BIND_SOURCE_MISSING: &str = "bind source path does not exist: ";

/// The prefix Docker Desktop prepends to the host paths it reports.
const HOST_MOUNT_PREFIX: &str = "/host_mnt";

/// Extracts the source path of a failed bind mount from a Docker error
/// message.
///
/// Returns `None` when the message is not a complaint about a missing bind
/// mount source. Docker Desktop reports the path as the daemon sees it, so a
/// `/host_mnt` prefix is removed to recover the path as it exists on this
/// machine.
fn bind_source_path(message: &str) -> Option<&Path> {
    let (_, path) = message.rsplit_once(BIND_SOURCE_MISSING)?;
    let path = path.trim_end();
    Some(Path::new(
        path.strip_prefix(HOST_MOUNT_PREFIX)
            .filter(|stripped| stripped.starts_with('/'))
            .unwrap_or(path),
    ))
}

/// Explains a rejected bind mount whose source is present on this machine.
///
/// Docker resolves bind mounts through the daemon's view of the filesystem
/// rather than through the host's. A path that exists locally is still
/// rejected when it falls outside the directories shared with the daemon and,
/// on Docker Desktop, a directory tree that was deleted and recreated stays
/// briefly stale in that view. Both cases otherwise surface as Docker
/// claiming that a directory the engine created moments earlier does not
/// exist.
///
/// Errors of any other kind, along with those naming a path that genuinely is
/// missing, are returned unchanged.
fn explain_missing_bind_source(e: anyhow::Error) -> anyhow::Error {
    let message = format!("{e:#}");
    let Some(path) = bind_source_path(&message).filter(|path| path.exists()) else {
        return e;
    };

    let path = path.display().to_string();
    e.context(format!(
        "Docker cannot access `{path}`, but that path exists on this machine; Docker resolves \
         bind mounts through the daemon's view of the filesystem, so ensure the path is shared \
         with Docker (see Settings > Resources > File Sharing in Docker Desktop); a directory \
         that was recently deleted and recreated also stays briefly stale in that view, in which \
         case retrying shortly or writing to a different output directory will succeed"
    ))
}

/// Represents a task that runs with a Docker container.
struct DockerTask<'a> {
    /// The engine configuration.
    config: &'a Config,
    /// The task execution request.
    request: &'a ExecuteTaskRequest<'a>,
    /// The underlying Crankshaft backend.
    backend: &'a docker::Backend,
    /// The requested maximum CPU limit for the task.
    max_cpu: Option<f64>,
    /// The requested maximum memory limit for the task, in bytes.
    max_memory: Option<u64>,
    /// The requested GPU count for the task.
    gpu: Option<u64>,
}

impl ManagedTask for DockerTask<'_> {
    type Output = TaskExecutionResult;

    fn request(&self) -> &ExecuteTaskRequest<'_> {
        self.request
    }

    async fn run(self) -> Result<Option<TaskExecutionResult>> {
        // Create the working directory
        let work_dir = self.request.work_dir();
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
        let command_path = self.request.command_path();
        fs::write(&command_path, self.request.command).with_context(|| {
            format!(
                "failed to write command contents to `{path}`",
                path = command_path.display()
            )
        })?;

        // Allocate the inputs, which will always be, at most, the number of inputs plus
        // the working directory and command
        let mut inputs = Vec::with_capacity(self.request.backend_inputs.len() + 2);
        for input in self.request.backend_inputs.iter() {
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

        let stdout_path = self.request.stdout_path();
        let stderr_path = self.request.stderr_path();

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
            .constraints
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
            .name(self.request.name)
            .executions(NonEmpty::new(
                Execution::builder()
                    .images(collect_applicable_sources(
                        &self.request.constraints.sources,
                    )?)?
                    .program(&self.config.task.shell)
                    .args([GUEST_COMMAND_PATH.to_string()])
                    .work_dir(GUEST_WORK_DIR)
                    .env(self.request.env.clone())
                    .stdout(GUEST_STDOUT_PATH)
                    .stderr(GUEST_STDERR_PATH)
                    .build(),
            ))
            .inputs(inputs)
            .outputs(outputs)
            .resources(
                Resources::builder()
                    .cpu(self.request.constraints.cpu)
                    .maybe_cpu_limit(self.max_cpu)
                    .ram(self.request.constraints.memory as f64 / ONE_GIBIBYTE)
                    .maybe_ram_limit(self.max_memory.map(|m| m as f64 / ONE_GIBIBYTE))
                    .maybe_gpu(self.gpu)
                    .build(),
            )
            .volumes(volumes)
            .build();

        let results = match self
            .backend
            .run(
                task,
                self.request.events.crankshaft().cloned(),
                self.request.cancellation.second(),
            )?
            .await
        {
            Ok(results) => results,
            Err(TaskRunError::Canceled) => return Ok(None),
            Err(e) => return Err(explain_missing_bind_source(e.into())),
        };

        assert_eq!(results.len(), 1, "there should only be one exit status");
        let result = results.into_iter().next().unwrap();

        Ok(Some(TaskExecutionResult {
            image: result.image.map(ImageSource::Docker),
            exit_code: result.status.code().expect("should have exit code"),
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

/// Hands a task's work directory back to the user performing evaluation.
///
/// On Unix systems, this recursively runs `chown` in a container so that files
/// created by a container user (e.g. `root`) become owned by the user
/// performing evaluation, which is the only user that can then remove them.
///
/// This runs after a Docker task whatever the outcome of that task was.
#[cfg(unix)]
async fn chown_work_dir(backend: &docker::Backend, name: &str, work_dir: &Path) -> Result<()> {
    assert!(work_dir.is_absolute(), "work directory should be absolute");

    // SAFETY: `geteuid` and `getegid` are always safe to call and cannot fail.
    let (uid, gid) = unsafe { (libc::geteuid(), libc::getegid()) };
    let ownership = format!("{uid}:{gid}");

    let task = Task::builder()
        .name(name)
        .executions(NonEmpty::new(
            Execution::builder()
                // SAFETY: there is at least one image in the list
                .images(["alpine:latest"])?
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
        path = work_dir.display(),
    );

    // The cleanup runs on a token of its own that nothing cancels: it matters most
    // for a task that was canceled, and by then the evaluation's tokens are
    // canceled and Docker would refuse to start the container. Dropping this
    // future is the only way to stop waiting for the cleanup.
    //
    // Passing no event sender keeps the cleanup off the event stream entirely, so
    // no consumer can observe it, and none can cancel the token Crankshaft
    // publishes with `TaskCreated` after the user has canceled evaluation.
    match backend
        .run(task, None, CancellationToken::new())
        .context("failed to submit cleanup task")?
        .await
    {
        Ok(results) => {
            if results.first().status.success() {
                Ok(())
            } else {
                bail!(
                    "failed to chown task work directory `{path}`",
                    path = work_dir.display()
                );
            }
        }
        Err(TaskRunError::Canceled) => bail!("cleanup task `{name}` was canceled"),
        Err(e) => Err(e).context("failed to run cleanup task"),
    }
}

/// Collects only Docker image sources.
///
/// A warning is emitted for unsupported sources.
fn collect_applicable_sources(sources: &[ImageSource]) -> anyhow::Result<Vec<String>> {
    let mut results = PullResults::default();
    for source in sources {
        match source {
            ImageSource::Docker(s) => results.push(source.clone(), Ok(s.clone())),
            ImageSource::Library(_) | ImageSource::Oras(_) => {
                let err = anyhow!(
                    "Docker backend does not support `{source:#}`; use a Docker registry image \
                     instead"
                );
                warn!("{err:?}");
                results.push(source.clone(), Err(err));
            }
            ImageSource::SifFile(_) => {
                let err = anyhow!(
                    "Docker backend does not support local SIF file `{source:#}`; use a Docker \
                     registry image instead"
                );
                warn!("{err:?}");
                results.push(source.clone(), Err(err));
            }
            ImageSource::Unknown(_) => {
                let err = anyhow!(
                    "Docker backend does not support unknown container source `{source:#}`"
                );
                warn!("{err:?}");
                results.push(source.clone(), Err(err));
            }
        }
    }

    if results.successful_images().next().is_none() {
        return Err(anyhow!("{results}"));
    }

    Ok(results
        .successful_images()
        .map(|(_, v)| v.clone())
        .collect::<Vec<_>>())
}

/// Represents the Docker backend.
pub struct DockerBackend {
    /// The engine configuration.
    config: Arc<Config>,
    /// The underlying Crankshaft backend.
    inner: Arc<docker::Backend>,
    /// The maximum CPUs for any of one node.
    max_cpu: f64,
    /// The maximum memory for any of one node.
    max_memory: u64,
    /// The task manager for the backend.
    manager: TaskManager,
}

impl DockerBackend {
    /// Constructs a new Docker task execution backend with the given
    /// configuration.
    ///
    /// The provided configuration is expected to have already been validated.
    pub async fn new(config: Arc<Config>) -> Result<Self> {
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
            TaskManager::new(cpu, max_cpu, memory, max_memory)
        };

        Ok(Self {
            config,
            inner: Arc::new(backend),
            max_cpu,
            max_memory,
            manager,
        })
    }
}

impl TaskExecutionBackend for DockerBackend {
    fn name(&self) -> &'static str {
        "docker"
    }

    fn constraints(
        &self,
        inputs: &TaskInputs,
        requirements: &Object,
        hints: &Object,
    ) -> Result<TaskExecutionConstraints> {
        let sources = requirements::container(inputs, requirements, &self.config.task.container);

        let mut cpu = requirements::cpu(inputs, requirements);
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

        let mut memory = requirements::memory(inputs, requirements)? as u64;
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
        let gpu = requirements::gpu(inputs, requirements, hints)
            .map(|count| (0..count).map(|i| format!("nvidia-gpu-{i}")).collect())
            .unwrap_or_default();

        let disks = requirements::disks(inputs, requirements, hints)?
            .into_iter()
            .map(|(mount_point, disk)| (mount_point.to_string(), disk.size))
            .collect();

        Ok(TaskExecutionConstraints {
            sources,
            cpu,
            memory,
            gpu,
            fpga: Default::default(),
            disks,
        })
    }

    fn execute<'a>(
        &'a self,
        request: &'a ExecuteTaskRequest<'a>,
    ) -> BoxFuture<'a, Result<Option<TaskExecutionResult>>> {
        async move {
            let cpu = request.constraints.cpu;
            let memory = request.constraints.memory;
            // NOTE: in the Docker backend, we clamp `max_cpu` and `max_memory`
            // to what is reported by the backend, as the Docker daemon does not
            // respond gracefully to over-subscribing these.
            let max_cpu =
                hints::max_cpu(request.inputs, request.hints).map(|m| m.min(self.max_cpu));
            let max_memory = hints::max_memory(request.inputs, request.hints)?
                .map(|i| (i as u64).min(self.max_memory));
            let gpu = requirements::gpu(request.inputs, request.requirements, request.hints);

            let task = DockerTask {
                config: self.config.as_ref(),
                request,
                backend: self.inner.as_ref(),
                max_cpu,
                max_memory,
                gpu,
            };

            let result = self.manager.run(cpu, memory, task).await;

            // A container ordinarily runs as `root`, so on Unix the files it leaves in
            // the work directory are owned by another user and cannot be removed by the
            // user performing evaluation. Hand ownership back whatever the outcome of
            // the task was: a task that was canceled or that failed leaves the same
            // files behind as one that completed.
            //
            // This is awaited here rather than submitted to the task manager, which
            // abandons a task that has to wait for resources once evaluation has been
            // canceled.
            #[cfg(unix)]
            {
                let work_dir = request.work_dir();
                if work_dir.exists() {
                    let name = format!(
                        "{CLEANUP_TASK_NAME_PREFIX}chown-{name}",
                        name = request.name
                    );

                    if let Err(e) = chown_work_dir(self.inner.as_ref(), &name, &work_dir).await {
                        tracing::error!("Docker backend cleanup failed: {e:#}");
                    }
                }
            }

            result
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use tempfile::TempDir;

    use super::*;

    /// Builds the error Docker produces when a bind mount source is missing.
    fn bind_error(path: &str) -> anyhow::Error {
        anyhow!(
            "Docker responded with status code 400: invalid mount config for type \"bind\": bind \
             source path does not exist: {path}"
        )
    }

    #[test]
    fn unrelated_messages_have_no_bind_source() {
        assert!(bind_source_path("Docker responded with status code 500").is_none());
    }

    #[test]
    fn bind_source_is_recovered_from_the_daemon_path() {
        assert_eq!(
            bind_source_path(&bind_error("/host_mnt/tmp/work").to_string()),
            Some(Path::new("/tmp/work"))
        );
        assert_eq!(
            bind_source_path(&bind_error("/tmp/work").to_string()),
            Some(Path::new("/tmp/work"))
        );
    }

    #[test]
    fn a_host_mount_prefix_is_only_stripped_from_a_path_boundary() {
        assert_eq!(
            bind_source_path(&bind_error("/host_mnted/work").to_string()),
            Some(Path::new("/host_mnted/work"))
        );
    }

    #[test]
    fn a_genuinely_missing_source_is_not_explained() {
        let e = explain_missing_bind_source(bind_error("/does/not/exist"));
        assert!(!format!("{e:#}").contains("exists on this machine"));
    }

    #[test]
    fn a_present_source_is_explained() {
        // SAFETY: creating a temporary directory only fails if the system has no
        // usable temporary directory, which would fail the test suite as a whole.
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_str().expect("path should be UTF-8");

        let e = explain_missing_bind_source(bind_error(path));
        let message = format!("{e:#}");
        assert!(message.contains(&format!("Docker cannot access `{path}`")));
        assert!(message.contains("File Sharing"));
        // The underlying Docker error is preserved for diagnosis.
        assert!(message.contains("status code 400"));
    }

    /// Cancels a task while its container is running and confirms that the
    /// ownership cleanup still ran, and that it stayed off the event stream.
    ///
    /// Without the cleanup, the container's files remain owned by `root` and
    /// the user performing evaluation cannot delete them.
    ///
    /// The ownership assertion only distinguishes a missing cleanup where the
    /// host sees the ownership the container wrote, which is the case on Linux.
    /// Docker Desktop remaps a bind mount to the calling user, so on macOS this
    /// test checks the cancellation result and the absence of cleanup events
    /// but cannot observe whether ownership was handed back.
    #[cfg(all(unix, not(docker_tests_disabled)))]
    #[tokio::test]
    async fn cleanup_runs_after_a_cancellation() {
        use std::os::unix::fs::MetadataExt;
        use std::time::Duration;

        use crankshaft::events::Event as CrankshaftEvent;
        use indexmap::IndexMap;

        use crate::CancellationContext;
        use crate::Engine;
        use crate::Events;
        use crate::config::FailureMode;

        let root = TempDir::new().unwrap();
        let attempt_dir = root.path().join("attempts").join("0");
        let temp_dir = root.path().join("tmp");
        fs::create_dir_all(&attempt_dir).unwrap();
        fs::create_dir_all(&temp_dir).unwrap();

        let mut config = Config::default();
        // `alpine` has no `bash`
        config.task.shell = "/bin/sh".to_string();

        let engine = Engine::new(config.clone())
            .await
            .expect("engine should initialize");
        let backend = DockerBackend::new(Arc::new(config))
            .await
            .expect("Docker backend should initialize");

        let events = Events::new(1024);
        let mut receiver = events
            .subscribe_crankshaft()
            .expect("Crankshaft events should be enabled");
        let cancellation = CancellationContext::new(FailureMode::Fast);

        let env = IndexMap::new();
        let inputs = TaskInputs::default();
        let requirements = Object::empty();
        let hints = Object::empty();
        let base_dir = EvaluationPath::from_local_path(root.path().into());
        let constraints = TaskExecutionConstraints {
            sources: vec!["docker://alpine:latest".parse().unwrap()],
            cpu: 1.0,
            memory: ONE_GIBIBYTE as u64,
            gpu: Default::default(),
            fpga: Default::default(),
            disks: Default::default(),
        };

        let request = ExecuteTaskRequest {
            engine: &engine,
            name: "cleanup-after-cancellation-0",
            command: "mkdir -p testdir && echo hello > testdir/hello.txt && sleep 60",
            inputs: &inputs,
            backend_inputs: &[],
            requirements: &requirements,
            hints: &hints,
            env: &env,
            constraints: &constraints,
            base_dir: &base_dir,
            attempt_dir: &attempt_dir,
            temp_dir: &temp_dir,
            events: &events,
            cancellation: &cancellation,
        };

        let work_dir = request.work_dir();
        // The container creates this before it sleeps, so its presence means there is
        // something for the cleanup to hand back
        let marker = work_dir.join("testdir").join("hello.txt");

        let mut execute = std::pin::pin!(backend.execute(&request));
        let result = tokio::time::timeout(Duration::from_secs(300), async {
            loop {
                tokio::select! {
                    result = &mut execute => break result,
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        if marker.exists() {
                            let _ = cancellation.cancel();
                        }
                    }
                }
            }
        })
        .await
        .expect("the task should be canceled well within the timeout")
        .expect("execution should not fail");

        assert!(result.is_none(), "the task should have been canceled");
        assert!(
            marker.exists(),
            "the container should have created its file before the cancellation"
        );

        // SAFETY: `geteuid` and `getegid` are always safe to call and cannot fail.
        let (uid, gid) = unsafe { (libc::geteuid(), libc::getegid()) };
        let mut dirs = vec![work_dir];
        while let Some(dir) = dirs.pop() {
            for entry in fs::read_dir(&dir).expect("work directory should be readable") {
                let path = entry.expect("directory entry should be readable").path();
                let metadata = path
                    .symlink_metadata()
                    .expect("entry metadata should be readable");

                assert_eq!(
                    (metadata.uid(), metadata.gid()),
                    (uid, gid),
                    "`{path}` should be owned by the user performing evaluation",
                    path = path.display()
                );

                if metadata.is_dir() {
                    dirs.push(path);
                }
            }
        }

        // The cleanup is the backend's own bookkeeping, so it emits no events at all:
        // every event on the stream belongs to the task itself. Requiring the task's
        // own events keeps this from passing on a stream that carried nothing.
        let mut task_id = None;
        while let Ok(event) = receiver.try_recv() {
            let id = match &event {
                CrankshaftEvent::TaskCreated { id, name, .. } => {
                    assert!(
                        !name.starts_with(CLEANUP_TASK_NAME_PREFIX),
                        "cleanup task `{name}` should not appear on the event stream"
                    );

                    assert_eq!(name, request.name, "an unexpected task was reported");
                    task_id = Some(*id);
                    *id
                }
                CrankshaftEvent::TaskStarted { id }
                | CrankshaftEvent::TaskContainerCreated { id, .. }
                | CrankshaftEvent::TaskContainerExited { id, .. }
                | CrankshaftEvent::TaskCompleted { id, .. }
                | CrankshaftEvent::TaskFailed { id, .. }
                | CrankshaftEvent::TaskCanceled { id }
                | CrankshaftEvent::TaskPreempted { id }
                | CrankshaftEvent::TaskStdout { id, .. }
                | CrankshaftEvent::TaskStderr { id, .. }
                | CrankshaftEvent::ImagePullStarted { id, .. }
                | CrankshaftEvent::ImagePullFailed { id, .. }
                | CrankshaftEvent::ImagePullFinished { id, .. } => *id,
            };

            assert_eq!(
                Some(id),
                task_id,
                "an event was reported for a task other than `{name}`: {event:?}",
                name = request.name
            );
        }

        assert!(
            task_id.is_some(),
            "the task itself should have been reported on the event stream"
        );
    }
}
