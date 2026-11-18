//! Implementation of the TES backend.

use std::fs;
use std::sync::Arc;
use std::sync::Mutex;

use ::tes::v1::Client;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use cloud_copy::UrlExt;
use crankshaft::config::backend;
use crankshaft::engine::Task;
use crankshaft::engine::service::name::GeneratorIterator;
use crankshaft::engine::service::name::UniqueAlphanumeric;
use crankshaft::engine::service::runner::Backend as _;
use crankshaft::engine::service::runner::backend::TaskRunError;
use crankshaft::engine::service::runner::backend::tes::Backend;
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
use tes::auth::BasicAuthorizer;
use tes::auth::BearerTokenAuthorizer;
use tes::auth::oauth;
use tes::auth::oauth::OAuthAuthorizer;
use tokio::task::JoinSet;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;
use tracing_indicatif::indicatif_eprintln;

use super::ExecuteTaskRequest;
use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use super::TaskExecutionResult;
use crate::ContainerImageOverrides;
use crate::EngineEvent;
use crate::EvaluationPath;
use crate::EvaluationPathKind;
use crate::INITIAL_EXPECTED_NAMES;
use crate::ImageSource;
use crate::ONE_GIBIBYTE;
use crate::Object;
use crate::PrimitiveValue;
use crate::RetryCause;
use crate::TaskInputs;
use crate::backend::STDERR_FILE_NAME;
use crate::backend::STDOUT_FILE_NAME;
use crate::backend::WORK_DIR_NAME;
use crate::backend::usage;
use crate::backend::usage::UsageEvents;
use crate::config::Config;
use crate::config::ContentDigestMode;
use crate::config::TesBackendAuthConfig;
use crate::digest::UrlDigestExt;
use crate::resubmit_task_name;
use crate::v1::DEFAULT_DISK_MOUNT_POINT;
use crate::v1::DEFAULT_TASK_REQUIREMENT_DISKS;
use crate::v1::hints;
use crate::v1::requirements;

/// The guest working directory.
const GUEST_WORK_DIR: &str = "/mnt/task/work";

/// The guest path for the command file.
const GUEST_COMMAND_PATH: &str = "/mnt/task/command";

/// The path to the container's stdout.
const GUEST_STDOUT_PATH: &str = "/mnt/task/stdout";

/// The path to the container's stderr.
const GUEST_STDERR_PATH: &str = "/mnt/task/stderr";

/// A separate writable volume for the fallback recording.
const GUEST_USAGE_DIR: &str = "/mnt/task/usage";

/// The path to the fallback recording outside the exported work directory.
const GUEST_USAGE_PATH: &str = "/mnt/task/usage/recording";

/// The default poll interval, in seconds, for the TES backend.
const DEFAULT_TES_INTERVAL: u64 = 30;

/// The name of the attempt file that stores where TES writes the task's
/// outputs.
///
/// The location includes a timestamp taken at submission, so it can't be
/// rebuilt later.
const OUTPUTS_URL_FILE_NAME: &str = "tes_outputs";

/// Writes the outputs location of a TES task to the attempt folder.
///
/// A failure to write the file is logged, not returned, so that the task can
/// still run.
fn record_outputs_url(attempt_dir: &std::path::Path, url: &url::Url) {
    let path = attempt_dir.join(OUTPUTS_URL_FILE_NAME);
    if let Err(e) = fs::write(&path, format!("{url}\n")) {
        error!("failed to write file `{path}`: {e}", path = path.display());
    }
}

/// Returns an override only when the actual name differs from the requested
/// name.
fn execution_name_override(requested: &str, actual: String) -> Option<String> {
    (actual != requested).then_some(actual)
}

/// Represents the Task Execution Service (TES) backend.
pub struct TesBackend {
    /// The engine configuration.
    config: Arc<Config>,
    /// The underlying Crankshaft backend.
    inner: Backend,
}

impl TesBackend {
    /// Constructs a new TES task execution backend with the given
    /// configuration.
    ///
    /// The provided configuration is expected to have already been validated.
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        info!("initializing TES backend");

        let backend_config = config.backend()?;
        let backend_config = backend_config
            .as_tes()
            .context("configured backend is not TES")?;

        let http = backend::tes::http::Config {
            retries: backend_config.retries,
            max_concurrency: backend_config.max_concurrency.map(|c| c as usize),
            ..Default::default()
        };

        let names = Arc::new(Mutex::new(GeneratorIterator::new(
            UniqueAlphanumeric::default_with_expected_generations(INITIAL_EXPECTED_NAMES),
            INITIAL_EXPECTED_NAMES,
        )));

        // Construct the TES client for the backend
        let client = Client::builder().url(backend_config.service.clone());
        let client = match &backend_config.auth {
            Some(TesBackendAuthConfig::Basic { config }) => {
                client.authorizer(BasicAuthorizer::new(
                    &config.username,
                    config
                        .password
                        .as_ref()
                        .map(|p| p.inner().expose_secret().to_string()),
                ))
            }
            Some(TesBackendAuthConfig::Bearer { config }) => client.authorizer(
                BearerTokenAuthorizer::new(config.token.inner().expose_secret()),
            ),
            Some(TesBackendAuthConfig::OAuth { config }) => {
                let require_refresh = config.require_refresh;
                let authorizer = OAuthAuthorizer::new(
                    oauth::Config {
                        client_id: config.client_id.clone(),
                        client_secret: config
                            .client_secret
                            .as_ref()
                            .map(|s| s.inner().expose_secret().to_string()),
                        audience: Some(config.audience.clone().unwrap_or_else(|| {
                            backend_config.service.origin().ascii_serialization()
                        })),
                        authorization: config.authorization.clone(),
                        token: config.token.clone(),
                        scopes: config.scopes.clone(),
                    },
                    |r| {
                        indicatif_eprintln!(
                            "TES backend authorization is required: visit {url} and enter code \
                             `{code}`",
                            url = r.verification_uri(),
                            code = r.user_code().secret(),
                        );
                    },
                )
                .on_reauthorization(move |e| {
                    async move {
                        if let Some(e) = e {
                            error!("error refreshing TES OAuth access token: {e}");
                        }

                        !require_refresh
                    }
                    .boxed()
                });
                let can_refresh = authorizer.initialize().await?;
                if require_refresh && !can_refresh {
                    bail!(
                        "TES backend initialization failed: an OAuth refresh token was not \
                         returned and token refresh is required"
                    );
                }

                client.authorizer(authorizer)
            }
            None => client,
        };

        let inner = Backend::initialize_with_client(
            backend::tes::Config::builder()
                .url(backend_config.service.clone())
                .http(http)
                .interval(backend_config.interval.unwrap_or(DEFAULT_TES_INTERVAL))
                .build(),
            // SAFETY: the URL is the only required field and it was supplied
            // above
            client.try_build().unwrap(),
            names.clone(),
        )
        .await;

        Ok(Self { config, inner })
    }
}

impl TaskExecutionBackend for TesBackend {
    fn name(&self) -> &'static str {
        "tes"
    }

    fn constraints(
        &self,
        inputs: &TaskInputs,
        requirements: &Object,
        image_overrides: &ContainerImageOverrides,
        hints: &Object,
    ) -> Result<TaskExecutionConstraints> {
        let sources = requirements::container(
            inputs,
            requirements,
            image_overrides,
            &self.config.task.container,
        )?;
        for source in &sources {
            match source {
                ImageSource::Docker(_) | ImageSource::Library(_) | ImageSource::Oras(_) => {}
                ImageSource::SifFile(_) => {
                    bail!(
                        "TES backend does not support local SIF file `{source:#}`; use a \
                         registry-based container image instead"
                    )
                }
                ImageSource::Unknown(_) => {
                    bail!(
                        "TES backend does not support unknown container image source `{source:#}`"
                    )
                }
            }
        }

        let disks = requirements::disks(inputs, requirements, hints)?;
        if disks.values().any(|d| d.ty.is_some()) {
            debug!("disk type hints are not supported by the TES backend and will be ignored");
        }

        Ok(TaskExecutionConstraints {
            sources,
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
        request: &'a ExecuteTaskRequest<'a>,
    ) -> BoxFuture<'a, Result<Option<TaskExecutionResult>>> {
        async move {
            let backend_config = self.config.backend()?;
            let backend_config = backend_config
                .as_tes()
                .expect("configured backend should be TES");

            let preemptible = hints::preemptible(request.inputs, request.hints)?;
            let max_memory =
                hints::max_memory(request.inputs, request.hints)?.map(|m| m as f64 / ONE_GIBIBYTE);

            // Write the evaluated command to disk
            // This is done even for remote execution so that a copy exists
            // locally
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

            let inputs_url = Arc::new(backend_config.inputs.clone());

            // Start with the command file as an input
            let mut backend_inputs = vec![
                Input::builder()
                    .path(GUEST_COMMAND_PATH)
                    .contents(Contents::Path(command_path.to_path_buf()))
                    .ty(InputType::File)
                    .read_only(true)
                    .build(),
            ];

            // Spawn upload tasks for inputs available locally, and apply
            // authentication to the URLs for remote inputs.
            let mut uploads = JoinSet::new();
            for (i, input) in request.backend_inputs.iter().enumerate() {
                match input.path().kind() {
                    EvaluationPathKind::Local(path) => {
                        // Input is local, spawn an upload of it
                        let kind = input.kind();
                        let path = path.to_path_buf();
                        let client = request.context.http_client().clone();
                        let token = request.context.cancellation().first().clone();
                        let digests = request.context.digests().clone();
                        let inputs_url = inputs_url.clone();
                        uploads.spawn(async move {
                            let url = inputs_url.join_digest(
                                digests
                                    .calculate_local_digest(
                                        &path,
                                        kind,
                                        ContentDigestMode::Strong,
                                        &token,
                                    )
                                    .await
                                    .with_context(|| {
                                        format!(
                                            "failed to calculate digest of `{path}`",
                                            path = path.display()
                                        )
                                    })?,
                            );
                            client
                                .upload(&path, &url, &token)
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
                        // Input is already remote, add it to the Crankshaft
                        // inputs list
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

            // Notify that the task is transferring inputs; for TES this covers
            // both digesting each local input and uploading it to
            // remote storage, which dominates the runtime of large
            // inputs
            if !uploads.is_empty()
                && let Some(sender) = request.context.events().engine()
            {
                let _ = sender.send(EngineEvent::TaskLocalizing {
                    name: request.name.to_string(),
                });
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
                name = request.name,
                timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S")
            );

            // SAFETY: currently `outputs` is required by configuration
            // validation, so it should always unwrap
            let outputs_url = backend_config
                .outputs
                .join(&output_dir)
                .expect("should join");
            record_outputs_url(request.attempt_dir, &outputs_url);

            let mut work_dir_url = outputs_url.join(WORK_DIR_NAME).expect("should join");
            let stdout_url = outputs_url.join(STDOUT_FILE_NAME).expect("should join");
            let stderr_url = outputs_url.join(STDERR_FILE_NAME).expect("should join");
            let container_usage =
                backend_config.cgroup_usage && request.context.events().crankshaft().is_some();
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

            // Calculate the total size required for all disks as TES does not
            // have a way of specifying volume sizes; a single disk
            // will be created from which all volumes
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

            let mut volumes = request
                .constraints
                .disks
                .keys()
                .filter_map(|mp| {
                    // NOTE: the root mount point is already handled by the work
                    // directory mount, so we filter it here to avoid duplicate
                    // volume mapping.
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
            if container_usage {
                volumes.push(GUEST_USAGE_DIR.to_string());
            }

            let mut preemptible = preemptible;
            let mut name = request.name.to_string();
            let mut resubmission: u64 = 0;
            loop {
                let mut outputs = outputs.clone();
                let usage_url = container_usage.then(|| {
                    outputs_url
                        .join(&format!("usage-{resubmission}/"))
                        .expect("should join")
                });
                if let Some(url) = &usage_url {
                    outputs.push(
                        Output::builder()
                            .path(GUEST_USAGE_DIR)
                            .url(url.clone())
                            .ty(OutputType::Directory)
                            .build(),
                    );
                }
                let task = Task::builder()
                    .name(&name)
                    .executions(NonEmpty::new(
                        Execution::builder()
                            .images(request.constraints.sources.iter().map(|c| match c {
                                ImageSource::Docker(s) => s.clone(),
                                c => format!("{c:#}"),
                            }))?
                            .program(if container_usage {
                                usage::WRAPPER_SHELL
                            } else {
                                &self.config.task.shell
                            })
                            .args(if container_usage {
                                usage::command_args(
                                    &self.config.task.shell,
                                    GUEST_COMMAND_PATH,
                                    GUEST_USAGE_PATH,
                                )
                            } else {
                                vec![GUEST_COMMAND_PATH.to_string()]
                            })
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

                let mut observed = container_usage
                    .then(|| UsageEvents::new(request.context.events().crankshaft(), &name));
                let execution = self.inner.run(
                    task,
                    request.context.events().crankshaft().cloned(),
                    request.context.cancellation().second().clone(),
                )?;
                let results = match if let Some(observed) = &mut observed {
                    observed.run(execution).await
                } else {
                    execution.await
                } {
                    Ok(results) => results,
                    Err(TaskRunError::Preempted) if preemptible > 0 => {
                        // Decrement the preemptible count and resubmit under a
                        // derived name so that consumers can attribute each
                        // resubmission distinctly; see
                        // `RESUBMIT_NAME_SEPARATOR`.
                        preemptible -= 1;
                        resubmission += 1;
                        let next_name = resubmit_task_name(&name, resubmission);
                        if let Some(sender) = request.context.events().engine() {
                            let _ = sender.send(EngineEvent::TaskRetrying {
                                prior_name: name.clone(),
                                next_name: next_name.clone(),
                                cause: RetryCause::Preempted,
                            });
                        }
                        name = next_name;
                        continue;
                    }
                    Err(TaskRunError::Canceled) => return Ok(None),
                    Err(e) => return Err(e.into()),
                };

                assert_eq!(results.len(), 1, "there should only be one output");
                let result = results.into_iter().next().unwrap();

                if let Some(observed) = &observed
                    && observed.needs_fallback()
                    && let Some(url) = &usage_url
                {
                    match request
                        .context
                        .http_client()
                        .download(
                            &url.join("recording").expect("should join"),
                            request.context.cancellation().second(),
                        )
                        .await
                    {
                        Ok(location) => match fs::read_to_string(&*location) {
                            Ok(contents) => observed.publish(&contents),
                            Err(error) => warn!("failed to read TES usage recording: {error:#}"),
                        },
                        Err(error) => warn!("failed to retrieve TES usage recording: {error:#}"),
                    }
                }

                // Push an empty path segment so that future joins of the work
                // directory URL treat it as a directory
                work_dir_url.path_segments_mut().unwrap().push("");

                return Ok(Some(TaskExecutionResult {
                    execution_name_override: execution_name_override(request.name, name),
                    // SAFETY: parsing of an image source is infallible
                    image: result.image.map(|s| s.parse().unwrap()),
                    exit_code: result.status.code().expect("should have exit code"),
                    work_dir: EvaluationPath::try_from(work_dir_url)?,
                    stdout: PrimitiveValue::new_file(stdout_url).into(),
                    stderr: PrimitiveValue::new_file(stderr_url).into(),
                }));
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::*;

    #[tokio::test]
    async fn successful_tasks_keep_status_when_metrics_are_missing_and_native_zero_wins() {
        use std::time::Duration;

        use crankshaft::events::Event;
        use tokio::io::AsyncBufReadExt;
        use tokio::io::AsyncReadExt;
        use tokio::io::AsyncWriteExt;
        use tokio::io::BufReader;
        use tokio::net::TcpListener;

        use crate::CancellationContext;
        use crate::Events;
        use crate::backend::tests::EvalContext;

        for (enabled, recording, native, metadata_enabled, preempted) in [
            (
                true,
                Some("usage_usec 1000\nuser_usec 1000\nsystem_usec 0\nmemory_peak_bytes 32\n"),
                false,
                false,
                false,
            ),
            (true, Some(""), false, false, false),
            (true, None, false, false, false),
            (
                true,
                Some("usage_usec 1000\nuser_usec 1000\nsystem_usec 0\nmemory_peak_bytes 32\n"),
                true,
                true,
                false,
            ),
            (
                true,
                Some("usage_usec 1000\nuser_usec 1000\nsystem_usec 0\nmemory_peak_bytes 32\n"),
                true,
                false,
                false,
            ),
            (false, None, false, false, false),
            (
                true,
                Some("usage_usec 1000\nuser_usec 1000\nsystem_usec 0\nmemory_peak_bytes 32\n"),
                false,
                false,
                true,
            ),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let fetched = Arc::new(Mutex::new(Vec::new()));
            let fetched_by_server = fetched.clone();
            let polls = Arc::new(Mutex::new(Vec::new()));
            let polls_by_server = polls.clone();
            let server = tokio::spawn(async move {
                let mut task = None;
                let mut submissions = 0;
                loop {
                    let (stream, _) = listener.accept().await.unwrap();
                    let mut stream = BufReader::new(stream);
                    let mut line = String::new();
                    stream.read_line(&mut line).await.unwrap();
                    let request_line = line.clone();
                    let mut length = 0;
                    loop {
                        line.clear();
                        assert!(stream.read_line(&mut line).await.unwrap() > 0);
                        if line == "\r\n" {
                            break;
                        }
                        if let Some((key, value)) = line.split_once(':')
                            && key.eq_ignore_ascii_case("content-length")
                        {
                            length = value.trim().parse().unwrap();
                        }
                    }
                    let mut body = vec![0; length];
                    stream.read_exact(&mut body).await.unwrap();
                    let path = request_line.split_whitespace().nth(1).unwrap();
                    let (status, response) = if request_line.starts_with("POST ") {
                        let mut created: serde_json::Value = serde_json::from_slice(&body).unwrap();
                        submissions += 1;
                        let id = format!("tes-test-{submissions}");
                        created["id"] = id.clone().into();
                        created["state"] = if preempted && submissions == 1 {
                            "PREEMPTED"
                        } else {
                            "COMPLETE"
                        }
                        .into();
                        created["logs"] = serde_json::json!([{
                            "logs": [{"exit_code": 0}],
                            "outputs": [],
                            "metadata": if native { serde_json::json!({"cpu_time_ms": 0}) } else { serde_json::json!({}) },
                        }]);
                        task = Some(created);
                        ("200 OK", serde_json::json!({"id": id}).to_string())
                    } else if path.starts_with("/tasks?") {
                        polls_by_server.lock().unwrap().push(path.to_string());
                        let tasks: Vec<_> = task
                            .iter()
                            .map(|task| {
                                if path.contains("BASIC") {
                                    task.clone()
                                } else {
                                    serde_json::json!({"id": task["id"], "state": task["state"]})
                                }
                            })
                            .collect();
                        ("200 OK", serde_json::json!({"tasks": tasks}).to_string())
                    } else if path.starts_with("/tasks/") {
                        ("200 OK", task.as_ref().unwrap().to_string())
                    } else {
                        assert!(path.starts_with("/outputs/"), "{request_line}");
                        fetched_by_server.lock().unwrap().push(path.to_string());
                        match recording {
                            Some(contents) => ("200 OK", contents.to_string()),
                            None => ("404 Not Found", String::new()),
                        }
                    };
                    stream
                        .get_mut()
                        .write_all(
                            format!(
                                "HTTP/1.1 {status}\r\nContent-Type: \
                                 application/json\r\nContent-Length: {}\r\nConnection: \
                                 close\r\n\r\n{response}",
                                response.len(),
                            )
                            .as_bytes(),
                        )
                        .await
                        .unwrap();
                }
            });
            let config = toml_spanner::from_str::<Config>(&format!(
                r#"
backend = "tes"
[backends.tes]
type = "tes"
service = "http://{address}"
inputs = "http://{address}/inputs/"
outputs = "http://{address}/outputs/"
insecure = true
interval = 1
cgroup_usage = {enabled}
"#
            ))
            .unwrap();
            let config = Arc::new(config);
            let backend = if metadata_enabled {
                let url = config.backend().unwrap().as_tes().unwrap().service.clone();
                let inner = Backend::initialize_with_client(
                    backend::tes::Config::builder()
                        .url(url.clone())
                        .interval(1)
                        .resource_usage_metadata(true)
                        .build(),
                    Client::builder().url(url).try_build().unwrap(),
                    Arc::new(Mutex::new(GeneratorIterator::new(
                        UniqueAlphanumeric::default_with_expected_generations(
                            INITIAL_EXPECTED_NAMES,
                        ),
                        INITIAL_EXPECTED_NAMES,
                    ))),
                )
                .await;
                TesBackend { config, inner }
            } else {
                TesBackend::new(config).await.unwrap()
            };
            let events = Events::default().with_crankshaft(32);
            let mut receiver = events.subscribe_crankshaft().unwrap();
            let context = EvalContext::new_with_http_client(
                events,
                CancellationContext::default(),
                crate::http::DefaultHttpClient::new(&Config::local()).unwrap(),
            )
            .await;
            let root = tempfile::tempdir().unwrap();
            let request = ExecuteTaskRequest {
                context: &context,
                name: "usage-success",
                command: "exit 0",
                inputs: &TaskInputs::default(),
                backend_inputs: &[],
                requirements: &Object::empty(),
                hints: &Object::new(if preempted {
                    [("preemptible".to_string(), 1_i64.into())]
                        .into_iter()
                        .collect()
                } else {
                    Default::default()
                }),
                env: &Default::default(),
                constraints: &TaskExecutionConstraints {
                    sources: vec![ImageSource::Docker("test:latest".into())],
                    cpu: 1.0,
                    memory: 1024 * 1024 * 1024,
                    gpu: Vec::new(),
                    fpga: Vec::new(),
                    disks: Default::default(),
                },
                base_dir: &EvaluationPath::from(root.path()),
                attempt_dir: root.path(),
                temp_dir: root.path(),
            };
            let result = tokio::time::timeout(Duration::from_secs(10), backend.execute(&request))
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            server.abort();
            let _ = server.await;
            assert_eq!(result.exit_code, 0);
            assert_eq!(
                result.execution_name_override,
                preempted.then(|| resubmit_task_name(request.name, 1))
            );
            let mut created = None;
            let mut measured = Vec::new();
            while let Ok(event) = receiver.try_recv() {
                match event {
                    Event::TaskCreated { id, .. } => created = Some(id),
                    Event::TaskResourceUsage { id, usage } if !usage.is_empty() => {
                        assert_eq!(Some(id), created);
                        measured.push(usage.cpu_time_ms.unwrap());
                    }
                    _ => {}
                }
            }
            assert_eq!(
                measured,
                if native && metadata_enabled {
                    vec![0]
                } else if enabled && recording.is_some_and(|r| !r.is_empty()) {
                    vec![1]
                } else {
                    vec![]
                }
            );
            let fetched = fetched.lock().unwrap();
            let polls = polls.lock().unwrap();
            assert!(!polls.is_empty());
            let view = if metadata_enabled {
                "view=BASIC"
            } else {
                "view=MINIMAL"
            };
            assert!(polls.iter().all(|path| path.contains(view)), "{polls:?}");
            if enabled && !(native && metadata_enabled) {
                assert!(!fetched.is_empty());
                let suffix = if preempted {
                    "/usage-1/recording"
                } else {
                    "/usage-0/recording"
                };
                assert!(
                    fetched.iter().all(|path| path.ends_with(suffix)),
                    "{fetched:?}"
                );
            } else {
                assert!(fetched.is_empty(), "{fetched:?}");
            }
        }
    }

    #[tokio::test]
    async fn fallback_recording_has_separate_storage_only_when_observed() {
        use std::time::Duration;

        use tokio::io::AsyncBufReadExt;
        use tokio::io::AsyncReadExt;
        use tokio::io::AsyncWriteExt;
        use tokio::io::BufReader;
        use tokio::net::TcpListener;

        use crate::CancellationContext;
        use crate::Events;
        use crate::backend::tests::EvalContext;

        for (setting, enabled, observed) in [
            (None, true, true),
            (Some(true), true, true),
            (Some(false), false, true),
            (Some(true), false, false),
        ] {
            let cgroup_setting = setting
                .map(|value| format!("cgroup_usage = {value}"))
                .unwrap_or_default();
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut stream = BufReader::new(stream);
                let mut line = String::new();
                stream.read_line(&mut line).await.unwrap();
                assert!(line.starts_with("POST "));
                let mut length = None;
                loop {
                    line.clear();
                    assert!(stream.read_line(&mut line).await.unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((key, value)) = line.split_once(':')
                        && key.eq_ignore_ascii_case("content-length")
                    {
                        length = Some(value.trim().parse::<usize>().unwrap());
                    }
                }
                let mut body = vec![0; length.expect("request must have content length")];
                stream.read_exact(&mut body).await.unwrap();
                stream
                    .get_mut()
                    .write_all(
                        b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await
                    .unwrap();
                serde_json::from_slice::<serde_json::Value>(&body).unwrap()
            });
            let config = toml_spanner::from_str::<Config>(&format!(
                r#"
backend = "tes"
[backends.tes]
type = "tes"
service = "http://{address}"
inputs = "http://{address}/inputs/"
outputs = "http://{address}/outputs/"
insecure = true
{cgroup_setting}
"#
            ))
            .unwrap();
            let backend = TesBackend::new(Arc::new(config)).await.unwrap();
            let events = if observed {
                Events::default().with_crankshaft(32)
            } else {
                Events::default()
            };
            let context = EvalContext::new(events, CancellationContext::default()).await;
            let root = tempfile::tempdir().unwrap();
            let request = ExecuteTaskRequest {
                context: &context,
                name: "usage-test",
                command: "exit 7",
                inputs: &TaskInputs::default(),
                backend_inputs: &[],
                requirements: &Object::empty(),
                hints: &Object::empty(),
                env: &Default::default(),
                constraints: &TaskExecutionConstraints {
                    sources: vec![ImageSource::Docker("test:latest".into())],
                    cpu: 1.0,
                    memory: 1024 * 1024 * 1024,
                    gpu: Vec::new(),
                    fpga: Vec::new(),
                    disks: Default::default(),
                },
                base_dir: &EvaluationPath::from(root.path()),
                attempt_dir: root.path(),
                temp_dir: root.path(),
            };
            assert!(
                tokio::time::timeout(Duration::from_secs(10), backend.execute(&request))
                    .await
                    .unwrap()
                    .is_err(),
                "the capture server rejects execution after recording the request"
            );
            let task = tokio::time::timeout(Duration::from_secs(10), server)
                .await
                .unwrap()
                .unwrap();
            let outputs = task["outputs"].as_array().unwrap();
            if enabled {
                let recording = outputs
                    .iter()
                    .find(|output| {
                        output["url"]
                            .as_str()
                            .unwrap()
                            .trim_end_matches('/')
                            .ends_with("/usage-0")
                    })
                    .expect("recording must be exported separately");
                let path = recording["path"].as_str().unwrap();
                assert_eq!(recording["type"], "DIRECTORY");
                assert_eq!(path, GUEST_USAGE_DIR);
                assert!(
                    task["volumes"]
                        .as_array()
                        .is_some_and(|volumes| volumes.iter().any(|volume| volume == path)),
                    "recording needs a provisioned volume: {task}"
                );
                assert!(!path.starts_with(GUEST_WORK_DIR));
                assert_eq!(
                    task["executors"][0]["command"]
                        .as_array()
                        .unwrap()
                        .last()
                        .unwrap(),
                    GUEST_USAGE_PATH
                );
                #[cfg(unix)]
                {
                    let startup = root.path().join("startup");
                    fs::write(&startup, "printf startup").unwrap();
                    let recording = root.path().join("recording");
                    let args = task["executors"][0]["command"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|value| match value.as_str().unwrap() {
                            GUEST_COMMAND_PATH => {
                                request.command_path().to_str().unwrap().to_string()
                            }
                            GUEST_USAGE_PATH => recording.to_str().unwrap().to_string(),
                            argument => argument.to_string(),
                        })
                        .collect::<Vec<_>>();
                    let output = tokio::process::Command::new(&args[0])
                        .args(&args[1..])
                        .env("BASH_ENV", startup)
                        .output()
                        .await
                        .unwrap();
                    assert_eq!(output.status.code(), Some(7));
                    assert_eq!(
                        output.stdout, b"startup",
                        "the configured shell's startup must run exactly once"
                    );
                    assert!(output.stderr.is_empty());
                }
            } else {
                assert_eq!(outputs.len(), 3);
                assert_eq!(
                    task["executors"][0]["command"],
                    serde_json::json!([backend.config.task.shell, GUEST_COMMAND_PATH])
                );
            }
        }
    }

    #[test]
    fn execution_name_override_is_only_reported_when_changed() {
        assert_eq!(execution_name_override("base", "base".to_string()), None);
        assert_eq!(
            execution_name_override("base", "base~1".to_string()),
            Some("base~1".to_string())
        );
    }

    #[test]
    fn outputs_url_is_recorded_in_the_attempt_dir() {
        let root = tempfile::tempdir().unwrap();
        let url: Url = "s3://bucket/outputs/task-0-20260101-000000/"
            .parse()
            .unwrap();

        record_outputs_url(root.path(), &url);

        assert_eq!(
            std::fs::read_to_string(root.path().join(OUTPUTS_URL_FILE_NAME)).unwrap(),
            "s3://bucket/outputs/task-0-20260101-000000/\n"
        );
    }
}
