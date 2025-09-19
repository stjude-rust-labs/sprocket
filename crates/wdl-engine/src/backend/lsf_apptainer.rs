use std::fmt::Write as _;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt as _;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::Context as _;
use anyhow::anyhow;
use tempfile::TempDir;
use tokio::fs::File;
use tokio::fs::{self};
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use tracing::trace;

use super::COMMAND_FILE_NAME;
use super::TaskExecutionBackend;
use super::TaskManager;
use super::TaskManagerRequest;
use super::TaskSpawnRequest;
use super::WORK_DIR_NAME;
use crate::PrimitiveValue;
use crate::STDERR_FILE_NAME;
use crate::STDOUT_FILE_NAME;
use crate::TaskExecutionResult;
use crate::config::Config;
use crate::path::EvaluationPath;
use crate::v1;

/// The name of the file where the Apptainer command invocation will be written.
const APPTAINER_COMMAND_FILE_NAME: &str = "apptainer_command";

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

/// The maximum length of an LSF job name.
///
/// See <https://www.ibm.com/docs/en/spectrum-lsf/10.1.0?topic=o-j>.
const LSF_JOB_NAME_MAX_LENGTH: usize = 4094;

#[derive(Debug)]
struct LsfApptainerTaskRequest {
    engine_config: Arc<Config>,
    backend_config: Arc<LsfApptainerBackendConfig>,
    name: String,
    spawn_request: TaskSpawnRequest,
    /// The requested container for the task.
    container: String,
    /// The requested CPU reservation for the task.
    cpu: f64,
    /// The requested memory reservation for the task, in bytes.
    memory: u64,
    // TODO ACF 2025-09-11: support cancellation
    cancellation_token: CancellationToken,
}

impl TaskManagerRequest for LsfApptainerTaskRequest {
    fn cpu(&self) -> f64 {
        self.cpu
    }

    fn memory(&self) -> u64 {
        self.memory
    }

    async fn run(self) -> anyhow::Result<super::TaskExecutionResult> {
        let attempt_dir = self.spawn_request.attempt_dir();

        // Create the host directory that will be mapped to the WDL working directory.
        let wdl_work_dir = attempt_dir.join(WORK_DIR_NAME);
        fs::create_dir_all(&wdl_work_dir).await.with_context(|| {
            format!(
                "failed to create WDL working directory `{path}`",
                path = wdl_work_dir.display()
            )
        })?;

        // Write the evaluated WDL command section to a host file.
        let wdl_command_path = self.spawn_request.attempt_dir().join(COMMAND_FILE_NAME);
        fs::write(&wdl_command_path, self.spawn_request.command())
            .await
            .with_context(|| {
                format!(
                    "failed to write WDL command contents to `{path}`",
                    path = wdl_command_path.display()
                )
            })?;
        fs::set_permissions(&wdl_command_path, Permissions::from_mode(0o777)).await?;

        // Create an empty file for the WDL command's stdout.
        let wdl_stdout_path = self.spawn_request.attempt_dir().join(STDOUT_FILE_NAME);
        let _ = File::create(&wdl_stdout_path).await.with_context(|| {
            format!(
                "failed to create WDL stdout file `{path}`",
                path = wdl_stdout_path.display()
            )
        })?;

        // Create an empty file for the WDL command's stderr.
        let wdl_stderr_path = self.spawn_request.attempt_dir().join(STDERR_FILE_NAME);
        let _ = File::create(&wdl_stderr_path).await.with_context(|| {
            format!(
                "failed to create WDL stderr file `{path}`",
                path = wdl_stderr_path.display()
            )
        })?;

        // Create a temp dir for the container's execution within the attempt dir
        // hierarchy. On many HPC systems, `/tmp` is mapped to a relatively
        // small, local scratch disk that can fill up easily. Mapping the
        // container's `/tmp` and `/var/tmp` paths to the filesystem we're using
        // for other inputs and outputs prevents this from being a capacity problem,
        // though potentially at the expense of execution speed if the
        // non-`/tmp` filesystem is significantly slower.
        //
        // TODO ACF 2025-09-10: make location of the tempdir configurable
        //
        // TODO ACF 2025-09-10: make the persistence of the tempdir configurable
        let container_temp_dir = TempDir::new_in(self.spawn_request.attempt_dir())?;
        let container_tmp_path = container_temp_dir.path().join("tmp").to_path_buf();
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
        let container_var_tmp_path = container_temp_dir.path().join("var_tmp").to_path_buf();
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

        // Assemble the Apptainer invocation. We'll write out this command to the host
        // filesystem, and ultimately submit it as the command to run via LSF.
        let apptainer_command_path = attempt_dir.join(APPTAINER_COMMAND_FILE_NAME);
        let mut apptainer_command = String::new();
        writeln!(&mut apptainer_command, "#!/bin/env bash")?;

        // Set up mounts for the inputs in an environment variable (ref:
        // https://apptainer.org/docs/user/1.3/bind_paths_and_mounts.html#mount-examples). Using an
        // environment variable rather than separate `--mount` arguments prevents tasks
        // with large numbers of inputs from exceeding the maximum number of
        // command line arguments.
        let inputs = self.spawn_request.inputs();
        if !inputs.is_empty() {
            write!(&mut apptainer_command, "export APPTAINER_MOUNT=$'")?;
            for input in inputs {
                write!(
                    &mut apptainer_command,
                    "type=bind,src={host_path},dst={guest_path},ro\\n",
                    host_path = input
                        .local_path()
                        .ok_or_else(|| anyhow!("input not localized: {input:?}"))?
                        .display(),
                    guest_path = input
                        .guest_path()
                        .ok_or_else(|| anyhow!("guest path missing: {input:?}"))?,
                )?;
            }
            writeln!(&mut apptainer_command, "'")?;
        }

        // Set up any WDL-specified guest environment variables, using the
        // `APPTAINERENV_` prefix approach (ref:
        // https://apptainer.org/docs/user/1.3/environment_and_metadata.html#apptainerenv-prefix) to
        // avoid command line argument limits.
        for (k, v) in self.spawn_request.env().iter() {
            writeln!(&mut apptainer_command, "export APPTAINERENV_{k}={v}")?;
        }

        // Begin writing the `apptainer` command itself. We're using the synchronous
        // `exec` command which keeps running until the containerized command is
        // finished.
        write!(&mut apptainer_command, "apptainer -v exec ")?;
        write!(&mut apptainer_command, "--cwd {GUEST_WORK_DIR} ")?;
        // These options make the Apptainer sandbox behave more like default Docker
        // behavior, e.g. by not auto-mounting the user's home directory and
        // inheriting all environment variables.
        write!(&mut apptainer_command, "--containall --cleanenv ")?;
        // Mount the instantiated WDL command as read-only.
        write!(
            &mut apptainer_command,
            "--mount type=bind,src={},dst={GUEST_COMMAND_PATH},ro ",
            wdl_command_path.display()
        )?;
        // Mount the working dir, temp dirs, and stdio files as read/write (no `,ro` on
        // the end like for the inputs).
        write!(
            &mut apptainer_command,
            "--mount type=bind,src={},dst={GUEST_WORK_DIR} ",
            wdl_work_dir.display()
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
            wdl_stdout_path.display()
        )?;
        write!(
            &mut apptainer_command,
            "--mount type=bind,src={},dst={GUEST_STDERR_PATH} ",
            wdl_stderr_path.display()
        )?;
        // Specify the container as a positional argument.
        //
        // TODO ACF 2025-09-10: must implement caching for `.sif` files instead of using
        // the `docker://` URI every time.
        write!(&mut apptainer_command, "docker://{} ", self.container)?;
        // Finally provide the instantiated WDL command, with its stdio handles
        // redirected to their respective guest paths.
        write!(
            &mut apptainer_command,
            "bash -c \"{GUEST_COMMAND_PATH} > {GUEST_STDOUT_PATH} 2> {GUEST_STDERR_PATH}\""
        )?;

        fs::write(&apptainer_command_path, apptainer_command)
            .await
            .with_context(|| {
                format!(
                    "failed to write Apptainer command file `{}`",
                    apptainer_command_path.display()
                )
            })?;
        fs::set_permissions(&apptainer_command_path, Permissions::from_mode(0o777)).await?;

        // The path for the LSF-level stdout and stderr. This primarily contains the job
        // report, as we redirect Apptainer and WDL output separately.
        let lsf_stdout_path = attempt_dir.join("lsf.stdout");
        let lsf_stderr_path = attempt_dir.join("lsf.stderr");

        let mut bsub_command = Command::new("bsub");

        // If an LSF queue has been configured, specify it. Otherwise, the job will end
        // up on the cluster's default queue.
        if let Some(queue) = &self.backend_config.queue {
            bsub_command.arg("-q");
            bsub_command.arg(queue);
        }

        bsub_command
            // Pipe stdout and stderr so we can trace them. This should just be the LSF output like
            // `<<Waiting for dispatch ...>>`.
            //
            // TODO ACF 2025-09-10: maybe only bother with this (and the loops consuming the output)
            // in a verbose mode
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // TODO ACF 2025-09-10: make this configurable; hardcode turning off LSF email spam for
            // now though.
            .env("LSB_JOB_REPORT_MAIL", "N")
            // This option makes the `bsub` invocation synchronous, so this command will not exit
            // until the job is complete.
            //
            // If the number of concurrent `bsub` processes becomes a problem, we can switch this to
            // an asynchronous model where we drop this argument, grab the job ID, and poll for it
            // using `bjobs`.
            .arg("-K")
            // Send LSF job stdout and stderr streams to these files. Since we redirect the
            // Apptainer invocation's stdio to separate files, this will typically amount to the LSF
            // job report.
            .arg("-oo")
            .arg(lsf_stdout_path)
            .arg("-eo")
            .arg(lsf_stderr_path)
            // CPU request is rounded up to the nearest whole CPU
            .arg("-R")
            .arg(format!(
                "affinity[cpu({cpu})]",
                cpu = self.cpu.ceil() as u64
            ))
            // Memory request is specified per job to avoid ambiguity on clusters which may be
            // configured to interpret memory requests as per-core or per-task. We also use an
            // explicit KB unit which LSF appears to interpret as base-2 kibibytes.
            .arg("-R")
            .arg(format!(
                "rusage[mem={memory_kb}KB/job]",
                memory_kb = self.memory / 1024
            ))
            .arg(apptainer_command_path);

        let mut bsub_child = bsub_command.spawn()?;

        // Take the stdio pipes from the child process and consume them for tracing
        // purposes.
        //
        // TODO ACF 2025-09-10: future extension could hook some progress reporting in
        // here based on "waiting for dispatch", "starting", etc messages. More
        // robust would probably be to drop the `-K` and use `bjobs` to monitor.
        let bsub_stdout = bsub_child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("bsub child stdout missing"))?;
        let task_name = self.name.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(bsub_stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                trace!(stdout = line, task_name);
            }
        });
        let bsub_stderr = bsub_child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("bsub child stderr missing"))?;
        let task_name = self.name.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(bsub_stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                trace!(stderr = line, task_name);
            }
        });

        // Await the result of the `bsub` command, which will only exit on error or once
        // the containerized command has completed.
        let bsub_result = bsub_child.wait().await?;

        // Hang onto the container tmp dir until execution is complete.
        drop(container_temp_dir);

        Ok(TaskExecutionResult {
            // Under normal circumstances, the exit code of `bsub -K` is the exit code of its
            // command, and the exit code of `apptainer exec` is likewise the exit code of its
            // command. One potential subtlety/problem here is that if `bsub` or `apptainer` exit
            // due to an error before running the WDL command, we could be erroneously ascribing an
            // exit code to the WDL command.
            exit_code: bsub_result
                .code()
                .ok_or(anyhow!("task did not return an exit code"))?,
            work_dir: EvaluationPath::Local(wdl_work_dir),
            stdout: PrimitiveValue::new_file(
                wdl_stdout_path
                    .into_os_string()
                    .into_string()
                    .expect("path should be UTF-8"),
            )
            .into(),
            stderr: PrimitiveValue::new_file(
                wdl_stderr_path
                    .into_os_string()
                    .into_string()
                    .expect("path should be UTF-8"),
            )
            .into(),
        })
    }
}

#[derive(Debug)]
pub struct LsfApptainerBackend {
    engine_config: Arc<Config>,
    backend_config: Arc<LsfApptainerBackendConfig>,
    manager: TaskManager<LsfApptainerTaskRequest>,
}

impl LsfApptainerBackend {
    pub fn new(engine_config: Arc<Config>, backend_config: Arc<LsfApptainerBackendConfig>) -> Self {
        Self {
            engine_config,
            backend_config,
            // TODO ACF 2025-09-11: the `MAX` values here mean that in addition to not limiting the
            // overall number of CPU and memory used, we don't limit per-task consumption. There is
            // potentially a path to pulling queue limits from LSF for these, but for now we just
            // throw jobs at the cluster.
            manager: TaskManager::new_unlimited(u64::MAX, u64::MAX),
        }
    }
}

impl TaskExecutionBackend for LsfApptainerBackend {
    fn max_concurrency(&self) -> u64 {
        // TODO ACF 2025-09-11: make this configurable
        200
    }

    fn constraints(
        &self,
        requirements: &std::collections::HashMap<String, crate::Value>,
        _hints: &std::collections::HashMap<String, crate::Value>,
    ) -> anyhow::Result<super::TaskExecutionConstraints> {
        Ok(super::TaskExecutionConstraints {
            container: Some(
                v1::container(requirements, self.engine_config.task.container.as_deref())
                    .into_owned(),
            ),
            // TODO ACF 2025-09-11: populate more meaningful values for these based on the given LSF
            // queue. Unfortunately, it's not straightforward to ask "what's the most CPUs I can ask
            // for and still hope to be scheduled?". A reasonable stopgap would be to make this a
            // config parameter, but the experience would be unfortunate when having to manually
            // update that if changing queues, or if handling multiple queues for short jobs.
            cpu: f64::MAX,
            memory: i64::MAX,
            gpu: Default::default(),
            fpga: Default::default(),
            disks: Default::default(),
        })
    }

    fn guest_inputs_dir(&self) -> Option<&'static str> {
        Some(GUEST_INPUTS_DIR)
    }

    fn needs_local_inputs(&self) -> bool {
        true
    }

    fn spawn(
        &self,
        request: TaskSpawnRequest,
        cancellation_token: CancellationToken,
    ) -> anyhow::Result<tokio::sync::oneshot::Receiver<anyhow::Result<TaskExecutionResult>>> {
        let (completed_tx, completed_rx) = tokio::sync::oneshot::channel();

        let requirements = request.requirements();
        let hints = request.hints();

        let container =
            v1::container(requirements, self.engine_config.task.container.as_deref()).into_owned();
        let cpu = v1::cpu(requirements);
        let memory = v1::memory(requirements)? as u64;
        // TODO ACF 2025-09-11: I don't _think_ LSF offers a hard/soft CPU limit
        // distinction, but we could potentially use a max as part of the
        // resource request. That would likely mean using `bsub -n min,max`
        // syntax as it doesn't seem that `affinity` strings support ranges
        let _max_cpu = v1::max_cpu(hints);
        // TODO ACF 2025-09-11: set a hard memory limit with `bsub -M !`?
        let _max_memory = v1::max_memory(hints)?.map(|i| i as u64);

        // Truncate the request ID to fit in the LSF job name length limit.
        //
        // TODO ACF 2025-09-12: test to see whether LSF even accepts non-ascii job
        // names...
        let request_id = request.id();
        let name = if request_id.len() > LSF_JOB_NAME_MAX_LENGTH {
            request_id
                .chars()
                .take(LSF_JOB_NAME_MAX_LENGTH)
                .collect::<String>()
        } else {
            request_id.to_string()
        };

        self.manager.send(
            LsfApptainerTaskRequest {
                engine_config: self.engine_config.clone(),
                backend_config: self.backend_config.clone(),
                spawn_request: request,
                name,
                container,
                cpu,
                memory,
                cancellation_token,
            },
            completed_tx,
        );

        Ok(completed_rx)
    }

    fn cleanup<'a, 'b, 'c>(
        &'a self,
        _output_dir: &'b std::path::Path,
        _token: CancellationToken,
    ) -> Option<futures::future::BoxFuture<'c, ()>>
    where
        'a: 'c,
        'b: 'c,
        Self: 'c,
    {
        // TODO ACF 2025-09-11: determine whether we need cleanup logic here;
        // Apptainer's security model is fairly different from Docker so
        // uid/gids on files shouldn't be as much of an issue, and using only
        // `apptainer exec` means no longer-running containers to tear down
        None
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct LsfApptainerBackendConfig {
    // TODO ACF 2025-09-12: add queue option for short tasks
    pub queue: Option<String>,
}

impl LsfApptainerBackendConfig {
    pub fn validate(&self) -> Result<(), anyhow::Error> {
        // TODO ACF 2025-09-12: what meaningful work to be done here? Maybe ensure the
        // queue exists, interrogate the queue for limits and match them up
        // against prospective future config options here?
        Ok(())
    }
}
