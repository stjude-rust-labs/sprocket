use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::fs::Permissions;
use std::fs::{self};
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

use anyhow::Context as _;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use nonempty::NonEmpty;
use tempfile::TempDir;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing::warn;

use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use super::TaskManager;
use super::TaskManagerRequest;
use super::TaskSpawnRequest;
use crate::COMMAND_FILE_NAME;
use crate::ONE_GIBIBYTE;
use crate::ONE_MEGABYTE;
use crate::PrimitiveValue;
use crate::STDERR_FILE_NAME;
use crate::STDOUT_FILE_NAME;
use crate::SYSTEM;
use crate::TaskExecutionResult;
use crate::Value;
use crate::WORK_DIR_NAME;
use crate::config::Config;
use crate::config::GenericBackendConfig;
use crate::config::TaskResourceLimitBehavior;
use crate::convert_unit_string;
use crate::path::EvaluationPath;

/// The root guest path for inputs.
const GUEST_INPUTS_DIR: &str = "/mnt/task/inputs/";

/// Represents a generic task request.
#[derive(Debug)]
struct GenericTaskRequest {
    /// The engine configuration.
    config: Arc<Config>,
    /// The backend configuration.
    backend_config: Arc<GenericBackendConfig>,
    /// The inner task spawn request.
    inner: TaskSpawnRequest,
    /// The requested CPU reservation for the task.
    cpu: f64,
    /// The requested memory reservation for the task.
    memory: u64,
    /// The cancellation token for the request.
    token: CancellationToken,
}

impl TaskManagerRequest for GenericTaskRequest {
    fn cpu(&self) -> f64 {
        self.cpu
    }

    fn memory(&self) -> u64 {
        self.memory
    }

    async fn run(self) -> Result<TaskExecutionResult> {
        // Create the working directory
        let work_dir = self.inner.attempt_dir().join(WORK_DIR_NAME);
        fs::create_dir_all(&work_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = work_dir.display()
            )
        })?;

        // Write the evaluated command to disk
        let command_path = self.inner.attempt_dir().join(COMMAND_FILE_NAME);
        fs::write(&command_path, self.inner.command()).with_context(|| {
            format!(
                "failed to write command contents to `{path}`",
                path = command_path.display()
            )
        })?;
        fs::set_permissions(&command_path, Permissions::from_mode(0o777))?;

        // Create an empty file for the stdout
        let stdout_path = self.inner.attempt_dir().join(STDOUT_FILE_NAME);
        let _ = File::create(&stdout_path).with_context(|| {
            format!(
                "failed to create stdout file `{path}`",
                path = stdout_path.display()
            )
        })?;

        // Create an empty file for the stderr
        let stderr_path = self.inner.attempt_dir().join(STDERR_FILE_NAME);
        let _ = File::create(&stderr_path).with_context(|| {
            format!(
                "failed to create stderr file `{path}`",
                path = stderr_path.display()
            )
        })?;

        let temp_dir = TempDir::new()?;
        let mut attributes = HashMap::new();
        // TODO ACF 2025-08-12: might want to bake a tmpdir in as a more fully-fledged
        // concept eventually, but for now putting things in a separate location
        // is required to avoid spurious test failures from the fixture checking
        // every file in `cwd` for equivalence
        attributes.insert(
            Cow::Borrowed("temp_dir"),
            temp_dir.path().display().to_string().into(),
        );
        let task_exit_code = temp_dir.path().join("task_exit_code");
        attributes.insert(
            Cow::Borrowed("task_exit_code"),
            task_exit_code.display().to_string().into(),
        );
        let container = crate::v1::container(
            self.inner.requirements(),
            self.config.task.container.as_deref(),
        )
        .into_owned();
        attributes.insert(Cow::Borrowed("container"), container.into());
        let cpu = crate::v1::cpu(self.inner.requirements()).ceil() as u64;
        attributes.insert(Cow::Borrowed("cpu"), cpu.to_string().into());
        let memory_mb = crate::v1::memory(self.inner.requirements())? as f64 / ONE_MEGABYTE;
        attributes.insert(Cow::Borrowed("memory_mb"), memory_mb.to_string().into());
        // let crankshaft_generic_backend_driver =
        //     crankshaft::config::backend::generic::driver::Config::builder()
        //         .locale(crankshaft::config::backend::generic::driver::Locale::Local)
        //         .shell(crankshaft::config::backend::generic::driver::Shell::Bash)
        //         .build();
        // let crankshaft_generic_backend_config =
        //     crankshaft::config::backend::generic::Config::builder()
        //         .driver(crankshaft_generic_backend_driver)
        //         .submit(
        //             r#"((cd ~{cwd}; ~{command} > ~{stdout} 2> ~{stderr}; echo $? >
        // ~{task_exit_code}) & echo $!)"#         )
        //         .job_id_regex(r#"(\d+)"#)
        //         .monitor("file -E ~{task_exit_code}")
        //         .get_exit_code("cat ~{task_exit_code}")
        //         .kill("kill ~{job_id}")
        //         .attributes(attributes)
        //     .build();

        let mut crankshaft_generic_backend_config = self.backend_config.backend_config.clone();
        *crankshaft_generic_backend_config.attributes_mut() = attributes;

        const BACKEND_NAME: &'static str = "crankshaft_generic";
        let backend = crankshaft::Engine::default()
            .with(
                crankshaft::config::backend::Config::builder()
                    .name(BACKEND_NAME)
                    .max_tasks(10)
                    .kind(crankshaft::config::backend::Kind::Generic(
                        crankshaft_generic_backend_config,
                    ))
                    .build(),
            )
            .await?;

        let mut inputs = vec![];
        for input in self.inner.inputs().iter() {
            let guest_path = input.guest_path().expect("input should have guest path");
            let local_path = input.local_path().expect("input should be localized");

            // The local path must exist for Docker to mount
            if !local_path.exists() {
                bail!(
                    "cannot mount input `{path}` as it does not exist",
                    path = local_path.display()
                );
            }

            inputs.push(
                crankshaft::engine::task::Input::builder()
                    .path(guest_path.as_str())
                    .contents(crankshaft::engine::task::input::Contents::Path(
                        local_path.into(),
                    ))
                    .ty(input.kind())
                    .read_only(true)
                    .build(),
            );
        }
        inputs.push(
            crankshaft::engine::task::Input::builder()
                .contents(crankshaft::engine::task::input::Contents::Path(
                    command_path.clone(),
                ))
                .path(self.backend_config.guest_command_path.clone())
                .ty(crate::InputKind::File)
                .build(),
        );
        inputs.push(
            crankshaft::engine::task::Input::builder()
                .contents(crankshaft::engine::task::input::Contents::Path(
                    work_dir.clone(),
                ))
                .path(self.backend_config.guest_work_dir.clone())
                .ty(crate::InputKind::File)
                .build(),
        );
        // TODO ACF 2025-08-27: make these outputs instead?
        inputs.push(
            crankshaft::engine::task::Input::builder()
                .contents(crankshaft::engine::task::input::Contents::Path(
                    stdout_path.clone(),
                ))
                .path(self.backend_config.guest_stdout_path.clone())
                .ty(crate::InputKind::File)
                .build(),
        );
        inputs.push(
            crankshaft::engine::task::Input::builder()
                .contents(crankshaft::engine::task::input::Contents::Path(
                    stderr_path.clone(),
                ))
                .path(self.backend_config.guest_stderr_path.clone())
                .ty(crate::InputKind::File)
                .build(),
        );
        let generic_task = crankshaft::engine::Task::builder()
            // TODO ACF 2025-08-22: outputs? It looks like other backends just treat the attempt dir
            // contents as the only output rather than enumerating based on the task definition
            .inputs(inputs)
            .executions(NonEmpty::new(
                crankshaft::engine::task::Execution::builder()
                    .image("not_an_image")
                    .program(self.backend_config.guest_command_path.clone())
                    .work_dir(self.backend_config.guest_work_dir.clone())
                    .stdout(self.backend_config.guest_stdout_path.clone())
                    .stderr(self.backend_config.guest_stderr_path.clone())
                    .build(),
            ))
            .build();
        let handle = backend.spawn(BACKEND_NAME, generic_task, self.token.clone())?;

        let res = handle.wait().await?;

        // Set the PATH variable for the child on Windows to get consistent PATH
        // searching. See: https://github.com/rust-lang/rust/issues/122660
        #[cfg(windows)]
        if let Ok(path) = std::env::var("PATH") {
            command.env("PATH", path);
        }

        return Ok(TaskExecutionResult {
            exit_code: res
                .last()
                .code()
                .ok_or(anyhow!("task did not return an exit code"))?,
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
        });
    }
}

/// Represents a task execution backend that uses Crankshaft's generic backend
/// to execute tasks.
pub struct GenericBackend {
    /// The engine configuration.
    config: Arc<Config>,
    /// The backend configuration.
    backend_config: Arc<GenericBackendConfig>,
    /// The total CPU of the host.
    cpu: u64,
    /// The total memory of the host.
    memory: u64,
    /// The underlying task manager.
    manager: TaskManager<GenericTaskRequest>,
}

impl GenericBackend {
    /// Constructs a new generic task execution backend with the given
    /// configuration.
    ///
    /// The provided configuration is expected to have already been validated.
    pub fn new(config: Arc<Config>, backend_config: &GenericBackendConfig) -> Result<Self> {
        info!("initializing generic backend");

        let cpu = backend_config
            .cpu
            .unwrap_or_else(|| SYSTEM.cpus().len() as u64);
        let memory = backend_config
            .memory
            .as_ref()
            .map(|s| convert_unit_string(s).expect("value should be valid"))
            .unwrap_or_else(|| SYSTEM.total_memory());
        let manager = TaskManager::new(cpu, cpu, memory, memory);

        Ok(Self {
            config,
            // TODO ACF 2025-08-12: sort out this excess cloning nonsense
            backend_config: Arc::new(backend_config.clone()),
            cpu,
            memory,
            manager,
        })
    }
}

impl TaskExecutionBackend for GenericBackend {
    fn max_concurrency(&self) -> u64 {
        self.cpu
    }

    fn constraints(
        &self,
        requirements: &HashMap<String, Value>,
        _: &HashMap<String, Value>,
    ) -> Result<TaskExecutionConstraints> {
        let mut cpu = crate::v1::cpu(requirements);
        if (self.cpu as f64) < cpu {
            let env_specific = if self.config.suppress_env_specific_output {
                String::new()
            } else {
                format!(
                    ", but the host only has {total_cpu} available",
                    total_cpu = self.cpu
                )
            };
            match self.config.task.cpu_limit_behavior {
                TaskResourceLimitBehavior::TryWithMax => {
                    warn!(
                        "task requires at least {cpu} CPU{s}{env_specific}",
                        s = if cpu == 1.0 { "" } else { "s" },
                    );
                    // clamp the reported constraint to what's available
                    cpu = self.cpu as f64;
                }
                TaskResourceLimitBehavior::Deny => {
                    bail!(
                        "task requires at least {cpu} CPU{s}{env_specific}",
                        s = if cpu == 1.0 { "" } else { "s" },
                    );
                }
            }
        }

        let mut memory = crate::v1::memory(requirements)?;
        if self.memory < memory as u64 {
            let env_specific = if self.config.suppress_env_specific_output {
                String::new()
            } else {
                format!(
                    ", but the host only has {total_memory} GiB available",
                    total_memory = self.memory as f64 / ONE_GIBIBYTE,
                )
            };
            match self.config.task.memory_limit_behavior {
                TaskResourceLimitBehavior::TryWithMax => {
                    warn!(
                        "task requires at least {memory} GiB of memory{env_specific}",
                        // Display the error in GiB, as it is the most common unit for memory
                        memory = memory as f64 / ONE_GIBIBYTE,
                    );
                    // clamp the reported constraint to what's available
                    memory = self.memory.try_into().unwrap_or(i64::MAX);
                }
                TaskResourceLimitBehavior::Deny => {
                    bail!(
                        "task requires at least {memory} GiB of memory{env_specific}",
                        // Display the error in GiB, as it is the most common unit for memory
                        memory = memory as f64 / ONE_GIBIBYTE,
                    );
                }
            }
        }

        Ok(TaskExecutionConstraints {
            container: None,
            cpu,
            memory,
            gpu: Default::default(),
            fpga: Default::default(),
            disks: Default::default(),
        })
    }

    fn spawn(
        &self,
        request: TaskSpawnRequest,
        token: CancellationToken,
    ) -> Result<oneshot::Receiver<Result<TaskExecutionResult>>> {
        let (completed_tx, completed_rx) = oneshot::channel();

        let requirements = request.requirements();
        let mut cpu = crate::v1::cpu(requirements);
        if let TaskResourceLimitBehavior::TryWithMax = self.config.task.cpu_limit_behavior {
            cpu = std::cmp::min(cpu.ceil() as u64, self.cpu) as f64;
        }
        let mut memory = crate::v1::memory(requirements)? as u64;
        if let TaskResourceLimitBehavior::TryWithMax = self.config.task.memory_limit_behavior {
            memory = std::cmp::min(memory, self.memory);
        }

        self.manager.send(
            GenericTaskRequest {
                config: self.config.clone(),
                backend_config: self.backend_config.clone(),
                inner: request,
                cpu,
                memory,
                token,
            },
            completed_tx,
        );

        Ok(completed_rx)
    }

    fn guest_inputs_dir(&self) -> Option<&'static str> {
        // TODO ACF 2025-09-29: whether and where to localize should be part of the
        // generic backend config
        Some(GUEST_INPUTS_DIR)
    }

    fn needs_local_inputs(&self) -> bool {
        // TODO ACF 2025-09-29: whether and where to localize should be part of the
        // generic backend config
        true
    }
}
