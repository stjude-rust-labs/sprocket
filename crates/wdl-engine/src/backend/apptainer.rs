//! Support for using Apptainer (a.k.a. Singularity) container runtime.
//!
//! There are two primary responsibilities of this module: `.sif` image cache
//! management and command script generation.
//!
//! The entrypoint for both of these is [`ApptainerRuntime::generate_script`].

use std::borrow::Cow;
use std::fmt::Write as _;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::process::Stdio;

use anyhow::Context as _;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use blake3::Hasher;
use crankshaft::events::Event;
use crankshaft::events::TaskId;
use crankshaft::events::send_event;
use tokio::process::Command;
use tokio_retry2::Retry;
use tokio_retry2::RetryError;
use tokio_retry2::strategy::ExponentialBackoff;
use tracing::debug;
use tracing::error;
use tracing::warn;

use crate::EngineEvent;
use crate::Events;
use crate::ImageSource;
use crate::Value;
use crate::backend::ExecuteTaskRequest;
use crate::backend::PullResults;
use crate::config::ApptainerConfig;
use crate::lock::LockedFile;

/// The guest working directory.
const GUEST_WORK_DIR: &str = "/mnt/task/work";

/// The guest path for the command file.
const GUEST_COMMAND_PATH: &str = "/mnt/task/command";

/// The path to the container's stdout.
const GUEST_STDOUT_PATH: &str = "/mnt/task/stdout";

/// The path to the container's stderr.
const GUEST_STDERR_PATH: &str = "/mnt/task/stderr";

/// The environment variable prefix for Apptainer.
const APPTAINER_ENV_PREFIX: &str = "APPTAINERENV";

/// The environment variable prefix for Singularity.
const SINGULARITY_ENV_PREFIX: &str = "SINGULARITYENV";

/// The name of the file that is used as an advisory lock for the image cache.
const LOCK_FILE_NAME: &str = ".lock";

/// The name of the attempt file that stores the scheduler's job ID.
pub(crate) const JOB_ID_FILE_NAME: &str = "job_id";

/// The name of the attempt file that stores the Apptainer exit code.
///
/// The job script writes it when Apptainer exits. It is missing when the
/// scheduler stops the job first, for example on a timeout or a cancel.
pub(crate) const APPTAINER_EXIT_CODE_FILE_NAME: &str = "apptainer_exit_code";

/// Records the job ID that a scheduler assigned to a task attempt.
///
/// Writes the ID to the attempt's `job_id` file and sends a
/// [`EngineEvent::TaskSubmitted`] event.
///
/// A failure to write the file is logged, not returned, so that the engine can
/// continue to monitor the task.
pub(crate) async fn record_job_id(events: &Events, attempt_dir: &Path, name: &str, job_id: u64) {
    let path = attempt_dir.join(JOB_ID_FILE_NAME);
    if let Err(e) = tokio::fs::write(&path, format!("{job_id}\n")).await {
        error!("failed to write file `{path}`: {e}", path = path.display());
    }

    if let Some(sender) = events.engine() {
        let _ = sender.send(EngineEvent::TaskSubmitted {
            name: name.to_string(),
            external_id: job_id.to_string(),
        });
    }
}

/// Quotes one literal shell argument, rejecting NUL bytes.
fn quote_shell_arg(value: &str) -> Result<Cow<'_, str>> {
    shlex::try_quote(value).context("shell arguments must not contain NUL bytes")
}

/// Encodes a bind mount as CSV and quotes it as one shell argument.
fn quote_mount_arg(source: &Path, destination: &str, read_only: bool) -> Result<String> {
    let source = source
        .to_str()
        .context("mount source path is not valid UTF-8")?;
    // Apptainer parses mount fields as CSV after the host shell parses the
    // argument.
    let mount = format!(
        "type=bind,\"src={}\",\"dst={}\"{}",
        source.replace('"', "\"\""),
        destination.replace('"', "\"\""),
        if read_only { ",ro" } else { "" },
    );
    // The mount always contains `"`, so the quoted result is always owned.
    quote_shell_arg(&mount).map(Cow::into_owned)
}

/// Represents the Apptainer container runtime.
pub struct ApptainerRuntime {
    /// The shared image cache lock.
    ///
    /// This is used to signal that the cache is in use by this process.
    ///
    /// In the future, an exclusive lock could be obtained for clearing the
    /// image cache.
    _lock: LockedFile,
    /// The cache directory for `.sif` images.
    cache_dir: PathBuf,
}

impl ApptainerRuntime {
    /// Creates a new [`ApptainerRuntime`] with the specified image cache
    /// directory.
    pub async fn new(cache_dir: impl Into<PathBuf>) -> Result<Self> {
        let cache_dir = cache_dir.into();

        // Make the cache directory absolute
        let cache_dir = absolute(&cache_dir).with_context(|| {
            format!(
                "failed to make path `{path}` absolute",
                path = cache_dir.display()
            )
        })?;

        // Create the cache directory
        tokio::fs::create_dir_all(&cache_dir)
            .await
            .with_context(|| {
                format!(
                    "failed to create directory `{parent}`",
                    parent = cache_dir.display()
                )
            })?;

        // SAFETY: the lock file is requested to be created so `Some` is always
        // returned
        let _lock = LockedFile::acquire_shared(&cache_dir.join(LOCK_FILE_NAME), true)
            .await?
            .unwrap();

        Ok(Self { _lock, cache_dir })
    }

    /// Generates the script to run the given task using the Apptainer runtime.
    ///
    /// Returns the generated script along with the [`ImageSource`] that
    /// was actually pulled and selected for execution.
    ///
    /// # Shared filesystem assumptions
    ///
    /// The returned script should be run in an environment that shares a
    /// filesystem with the environment where this method is invoked, except
    /// for node-specific mounts like `/tmp` and `/var`. This assumption
    /// typically holds on HPC systems with shared filesystems like Lustre or
    /// GPFS.
    pub async fn generate_script(
        &self,
        config: &ApptainerConfig,
        shell: &str,
        request: &ExecuteTaskRequest<'_>,
        task_id: TaskId,
    ) -> Result<Option<(String, ImageSource)>> {
        let results = match self
            .pull_first_available_image(&config.executable, request, task_id)
            .await
        {
            Some(results) => results,
            None => return Ok(None),
        };

        let (image, path) = results
            .successful_images()
            .next()
            .ok_or_else(|| anyhow!("{results}"))?;
        Ok(Some((
            self.generate_apptainer_script(config, shell, path, request)
                .await?,
            image.clone(),
        )))
    }

    /// Generate the script, given a container path that's already assumed to be
    /// populated.
    ///
    /// This is a separate method in order to facilitate testing, and should not
    /// be called from outside this module.
    async fn generate_apptainer_script(
        &self,
        config: &ApptainerConfig,
        shell: &str,
        image_path: &Path,
        request: &ExecuteTaskRequest<'_>,
    ) -> Result<String> {
        // Create a temp dir for the container's execution within the attempt
        // dir hierarchy. On many HPC systems, `/tmp` is mapped to a
        // relatively small, local scratch disk that can fill up easily.
        // Mapping the container's `/tmp` and `/var/tmp` paths to the
        // filesystem we're using for other inputs and outputs prevents
        // this from being a capacity problem, though potentially at the
        // expense of execution speed if the non-`/tmp` filesystem is
        // significantly slower.
        let container_tmp_path = request.temp_dir.join("container_tmp");
        tokio::fs::DirBuilder::new()
            .recursive(true)
            .create(&container_tmp_path)
            .await
            .with_context(|| {
                format!(
                    "failed to create container /tmp directory at `{path}`",
                    path = container_tmp_path.display()
                )
            })?;
        let container_var_tmp_path = request.temp_dir.join("container_var_tmp");
        tokio::fs::DirBuilder::new()
            .recursive(true)
            .create(&container_var_tmp_path)
            .await
            .with_context(|| {
                format!(
                    "failed to create container /var/tmp directory at `{path}`",
                    path = container_var_tmp_path.display()
                )
            })?;

        let env_prefix = if config.executable.contains("singularity") {
            SINGULARITY_ENV_PREFIX
        } else {
            APPTAINER_ENV_PREFIX
        };

        let mut apptainer_command = String::new();
        writeln!(&mut apptainer_command, "#!/usr/bin/env bash")?;
        for (k, v) in request.env.iter() {
            // Keys are WDL declaration names, which are always valid shell
            // names.
            assert!(
                wdl_grammar::lexer::v1::is_ident(k),
                "environment variable name `{k}` is not a WDL identifier"
            );
            writeln!(
                &mut apptainer_command,
                "export {env_prefix}_{k}={}",
                quote_shell_arg(v)?
            )?;
        }
        writeln!(
            &mut apptainer_command,
            "{} -v exec \\",
            quote_shell_arg(&config.executable)?
        )?;
        writeln!(&mut apptainer_command, "--pwd \"{GUEST_WORK_DIR}\" \\")?;
        writeln!(&mut apptainer_command, "--containall --cleanenv \\")?;
        for input in request.backend_inputs {
            let host_path = input
                .local_path()
                .ok_or_else(|| anyhow!("input not localized: {input:?}"))?;
            let guest_path = input
                .guest_path()
                .ok_or_else(|| anyhow!("guest path missing: {input:?}"))?;
            writeln!(
                &mut apptainer_command,
                "--mount {} \\",
                quote_mount_arg(host_path, guest_path.as_str(), true)?
            )?;
        }
        for (source, destination, read_only) in [
            (request.command_path(), GUEST_COMMAND_PATH, true),
            (request.work_dir(), GUEST_WORK_DIR, false),
            (container_tmp_path, "/tmp", false),
            (container_var_tmp_path, "/var/tmp", false),
            (request.stdout_path(), GUEST_STDOUT_PATH, false),
            (request.stderr_path(), GUEST_STDERR_PATH, false),
        ] {
            writeln!(
                &mut apptainer_command,
                "--mount {} \\",
                quote_mount_arg(&source, destination, read_only)?
            )?;
        }

        if let Some(true) = request
            .requirements
            .get(wdl_ast::v1::TASK_REQUIREMENT_GPU)
            .and_then(Value::as_boolean)
        {
            writeln!(&mut apptainer_command, "--nv \\")?;
        }

        for arg in &config.extra_args {
            writeln!(&mut apptainer_command, "{} \\", quote_shell_arg(arg)?)?;
        }

        let image_path =
            absolute(image_path).context("failed to make container image path absolute")?;
        writeln!(
            &mut apptainer_command,
            "{} \\",
            quote_shell_arg(
                image_path
                    .to_str()
                    .context("container image path is not valid UTF-8")?
            )?
        )?;
        writeln!(
            &mut apptainer_command,
            "{} -c {} \\",
            quote_shell_arg(shell)?,
            quote_shell_arg(&format!(
                "\"{GUEST_COMMAND_PATH}\" > \"{GUEST_STDOUT_PATH}\" 2> \"{GUEST_STDERR_PATH}\""
            ))?,
        )?;
        let attempt_dir = request.attempt_dir;
        let apptainer_stdout_path = attempt_dir.join("apptainer.stdout");
        let apptainer_stderr_path = attempt_dir.join("apptainer.stderr");
        writeln!(
            &mut apptainer_command,
            "> {} 2> {}",
            quote_shell_arg(
                apptainer_stdout_path
                    .to_str()
                    .context("Apptainer stdout path is not valid UTF-8")?
            )?,
            quote_shell_arg(
                apptainer_stderr_path
                    .to_str()
                    .context("Apptainer stderr path is not valid UTF-8")?
            )?,
        )?;

        // Save the exit code so it can be read back after a restart if
        // the job completes while Sprocket is not running. The scheduler may no
        // longer report the job. Write to a temporary file and rename it so
        // that a reader never sees a partly written file.
        let exit_code_path = attempt_dir.join(APPTAINER_EXIT_CODE_FILE_NAME);
        writeln!(&mut apptainer_command, "rc=$?")?;
        writeln!(
            &mut apptainer_command,
            "echo \"$rc\" > \"{path}.tmp\" && mv -f \"{path}.tmp\" \"{path}\"",
            path = exit_code_path.display()
        )?;
        writeln!(&mut apptainer_command, "exit \"$rc\"")?;
        Ok(apptainer_command)
    }

    /// Pulls the image for the given image source and returns the path to
    /// the image file (SIF).
    ///
    /// If the image source is already a SIF file, the given source path is
    /// returned.
    ///
    /// If the image has already been pulled, the pull is skipped and the path
    /// to the previous location is returned.
    pub(crate) async fn pull_image(
        &self,
        executable: &str,
        image: &ImageSource,
        request: &ExecuteTaskRequest<'_>,
        task_id: TaskId,
    ) -> Result<Option<PathBuf>> {
        let events = request.context.events();
        let cancellation = request.context.cancellation().first();

        // For local SIF files, return the path directly.
        if let ImageSource::SifFile(path) = image {
            return Ok(Some(path.clone()));
        }

        // For unknown image sources, error early.
        if let ImageSource::Unknown(s) = image {
            bail!("unknown container image source `{s}`");
        }

        // For registry-based images, pull and cache.
        let uri = image.uri();

        // Hash the image's URI
        let mut hasher = Hasher::new();
        hasher.update(uri.as_bytes());
        let key = hasher.finalize().to_hex();

        // Format the path to the output image
        let mut path = self.cache_dir.join(key.as_str());
        path.add_extension("sif");

        // Take an exclusive lock to prevent another process from also
        // attempting a pull for this cache entry
        let _lock = LockedFile::acquire_exclusive(path.with_extension("lock")).await?;

        // If the image already exists, then a previous pull was successful
        if path.exists() {
            debug!(path = %path.display(), "Apptainer image `{image:#}` already cached; using existing image");
            return Ok(Some(path));
        }

        debug!(path = %path.display(), "pulling Apptainer image `{image:#}`");

        // Image doesn't exist, so pull it now
        let executable = executable.to_string();

        send_event!(
            events.crankshaft(),
            Event::ImagePullStarted {
                id: task_id,
                name: uri.clone()
            }
        );

        let pull = Retry::spawn_notify(
            // TODO ACF 2025-09-22: configure the retry behavior based on actual
            // experience with flakiness of the container registries. This is a
            // finger-in-the-wind guess at some reasonable parameters that
            // shouldn't lead to us making our own problems worse by
            // overwhelming registries with repeated retries.
            ExponentialBackoff::from_millis(50)
                .max_delay_millis(60_000)
                .take(10),
            || Self::try_pull_image(&executable, image, &path),
            {
                let executable = executable.clone();
                move |e: &anyhow::Error, _| {
                    warn!(e = %e, "`{executable} pull` failed");
                }
            },
        );

        tokio::select! {
            _ = cancellation.cancelled() => {
                send_event!(events.crankshaft(), Event::ImagePullFailed { id: task_id, name: uri, message: String::from("pull canceled") });
                Ok(None)
            },
            res = pull => match res.with_context(|| format!("failed pulling Apptainer image `{image:#}`")) {
                Ok(_) => {
                    send_event!(events.crankshaft(), Event::ImagePullFinished { id: task_id, name: image.uri() });
                    debug!(path = %path.display(), "Apptainer image `{image:#}` pulled successfully");
                    Ok(Some(path))
                },
                Err(e) => {
                    send_event!(events.crankshaft(), Event::ImagePullFailed { id: task_id, name: uri, message: format!("{e:#}") });
                    Err(e)
                },
            },
        }
    }

    /// Attempts to pull the first available image from a list of candidates.
    ///
    /// Iterates through the candidates in order, returning the path of the
    /// first image that pulls successfully. Returns a [`PullResults`]
    /// containing the outcome of each attempt, stopping after the first
    /// success. Returns `None` if a pull was cancelled.
    pub(crate) async fn pull_first_available_image(
        &self,
        executable: &str,
        request: &ExecuteTaskRequest<'_>,
        task_id: TaskId,
    ) -> Option<PullResults<PathBuf>> {
        let mut results = PullResults::default();

        for candidate in &request.constraints.sources {
            debug!("attempting to pull container image `{candidate:#}`");
            match self
                .pull_image(executable, candidate, request, task_id)
                .await
            {
                Ok(Some(path)) => {
                    debug!("successfully pulled container image `{candidate:#}`");
                    results.push(candidate.clone(), Ok(path));
                    return Some(results);
                }
                Ok(None) => return None,
                Err(e) => {
                    warn!("failed to pull container image `{candidate:#}`: {e:#}");
                    results.push(candidate.clone(), Err(e));
                }
            }
        }

        Some(results)
    }

    /// Tries to pull an image.
    ///
    /// The tricky thing about this function is determining whether a failure is
    /// transient or permanent. When in doubt, choose transient; the downside is
    /// a permanent failure may take longer to finally bring down an
    /// execution, but this is better for a long-running task than letting a
    /// transient failure bring it down before a retry.
    ///
    /// `apptainer pull` doesn't have a well-defined interface for us to tell
    /// whether a failure is transient, but as we gain experience recognizing
    /// its output patterns, we can enhance the fidelity of the error
    /// handling.
    async fn try_pull_image(
        executable: &str,
        image: &ImageSource,
        path: &Path,
    ) -> Result<(), RetryError<anyhow::Error>> {
        debug!("spawning `{executable}` to pull image `{image:#}`");

        let child = Command::new(executable)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .arg("pull")
            .arg(path)
            .arg(format!("{image:#}"))
            .spawn()
            .with_context(|| {
                format!(
                    "failed to spawn `{executable} pull '{path}' '{image:#}'`",
                    path = path.display()
                )
            })
            // If the system can't handle spawning a process, we're better off failing quickly
            .map_err(RetryError::permanent)?;

        let output = child
            .wait_with_output()
            .await
            .context(format!("failed to wait for `{executable}`"))
            .map_err(RetryError::permanent)?;
        if !output.status.success() {
            let permanent = if let Ok(stderr) = str::from_utf8(&output.stderr) {
                let mut permanent = false;
                // A collection of strings observed in `apptainer pull` stderr
                // in unrecoverable conditions. Finding one of
                // these in the output marks the attempt as a
                // permanent failure.
                let needles = ["manifest unknown", "403 (Forbidden)"];
                for needle in needles {
                    if stderr.contains(needle) {
                        permanent = true;
                        break;
                    }
                }

                permanent
            } else {
                false
            };

            let e = anyhow!(
                "`{executable}` failed: {status}: {stderr}",
                status = output.status,
                stderr = str::from_utf8(&output.stderr)
                    .unwrap_or("<output not UTF-8>")
                    .trim()
            );
            return if permanent {
                Err(RetryError::permanent(e))
            } else {
                Err(RetryError::transient(e))
            };
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crankshaft::events::next_task_id;
    use indexmap::IndexMap;
    use tempfile::TempDir;
    use url::Url;

    use super::*;
    use crate::ContentKind;
    use crate::EvaluationPath;
    use crate::Events;
    use crate::GuestPath;
    use crate::ONE_GIBIBYTE;
    use crate::Object;
    use crate::TaskInputs;
    use crate::backend::ExecuteTaskRequest;
    use crate::backend::Input;
    use crate::backend::TaskExecutionConstraints;
    use crate::backend::tests::EvalContext;
    use crate::config::DEFAULT_TASK_SHELL;

    async fn generate_test_script(
        root: &Path,
        config: &ApptainerConfig,
        shell: &str,
        image_path: &Path,
        env: &IndexMap<String, String>,
        backend_inputs: &[Input],
    ) -> Result<String> {
        tokio::fs::create_dir_all(root.join("0")).await?;
        let runtime = ApptainerRuntime::new(root.join("cache")).await?;
        let context = EvalContext::new(Events::default(), Default::default()).await;
        runtime
            .generate_apptainer_script(
                config,
                shell,
                image_path,
                &ExecuteTaskRequest {
                    context: &context,
                    name: "quoting-test",
                    command: "true",
                    inputs: &TaskInputs::default(),
                    backend_inputs,
                    requirements: &Object::empty(),
                    hints: &Object::empty(),
                    env,
                    constraints: &TaskExecutionConstraints {
                        sources: Vec::new(),
                        cpu: 1.0,
                        memory: ONE_GIBIBYTE as u64,
                        gpu: Default::default(),
                        fpga: Default::default(),
                        disks: Default::default(),
                    },
                    base_dir: &EvaluationPath::from_local_path(root.into()),
                    attempt_dir: &root.join("0"),
                    temp_dir: &root.join("temp"),
                    preemptions: 0,
                },
            )
            .await
    }

    #[cfg(unix)]
    async fn capturing_runtime(root: &Path, name: &str) -> ApptainerConfig {
        use std::os::unix::fs::PermissionsExt;

        let executable = root.join(name);
        tokio::fs::write(
            &executable,
            "#!/usr/bin/env bash\nprintf '%s\\0' \
             \"${APPTAINERENV_VALUE-${SINGULARITYENV_VALUE-}}\" \"$@\"\nprintf '%s' \
             runtime-stderr >&2\n",
        )
        .await
        .unwrap();
        tokio::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
            .await
            .unwrap();
        ApptainerConfig {
            executable: executable.to_str().unwrap().to_string(),
            ..Default::default()
        }
    }

    #[cfg(unix)]
    async fn capture_script_arguments(root: &Path, script: &str) -> Vec<String> {
        let script_path = root.join("script.sh");
        tokio::fs::write(&script_path, script).await.unwrap();
        let output = Command::new("bash")
            .arg(&script_path)
            .current_dir(root)
            .env("QUOTING_MARKER", "expanded")
            .env_remove("APPTAINERENV_VALUE")
            .env_remove("SINGULARITYENV_VALUE")
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "generated script failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            tokio::fs::read_to_string(root.join("0/apptainer.stderr"))
                .await
                .unwrap(),
            "runtime-stderr"
        );
        tokio::fs::read_to_string(root.join("0/apptainer.stdout"))
            .await
            .unwrap()
            .split_terminator('\0')
            .map(str::to_string)
            .collect()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn generated_environment_values_remain_literal() {
        for runtime_name in ["apptainer", "singularity"] {
            let root = TempDir::new().unwrap();
            let config = capturing_runtime(root.path(), runtime_name).await;
            for value in [
                "",
                "spaces ' \" $QUOTING_MARKER $(printf changed) `printf changed` \\\n\u{e9}",
            ] {
                let env = [("VALUE".to_string(), value.to_string())]
                    .into_iter()
                    .collect();
                let script = generate_test_script(
                    root.path(),
                    &config,
                    DEFAULT_TASK_SHELL,
                    &root.path().join("image.sif"),
                    &env,
                    &[],
                )
                .await
                .unwrap();
                let captured = capture_script_arguments(root.path(), &script).await;
                assert_eq!(captured[0], value, "runtime: {runtime_name}");
            }
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn generated_arguments_and_redirects_remain_literal() {
        let root = tempfile::Builder::new()
            .prefix("wdl $QUOTING_MARKER ")
            .tempdir()
            .unwrap();
        let mut config = capturing_runtime(root.path(), "apptainer ' $QUOTING_MARKER").await;
        config.extra_args = vec![
            "--env".to_string(),
            "EXTRA=spaces ' \" $QUOTING_MARKER $(printf changed)".to_string(),
            String::new(),
        ];
        let image_path = root
            .path()
            .join("image ' \" $QUOTING_MARKER $(printf changed).sif");
        let shell = "/bin/shell ' $QUOTING_MARKER";
        let input_path = root.path().join("input, \"quoted\":$QUOTING_MARKER");
        let inputs = [Input::new(
            ContentKind::File,
            EvaluationPath::from_local_path(input_path),
            Some(GuestPath::new("/mnt/input, \"quoted\":$QUOTING_MARKER")),
        )];
        let script = generate_test_script(
            root.path(),
            &config,
            shell,
            &image_path,
            &IndexMap::new(),
            &inputs,
        )
        .await
        .unwrap();
        let captured = capture_script_arguments(root.path(), &script).await;
        let args = &captured[1..];
        assert_eq!(
            &args[..6],
            [
                "-v",
                "exec",
                "--pwd",
                GUEST_WORK_DIR,
                "--containall",
                "--cleanenv"
            ]
        );
        assert_eq!(
            args[7],
            format!(
                "type=bind,\"src={}/input, \"\"quoted\"\":$QUOTING_MARKER\",\"dst=/mnt/input, \
                 \"\"quoted\"\":$QUOTING_MARKER\",ro",
                root.path().display()
            )
        );
        for (index, source, destination) in [
            (9, "0/command", GUEST_COMMAND_PATH),
            (11, "0/work", GUEST_WORK_DIR),
            (13, "temp/container_tmp", "/tmp"),
            (15, "temp/container_var_tmp", "/var/tmp"),
            (17, "0/stdout", GUEST_STDOUT_PATH),
            (19, "0/stderr", GUEST_STDERR_PATH),
        ] {
            assert_eq!(args[index - 1], "--mount");
            assert_eq!(
                args[index],
                format!(
                    "type=bind,\"src={}/{source}\",\"dst={destination}\"{}",
                    root.path().display(),
                    if index == 9 { ",ro" } else { "" },
                )
            );
        }
        assert_eq!(&args[20..23], config.extra_args);
        assert_eq!(args[23], image_path.to_str().unwrap());
        assert_eq!(args[24], shell);
        assert_eq!(args[25], "-c");
        assert_eq!(
            args[26],
            "\"/mnt/task/command\" > \"/mnt/task/stdout\" 2> \"/mnt/task/stderr\""
        );
        assert_eq!(args.len(), 27);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn relative_image_paths_cannot_be_runtime_options() {
        let root = TempDir::new().unwrap();
        let config = capturing_runtime(root.path(), "apptainer").await;
        let script = generate_test_script(
            root.path(),
            &config,
            DEFAULT_TASK_SHELL,
            Path::new("-image.sif"),
            &IndexMap::new(),
            &[],
        )
        .await
        .unwrap();
        let captured = capture_script_arguments(root.path(), &script).await;
        let image = Path::new(&captured[captured.len() - 4]);
        assert!(image.is_absolute());
        assert_eq!(image.file_name().unwrap(), "-image.sif");
    }

    #[tokio::test]
    async fn nul_in_environment_values_is_rejected() {
        let root = TempDir::new().unwrap();
        let env = [("VALID".to_string(), "contains\0nul".to_string())]
            .into_iter()
            .collect();
        assert!(
            generate_test_script(
                root.path(),
                &ApptainerConfig::default(),
                DEFAULT_TASK_SHELL,
                &root.path().join("image.sif"),
                &env,
                &[],
            )
            .await
            .is_err(),
            "environment value with NUL was accepted"
        );
    }

    #[tokio::test]
    #[should_panic(expected = "is not a WDL identifier")]
    async fn invalid_environment_names_panic() {
        let root = TempDir::new().unwrap();
        let env = [("BAD-NAME;id".to_string(), "value".to_string())]
            .into_iter()
            .collect();
        let _ = generate_test_script(
            root.path(),
            &ApptainerConfig::default(),
            DEFAULT_TASK_SHELL,
            &root.path().join("image.sif"),
            &env,
            &[],
        )
        .await;
    }

    #[tokio::test]
    async fn nul_in_runtime_arguments_is_rejected() {
        let root = TempDir::new().unwrap();
        let invalid = "contains\0nul";
        for (config, shell, image) in [
            (
                ApptainerConfig {
                    executable: invalid.to_string(),
                    ..Default::default()
                },
                DEFAULT_TASK_SHELL,
                root.path().join("image.sif"),
            ),
            (
                ApptainerConfig {
                    extra_args: vec![invalid.to_string()],
                    ..Default::default()
                },
                DEFAULT_TASK_SHELL,
                root.path().join("image.sif"),
            ),
            (
                ApptainerConfig::default(),
                invalid,
                root.path().join("image.sif"),
            ),
            (
                ApptainerConfig::default(),
                DEFAULT_TASK_SHELL,
                root.path().join(invalid),
            ),
        ] {
            assert!(
                generate_test_script(root.path(), &config, shell, &image, &IndexMap::new(), &[],)
                    .await
                    .is_err()
            );
        }
        let inputs = [Input::new(
            ContentKind::File,
            EvaluationPath::from_local_path(root.path().join("input")),
            Some(GuestPath::new(invalid)),
        )];
        assert!(
            generate_test_script(
                root.path(),
                &ApptainerConfig::default(),
                DEFAULT_TASK_SHELL,
                &root.path().join("image.sif"),
                &IndexMap::new(),
                &inputs,
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn recording_a_job_id_writes_the_file_and_announces_it() {
        let root = TempDir::new().unwrap();
        let events = Events::default().with_engine(10);
        let mut receiver = events.subscribe_engine().unwrap();

        record_job_id(&events, root.path(), "task-0", 42).await;

        assert_eq!(
            std::fs::read_to_string(root.path().join(JOB_ID_FILE_NAME)).unwrap(),
            "42\n"
        );
        let crate::EngineEvent::TaskSubmitted { name, external_id } = receiver.try_recv().unwrap()
        else {
            panic!("expected a `TaskSubmitted` event");
        };
        assert_eq!(name, "task-0");
        assert_eq!(external_id, "42");
    }

    #[tokio::test]
    async fn example_task_generates() {
        let root = TempDir::new().unwrap();

        let mut env = IndexMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        env.insert("BAZ".to_string(), "\"quux\"".to_string());

        let runtime = ApptainerRuntime::new(root.path().join("cache"))
            .await
            .unwrap();

        let context = EvalContext::new(Events::default(), Default::default()).await;
        let _ = runtime
            .generate_script(
                &ApptainerConfig::default(),
                DEFAULT_TASK_SHELL,
                &ExecuteTaskRequest {
                    context: &context,
                    name: "example-task-0",
                    command: "echo hello",
                    inputs: &TaskInputs::default(),
                    backend_inputs: &[],
                    requirements: &Object::empty(),
                    hints: &Object::empty(),
                    env: &env,
                    constraints: &TaskExecutionConstraints {
                        sources: vec![
                            String::from(
                                Url::from_file_path(root.path().join("non-existent.sif")).unwrap(),
                            )
                            .parse()
                            .unwrap(),
                        ],
                        cpu: 1.0,
                        memory: ONE_GIBIBYTE as u64,
                        gpu: Default::default(),
                        fpga: Default::default(),
                        disks: Default::default(),
                    },
                    base_dir: &EvaluationPath::from_local_path(root.path().into()),
                    attempt_dir: &root.path().join("0"),
                    temp_dir: &root.path().join("temp"),
                    preemptions: 0,
                },
                next_task_id(),
            )
            .await
            .inspect_err(|e| eprintln!("{e:#?}"))
            .expect("example task script should generate");
    }

    // `shellcheck` works quite differently on Windows, and since we're not
    // going to run Apptainer on Windows anytime soon, we limit this test to
    // Unixy systems
    #[cfg(unix)]
    #[tokio::test]
    async fn example_task_shellchecks() {
        use tokio::process::Command;

        use crate::Events;

        let root = TempDir::new().unwrap();

        let mut env = IndexMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        env.insert("BAZ".to_string(), "\"quux\"".to_string());

        let runtime = ApptainerRuntime::new(root.path().join("cache"))
            .await
            .unwrap();

        let context = EvalContext::new(Events::default(), Default::default()).await;
        let (script, _) = runtime
            .generate_script(
                &ApptainerConfig::default(),
                DEFAULT_TASK_SHELL,
                &ExecuteTaskRequest {
                    context: &context,
                    name: "example-task-0",
                    command: "echo hello",
                    inputs: &TaskInputs::default(),
                    backend_inputs: &[],
                    requirements: &Object::empty(),
                    hints: &Object::empty(),
                    env: &env,
                    constraints: &TaskExecutionConstraints {
                        sources: vec![
                            String::from(
                                Url::from_file_path(root.path().join("non-existent.sif")).unwrap(),
                            )
                            .parse()
                            .unwrap(),
                        ],
                        cpu: 1.0,
                        memory: ONE_GIBIBYTE as u64,
                        gpu: Default::default(),
                        fpga: Default::default(),
                        disks: Default::default(),
                    },
                    base_dir: &EvaluationPath::from_local_path(root.path().into()),
                    attempt_dir: &root.path().join("0"),
                    temp_dir: &root.path().join("temp"),
                    preemptions: 0,
                },
                next_task_id(),
            )
            .await
            .inspect_err(|e| eprintln!("{e:#?}"))
            .expect("example task script should generate")
            .expect("operation should not be canceled");
        let script_file = root.path().join("apptainer_script");
        tokio::fs::write(&script_file, &script)
            .await
            .expect("can write script to disk");
        let shellcheck_status = Command::new("shellcheck")
            .arg("--shell=bash")
            .arg("--severity=style")
            // all the quotes in the generated `--mount` args look suspicious but are okay
            .arg("--exclude=SC2140")
            .arg(&script_file)
            .status()
            .await
            .unwrap();
        assert!(shellcheck_status.success());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn script_records_the_exit_code() {
        use std::os::unix::fs::PermissionsExt;

        let root = TempDir::new().unwrap();
        let attempt_dir = root.path().join("0");
        std::fs::create_dir_all(&attempt_dir).unwrap();

        let fake_apptainer = root.path().join("apptainer");
        std::fs::write(&fake_apptainer, "#!/bin/sh\nexit 3\n").unwrap();
        std::fs::set_permissions(&fake_apptainer, std::fs::Permissions::from_mode(0o755)).unwrap();

        let runtime = ApptainerRuntime::new(root.path().join("cache"))
            .await
            .unwrap();
        let config = ApptainerConfig {
            executable: fake_apptainer.display().to_string(),
            ..Default::default()
        };
        let context = EvalContext::new(Events::default(), Default::default()).await;
        let script = runtime
            .generate_apptainer_script(
                &config,
                DEFAULT_TASK_SHELL,
                &root.path().join("image.sif"),
                &ExecuteTaskRequest {
                    context: &context,
                    name: "example-task-0",
                    command: "exit 3",
                    inputs: &TaskInputs::default(),
                    backend_inputs: &[],
                    requirements: &Object::empty(),
                    hints: &Object::empty(),
                    env: &IndexMap::new(),
                    constraints: &TaskExecutionConstraints {
                        sources: Vec::new(),
                        cpu: 1.0,
                        memory: ONE_GIBIBYTE as u64,
                        gpu: Default::default(),
                        fpga: Default::default(),
                        disks: Default::default(),
                    },
                    base_dir: &EvaluationPath::from_local_path(root.path().into()),
                    attempt_dir: &attempt_dir,
                    temp_dir: &root.path().join("temp"),
                    preemptions: 0,
                },
            )
            .await
            .unwrap();

        let script_file = root.path().join("apptainer_script");
        std::fs::write(&script_file, script).unwrap();
        let status = tokio::process::Command::new("bash")
            .arg(&script_file)
            .status()
            .await
            .unwrap();

        assert_eq!(status.code(), Some(3));
        assert_eq!(
            std::fs::read_to_string(attempt_dir.join(APPTAINER_EXIT_CODE_FILE_NAME)).unwrap(),
            "3\n"
        );
        assert!(
            !attempt_dir
                .join(format!("{APPTAINER_EXIT_CODE_FILE_NAME}.tmp"))
                .exists()
        );
    }
}
