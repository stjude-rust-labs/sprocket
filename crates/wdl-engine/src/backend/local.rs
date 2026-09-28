//! Implementation of the local backend.

use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use crankshaft::events::Event as CrankshaftEvent;
use crankshaft::events::next_task_id;
use crankshaft::events::send_event;
use futures::FutureExt;
use futures::future::BoxFuture;
use nonempty::NonEmpty;
use tokio::process::Command;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing::warn;

use super::TaskExecutionBackend;
use super::TaskExecutionConstraints;
use crate::ContainerImageOverrides;
use crate::EvaluationPath;
use crate::ONE_GIBIBYTE;
use crate::Object;
use crate::PrimitiveValue;
use crate::SYSTEM;
use crate::TaskInputs;
use crate::backend::ExecuteTaskRequest;
use crate::backend::TaskExecutionResult;
use crate::backend::manager::ManagedTask;
use crate::backend::manager::TaskManager;
use crate::config::Config;
use crate::config::TaskResourceLimitBehavior;
use crate::convert_unit_string;
use crate::v1::requirements;

/// Represents a local task request.
///
/// This request contains the requested cpu and memory reservations for the task
/// as well as the result receiver channel.
struct LocalTask<'a> {
    /// The engine configuration.
    config: Arc<Config>,
    /// The task execution request.
    request: &'a ExecuteTaskRequest<'a>,
    /// The name of the task.
    name: String,
}

/// Terminates a task's process group if execution is canceled or dropped.
#[cfg(unix)]
struct TaskProcessGroup(Option<libc::pid_t>);

#[cfg(unix)]
impl Drop for TaskProcessGroup {
    fn drop(&mut self) {
        let Some(id) = self.0 else {
            return;
        };

        // The child starts its own group, so this cannot signal Sprocket's
        // group.
        if unsafe { libc::kill(-id, libc::SIGKILL) } != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                warn!("failed to terminate task process group {id}: {error}");
            }
        }
    }
}

impl ManagedTask for LocalTask<'_> {
    type Output = TaskExecutionResult;

    fn request(&self) -> &ExecuteTaskRequest<'_> {
        self.request
    }

    async fn run(self) -> Result<Option<TaskExecutionResult>> {
        let id = next_task_id();
        let work_dir = self.request.work_dir();
        let stdout_path = self.request.stdout_path();
        let stderr_path = self.request.stderr_path();

        let run = async {
            // Create the working directory
            fs::create_dir_all(&work_dir).with_context(|| {
                format!(
                    "failed to create directory `{path}`",
                    path = work_dir.display()
                )
            })?;

            // Write the evaluated command to disk
            let command_path = self.request.command_path();
            fs::write(&command_path, self.request.command).with_context(|| {
                format!(
                    "failed to write command contents to `{path}`",
                    path = command_path.display()
                )
            })?;

            // Create a file for the stdout
            let stdout = File::create(&stdout_path).with_context(|| {
                format!(
                    "failed to create stdout file `{path}`",
                    path = stdout_path.display()
                )
            })?;

            // Create a file for the stderr
            let stderr = File::create(&stderr_path).with_context(|| {
                format!(
                    "failed to create stderr file `{path}`",
                    path = stderr_path.display()
                )
            })?;

            let mut command = Command::new(&self.config.task.shell);
            command
                .current_dir(&work_dir)
                .arg(command_path)
                .stdin(Stdio::null())
                .stdout(stdout)
                .stderr(stderr)
                .envs(
                    self.request
                        .env
                        .iter()
                        .map(|(k, v)| (OsStr::new(k), OsStr::new(v))),
                )
                .kill_on_drop(true);

            #[cfg(unix)]
            command.process_group(0);

            // Set the PATH variable for the child on Windows to get consistent
            // PATH searching. See: https://github.com/rust-lang/rust/issues/122660
            #[cfg(windows)]
            if let Ok(path) = std::env::var("PATH") {
                command.env("PATH", path);
            }

            let mut child = command.spawn().context("failed to spawn shell")?;
            let pid = child.id().expect("should have id");
            #[cfg(unix)]
            let mut process_group =
                TaskProcessGroup(Some(pid.try_into().context("task PID is out of range")?));

            // Notify that the process has spawned
            send_event!(
                self.request.context.events().crankshaft(),
                CrankshaftEvent::TaskStarted { id }
            );

            info!(
                "spawned local shell process {pid} for execution of task `{name}`",
                name = self.name
            );

            let status = child.wait().await.with_context(|| {
                format!("failed to wait for termination of task child process {pid}")
            })?;
            #[cfg(unix)]
            {
                process_group.0 = None;
            }

            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(signal) = status.signal() {
                    tracing::warn!("task process {pid} has terminated with signal {signal}");

                    bail!(
                        "task child process {pid} has terminated with signal {signal}; see stderr \
                         file `{path}` for more details",
                        path = stderr_path.display()
                    );
                }
            }

            Ok(status)
        };

        // Send the created event
        let task_token = CancellationToken::new();
        send_event!(
            self.request.context.events().crankshaft(),
            CrankshaftEvent::TaskCreated {
                id,
                name: self.name.clone(),
                tes_id: None,
                token: task_token.clone(),
            }
        );

        let token = self.request.context.cancellation().second();

        select! {
            // Poll the cancellation tokens before the child future
            biased;
            _ = task_token.cancelled() => {
                send_event!(self.request.context.events().crankshaft(), CrankshaftEvent::TaskCanceled { id });
                Ok(None)
            }
            _ = token.cancelled() => {
                send_event!(self.request.context.events().crankshaft(), CrankshaftEvent::TaskCanceled { id });
                Ok(None)
            }
            result = run => {
                match result {
                    Ok(status) => {
                        send_event!(self.request.context.events().crankshaft(), CrankshaftEvent::TaskCompleted { id, exit_statuses: NonEmpty::new(status) });

                        let exit_code = status.code().expect("process should have exited");
                        info!("process {id} for task `{name}` has terminated with status code {exit_code}", name = self.name);
                        Ok(Some(TaskExecutionResult {
                            execution_name_override: None,
                            image: None,
                            exit_code,
                            work_dir: EvaluationPath::from_local_path(work_dir),
                            usage_file: None,
                            stdout: PrimitiveValue::new_file(stdout_path.into_os_string().into_string().expect("path should be UTF-8")).into(),
                            stderr: PrimitiveValue::new_file(stderr_path.into_os_string().into_string().expect("path should be UTF-8")).into(),
                        }))
                    }
                    Err(e) => {
                        send_event!(self.request.context.events().crankshaft(), CrankshaftEvent::TaskFailed { id, message: format!("{e:#}") });
                        Err(e)
                    }
                }
            }
        }
    }
}

/// Represents a task execution backend that locally executes tasks.
///
/// <div class="warning">
/// Warning: the local task execution backend spawns processes on the host
/// directly without the use of a container; only use this backend on trusted
/// WDL. </div>
pub struct LocalBackend {
    /// The engine configuration.
    config: Arc<Config>,
    /// The total CPU of the host.
    cpu: f64,
    /// The total memory of the host.
    memory: u64,
    /// The underlying task manager.
    manager: TaskManager,
}

impl LocalBackend {
    /// Constructs a new local task execution backend with the given
    /// configuration.
    ///
    /// The provided configuration is expected to have already been validated.
    pub fn new(config: Arc<Config>) -> Result<Self> {
        info!("initializing local backend");

        let backend_config = config.backend()?;
        let backend_config = backend_config
            .as_local()
            .context("configured backend is not local")?;
        let cpu = backend_config
            .cpu
            .map(|v| v as f64)
            .unwrap_or_else(|| SYSTEM.cpus().len() as f64);
        let memory = backend_config
            .memory
            .as_ref()
            .map(|s| convert_unit_string(s).expect("value should be valid"))
            .unwrap_or_else(|| SYSTEM.total_memory());
        let manager = TaskManager::new(cpu, cpu, memory, memory);

        Ok(Self {
            config,
            cpu,
            memory,
            manager,
        })
    }
}

impl TaskExecutionBackend for LocalBackend {
    fn name(&self) -> &'static str {
        "local"
    }

    fn constraints(
        &self,
        inputs: &TaskInputs,
        requirements: &Object,
        _: &ContainerImageOverrides,
        _: &Object,
    ) -> Result<TaskExecutionConstraints> {
        let mut cpu = requirements::cpu(inputs, requirements);
        if self.cpu < cpu {
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
                    cpu = self.cpu;
                }
                TaskResourceLimitBehavior::Deny => {
                    bail!(
                        "task requires at least {cpu} CPU{s}{env_specific}",
                        s = if cpu == 1.0 { "" } else { "s" },
                    );
                }
            }
        }

        let mut memory = requirements::memory(inputs, requirements)? as u64;
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
                        // Display the error in GiB, as it is the most common
                        // unit for memory
                        memory = memory as f64 / ONE_GIBIBYTE,
                    );
                    // clamp the reported constraint to what's available
                    memory = self.memory;
                }
                TaskResourceLimitBehavior::Deny => {
                    bail!(
                        "task requires at least {memory} GiB of memory{env_specific}",
                        // Display the error in GiB, as it is the most common
                        // unit for memory
                        memory = memory as f64 / ONE_GIBIBYTE,
                    );
                }
            }
        }

        Ok(TaskExecutionConstraints {
            sources: Vec::new(),
            cpu,
            memory,
            gpu: Default::default(),
            fpga: Default::default(),
            disks: Default::default(),
        })
    }

    fn guest_inputs_dir(&self) -> Option<&'static str> {
        // Local execution does not use a container
        None
    }

    fn execute<'a>(
        &'a self,
        request: &'a ExecuteTaskRequest<'a>,
    ) -> BoxFuture<'a, Result<Option<TaskExecutionResult>>> {
        async move {
            let name = request.name.to_string();

            let cpu = request.constraints.cpu;
            let memory = request.constraints.memory;

            let task = LocalTask {
                config: self.config.clone(),
                request,
                name,
            };

            self.manager.run(cpu, memory, task).await
        }
        .boxed()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::CancellationContext;
    use crate::Events;
    use crate::backend::tests::EvalContext;
    use crate::config::FailureMode;

    struct PayloadCleanup(libc::pid_t);

    impl Drop for PayloadCleanup {
        fn drop(&mut self) {
            if self.0 > 0 {
                // This PID was reported by the payload started by this test.
                unsafe { libc::kill(self.0, libc::SIGKILL) };
            }
        }
    }

    #[tokio::test]
    async fn cancellation_and_dropped_executions_stop_payloads() {
        for command in [
            "exec sh -c 'echo $$ > payload.pid; exec sleep 60'",
            "(exec sh -c 'echo $$ > payload.pid; exec sleep 60')\nexit $?",
            "sh -c 'echo $$ > payload.pid; exec sleep 60'\nexit $?",
        ] {
            for drop_execution in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let cancellation = CancellationContext::new(FailureMode::Fast);
                let context = EvalContext::new(Events::default(), cancellation.clone()).await;
                let request = ExecuteTaskRequest {
                    context: &context,
                    name: "cancel-test",
                    command,
                    measure_resource_usage: false,
                    inputs: &TaskInputs::default(),
                    backend_inputs: &[],
                    requirements: &Object::empty(),
                    hints: &Object::empty(),
                    env: &Default::default(),
                    constraints: &TaskExecutionConstraints {
                        sources: Vec::new(),
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
                let mut execution = LocalTask {
                    config: Arc::new(Config::local()),
                    request: &request,
                    name: request.name.to_string(),
                }
                .run()
                .boxed();

                let pid_file = request.work_dir().join("payload.pid");
                let pid = tokio::time::timeout(Duration::from_secs(10), async {
                    loop {
                        select! {
                            result = &mut execution => panic!("task ended before readiness: {result:?}"),
                            _ = tokio::time::sleep(Duration::from_millis(10)) => {
                                if let Ok(contents) = fs::read_to_string(&pid_file)
                                    && let Ok(pid) = contents.trim().parse::<libc::pid_t>()
                                    && pid > 0
                                {
                                    break pid;
                                }
                            }
                        }
                    }
                })
                .await
                .expect("payload did not report readiness");
                let mut cleanup = PayloadCleanup(pid);

                if drop_execution {
                    drop(execution);
                } else {
                    let _ = cancellation.cancel();
                    assert!(
                        tokio::time::timeout(Duration::from_secs(10), execution)
                            .await
                            .expect("cancellation timed out")
                            .expect("cancellation failed")
                            .is_none()
                    );
                }

                let stopped = tokio::time::timeout(Duration::from_secs(3), async {
                    loop {
                        let output = Command::new("ps")
                            .args(["-p", &pid.to_string(), "-o", "stat="])
                            .output()
                            .await
                            .expect("failed to inspect payload");
                        if output.status.code() == Some(1)
                            || String::from_utf8_lossy(&output.stdout)
                                .trim_start()
                                .starts_with('Z')
                        {
                            break;
                        }
                        assert!(output.status.success(), "ps failed: {output:?}");
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                })
                .await;
                if stopped.is_ok() {
                    cleanup.0 = 0;
                }
                assert!(
                    stopped.is_ok(),
                    "payload {pid} survived (drop={drop_execution}): {command}"
                );
            }
        }
    }
}
