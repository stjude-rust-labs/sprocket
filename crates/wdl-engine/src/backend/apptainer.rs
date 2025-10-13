#![allow(clippy::missing_docs_in_private_items)]

//! Support for using Apptainer (a.k.a. Singularity) as an in-place container
//! runtime for task execution.
//!
//! There are two primary responsibilities of this module: `.sif` image cache
//! management and command script generation. The entrypoint for both of these
//! is [`ApptainerConfig::prepare_apptainer_command()`].

use std::fmt::Write as _;
use std::path::Path;

use anyhow::Context as _;
use anyhow::anyhow;
use images::sif_for_container;
use tokio_util::sync::CancellationToken;

use super::TaskSpawnRequest;
use crate::Value;

mod images;

/// The guest working directory.
const GUEST_WORK_DIR: &str = "/mnt/task/work";

/// The guest path for the command file.
const GUEST_COMMAND_PATH: &str = "/mnt/task/command";

/// The path to the container's stdout.
const GUEST_STDOUT_PATH: &str = "/mnt/task/stdout";

/// The path to the container's stderr.
const GUEST_STDERR_PATH: &str = "/mnt/task/stderr";

/// Configuration for the Apptainer container runtime.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ApptainerConfig {
    /// Additional command-line arguments to pass to `apptainer exec` when
    /// executing tasks.
    pub extra_apptainer_exec_args: Option<Vec<String>>,
    /// The directory in which temporary directories will be created containing
    /// Apptainer `.sif` files.
    ///
    /// This should be a location that is accessible by all locations where a
    /// task may be executed via Apptainer.
    ///
    /// By default, this is `$HOME/.cache/sprocket-apptainer-images`, or
    /// `/tmp/sprocket-apptainer-images` if the home directory cannot be
    /// determined.
    ///
    /// Shell-expansion is performed on this path before use, so configurations
    /// can contain environment variables. Note that these are expanded on
    /// the host where this code is executing, which may be different from
    /// hosts where a scheduler may dispatch tasks for execution. Errors
    /// will occur if the same path is not accessible in both environments.
    #[serde(default = "default_apptainer_images_dir")]
    pub apptainer_images_dir: String,
}

fn default_apptainer_images_dir() -> String {
    if let Some(cache) = dirs::cache_dir() {
        cache
            .join("sprocket-apptainer-images")
            .display()
            .to_string()
    } else {
        std::env::temp_dir()
            .join("sprocket-apptainer-images")
            .display()
            .to_string()
    }
}

impl Default for ApptainerConfig {
    fn default() -> Self {
        Self {
            extra_apptainer_exec_args: None,
            apptainer_images_dir: default_apptainer_images_dir(),
        }
    }
}

impl ApptainerConfig {
    /// Prepare for an Apptainer execution by ensuring the image cache is
    /// populated with the necessary container, and return a Bash script
    /// that invokes the task's `command` in the container context.
    ///
    /// # Shared filesystem assumptions
    ///
    /// The returned script should be run in an environment that shares a
    /// filesystem with the environment where this method is invoked, except
    /// for node-specific mounts like `/tmp` and `/var`. This assumption
    /// typically holds on HPC systems with shared filesystems like Lustre or
    /// GPFS.
    pub async fn prepare_apptainer_command(
        &self,
        container: &str,
        cancellation_token: CancellationToken,
        spawn_request: &TaskSpawnRequest,
    ) -> Result<String, anyhow::Error> {
        let container_sif = sif_for_container(self, container, cancellation_token).await?;
        self.generate_apptainer_script(&container_sif, spawn_request)
            .await
    }

    /// Generate the script, given a container path that's already assumed to be
    /// populated.
    ///
    /// This is a separate method in order to facilitate testing, and should not
    /// be called from outside this module.
    async fn generate_apptainer_script(
        &self,
        container_sif: &Path,
        spawn_request: &TaskSpawnRequest,
    ) -> Result<String, anyhow::Error> {
        // Create a temp dir for the container's execution within the attempt dir
        // hierarchy. On many HPC systems, `/tmp` is mapped to a relatively
        // small, local scratch disk that can fill up easily. Mapping the
        // container's `/tmp` and `/var/tmp` paths to the filesystem we're using
        // for other inputs and outputs prevents this from being a capacity problem,
        // though potentially at the expense of execution speed if the
        // non-`/tmp` filesystem is significantly slower.
        let container_tmp_path = spawn_request.temp_dir().join("container_tmp");
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
        let container_var_tmp_path = spawn_request.temp_dir().join("container_var_tmp");
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
        writeln!(&mut apptainer_command, "#!/bin/env bash")?;
        for (k, v) in spawn_request.env().iter() {
            writeln!(&mut apptainer_command, "export APPTAINERENV_{k}={v}")?;
        }
        write!(&mut apptainer_command, "apptainer -v exec ")?;
        write!(&mut apptainer_command, "--cwd {GUEST_WORK_DIR} ")?;
        write!(&mut apptainer_command, "--containall --cleanenv ")?;
        for input in spawn_request.inputs() {
            write!(
                &mut apptainer_command,
                "--mount type=bind,src={host_path},dst={guest_path},ro ",
                host_path = input
                    .local_path()
                    .ok_or_else(|| anyhow!("input not localized: {input:?}"))?
                    .display(),
                guest_path = input
                    .guest_path()
                    .ok_or_else(|| anyhow!("guest path missing: {input:?}"))?,
            )?;
        }
        write!(
            &mut apptainer_command,
            "--mount type=bind,src={},dst={GUEST_COMMAND_PATH},ro ",
            spawn_request.wdl_command_host_path().display()
        )?;
        write!(
            &mut apptainer_command,
            "--mount type=bind,src={},dst={GUEST_WORK_DIR} ",
            spawn_request.wdl_work_dir_host_path().display()
        )?;
        write!(
            &mut apptainer_command,
            "--mount type=bind,src={},dst=/tmp ",
            container_tmp_path.display()
        )?;
        write!(
            &mut apptainer_command,
            "--mount type=bind,src={},dst=/var/tmp ",
            container_var_tmp_path.display()
        )?;
        write!(
            &mut apptainer_command,
            "--mount type=bind,src={},dst={GUEST_STDOUT_PATH} ",
            spawn_request.wdl_stdout_host_path().display()
        )?;
        write!(
            &mut apptainer_command,
            "--mount type=bind,src={},dst={GUEST_STDERR_PATH} ",
            spawn_request.wdl_stderr_host_path().display()
        )?;
        if let Some(true) = spawn_request
            .requirements()
            .get(wdl_ast::v1::TASK_REQUIREMENT_GPU)
            .and_then(Value::as_boolean)
        {
            write!(&mut apptainer_command, "--nv ")?;
        }
        if let Some(args) = &self.extra_apptainer_exec_args {
            for arg in args {
                write!(&mut apptainer_command, "{arg} ")?;
            }
        }
        write!(&mut apptainer_command, "{} ", container_sif.display())?;
        write!(
            &mut apptainer_command,
            "bash -c \"{GUEST_COMMAND_PATH} > {GUEST_STDOUT_PATH} 2> {GUEST_STDERR_PATH}\" "
        )?;
        let attempt_dir = spawn_request.attempt_dir();
        let apptainer_stdout_path = attempt_dir.join("apptainer.stdout");
        let apptainer_stderr_path = attempt_dir.join("apptainer.stderr");
        writeln!(
            &mut apptainer_command,
            "> {stdout} 2> {stderr}",
            stdout = apptainer_stdout_path.display(),
            stderr = apptainer_stderr_path.display()
        )?;
        Ok(apptainer_command)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use indexmap::IndexMap;
    use tempfile::TempDir;
    use tokio::process::Command;

    use super::*;
    use crate::TaskSpawnInfo;
    use crate::http::Transferer;
    use crate::v1::test::TestEnv;

    fn mk_example_task() -> (TempDir, ApptainerConfig, TaskSpawnRequest) {
        let tmp = tempfile::tempdir().unwrap();
        let config = ApptainerConfig {
            apptainer_images_dir: tmp.path().display().to_string(),
            ..ApptainerConfig::default()
        };
        let mut env = IndexMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        env.insert("BAZ".to_string(), "\"quux\"".to_string());
        let info = TaskSpawnInfo::new(
            "echo hello".to_string(),
            vec![],
            HashMap::new().into(),
            HashMap::new().into(),
            env.into(),
            Arc::new(TestEnv::default()) as Arc<dyn Transferer>,
        );
        let spawn_request = TaskSpawnRequest {
            id: "example_task".to_string(),
            info,
            attempt: 0,
            attempt_dir: tmp.path().join("0"),
            root_dir: tmp.path().to_path_buf(),
            temp_dir: tmp.path().join("tmp"),
        };
        (tmp, config, spawn_request)
    }

    #[tokio::test]
    async fn example_task_generates() {
        let (tmp, config, spawn_request) = mk_example_task();
        let _ = config
            .generate_apptainer_script(&tmp.path().join("non-existent.sif"), &spawn_request)
            .await
            .inspect_err(|e| eprintln!("{e:#?}"))
            .expect("example task script should generate");
    }

    #[tokio::test]
    // `shellcheck` works quite differently on Windows, and since we're not going to run Apptainer
    // on Windows anytime soon, we limit this test to Unixy systems
    #[cfg(unix)]
    async fn example_task_shellchecks() {
        let (tmp, config, spawn_request) = mk_example_task();
        let script = config
            .generate_apptainer_script(&tmp.path().join("non-existent.sif"), &spawn_request)
            .await
            .inspect_err(|e| eprintln!("{e:#?}"))
            .expect("example task script should generate");
        let script_file = tmp.path().join("apptainer_script");
        tokio::fs::write(&script_file, &script)
            .await
            .expect("can write script to disk");
        let shellcheck_status = Command::new("shellcheck")
            .arg("--shell=bash")
            .arg("--severity=style")
            .arg(&script_file)
            .status()
            .await
            .unwrap();
        assert!(shellcheck_status.success());
    }
}
