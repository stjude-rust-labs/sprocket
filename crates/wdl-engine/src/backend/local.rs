//! Implementation of the local backend.

use std::ffi::OsStr;
use std::fs;
use std::fs::File;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::unix::process::CommandExt;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::process::Command;
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
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
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
#[cfg(any(target_os = "linux", target_os = "macos"))]
struct TaskProcessGroup(Arc<std::sync::Mutex<Option<libc::pid_t>>>);

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Drop for TaskProcessGroup {
    fn drop(&mut self) {
        let mut ownership = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(id) = *ownership
            && terminate_process_group(id)
        {
            *ownership = None;
        }
    }
}

/// Terminates the process group rooted at a task's child process.
///
/// Returns true after successful termination or when the group is already gone.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn terminate_process_group(id: libc::pid_t) -> bool {
    // The child starts its own group, never Sprocket's group.
    if unsafe { libc::kill(-id, libc::SIGKILL) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            warn!("failed to terminate task process group {id}: {error}");
            return false;
        }
    }
    true
}

/// Reaps exactly this child, including when its async execution was dropped.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn wait_with_usage(
    mut child: std::process::Child,
    ownership: Arc<std::sync::Mutex<Option<libc::pid_t>>>,
) -> Result<(
    std::process::ExitStatus,
    crankshaft::events::TaskResourceUsage,
)> {
    use std::os::unix::process::ExitStatusExt;

    let pid = child.id().try_into().context("task PID is out of range")?;
    let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::uninit();
    let wait_error = loop {
        // Waiting must not hold the ownership lock: cancellation needs it to
        // kill a live child. WNOWAIT keeps the exited child's identity
        // anchored.
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                info.as_mut_ptr(),
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        if result == 0 {
            break None;
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            break Some(error);
        }
    };
    let mut ownership = ownership.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(error) = &wait_error {
        if error.raw_os_error() == Some(libc::ECHILD) {
            // No child identity remains anchored; signaling is unsafe.
            *ownership = None;
            return Err(std::io::Error::from_raw_os_error(libc::ECHILD))
                .with_context(|| format!("failed to await task process {pid}"));
        }
        if let Some(id) = *ownership {
            terminate_process_group(id);
        }
    } else {
        let info = unsafe { info.assume_init() };
        if matches!(info.si_code, libc::CLD_KILLED | libc::CLD_DUMPED)
            && let Some(id) = *ownership
        {
            terminate_process_group(id);
        }
    }
    // Disarm while still unreaped, under the same lock used by async Drop.
    *ownership = None;
    // Even an unusual waitid error cannot leave a live child blocking Drop.
    // Ownership is already disarmed; only this blocking waiter may reap.
    drop(ownership);
    let mut status = 0;
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    loop {
        // Only this waiter owns reaping the child. Both outputs are initialized
        // when wait4 returns the requested PID.
        let result = unsafe { libc::wait4(pid, &mut status, 0, usage.as_mut_ptr()) };
        if result == pid {
            if let Some(error) = wait_error {
                return Err(error).with_context(|| format!("failed to await task process {pid}"));
            }
            let usage = unsafe { usage.assume_init() };
            let millis = |time: libc::timeval| -> Result<u64> {
                let seconds = u64::try_from(time.tv_sec).context("invalid native CPU seconds")?;
                let micros =
                    u64::try_from(time.tv_usec).context("invalid native CPU microseconds")?;
                seconds
                    .checked_mul(1000)
                    .and_then(|seconds| seconds.checked_add(micros / 1000))
                    .context("native CPU time is out of range")
            };
            let mut measured = crankshaft::events::TaskResourceUsage::default();
            let user = millis(usage.ru_utime)?;
            let system = millis(usage.ru_stime)?;
            measured.user_cpu_time_ms = Some(user);
            measured.system_cpu_time_ms = Some(system);
            measured.cpu_time_ms = Some(
                user.checked_add(system)
                    .context("native CPU time is out of range")?,
            );
            let memory = u64::try_from(usage.ru_maxrss).context("invalid native peak RSS")?;
            #[cfg(target_os = "linux")]
            let memory = memory
                .checked_mul(1024)
                .context("native peak RSS is out of range")?;
            measured.max_memory = Some(memory);
            return Ok((std::process::ExitStatus::from_raw(status), measured));
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            if error.raw_os_error() != Some(libc::ECHILD) {
                // Retain reaping on an unexpected wait4 error. std Child drop
                // neither kills nor waits for the process.
                if let Err(reap_error) = child.wait() {
                    warn!("failed to reap task process {pid} after wait4 error: {reap_error}");
                }
            }
            return Err(error).with_context(|| format!("failed to reap task process {pid}"));
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
                );

            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            command.kill_on_drop(true);

            #[cfg(any(target_os = "linux", target_os = "macos"))]
            command.process_group(0);

            // Set the PATH variable for the child on Windows to get consistent
            // PATH searching. See: https://github.com/rust-lang/rust/issues/122660
            #[cfg(windows)]
            if let Ok(path) = std::env::var("PATH") {
                command.env("PATH", path);
            }

            let child = command.spawn().context("failed to spawn shell")?;
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            let pid = child.id();
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            let pid = child.id().expect("should have id");
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            let ownership = Arc::new(std::sync::Mutex::new(Some(
                pid.try_into().context("task PID is out of range")?,
            )));
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            let _process_group = TaskProcessGroup(ownership.clone());

            // Notify that the process has spawned
            send_event!(
                self.request.context.events().crankshaft(),
                CrankshaftEvent::TaskStarted { id }
            );

            info!(
                "spawned local shell process {pid} for execution of task `{name}`",
                name = self.name
            );

            #[cfg(any(target_os = "linux", target_os = "macos"))]
            let status = {
                let (status, usage) =
                    tokio::task::spawn_blocking(move || wait_with_usage(child, ownership))
                        .await
                        .context("task process waiter failed")??;
                send_event!(
                    self.request.context.events().crankshaft(),
                    CrankshaftEvent::TaskResourceUsage { id, usage }
                );
                status
            };
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            let status = {
                let mut child = child;
                child.wait().await.with_context(|| {
                    format!("failed to wait for termination of task child process {pid}")
                })?
            };

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
/// Linux and macOS report CPU time and peak resident memory through native
/// child accounting. Descendant CPU usage is included when intermediate
/// processes wait for their children. Peak RSS is not a summed, simultaneous
/// process-tree memory peak. Other platforms do not report CPU or memory usage.
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

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
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
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    async fn successful_cancellation_consumes_group_cleanup_before_reaping() {
        use std::os::unix::process::ExitStatusExt;

        let child = Command::new("/bin/sh")
            .args(["-c", "exec sleep 60"])
            .process_group(0)
            .spawn()
            .unwrap();
        let ownership = Arc::new(std::sync::Mutex::new(Some(child.id().try_into().unwrap())));
        drop(TaskProcessGroup(ownership.clone()));
        let cleanup_consumed = ownership.lock().unwrap().is_none();
        let (status, _) = wait_with_usage(child, ownership.clone()).unwrap();
        assert_eq!(status.signal(), Some(libc::SIGKILL));
        assert!(
            cleanup_consumed,
            "successful cancellation must consume cleanup ownership before the reaper can retry"
        );
        assert!(ownership.lock().unwrap().is_none());
    }

    #[tokio::test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    async fn waiter_disarms_group_before_reaping() {
        let child = Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .process_group(0)
            .spawn()
            .unwrap();
        let ownership = Arc::new(std::sync::Mutex::new(Some(child.id().try_into().unwrap())));
        let group = TaskProcessGroup(ownership.clone());
        let locked = ownership.lock().unwrap();
        let pid = locked.unwrap();
        let waiter_ownership = ownership.clone();
        let waiter = std::thread::spawn(move || wait_with_usage(child, waiter_ownership));
        // The waiter cannot reap while cleanup owns the lock, even after exit.
        let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::uninit();
        assert_eq!(
            unsafe {
                libc::waitid(
                    libc::P_PID,
                    pid as libc::id_t,
                    info.as_mut_ptr(),
                    libc::WEXITED | libc::WNOWAIT,
                )
            },
            0
        );
        drop(locked);
        waiter.join().unwrap().unwrap();
        let disarmed = ownership.lock().unwrap().is_none();
        drop(group);
        assert!(
            disarmed,
            "reaped child must no longer own a signalable group"
        );
    }

    #[tokio::test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    async fn native_resource_usage_reports_cpu_and_rss_for_nonzero_exits() {
        let root = tempfile::tempdir().unwrap();
        let events = Events::default().with_crankshaft(32);
        let mut receiver = events.subscribe_crankshaft().unwrap();
        let context = EvalContext::new(events, CancellationContext::default()).await;
        let request = ExecuteTaskRequest {
            context: &context,
            name: "native-usage",
            command: "i=0; while [ \"$i\" -lt 20000 ]; do i=$((i + 1)); done; exit 7",
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
        let result = LocalTask {
            config: Arc::new(Config::local()),
            request: &request,
            name: request.name.to_string(),
        }
        .run()
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result.exit_code, 7);
        let mut created = None;
        let mut usage = None;
        while let Ok(event) = receiver.try_recv() {
            match event {
                CrankshaftEvent::TaskCreated { id, .. } => created = Some(id),
                CrankshaftEvent::TaskResourceUsage {
                    id,
                    usage: measured,
                } => {
                    assert_eq!(Some(id), created);
                    usage = Some(measured);
                }
                _ => {}
            }
        }
        let usage = usage.expect("local execution did not report native resource usage");
        assert!(usage.cpu_time_ms.unwrap() > 0);
        assert!(usage.max_memory.unwrap() > 0);
        assert!(usage.user_cpu_time_ms.is_some());
        assert!(usage.system_cpu_time_ms.is_some());
        assert!(usage.avg_memory.is_none());
    }

    #[tokio::test]
    async fn cancellation_and_dropped_executions_stop_payloads() {
        for command in [
            "exec sh -c 'echo $$ > payload.pid; exec sleep 60'",
            "(exec sh -c 'echo $$ > payload.pid; exec sleep 60')\nexit $?",
            "sh -c 'echo $$ > payload.pid; exec sleep 60'\nexit $?",
        ] {
            for termination in ["cancel", "slow-cancel", "drop", "signal"] {
                let command = format!("echo $$ > shell.pid\n{command}");
                let root = tempfile::tempdir().unwrap();
                let cancellation = CancellationContext::new(if termination == "slow-cancel" {
                    FailureMode::Slow
                } else {
                    FailureMode::Fast
                });
                let context = EvalContext::new(Events::default(), cancellation.clone()).await;
                let request = ExecuteTaskRequest {
                    context: &context,
                    name: "cancel-test",
                    command: &command,
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

                if termination == "drop" {
                    drop(execution);
                } else if termination == "signal" {
                    let shell: libc::pid_t =
                        fs::read_to_string(request.work_dir().join("shell.pid"))
                            .unwrap()
                            .trim()
                            .parse()
                            .unwrap();
                    assert_eq!(unsafe { libc::kill(shell, libc::SIGTERM) }, 0);
                    assert!(
                        tokio::time::timeout(Duration::from_secs(10), execution)
                            .await
                            .expect("signal handling timed out")
                            .is_err()
                    );
                } else {
                    if termination == "slow-cancel" {
                        let _ = cancellation.cancel();
                        assert!(
                            tokio::time::timeout(Duration::from_millis(100), &mut execution)
                                .await
                                .is_err(),
                            "the first slow-mode cancellation must leave the task running"
                        );
                    }
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
                        let output = tokio::process::Command::new("ps")
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
                    "payload {pid} survived ({termination}): {command}"
                );
            }
        }
    }
}
