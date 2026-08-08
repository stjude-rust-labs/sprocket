//! Support for using Apptainer (a.k.a. Singularity) container runtime.
//!
//! There are two primary responsibilities of this module: `.sif` image cache
//! management and command script generation.
//!
//! The entrypoint for both of these is [`ApptainerRuntime::generate_script`].

use std::fmt::Write as _;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context as _;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use tracing::warn;

use crate::Value;
use crate::backend::ExecuteTaskRequest;
use crate::backend::PullResults;
use crate::config::ApptainerConfig;
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

/// The environment variable prefix for Apptainer.
const APPTAINER_ENV_PREFIX: &str = "APPTAINERENV";

/// The environment variable prefix for Singularity.
const SINGULARITY_ENV_PREFIX: &str = "SINGULARITYENV";

mod image_cache;

use image_cache::ApptainerImageCache;

/// Represents the Apptainer container runtime.
#[derive(Debug)]
pub struct ApptainerRuntime {
    /// The coordinator for the runtime's `.sif` image cache.
    ///
    /// Shared with every other runtime in this process constructed with the
    /// same `image_cache_dir`, and coordinated with runtimes in other
    /// processes that share the same cache directory on disk.
    image_cache: Arc<ApptainerImageCache>,
}

impl ApptainerRuntime {
    /// Creates a new [`ApptainerRuntime`] with the specified root directory.
    ///
    /// If `config.image_cache_dir` is set, it is used as the directory for
    /// caching `.sif` images, shared with every other runtime constructed
    /// with the same directory. Otherwise, a default subdirectory of
    /// `root_dir` is used, and the cache is not shared with a runtime
    /// constructed from a different `root_dir`.
    pub async fn new(root_dir: &Path, config: &ApptainerConfig) -> Result<Self> {
        let cache_dir = config
            .image_cache_dir
            .clone()
            .unwrap_or_else(|| root_dir.join(IMAGES_CACHE_DIR));

        Ok(Self {
            image_cache: ApptainerImageCache::get(&cache_dir, config.max_concurrent_pulls).await?,
        })
    }

    /// Generates the script to run the given task using the Apptainer runtime.
    ///
    /// Returns the generated script along with the [`ContainerSource`] that
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
        token: CancellationToken,
    ) -> Result<Option<(String, ContainerSource)>> {
        let results = match self
            .pull_first_available_image(
                &config.executable,
                request
                    .constraints
                    .container
                    .as_deref()
                    .ok_or_else(|| anyhow!("task does not use a container"))?,
                token,
            )
            .await
        {
            Some(results) => results,
            None => return Ok(None),
        };

        let (container, path) = results
            .successful_container()
            .ok_or_else(|| anyhow!("{results}"))?;
        let container = container.clone();
        let path = path.clone();

        Ok(Some((
            self.generate_apptainer_script(config, shell, &path, request)
                .await?,
            container,
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
        container_sif: &Path,
        request: &ExecuteTaskRequest<'_>,
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

        let env_prefix = if config.executable.contains("singularity") {
            SINGULARITY_ENV_PREFIX
        } else {
            APPTAINER_ENV_PREFIX
        };

        let mut apptainer_command = String::new();
        writeln!(&mut apptainer_command, "#!/usr/bin/env bash")?;
        for (k, v) in request.env.iter() {
            writeln!(&mut apptainer_command, "export {env_prefix}_{k}={v:?}")?;
        }
        writeln!(&mut apptainer_command, "{} -v exec \\", config.executable)?;
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

        for arg in &config.extra_args {
            writeln!(&mut apptainer_command, "{arg} \\")?;
        }

        writeln!(&mut apptainer_command, "\"{}\" \\", container_sif.display())?;
        writeln!(
            &mut apptainer_command,
            "{shell} -c \"\\\"{GUEST_COMMAND_PATH}\\\" > \\\"{GUEST_STDOUT_PATH}\\\" 2> \
             \\\"{GUEST_STDERR_PATH}\\\"\" \\"
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
        executable: &str,
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

        // For registry-based images, delegate coordination and the actual pull to
        // the shared image cache.
        let final_path = Self::registry_image_path(self.image_cache.cache_dir(), container);
        self.image_cache
            .pull(executable, container, &final_path, token)
            .await
    }

    /// Computes the final `.sif` path for a registry-based `container` within
    /// `cache_dir`.
    ///
    /// The layout is `cache_dir/<scheme>/<name-parts>.sif`, splitting the
    /// container's name on `/` and `:` so a path segment, tag, or digest each
    /// becomes its own directory component. This layout must stay
    /// byte-for-byte identical to what earlier engine releases produced,
    /// since an existing cache directory may already contain images at these
    /// paths.
    fn registry_image_path(cache_dir: &Path, container: &ContainerSource) -> PathBuf {
        // SAFETY: the next two `unwrap` calls are safe because callers only reach
        // this helper for registry sources (`Docker`, `Library`, `Oras`); local
        // `SifFile` and `Unknown` sources are handled earlier in `pull_image` and
        // never reach this helper.
        let mut path = cache_dir.join(container.scheme().unwrap());
        for part in container.name().unwrap().split("/") {
            for part in part.split(':') {
                path.push(part);
            }
        }

        path.add_extension("sif");
        path
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
        candidates: &[ContainerSource],
        token: CancellationToken,
    ) -> Option<PullResults<PathBuf>> {
        let mut results = PullResults::default();

        for candidate in candidates {
            debug!("attempting to pull container image `{candidate:#}`");
            match self.pull_image(executable, candidate, token.clone()).await {
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
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;
    use tempfile::TempDir;
    use url::Url;

    use super::*;
    use crate::EvaluationPath;
    use crate::ONE_GIBIBYTE;
    use crate::Object;
    use crate::TaskInputs;
    use crate::backend::ExecuteTaskRequest;
    use crate::backend::TaskExecutionConstraints;
    use crate::config::DEFAULT_TASK_SHELL;

    #[tokio::test]
    async fn shared_image_cache() {
        let root_a = TempDir::new().unwrap();
        let root_b = TempDir::new().unwrap();
        let shared_cache_dir = TempDir::new().unwrap();

        let config = ApptainerConfig {
            image_cache_dir: Some(shared_cache_dir.path().to_path_buf()),
            ..Default::default()
        };

        let runtime_a = ApptainerRuntime::new(&root_a.path().join("runs"), &config)
            .await
            .unwrap();
        let runtime_b = ApptainerRuntime::new(&root_b.path().join("runs"), &config)
            .await
            .unwrap();

        assert!(
            Arc::ptr_eq(&runtime_a.image_cache, &runtime_b.image_cache),
            "runtimes sharing `image_cache_dir` should reuse the same process-wide coordinator"
        );
    }

    #[tokio::test]
    async fn per_run_image_cache() {
        let root_a = TempDir::new().unwrap();
        let root_b = TempDir::new().unwrap();
        let config = ApptainerConfig::default();

        let runtime_a = ApptainerRuntime::new(&root_a.path().join("runs"), &config)
            .await
            .unwrap();
        let runtime_b = ApptainerRuntime::new(&root_b.path().join("runs"), &config)
            .await
            .unwrap();

        assert!(
            !Arc::ptr_eq(&runtime_a.image_cache, &runtime_b.image_cache),
            "runtimes with different run roots and no shared `image_cache_dir` should not reuse a \
             coordinator"
        );
        assert_eq!(
            runtime_a.image_cache.cache_dir(),
            root_a.path().join("runs").join(IMAGES_CACHE_DIR),
            "runtime_a's cache directory should be exactly `<run_root>/apptainer-images`"
        );
        assert_eq!(
            runtime_b.image_cache.cache_dir(),
            root_b.path().join("runs").join(IMAGES_CACHE_DIR),
            "runtime_b's cache directory should be exactly `<run_root>/apptainer-images`"
        );
    }

    #[tokio::test]
    async fn registry_image_path_matches_legacy_layout() {
        let root = TempDir::new().unwrap();
        let runtime = ApptainerRuntime::new(&root.path().join("runs"), &ApptainerConfig::default())
            .await
            .unwrap();

        let container: ContainerSource = "docker://ubuntu:latest".parse().unwrap();
        let path =
            ApptainerRuntime::registry_image_path(runtime.image_cache.cache_dir(), &container);

        assert_eq!(
            path,
            runtime
                .image_cache
                .cache_dir()
                .join("docker")
                .join("ubuntu")
                .join("latest.sif"),
            "`docker://ubuntu:latest` must resolve to `<cache>/docker/ubuntu/latest.sif`, \
             matching the on-disk layout produced by earlier engine releases"
        );
    }

    /// Restores `path`'s permissions to `mode`.
    ///
    /// Used so a deliberately read-only cache directory can be made writable
    /// again before its [`TempDir`] is dropped.
    #[cfg(unix)]
    fn set_mode(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(mode);
        std::fs::set_permissions(path, perms).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn existing_image_resolves_from_a_read_only_cache() {
        // A shared cache directory that a run may read but not write is a real
        // deployment shape: an administrator populates the cache and exposes it
        // read-only to every compute node. Constructing a runtime must not write
        // anything to the cache, and an image already present in the legacy layout
        // must resolve without creating the coordination directory.
        let root = TempDir::new().unwrap();
        let cache_dir = TempDir::new().unwrap();

        let container: ContainerSource = "docker://ubuntu:latest".parse().unwrap();
        let final_path = ApptainerRuntime::registry_image_path(cache_dir.path(), &container);
        std::fs::create_dir_all(final_path.parent().unwrap()).unwrap();
        std::fs::write(&final_path, b"legacy sif").unwrap();

        set_mode(cache_dir.path(), 0o555);

        let config = ApptainerConfig {
            image_cache_dir: Some(cache_dir.path().to_path_buf()),
            ..Default::default()
        };

        // Every fallible step runs inside this block so the cache directory is
        // always made writable again before the assertions run, keeping the
        // `TempDir` cleanup working even when an assertion fails.
        let outcome = async {
            let runtime = ApptainerRuntime::new(&root.path().join("runs"), &config).await?;
            let hit = runtime
                .pull_image(
                    "this-executable-must-never-run",
                    &container,
                    CancellationToken::new(),
                )
                .await?;
            let miss = runtime
                .pull_image(
                    "this-executable-must-never-run",
                    &"docker://ubuntu:absent".parse::<ContainerSource>().unwrap(),
                    CancellationToken::new(),
                )
                .await;
            anyhow::Ok((hit, miss))
        }
        .await;

        let coordination_dir_exists = cache_dir
            .path()
            .join(image_cache::COORDINATION_DIR_NAME)
            .exists();
        set_mode(cache_dir.path(), 0o755);

        let (hit, miss) = outcome.expect("a read-only cache must still serve an existing image");
        assert_eq!(
            hit.expect("the existing image should resolve"),
            final_path,
            "an image already present in the cache must resolve to its existing path"
        );
        assert!(
            !coordination_dir_exists,
            "serving an existing image must not create the cache coordination directory"
        );

        let error = format!(
            "{error:#}",
            error = miss.expect_err("a cache miss in a read-only cache must fail clearly")
        );
        assert!(
            error.contains("Apptainer image cache"),
            "a read-only cache miss should report why coordination could not start: {error}"
        );
    }

    #[tokio::test]
    async fn example_task_generates() {
        let root = TempDir::new().unwrap();

        let mut env = IndexMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        env.insert("BAZ".to_string(), "\"quux\"".to_string());

        let runtime = ApptainerRuntime::new(&root.path().join("runs"), &ApptainerConfig::default())
            .await
            .unwrap();
        let _ = runtime
            .generate_script(
                &ApptainerConfig::default(),
                DEFAULT_TASK_SHELL,
                &ExecuteTaskRequest {
                    id: "example-task",
                    command: "echo hello",
                    inputs: &TaskInputs::default(),
                    backend_inputs: &[],
                    requirements: &Object::empty(),
                    hints: &Object::empty(),
                    env: &env,
                    constraints: &TaskExecutionConstraints {
                        container: Some(vec![
                            String::from(
                                Url::from_file_path(root.path().join("non-existent.sif")).unwrap(),
                            )
                            .parse()
                            .unwrap(),
                        ]),
                        cpu: 1.0,
                        memory: ONE_GIBIBYTE as u64,
                        gpu: Default::default(),
                        fpga: Default::default(),
                        disks: Default::default(),
                    },
                    base_dir: &EvaluationPath::from_local_path(root.path().into()),
                    attempt_dir: &root.path().join("0"),
                    temp_dir: &root.path().join("temp"),
                },
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

        let runtime = ApptainerRuntime::new(&root.path().join("runs"), &ApptainerConfig::default())
            .await
            .unwrap();
        let (script, _) = runtime
            .generate_script(
                &ApptainerConfig::default(),
                DEFAULT_TASK_SHELL,
                &ExecuteTaskRequest {
                    id: "example-task",
                    command: "echo hello",
                    inputs: &TaskInputs::default(),
                    backend_inputs: &[],
                    requirements: &Object::empty(),
                    hints: &Object::empty(),
                    env: &env,
                    constraints: &TaskExecutionConstraints {
                        container: Some(vec![
                            String::from(
                                Url::from_file_path(root.path().join("non-existent.sif")).unwrap(),
                            )
                            .parse()
                            .unwrap(),
                        ]),
                        cpu: 1.0,
                        memory: ONE_GIBIBYTE as u64,
                        gpu: Default::default(),
                        fpga: Default::default(),
                        disks: Default::default(),
                    },
                    base_dir: &EvaluationPath::from_local_path(root.path().into()),
                    attempt_dir: &root.path().join("0"),
                    temp_dir: &root.path().join("temp"),
                },
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
