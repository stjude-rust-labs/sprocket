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
use futures::FutureExt;
use futures::future::BoxFuture;
use nonempty::NonEmpty;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use super::TaskExecutionEvents;
use super::TaskExecutionResult;
use super::TaskManager;
use super::TaskManagerRequest;
use super::TaskSpawnRequest;
use crate::InputTrie;
use crate::ONE_GIBIBYTE;
use crate::Value;
use crate::WORK_DIR_NAME;
use crate::config::CrankshaftBackendConfig;
use crate::config::CrankshaftBackendKind;
use crate::config::DEFAULT_TASK_SHELL;
use crate::config::TaskConfig;
use crate::http::Downloader;
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

/// The guest path for the output directory.
#[cfg(unix)]
const GUEST_OUT_DIR: &str = "/workflow_output";

/// Amount of CPU to reserve for the cleanup task.
#[cfg(unix)]
const CLEANUP_CPU: f64 = 0.1;

/// Amount of memory to reserve for the cleanup task.
#[cfg(unix)]
const CLEANUP_MEMORY: f64 = 0.05;

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

    async fn run(self, spawned: oneshot::Sender<()>) -> Result<TaskExecutionResult> {
        // Create the working directory
        // TODO: this should only be done for local task execution
        let work_dir = self.inner.root.attempt_dir().join(WORK_DIR_NAME);
        fs::create_dir_all(&work_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = work_dir.display()
            )
        })?;

        // Write the evaluated command to disk
        // This is done even for remote execution so that a copy exists locally
        let command_path = self.inner.root.command();
        fs::write(command_path, self.inner.command()).with_context(|| {
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
                let (exists, is_dir) = input
                    .location()
                    .map(|p| {
                        p.metadata()
                            .map(|m| (true, m.is_dir()))
                            .unwrap_or((false, false))
                    })
                    .unwrap_or_else(|| match input.path() {
                        EvaluationPath::Local(p) => p
                            .metadata()
                            .map(|m| (true, m.is_dir()))
                            .unwrap_or((false, false)),
                        EvaluationPath::Remote(url) => (true, url.as_str().ends_with('/')),
                    });

                if exists {
                    inputs.push(Arc::new(
                        Input::builder()
                            .path(guest_path)
                            .contents(
                                input
                                    .location()
                                    .map(|l| Contents::Path(l.to_path_buf()))
                                    .unwrap_or_else(|| match input.path() {
                                        EvaluationPath::Local(path) => Contents::Path(path.clone()),
                                        EvaluationPath::Remote(url) => Contents::Url(url.clone()),
                                    }),
                            )
                            .ty(if is_dir { Type::Directory } else { Type::File })
                            .read_only(true)
                            .build(),
                    ));
                }
            }
        }

        // Add an input for the work directory
        // TODO: we should not do this for remote backends
        inputs.push(Arc::new(
            Input::builder()
                .path(GUEST_WORK_DIR)
                .contents(Contents::Path(work_dir.to_path_buf()))
                .ty(Type::Directory)
                .read_only(false)
                .build(),
        ));

        // Add an input for the command
        inputs.push(Arc::new(
            Input::builder()
                .path(GUEST_COMMAND_PATH)
                .contents(Contents::Path(command_path.to_path_buf()))
                .ty(Type::File)
                .read_only(true)
                .build(),
        ));

        // TODO: for remote backends, add an output for the working directory

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

        Ok(TaskExecutionResult {
            exit_code: output.status.code().expect("should have exit code"),
            // TODO: fix this for remote execution
            work_dir: EvaluationPath::Local(work_dir),
        })
    }
}

/// Represents the crankshaft backend.
pub struct CrankshaftBackend {
    /// The underlying Crankshaft backend.
    inner: Arc<dyn Backend>,
    /// The kind of backend to use.
    kind: CrankshaftBackendKind,
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

        let kind = config.default.clone();

        let (inner, max_concurrency, manager, max_cpu, max_memory) = match &kind {
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
            kind,
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

    fn guest_work_dir(&self) -> Option<&Path> {
        Some(Path::new(GUEST_WORK_DIR))
    }

    fn localize_inputs<'a, 'b, 'c, 'd>(
        &'a self,
        downloader: &'b dyn Downloader,
        inputs: &'c mut [crate::eval::Input],
    ) -> BoxFuture<'d, Result<()>>
    where
        'a: 'd,
        'b: 'd,
        'c: 'd,
        Self: 'd,
    {
        async {
            // Construct a trie for mapping input guest paths
            let mut trie = InputTrie::default();
            for input in inputs.iter() {
                trie.insert(input)?;
            }

            for (index, guest_path) in trie.calculate_guest_paths(GUEST_INPUTS_DIR)? {
                inputs[index].set_guest_path(guest_path);
            }

            // Localize all inputs
            // TODO: only do this for local task execution
            for input in inputs {
                // TODO: parallelize the downloads
                let location = match input.path() {
                    EvaluationPath::Local(path) => Location::Path(path.into()),
                    EvaluationPath::Remote(url) => downloader
                        .download(url)
                        .await
                        .map_err(|e| anyhow!("failed to localize `{url}`: {e:?}"))?,
                };

                input.set_location(location.into_owned());
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

        Ok(TaskExecutionEvents {
            spawned: spawned_rx,
            completed: completed_rx,
        })
    }

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
        if self.kind != CrankshaftBackendKind::Docker {
            return None;
        }

        #[cfg(unix)]
        {
            let inner_backend = self.inner.clone();
            let generator = self.generator.clone();
            let output_path = output_dir.to_path_buf();
            Some(
                async move {
                    let result = async {
                        let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
                        let ownership = format!("{uid}:{gid}");
                        info!(
                            "cleanup target: '{}', attempting to set ownership to: {}",
                            output_path.display(),
                            ownership
                        );

                        if !output_path.exists() {
                            info!("output directory does not exist, skipping cleanup");
                            return Ok(());
                        }
                        if !output_path.is_dir() {
                            bail!(
                                "output directory `{path}` is not a directory",
                                path = output_path.display()
                            );
                        }

                        let output_mount = Input::builder()
                            .path(GUEST_OUT_DIR)
                            .contents(Contents::Path(output_path.clone()))
                            .ty(Type::Directory)
                            // need write access
                            .read_only(false)
                            .build();

                        let cleanup_task_name = format!(
                            "wdl-engine-chown-cleanup-{}",
                            generator
                                .lock()
                                .expect("generator should always acquire")
                                .next()
                                .expect("generator should never be exhausted")
                        );

                        let cleanup_resources = Resources::builder()
                            .cpu(CLEANUP_CPU)
                            .ram(CLEANUP_MEMORY)
                            .build();

                        let cleanup_task = Task::builder()
                            .name(&cleanup_task_name)
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
                            .inputs([Arc::new(output_mount)])
                            .resources(cleanup_resources)
                            .build();

                        info!(
                            "running cleanup task '{}' to chown '{}' to '{}'",
                            cleanup_task_name,
                            output_path.display(),
                            ownership
                        );

                        let (spawned_tx, _) = oneshot::channel();

                        let output_rx = inner_backend
                            .run(cleanup_task, Some(spawned_tx), token)
                            .map_err(|e| anyhow!("failed to submit cleanup task: {e}"))?;

                        match output_rx.await {
                            Ok(outputs) => {
                                if outputs.is_empty() {
                                    bail!(
                                        "cleanup task '{}' did not produce any outputs",
                                        cleanup_task_name
                                    );
                                }
                                let output = outputs.first();
                                if output.status.success() {
                                    info!(
                                        "cleanup task '{}' completed successfully",
                                        cleanup_task_name
                                    );
                                    Ok(())
                                } else {
                                    let stderr = String::from_utf8_lossy(&output.stderr);
                                    tracing::error!(
                                        "failed to chown output directory: '{}'. Exit status: \
                                         '{}'. Stderr: '{}'",
                                        output_path.display(),
                                        output.status,
                                        stderr
                                    );
                                    bail!(
                                        "failed to chown output directory: '{}'",
                                        output_path.display()
                                    );
                                }
                            }
                            Err(e) => {
                                tracing::error!(
                                    "receiving result for cleanup task '{}' failed: {e}",
                                    cleanup_task_name
                                );
                                bail!(
                                    "receiving result for cleanup task '{}' failed: {e}",
                                    cleanup_task_name
                                );
                            }
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
        {
            let _ = token;
            let _ = output_dir;
            info!("cleanup task is not supported on this platform");

            None
        }
    }
}
