//! Implementation of the TES backend.

use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use cloud_copy::UrlExt;
use crankshaft::config::backend;
use crankshaft::config::backend::tes::http::HttpAuthConfig;
use crankshaft::engine::Task;
use crankshaft::engine::service::name::GeneratorIterator;
use crankshaft::engine::service::name::UniqueAlphanumeric;
use crankshaft::engine::service::runner::Backend;
use crankshaft::engine::service::runner::backend::TaskRunError;
use crankshaft::engine::service::runner::backend::tes;
use crankshaft::engine::task::Execution;
use crankshaft::engine::task::Input;
use crankshaft::engine::task::Output;
use crankshaft::engine::task::Resources;
use crankshaft::engine::task::input::Contents;
use crankshaft::engine::task::input::Type as InputType;
use crankshaft::engine::task::output::Type as OutputType;
use crankshaft::events::Event;
use nonempty::NonEmpty;
use secrecy::ExposeSecret;
use tokio::sync::broadcast;
use tokio::sync::oneshot;
use tokio::sync::oneshot::Receiver;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::info;
use wdl_ast::v1::TASK_REQUIREMENT_DISKS;

use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use super::TaskExecutionResult;
use super::TaskManager;
use super::TaskManagerRequest;
use super::TaskSpawnRequest;
use crate::ONE_GIBIBYTE;
use crate::PrimitiveValue;
use crate::Value;
use crate::backend::COMMAND_FILE_NAME;
use crate::backend::INITIAL_EXPECTED_NAMES;
use crate::backend::STDERR_FILE_NAME;
use crate::backend::STDOUT_FILE_NAME;
use crate::backend::WORK_DIR_NAME;
use crate::config::Config;
use crate::config::ContentDigestMode;
use crate::config::DEFAULT_TASK_SHELL;
use crate::config::TesBackendAuthConfig;
use crate::config::TesBackendConfig;
use crate::digest::UrlDigestExt;
use crate::digest::calculate_local_digest;
use crate::path::EvaluationPath;
use crate::v1::DEFAULT_TASK_REQUIREMENT_DISKS;
use crate::v1::container;
use crate::v1::cpu;
use crate::v1::disks;
use crate::v1::max_cpu;
use crate::v1::max_memory;
use crate::v1::memory;
use crate::v1::preemptible;

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

/// The default poll interval, in seconds, for the TES backend.
const DEFAULT_TES_INTERVAL: u64 = 1;

/// Represents a TES task request.
///
/// This request contains the requested cpu and memory reservations for the task
/// as well as the result receiver channel.
#[derive(Debug)]
struct TesTaskRequest {
    /// The engine configuration.
    config: Arc<Config>,
    /// The backend configuration.
    backend_config: Arc<TesBackendConfig>,
    /// The inner task spawn request.
    inner: TaskSpawnRequest,
    /// The Crankshaft TES backend to use.
    backend: Arc<tes::Backend>,
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
    /// The number of preemptible task retries to do before using a
    /// non-preemptible task.
    ///
    /// If this value is 0, no preemptible tasks are requested from the TES
    /// server.
    preemptible: i64,
    /// The cancellation token for the request.
    token: CancellationToken,
}

impl TesTaskRequest {
    /// Gets the TES disk resource for the request.
    fn disk_resource(&self) -> Result<f64> {
        let disks = disks(self.inner.requirements(), self.inner.hints())?;
        if disks.len() > 1 {
            bail!(
                "TES backend does not support more than one disk specification for the \
                 `{TASK_REQUIREMENT_DISKS}` task requirement"
            );
        }

        if let Some(mount_point) = disks.keys().next()
            && *mount_point != "/"
        {
            bail!(
                "TES backend does not support a disk mount point other than `/` for the \
                 `{TASK_REQUIREMENT_DISKS}` task requirement"
            );
        }

        Ok(disks
            .values()
            .next()
            .map(|d| d.size as f64)
            .unwrap_or(DEFAULT_TASK_REQUIREMENT_DISKS))
    }
}

impl TaskManagerRequest for TesTaskRequest {
    fn cpu(&self) -> f64 {
        self.cpu
    }

    fn memory(&self) -> u64 {
        self.memory
    }

    async fn run(self) -> Result<TaskExecutionResult> {
        // Create the attempt directory
        let attempt_dir = self.inner.attempt_dir();
        fs::create_dir_all(attempt_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = attempt_dir.display()
            )
        })?;

        // Write the evaluated command to disk
        // This is done even for remote execution so that a copy exists locally
        let command_path = attempt_dir.join(COMMAND_FILE_NAME);
        fs::write(&command_path, self.inner.command()).with_context(|| {
            format!(
                "failed to write command contents to `{path}`",
                path = command_path.display()
            )
        })?;

        // SAFETY: currently `inputs` is required by configuration validation, so it
        // should always unwrap
        let inputs_url = Arc::new(
            self.backend_config
                .inputs
                .clone()
                .expect("should have inputs URL"),
        );

        // Start with the command file as an input
        let mut inputs = vec![
            Input::builder()
                .path(GUEST_COMMAND_PATH)
                .contents(Contents::Path(command_path.to_path_buf()))
                .ty(InputType::File)
                .read_only(true)
                .build(),
        ];

        // Spawn upload tasks for inputs available locally, and apply authentication to
        // the URLs for remote inputs.
        let mut uploads = JoinSet::new();
        for (i, input) in self.inner.inputs().iter().enumerate() {
            match input.path() {
                EvaluationPath::Local(path) => {
                    // Input is local, spawn an upload of it
                    let kind = input.kind();
                    let path = path.to_path_buf();
                    let transferer = self.inner.transferer().clone();
                    let inputs_url = inputs_url.clone();
                    uploads.spawn(async move {
                        let url = inputs_url.join_digest(
                            calculate_local_digest(&path, kind, ContentDigestMode::Strong)
                                .await
                                .with_context(|| {
                                    format!(
                                        "failed to calculate digest of `{path}`",
                                        path = path.display()
                                    )
                                })?,
                        );
                        transferer
                            .upload(&path, &url)
                            .await
                            .with_context(|| {
                                format!(
                                    "failed to upload `{path}` to `{url}`",
                                    path = path.display(),
                                    url = url.display()
                                )
                            })
                            .map(|_| (i, url))
                    });
                }
                EvaluationPath::Remote(url) => {
                    // Input is already remote, add it to the Crankshaft inputs list
                    inputs.push(
                        Input::builder()
                            .path(
                                input
                                    .guest_path()
                                    .expect("input should have guest path")
                                    .as_str(),
                            )
                            .contents(Contents::Url(url.clone()))
                            .ty(input.kind())
                            .read_only(true)
                            .build(),
                    );
                }
            }
        }

        // Wait for any uploads to complete
        while let Some(result) = uploads.join_next().await {
            let (i, url) = result.context("upload task")??;
            let input = &self.inner.inputs()[i];
            inputs.push(
                Input::builder()
                    .path(
                        input
                            .guest_path()
                            .expect("input should have guest path")
                            .as_str(),
                    )
                    .contents(Contents::Url(url))
                    .ty(input.kind())
                    .read_only(true)
                    .build(),
            );
        }

        let output_dir = format!(
            "{name}-{timestamp}/",
            name = self.name,
            timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S")
        );

        // SAFETY: currently `outputs` is required by configuration validation, so it
        // should always unwrap
        let outputs_url = self
            .backend_config
            .outputs
            .as_ref()
            .expect("should have outputs URL")
            .join(&output_dir)
            .expect("should join");

        let mut work_dir_url = outputs_url.join(WORK_DIR_NAME).expect("should join");
        let stdout_url = outputs_url.join(STDOUT_FILE_NAME).expect("should join");
        let stderr_url = outputs_url.join(STDERR_FILE_NAME).expect("should join");

        // The TES backend will output three things: the working directory contents,
        // stdout, and stderr.
        let outputs = vec![
            Output::builder()
                .path(GUEST_WORK_DIR)
                .url(work_dir_url.clone())
                .ty(OutputType::Directory)
                .build(),
            Output::builder()
                .path(GUEST_STDOUT_PATH)
                .url(stdout_url.clone())
                .ty(OutputType::File)
                .build(),
            Output::builder()
                .path(GUEST_STDERR_PATH)
                .url(stderr_url.clone())
                .ty(OutputType::File)
                .build(),
        ];

        let mut preemptible = self.preemptible;
        loop {
            let task = Task::builder()
                .name(&self.name)
                .executions(NonEmpty::new(
                    Execution::builder()
                        .image(&self.container)
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
                .inputs(inputs.clone())
                .outputs(outputs.clone())
                .resources(
                    Resources::builder()
                        .cpu(self.cpu)
                        .maybe_cpu_limit(self.max_cpu)
                        .ram(self.memory as f64 / ONE_GIBIBYTE)
                        .disk(self.disk_resource()?)
                        .maybe_ram_limit(self.max_memory.map(|m| m as f64 / ONE_GIBIBYTE))
                        .preemptible(preemptible > 0)
                        .build(),
                )
                .build();

            let statuses = match self.backend.run(task, self.token.clone())?.await {
                Ok(statuses) => statuses,
                Err(TaskRunError::Preempted) if preemptible > 0 => {
                    // Decrement the preemptible count and retry
                    preemptible -= 1;
                    continue;
                }
                Err(e) => {
                    return Err(e.into());
                }
            };

            assert_eq!(statuses.len(), 1, "there should only be one output");
            let status = statuses.first();

            // Push an empty path segment so that future joins of the work directory URL
            // treat it as a directory
            work_dir_url.path_segments_mut().unwrap().push("");

            return Ok(TaskExecutionResult {
                exit_code: status.code().expect("should have exit code"),
                work_dir: EvaluationPath::Remote(work_dir_url),
                stdout: PrimitiveValue::new_file(stdout_url).into(),
                stderr: PrimitiveValue::new_file(stderr_url).into(),
                attempt_dir: Some(self.inner.attempt_dir_host().to_path_buf()),
            });
        }
    }
}

/// Represents the Task Execution Service (TES) backend.
pub struct TesBackend {
    /// The engine configuration.
    config: Arc<Config>,
    /// The backend configuration.
    backend_config: Arc<TesBackendConfig>,
    /// The underlying Crankshaft backend.
    inner: Arc<tes::Backend>,
    /// The maximum CPUs for any of one node.
    max_cpu: u64,
    /// The maximum memory for any of one node.
    max_memory: u64,
    /// The task manager for the backend.
    manager: TaskManager<TesTaskRequest>,
    /// The name generator for tasks.
    names: Arc<Mutex<GeneratorIterator<UniqueAlphanumeric>>>,
}

impl TesBackend {
    /// Constructs a new TES task execution backend with the given
    /// configuration.
    ///
    /// The provided configuration is expected to have already been validated.
    pub async fn new(
        config: Arc<Config>,
        backend_config: &TesBackendConfig,
        events: Option<broadcast::Sender<Event>>,
    ) -> Result<Self> {
        info!("initializing TES backend");

        // There's no way to ask the TES service for its limits, so use the maximums
        // allowed
        let max_cpu = u64::MAX;
        let max_memory = u64::MAX;
        let manager = TaskManager::new_unlimited(max_cpu, max_memory);

        let mut http = backend::tes::http::Config::default();
        match &backend_config.auth {
            Some(TesBackendAuthConfig::Basic(config)) => {
                http.auth = Some(HttpAuthConfig::Basic {
                    username: config.username.clone(),
                    password: config.password.inner().expose_secret().to_string(),
                });
            }
            Some(TesBackendAuthConfig::Bearer(config)) => {
                http.auth = Some(HttpAuthConfig::Bearer {
                    token: config.token.inner().expose_secret().to_string(),
                });
            }
            None => {}
        }

        http.retries = backend_config.retries;
        http.max_concurrency = backend_config.max_concurrency.map(|c| c as usize);

        let names = Arc::new(Mutex::new(GeneratorIterator::new(
            UniqueAlphanumeric::default_with_expected_generations(INITIAL_EXPECTED_NAMES),
            INITIAL_EXPECTED_NAMES,
        )));

        let backend = tes::Backend::initialize(
            backend::tes::Config::builder()
                .url(backend_config.url.clone().expect("should have URL"))
                .http(http)
                .interval(backend_config.interval.unwrap_or(DEFAULT_TES_INTERVAL))
                .build(),
            names.clone(),
            events,
        )
        .await;

        Ok(Self {
            config,
            backend_config: Arc::new(backend_config.clone()),
            inner: Arc::new(backend),
            max_cpu,
            max_memory,
            manager,
            names,
        })
    }
}

impl TaskExecutionBackend for TesBackend {
    fn max_concurrency(&self) -> u64 {
        // The TES backend doesn't limit the number of tasks that can be queued at a
        // time
        u64::MAX
    }

    fn constraints(
        &self,
        requirements: &HashMap<String, Value>,
        hints: &HashMap<String, Value>,
    ) -> Result<TaskExecutionConstraints> {
        let container = container(requirements, self.config.task.container.as_deref());

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

        // TODO: only parse the disks requirement once
        let disks = disks(requirements, hints)?
            .into_iter()
            .map(|(mp, disk)| (mp.to_string(), disk.size))
            .collect();

        Ok(TaskExecutionConstraints {
            container: Some(container.into_owned()),
            cpu,
            memory,
            gpu: Default::default(),
            fpga: Default::default(),
            disks,
        })
    }

    fn guest_inputs_dir(&self) -> Option<&'static str> {
        Some(GUEST_INPUTS_DIR)
    }

    fn needs_local_inputs(&self) -> bool {
        false
    }

    fn spawn(
        &self,
        request: TaskSpawnRequest,
        token: CancellationToken,
    ) -> Result<Receiver<Result<TaskExecutionResult>>> {
        let (completed_tx, completed_rx) = oneshot::channel();

        let requirements = request.requirements();
        let hints = request.hints();

        let container = container(requirements, self.config.task.container.as_deref()).into_owned();
        let cpu = cpu(requirements);
        let memory = memory(requirements)? as u64;
        let max_cpu = max_cpu(hints);
        let max_memory = max_memory(hints)?.map(|i| i as u64);
        let preemptible = preemptible(hints)?;

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
            TesTaskRequest {
                config: self.config.clone(),
                backend_config: self.backend_config.clone(),
                inner: request,
                backend: self.inner.clone(),
                name,
                container,
                cpu,
                memory,
                max_cpu,
                max_memory,
                token,
                preemptible,
            },
            completed_tx,
        );

        Ok(completed_rx)
    }
}
