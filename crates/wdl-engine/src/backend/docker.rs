//! Implementation of the Docker backend.

use std::collections::HashMap;
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
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::info;
use url::Url;

use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use super::TaskExecutionEvents;
use super::TaskExecutionResult;
use super::TaskManager;
use super::TaskManagerRequest;
use super::TaskSpawnRequest;
use crate::COMMAND_FILE_NAME;
use crate::InputTrie;
use crate::ONE_GIBIBYTE;
use crate::PrimitiveValue;
use crate::STDERR_FILE_NAME;
use crate::STDOUT_FILE_NAME;
use crate::Value;
use crate::WORK_DIR_NAME;
use crate::config::DEFAULT_TASK_SHELL;
use crate::config::DockerBackendConfig;
use crate::config::TaskConfig;
use crate::http::Downloader;
use crate::http::HttpDownloader;
use crate::http::Location;
use crate::path::EvaluationPath;
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

/// The root guest path for inputs.
const GUEST_INPUTS_DIR: &str = "/mnt/inputs";

/// The guest working directory.
const GUEST_WORK_DIR: &str = "/mnt/work";

/// The guest path for the command file.
const GUEST_COMMAND_PATH: &str = "/mnt/command";

/// The path to the container's stdout.
const GUEST_STDOUT_PATH: &str = "/stdout";

/// The path to the container's stderr.
const GUEST_STDERR_PATH: &str = "/stderr";

/// This request contains the requested cpu and memory reservations for the task
/// as well as the result receiver channel.
#[derive(Debug)]
struct DockerTaskRequest {
    /// The inner task spawn request.
    inner: TaskSpawnRequest,
    /// The underlying Crankshaft backend.
    backend: Arc<docker::Backend>,
    /// The name of the task.
    name: String,
    /// The optional shell to use.
    shell: Arc<Option<String>>,
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
            if let Some(guest_path) = input.guest_path() {
                let location = input.location().expect("all inputs should have localized");

                if location.exists() {
                    inputs.push(
                        Input::builder()
                            .path(guest_path)
                            .contents(Contents::Path(location.into()))
                            .ty(input.kind())
                            .read_only(true)
                            .build(),
                    );
                }
            }
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
                    .image(&self.container)
                    .program(self.shell.as_deref().unwrap_or(DEFAULT_TASK_SHELL))
                    .args(["-C".to_string(), GUEST_COMMAND_PATH.to_string()])
                    .work_dir(GUEST_WORK_DIR)
                    .env({
                        let mut final_env = indexmap::IndexMap::new();
                        for (k, v) in self.inner.env() {
                            let guest_path = self
                                .inner
                                .inputs()
                                .iter()
                                .find(|input| input.path().to_str() == Some(v))
                                .and_then(|input| input.guest_path());

                            final_env.insert(k.clone(), guest_path.unwrap_or(v).to_string());
                        }
                        final_env
                    })
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
                    .build(),
            )
            .build();

        let statuses = self
            .backend
            .run(task, Some(spawned), self.token.clone())
            .map_err(|e| anyhow!("{e:#}"))?
            .await
            .map_err(|e| anyhow!("{e:#}"))?;

        assert_eq!(statuses.len(), 1, "there should only be one exit status");
        let status = statuses.first();

        Ok(TaskExecutionResult {
            inputs: self.inner.info.inputs,
            exit_code: status.code().expect("should have exit code"),
            work_dir: EvaluationPath::Local(work_dir),
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
    /// The underlying Crankshaft backend.
    inner: Arc<docker::Backend>,
    /// The shell to use.
    shell: Arc<Option<String>>,
    /// The default container to use.
    container: Option<String>,
    /// The maximum amount of concurrency supported.
    max_concurrency: u64,
    /// The maximum CPUs for any of one node.
    max_cpu: u64,
    /// The maximum memory for any of one node.
    max_memory: u64,
    /// The task manager for the backend.
    manager: TaskManager<DockerTaskRequest>,
    /// The name generator for tasks.
    generator: Arc<Mutex<GeneratorIterator<UniqueAlphanumeric>>>,
}

impl DockerBackend {
    /// Constructs a new Docker task execution backend with the given
    /// configuration.
    pub async fn new(task: &TaskConfig, config: &DockerBackendConfig) -> Result<Self> {
        task.validate()?;
        config.validate()?;

        info!("initializing Docker backend");

        let backend = docker::Backend::initialize_default_with(
            backend::docker::Config::builder()
                .cleanup(config.cleanup)
                .build(),
        )
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

        Ok(Self {
            inner: Arc::new(backend),
            shell: Arc::new(task.shell.clone()),
            container: task.shell.clone(),
            max_concurrency: cpu,
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

impl TaskExecutionBackend for DockerBackend {
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

    fn guest_work_dir(&self) -> Option<&Path> {
        Some(Path::new(GUEST_WORK_DIR))
    }

    fn localize_inputs<'a, 'b, 'c, 'd>(
        &'a self,
        downloader: &'b HttpDownloader,
        inputs: &'c mut [crate::eval::Input],
    ) -> BoxFuture<'d, Result<()>>
    where
        'a: 'd,
        'b: 'd,
        'c: 'd,
        Self: 'd,
    {
        async move {
            // Construct a trie for mapping input guest paths
            let mut trie = InputTrie::default();
            for input in inputs.iter() {
                trie.insert(input)?;
            }

            for (index, guest_path) in trie.calculate_guest_paths(GUEST_INPUTS_DIR)? {
                if let Some(input) = inputs.get_mut(index) {
                    input.set_guest_path(guest_path);
                } else {
                    bail!("invalid index {} returned from trie", index);
                }
            }

            // Localize all inputs
            let mut downloads = JoinSet::new();
            for (idx, input) in inputs.iter_mut().enumerate() {
                match input.path() {
                    EvaluationPath::Local(path) => {
                        input.set_location(Location::Path(path.clone().into()));
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
                        inputs
                            .get_mut(idx)
                            .expect("index from should be valid")
                            .set_location(location);
                    }
                    Ok(Err(e)) => {
                        // Futures are aborted when the `JoinSet` is dropped.
                        bail!(e)
                    }
                    Err(e) => {
                        // Futures are aborted when the `JoinSet` is dropped.
                        bail!("download task failed: {e:?}")
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
        let hints = request.hints();

        let container = container(requirements, self.container.as_deref()).into_owned();
        let cpu = cpu(requirements);
        let memory = memory(requirements)? as u64;
        let max_cpu = max_cpu(hints);
        let max_memory = max_memory(hints)?.map(|i| i as u64);

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
            DockerTaskRequest {
                inner: request,
                shell: self.shell.clone(),
                backend: self.inner.clone(),
                name,
                container,
                cpu,
                memory,
                max_cpu,
                max_memory,
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

    #[cfg(unix)]
    fn cleanup<'a, 'b, 'c>(
        &'a self,
        output_dir: &'b Path,
        token: CancellationToken,
    ) -> Option<BoxFuture<'c, ()>>
    where
        'a: 'c,
        'b: 'c,
        Self: 'c,
    {
        /// The guest path for the output directory.
        const GUEST_OUT_DIR: &str = "/workflow_output";

        /// Amount of CPU to reserve for the cleanup task.
        const CLEANUP_CPU: f64 = 0.1;

        /// Amount of memory to reserve for the cleanup task.
        const CLEANUP_MEMORY: f64 = 0.05;

        let backend = self.inner.clone();
        let generator = self.generator.clone();
        let output_path = std::path::absolute(output_dir).expect("failed to get absolute path");
        if !output_path.is_dir() {
            info!("output directory does not exist: skipping cleanup");
            return None;
        }

        Some(
            async move {
                let result = async {
                    let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
                    let ownership = format!("{uid}:{gid}");
                    let output_mount = Input::builder()
                        .path(GUEST_OUT_DIR)
                        .contents(Contents::Path(output_path.clone()))
                        .ty(InputType::Directory)
                        // need write access
                        .read_only(false)
                        .build();

                    let name = format!(
                        "docker-backend-cleanup-{id}",
                        id = generator
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
                                    GUEST_OUT_DIR.to_string(),
                                ])
                                .work_dir("/")
                                .build(),
                        ))
                        .inputs([output_mount])
                        .resources(
                            Resources::builder()
                                .cpu(CLEANUP_CPU)
                                .ram(CLEANUP_MEMORY)
                                .build(),
                        )
                        .build();

                    info!(
                        "running cleanup task `{name}` to change ownership of `{path}` to \
                         `{ownership}`",
                        path = output_path.display(),
                    );

                    let (spawned_tx, _) = oneshot::channel();
                    let output_rx = backend
                        .run(task, Some(spawned_tx), token)
                        .map_err(|e| anyhow!("failed to submit cleanup task: {e}"))?;

                    let statuses = output_rx
                        .await
                        .map_err(|e| anyhow!("failed to run cleanup task: {e}"))?;
                    let status = statuses.first();
                    if status.success() {
                        Ok(())
                    } else {
                        bail!(
                            "failed to chown output directory `{path}`",
                            path = output_path.display()
                        );
                    }
                }
                .await;

                if let Err(e) = result {
                    tracing::error!("cleanup task failed: {e:#}");
                }
            }
            .boxed(),
        )
    }

    #[cfg(not(unix))]
    fn cleanup<'a, 'b, 'c>(&'a self, _: &'b Path, _: CancellationToken) -> Option<BoxFuture<'c, ()>>
    where
        'a: 'c,
        'b: 'c,
        Self: 'c,
    {
        tracing::debug!("cleanup task is not supported on this platform");
        None
    }
}
