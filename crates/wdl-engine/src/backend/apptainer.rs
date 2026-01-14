//! Support for using Apptainer (a.k.a. Singularity) container runtime.
//!
//! There are two primary responsibilities of this module: `.sif` image cache
//! management and command script generation.
//!
//! The entrypoint for both of these is [`ApptainerRuntime::generate_script`].

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Context as _;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command;
use tokio::sync::OnceCell;
use tokio_retry2::Retry;
use tokio_retry2::RetryError;
use tokio_retry2::strategy::ExponentialBackoff;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing::trace;
use tracing::warn;

use crate::Value;
use crate::backend::ExecuteTaskRequest;
use crate::config::Config;
use crate::config::DEFAULT_TASK_SHELL;
use crate::v1::requirements::ContainerSource;

/// The name of the images cache directory.
const IMAGES_CACHE_DIR: &str = "apptainer-images";

/// The guest working directory.
const GUEST_WORK_DIR: &str = "/mnt/task/work";

/// The guest path for the command file.
const GUEST_COMMAND_PATH: &str = "/mnt/task/command";

/// The path to the container's stdout.
const GUEST_STDOUT_PATH: &str = "/mnt/task/stdout";

/// The path to the container's stderr.
const GUEST_STDERR_PATH: &str = "/mnt/task/stderr";

/// Represents the Apptainer container runtime.
#[derive(Debug)]
pub struct ApptainerRuntime {
    /// The cache directory for `.sif` images.
    cache_dir: PathBuf,
    /// The map of container source to `.sif` path.
    images: Mutex<HashMap<ContainerSource, Arc<OnceCell<PathBuf>>>>,
}

impl ApptainerRuntime {
    /// Creates a new [`ApptainerRuntime`] with the specified root directory.
    ///
    /// An images cache directory will be created in the given root.
    pub fn new(root_dir: &Path) -> Self {
        Self {
            cache_dir: root_dir.join(IMAGES_CACHE_DIR),
            images: Default::default(),
        }
    }

    /// Generates the script to run the given task using the Apptainer runtime.
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
        config: &Config,
        request: &ExecuteTaskRequest<'_>,
        extra_args: impl Iterator<Item = &str>,
        token: CancellationToken,
    ) -> Result<Option<String>> {
        let path = match self
            .pull_image(
                request
                    .constraints
                    .container
                    .as_ref()
                    .ok_or_else(|| anyhow!("task does not use a container"))?,
                token,
            )
            .await?
        {
            Some(path) => path,
            None => return Ok(None),
        };

        Ok(Some(
            self.generate_apptainer_script(config, &path, request, extra_args)
                .await?,
        ))
    }

    /// Generate the script, given a container path that's already assumed to be
    /// populated.
    ///
    /// This is a separate method in order to facilitate testing, and should not
    /// be called from outside this module.
    async fn generate_apptainer_script(
        &self,
        config: &Config,
        container_sif: &Path,
        request: &ExecuteTaskRequest<'_>,
        extra_args: impl Iterator<Item = &str>,
    ) -> Result<String> {
        // Create a temp dir for the container's execution within the attempt dir
        // hierarchy. On many HPC systems, `/tmp` is mapped to a relatively
        // small, local scratch disk that can fill up easily. Mapping the
        // container's `/tmp` and `/var/tmp` paths to the filesystem we're using
        // for other inputs and outputs prevents this from being a capacity problem,
        // though potentially at the expense of execution speed if the
        // non-`/tmp` filesystem is significantly slower.
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

        let mut apptainer_command = String::new();
        writeln!(&mut apptainer_command, "#!/usr/bin/env bash")?;
        for (k, v) in request.env.iter() {
            writeln!(&mut apptainer_command, "export APPTAINERENV_{k}={v:?}")?;
        }
        writeln!(&mut apptainer_command, "apptainer -v exec \\")?;
        writeln!(&mut apptainer_command, "--pwd \"{GUEST_WORK_DIR}\" \\")?;
        writeln!(&mut apptainer_command, "--containall --cleanenv \\")?;
        for input in request.backend_inputs {
            writeln!(
                &mut apptainer_command,
                "--mount type=bind,src=\"{host_path}\",dst=\"{guest_path}\",ro \\",
                host_path = input
                    .local_path()
                    .ok_or_else(|| anyhow!("input not localized: {input:?}"))?
                    .display(),
                guest_path = input
                    .guest_path()
                    .ok_or_else(|| anyhow!("guest path missing: {input:?}"))?,
            )?;
        }
        writeln!(
            &mut apptainer_command,
            "--mount type=bind,src=\"{}\",dst=\"{GUEST_COMMAND_PATH}\",ro \\",
            request.command_path().display()
        )?;
        writeln!(
            &mut apptainer_command,
            "--mount type=bind,src=\"{}\",dst=\"{GUEST_WORK_DIR}\" \\",
            request.work_dir().display()
        )?;
        writeln!(
            &mut apptainer_command,
            "--mount type=bind,src=\"{}\",dst=\"/tmp\" \\",
            container_tmp_path.display()
        )?;
        writeln!(
            &mut apptainer_command,
            "--mount type=bind,src=\"{}\",dst=\"/var/tmp\" \\",
            container_var_tmp_path.display()
        )?;
        writeln!(
            &mut apptainer_command,
            "--mount type=bind,src=\"{}\",dst=\"{GUEST_STDOUT_PATH}\" \\",
            request.stdout_path().display()
        )?;
        writeln!(
            &mut apptainer_command,
            "--mount type=bind,src=\"{}\",dst=\"{GUEST_STDERR_PATH}\" \\",
            request.stderr_path().display()
        )?;

        if let Some(true) = request
            .requirements
            .get(wdl_ast::v1::TASK_REQUIREMENT_GPU)
            .and_then(Value::as_boolean)
        {
            writeln!(&mut apptainer_command, "--nv \\")?;
        }

        for arg in extra_args {
            writeln!(&mut apptainer_command, "{arg} \\")?;
        }

        writeln!(&mut apptainer_command, "\"{}\" \\", container_sif.display())?;
        writeln!(
            &mut apptainer_command,
            "{shell} -c \"\\\"{GUEST_COMMAND_PATH}\\\" > \\\"{GUEST_STDOUT_PATH}\\\" 2> \
             \\\"{GUEST_STDERR_PATH}\\\"\" \\",
            shell = config.task.shell.as_deref().unwrap_or(DEFAULT_TASK_SHELL)
        )?;
        let attempt_dir = request.attempt_dir;
        let apptainer_stdout_path = attempt_dir.join("apptainer.stdout");
        let apptainer_stderr_path = attempt_dir.join("apptainer.stderr");
        writeln!(
            &mut apptainer_command,
            "> \"{stdout}\" 2> \"{stderr}\"",
            stdout = apptainer_stdout_path.display(),
            stderr = apptainer_stderr_path.display()
        )?;
        Ok(apptainer_command)
    }

    /// Pulls the image for the given container source and returns the path to
    /// the image file (SIF).
    ///
    /// If the container source is already a SIF file, the given source path is
    /// returned.
    ///
    /// If the image has already been pulled, the pull is skipped and the path
    /// to the previous location is returned.
    pub(crate) async fn pull_image(
        &self,
        container: &ContainerSource,
        token: CancellationToken,
    ) -> Result<Option<PathBuf>> {
        // For local SIF files, return the path directly.
        if let ContainerSource::SifFile(path) = container {
            return Ok(Some(path.clone()));
        }

        // For unknown container sources, error early.
        if let ContainerSource::Unknown(s) = container {
            bail!("unknown container source `{s}`");
        }

        // For registry-based images, pull and cache.
        let once = {
            let mut map = self.images.lock().unwrap();
            map.entry(container.clone())
                .or_insert_with(|| Arc::new(OnceCell::new()))
                .clone()
        };

        let pull = once.get_or_try_init(|| async move {
            // SAFETY: the next two `unwrap` calls are safe because the source can't be a
            // file or an unknown source at this point
            let mut path = self.cache_dir.join(container.scheme().unwrap());
            for part in container.name().unwrap().split("/") {
                for part in part.split(':') {
                    path.push(part);
                }
            }

            path.set_extension("sif");

            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await.with_context(|| {
                    format!(
                        "failed to create directory `{parent}`",
                        parent = parent.display()
                    )
                })?;
            }

            let container = format!("{container:#}");

            Retry::spawn_notify(
                // TODO ACF 2025-09-22: configure the retry behavior based on actual experience
                // with flakiness of the container registries. This is a
                // finger-in-the-wind guess at some reasonable parameters that
                // shouldn't lead to us making our own problems worse by
                // overwhelming registries with repeated retries.
                ExponentialBackoff::from_millis(50)
                    .max_delay_millis(60_000)
                    .take(10),
                || Self::try_pull_image(&container, &path),
                |e, _| {
                    warn!(e = %e, "`apptainer pull` failed");
                },
            )
            .await
            .with_context(|| format!("failed pulling Apptainer image `{container}`"))?;

            info!(path = %path.display(), "Apptainer image `{container}` pulled successfully");
            Ok(path)
        });

        tokio::select! {
            _ = token.cancelled() => Ok(None),
            res = pull => res.map(|p| Some(p.clone())),
        }
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
    async fn try_pull_image(image: &str, path: &Path) -> Result<(), RetryError<anyhow::Error>> {
        info!(image, "pulling image");

        // Pipe the stdio handles, both for tracing and to inspect for telltale signs of
        // permanent errors
        let mut apptainer_pull_child = Command::new("apptainer")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .arg("pull")
            .arg(path)
            .arg(image)
            .spawn()
            .with_context(|| {
                format!(
                    "failed to spawn `apptainer pull '{path}' '{image}'",
                    path = path.display()
                )
            })
            // If the system can't handle spawning a process, we're better off failing quickly
            .map_err(RetryError::permanent)?;

        let is_permanent = Arc::new(Mutex::new(false));

        let child_stdout = apptainer_pull_child
            .stdout
            .take()
            .ok_or_else(|| RetryError::permanent(anyhow!("apptainer pull child stdout missing")))?;
        let stdout_image: String = image.into();
        let _stdout_is_permanent = is_permanent.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(child_stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                trace!(line = line, image = stdout_image, "`apptainer` stdout");
            }
        });
        let child_stderr = apptainer_pull_child
            .stderr
            .take()
            .ok_or_else(|| RetryError::permanent(anyhow!("apptainer pull child stderr missing")))?;
        let stderr_image: String = image.into();
        let stderr_is_permanent = is_permanent.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(child_stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                // A collection of strings observed in `apptainer pull` stderr in unrecoverable
                // conditions. Finding one of these in the output marks the attempt as a
                // permanent failure.
                let needles = ["manifest unknown", "403 (Forbidden)"];
                for needle in needles {
                    if line.contains(needle) {
                        trace!(line = line, image = stderr_image, "`apptainer` stderr");
                        *stderr_is_permanent.lock().unwrap() = true;
                        break;
                    }
                }
                trace!(stderr = line, image = stderr_image);
            }
        });

        let child_result = apptainer_pull_child
            .wait()
            .await
            .context("failed to wait for `apptainer` to exit")
            // Permanently error if something goes wrong trying to wait for the child process
            .map_err(RetryError::permanent)?;

        if !child_result.success() {
            let e = anyhow!(
                "`apptainer pull` failed with exit code {:?}",
                child_result.code()
            );
            return if *is_permanent.lock().unwrap() {
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
    use std::iter::empty;

    use indexmap::IndexMap;
    use tempfile::TempDir;
    use url::Url;

    use super::*;
    use crate::ONE_GIBIBYTE;
    use crate::TaskInputs;
    use crate::backend::ExecuteTaskRequest;
    use crate::backend::TaskExecutionConstraints;

    #[tokio::test]
    async fn example_task_generates() {
        let root = TempDir::new().unwrap();

        let mut env = IndexMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        env.insert("BAZ".to_string(), "\"quux\"".to_string());

        let runtime = ApptainerRuntime::new(&root.path().join("runs"));
        let _ = runtime
            .generate_script(
                &Default::default(),
                &ExecuteTaskRequest {
                    id: "example-task",
                    command: "echo hello",
                    inputs: &TaskInputs::default(),
                    backend_inputs: &[],
                    requirements: &Default::default(),
                    hints: &Default::default(),
                    env: &env,
                    constraints: &TaskExecutionConstraints {
                        container: Some(
                            String::from(
                                Url::from_file_path(root.path().join("non-existent.sif")).unwrap(),
                            )
                            .parse()
                            .unwrap(),
                        ),
                        cpu: 1.0,
                        memory: ONE_GIBIBYTE as u64,
                        gpu: Default::default(),
                        fpga: Default::default(),
                        disks: Default::default(),
                    },
                    attempt_dir: &root.path().join("0"),
                    temp_dir: &root.path().join("temp"),
                },
                empty(),
                CancellationToken::new(),
            )
            .await
            .inspect_err(|e| eprintln!("{e:#?}"))
            .expect("example task script should generate");
    }

    // `shellcheck` works quite differently on Windows, and since we're not going to
    // run Apptainer on Windows anytime soon, we limit this test to Unixy
    // systems
    #[cfg(unix)]
    #[tokio::test]
    async fn example_task_shellchecks() {
        use tokio::process::Command;

        let root = TempDir::new().unwrap();

        let mut env = IndexMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        env.insert("BAZ".to_string(), "\"quux\"".to_string());

        let runtime = ApptainerRuntime::new(&root.path().join("runs"));
        let script = runtime
            .generate_script(
                &Default::default(),
                &ExecuteTaskRequest {
                    id: "example-task",
                    command: "echo hello",
                    inputs: &TaskInputs::default(),
                    backend_inputs: &[],
                    requirements: &Default::default(),
                    hints: &Default::default(),
                    env: &env,
                    constraints: &TaskExecutionConstraints {
                        container: Some(
                            String::from(
                                Url::from_file_path(root.path().join("non-existent.sif")).unwrap(),
                            )
                            .parse()
                            .unwrap(),
                        ),
                        cpu: 1.0,
                        memory: ONE_GIBIBYTE as u64,
                        gpu: Default::default(),
                        fpga: Default::default(),
                        disks: Default::default(),
                    },
                    attempt_dir: &root.path().join("0"),
                    temp_dir: &root.path().join("temp"),
                },
                empty(),
                CancellationToken::new(),
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
}
