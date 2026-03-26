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
use futures::FutureExt;
use futures::future::BoxFuture;
use nonempty::NonEmpty;
use secrecy::ExposeSecret;
use tokio::task::JoinSet;
use tracing::debug;
use tracing::info;

use super::ExecuteTaskRequest;
use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use super::TaskExecutionResult;
use crate::CancellationContext;
use crate::EvaluationPath;
use crate::EvaluationPathKind;
use crate::Events;
use crate::ONE_GIBIBYTE;
use crate::PrimitiveValue;
use crate::TaskInputs;
use crate::Value;
use crate::backend::INITIAL_EXPECTED_NAMES;
use crate::backend::STDERR_FILE_NAME;
use crate::backend::STDOUT_FILE_NAME;
use crate::backend::WORK_DIR_NAME;
use crate::config::Config;
use crate::config::ContentDigestMode;
use crate::config::DEFAULT_TASK_SHELL;
use crate::config::TesBackendAuthConfig;
use crate::digest::UrlDigestExt;
use crate::digest::calculate_local_digest;
use crate::http::Transferer;
use crate::v1::DEFAULT_DISK_MOUNT_POINT;
use crate::v1::DEFAULT_TASK_REQUIREMENT_DISKS;
use crate::v1::hints;
use crate::v1::requirements;
use crate::v1::requirements::ContainerSource;

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

/// Represents the Task Execution Service (TES) backend.
pub struct TesBackend {
    /// The engine configuration.
    config: Arc<Config>,
    /// The underlying Crankshaft backend.
    inner: tes::Backend,
    /// The name generator for tasks.
    names: Arc<Mutex<GeneratorIterator<UniqueAlphanumeric>>>,
    /// The evaluation cancellation context.
    cancellation: CancellationContext,
}

impl TesBackend {
    /// Constructs a new TES task execution backend with the given
    /// configuration.
    ///
    /// The provided configuration is expected to have already been validated.
    pub async fn new(
        config: Arc<Config>,
        events: Events,
        cancellation: CancellationContext,
    ) -> Result<Self> {
        info!("initializing TES backend");

        let backend_config = config.backend()?;
        let backend_config = backend_config
            .as_tes()
            .context("configured backend is not TES")?;

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

        let inner = tes::Backend::initialize(
            backend::tes::Config::builder()
                .url(backend_config.url.clone().expect("should have URL"))
                .http(http)
                .interval(backend_config.interval.unwrap_or(DEFAULT_TES_INTERVAL))
                .build(),
            names.clone(),
            events.crankshaft().clone(),
        )
        .await;

        Ok(Self {
            config,
            inner,
            names,
            cancellation,
        })
    }
}

impl TaskExecutionBackend for TesBackend {
    fn constraints(
        &self,
        inputs: &TaskInputs,
        requirements: &HashMap<String, Value>,
        hints: &HashMap<String, Value>,
    ) -> Result<TaskExecutionConstraints> {
        let container =
            requirements::container(inputs, requirements, self.config.task.container.as_deref());
        match &container {
            ContainerSource::Docker(_) | ContainerSource::Library(_) | ContainerSource::Oras(_) => {
            }
            ContainerSource::SifFile(_) => {
                bail!(
                    "TES backend does not support local SIF file `{container:#}`; use a \
                     registry-based container image instead"
                )
            }
            ContainerSource::Unknown(_) => {
                bail!("TES backend does not support unknown container source `{container:#}`")
            }
        };

        let disks = requirements::disks(inputs, requirements, hints)?;
        if disks.values().any(|d| d.ty.is_some()) {
            debug!("disk type hints are not supported by the TES backend and will be ignored");
        }

        Ok(TaskExecutionConstraints {
            container: Some(container),
            cpu: requirements::cpu(inputs, requirements),
            memory: requirements::memory(inputs, requirements)? as u64,
            gpu: Default::default(),
            fpga: Default::default(),
            disks: disks
                .into_iter()
                .map(|(mp, disk)| (mp.to_string(), disk.size))
                .collect(),
        })
    }

    fn needs_local_inputs(&self) -> bool {
        false
    }

    fn execute<'a>(
        &'a self,
        transferer: &'a Arc<dyn Transferer>,
        request: ExecuteTaskRequest<'a>,
    ) -> BoxFuture<'a, Result<Option<TaskExecutionResult>>> {
        async move {
            let backend_config = self.config.backend()?;
            let backend_config = backend_config
                .as_tes()
                .expect("configured backend should be TES");

            let preemptible = hints::preemptible(request.inputs, request.hints)?;
            let max_memory =
                hints::max_memory(request.inputs, request.hints)?.map(|m| m as f64 / ONE_GIBIBYTE);
            let name = format!(
                "{id}-{generated}",
                id = request.id,
                generated = self
                    .names
                    .lock()
                    .expect("generator should always acquire")
                    .next()
                    .expect("generator should never be exhausted")
            );

            // Write the evaluated command to disk
            // This is done even for remote execution so that a copy exists locally
            let command_path = request.command_path();
            if let Some(parent) = command_path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "failed to create directory `{path}`",
                        path = parent.display()
                    )
                })?;
            }

            fs::write(&command_path, request.command).with_context(|| {
                format!(
                    "failed to write command contents to `{path}`",
                    path = command_path.display()
                )
            })?;

            // SAFETY: currently `inputs` is required by configuration validation, so it
            // should always unwrap
            let inputs_url = Arc::new(
                backend_config
                    .inputs
                    .clone()
                    .expect("should have inputs URL"),
            );

            // Start with the command file as an input
            let mut backend_inputs = vec![
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
            for (i, input) in request.backend_inputs.iter().enumerate() {
                match input.path().kind() {
                    EvaluationPathKind::Local(path) => {
                        // Input is local, spawn an upload of it
                        let kind = input.kind();
                        let path = path.to_path_buf();
                        let transferer = transferer.clone();
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
                    EvaluationPathKind::Remote(url) => {
                        // Input is already remote, add it to the Crankshaft inputs list
                        backend_inputs.push(
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
                let input = &request.backend_inputs[i];
                backend_inputs.push(
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
                timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S")
            );

            // SAFETY: currently `outputs` is required by configuration validation, so it
            // should always unwrap
            let outputs_url = backend_config
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

            // Calculate the total size required for all disks as TES does not have a way of
            // specifying volume sizes; a single disk will be created from which all volumes
            // will be mounted
            let disks = &request.constraints.disks;
            let disk: f64 = if disks.is_empty() {
                DEFAULT_TASK_REQUIREMENT_DISKS
            } else {
                let sum: f64 = disks.values().map(|size| *size as f64).sum();
                if disks.contains_key(DEFAULT_DISK_MOUNT_POINT) {
                    sum
                } else {
                    sum + DEFAULT_TASK_REQUIREMENT_DISKS
                }
            };

            let volumes = request
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

            let mut preemptible = preemptible;
            loop {
                let task = Task::builder()
                    .name(&name)
                    .executions(NonEmpty::new(
                        Execution::builder()
                            .image(
                                match request
                                    .constraints
                                    .container
                                    .as_ref()
                                    .expect("constraints should have a container")
                                {
                                    // For Docker container image sources, omit the protocol
                                    ContainerSource::Docker(s) => s.clone(),
                                    c => format!("{c:#}"),
                                },
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
                            .env(request.env.clone())
                            .stdout(GUEST_STDOUT_PATH)
                            .stderr(GUEST_STDERR_PATH)
                            .build(),
                    ))
                    .inputs(backend_inputs.clone())
                    .outputs(outputs.clone())
                    .resources(
                        Resources::builder()
                            .cpu(request.constraints.cpu)
                            .maybe_cpu_limit(hints::max_cpu(request.inputs, request.hints))
                            .ram(request.constraints.memory as f64 / ONE_GIBIBYTE)
                            .disk(disk)
                            .maybe_ram_limit(max_memory)
                            .preemptible(preemptible > 0)
                            .build(),
                    )
                    .volumes(volumes.clone())
                    .build();

                let statuses = match self.inner.run(task, self.cancellation.second())?.await {
                    Ok(statuses) => statuses,
                    Err(TaskRunError::Preempted) if preemptible > 0 => {
                        // Decrement the preemptible count and retry
                        preemptible -= 1;
                        continue;
                    }
                    Err(TaskRunError::Canceled) => return Ok(None),
                    Err(e) => return Err(e.into()),
                };

                assert_eq!(statuses.len(), 1, "there should only be one output");
                let status = statuses.first();

                // Push an empty path segment so that future joins of the work directory URL
                // treat it as a directory
                work_dir_url.path_segments_mut().unwrap().push("");

                return Ok(Some(TaskExecutionResult {
                    exit_code: status.code().expect("should have exit code"),
                    work_dir: EvaluationPath::try_from(work_dir_url)?,
                    stdout: PrimitiveValue::new_file(stdout_url).into(),
                    stderr: PrimitiveValue::new_file(stderr_url).into(),
                }));
            }
        }
        .boxed()
    }
}
