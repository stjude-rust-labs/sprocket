//! Experimental Slurm + Apptainer (aka Singularity) task execution backend.
//!
//! This experimental backend submits each task as an Slurm job which invokes
//! Apptainer to provide the appropriate container environment for the WDL
//! command to execute.
//!
//! Due to the difficulty of getting a Slurm test cluster spun up, and limited
//! ability to install Apptainer locally or in CI, this is currently tested by
//! hand; expect (and report) bugs! In follow-up work, we hope to build a
//! limited test suite based on mocking CLI invocations and/or golden testing of
//! generated `srun`/`apptainer` scripts.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::process::ExitStatus;
use std::process::Stdio;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Context as _;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use bytesize::ByteSize;
use crankshaft::engine::service::name::GeneratorIterator;
use crankshaft::engine::service::name::UniqueAlphanumeric;
use crankshaft::events::Event as CrankshaftEvent;
use crankshaft::events::send_event;
use futures::FutureExt;
use futures::future::BoxFuture;
use itertools::Itertools;
use nonempty::NonEmpty;
use tokio::fs;
use tokio::fs::File;
use tokio::process::Command;
use tokio::select;
use tokio::sync::Semaphore;
use tokio::sync::oneshot;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use tracing::error;
use tracing::trace;
use tracing::warn;

use super::ApptainerRuntime;
use super::TaskExecutionBackend;
use crate::CancellationContext;
use crate::EvaluationPath;
use crate::Events;
use crate::ONE_GIBIBYTE;
use crate::PrimitiveValue;
use crate::TaskInputs;
use crate::Value;
use crate::backend::ExecuteTaskRequest;
use crate::backend::INITIAL_EXPECTED_NAMES;
use crate::backend::TaskExecutionConstraints;
use crate::backend::TaskExecutionResult;
use crate::config::Config;
use crate::config::SlurmApptainerBackendConfig;
use crate::config::TaskResourceLimitBehavior;
use crate::http::Transferer;
use crate::v1::requirements;
use crate::v1::requirements::ContainerSource;

/// The name of the file where the Apptainer command invocation will be written.
const APPTAINER_COMMAND_FILE_NAME: &str = "apptainer_command";

/// The default monitor interval, in seconds.
const DEFAULT_MONITOR_INTERVAL: u64 = 30;

/// The default maximum concurrency for `sbatch` and `scancel` operations.
const DEFAULT_MAX_CONCURRENCY: u32 = 10;

/// Represents a Slurm job state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum JobState {
    /// The job was terminated due to node boot failure.
    BootFail,
    /// The job was canceled by the user or administrator.
    Canceled,
    /// The job completed successfully and finished with an exit code of 0.
    Completed,
    /// The job was terminated due to exceeding a deadline.
    Deadline,
    /// the job failed and finished with a non-zero exit code.
    Failed,
    /// The job was terminated due to node failure.
    NodeFail,
    /// The job was terminated due to out-of-memory conditions.
    OutOfMemory,
    /// The job is queued and waiting for initiation.
    Pending,
    /// The job was terminated due to being preempted.
    Preempted,
    /// The job is currently running.
    Running,
    /// The job is currently suspended.
    Suspended,
    /// The job was terminated due to reaching a time limit.
    Timeout,
}

impl JobState {
    /// Determines if the job is in a terminated state.
    fn terminated(&self) -> bool {
        matches!(
            self,
            Self::BootFail
                | Self::Canceled
                | Self::Completed
                | Self::Deadline
                | Self::Failed
                | Self::NodeFail
                | Self::OutOfMemory
                | Self::Preempted
                | Self::Timeout
        )
    }
}

impl fmt::Display for JobState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BootFail => write!(f, "node boot failure"),
            Self::Canceled => write!(f, "canceled"),
            Self::Completed => write!(f, "completed"),
            Self::Deadline => write!(f, "deadline reached"),
            Self::Failed => write!(f, "failed"),
            Self::NodeFail => write!(f, "node failure"),
            Self::OutOfMemory => write!(f, "out of memory"),
            Self::Pending => write!(f, "pending"),
            Self::Preempted => write!(f, "preempted"),
            Self::Running => write!(f, "running"),
            Self::Suspended => write!(f, "suspended"),
            Self::Timeout => write!(f, "timeout"),
        }
    }
}

impl FromStr for JobState {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        // See: https://slurm.schedmd.com/job_state_codes.html
        match s {
            "BOOT_FAIL" => Ok(Self::BootFail),
            "CANCELLED" => Ok(Self::Canceled),
            "COMPLETED" => Ok(Self::Completed),
            "DEADLINE" => Ok(Self::Deadline),
            "FAILED" => Ok(Self::Failed),
            "NODE_FAIL" => Ok(Self::NodeFail),
            "OUT_OF_MEMORY" => Ok(Self::OutOfMemory),
            "PENDING" => Ok(Self::Pending),
            "PREEMPTED" => Ok(Self::Preempted),
            "RUNNING" => Ok(Self::Running),
            "SUSPENDED" => Ok(Self::Suspended),
            "TIMEOUT" => Ok(Self::Timeout),
            _ => bail!("unknown Slurm job state `{s}"),
        }
    }
}

/// Represents a job exit code as output from `sacct`.
#[derive(Debug, Clone, Copy)]
struct JobExitCode {
    /// The exit code for the job when the job exited normally.
    exit_code: u8,
    /// The signal number when the job was terminated by a signal.
    ///
    /// A value of `0` indicates no signal.
    signal: u8,
}

impl JobExitCode {
    /// Gets a unified exit code of the job.
    ///
    /// If the job terminated from a signal, this will be 128 + the signal
    /// number.
    fn code(&self) -> u8 {
        if self.signal > 0 {
            128 + (self.signal & 0x7F)
        } else {
            self.exit_code
        }
    }

    /// Converts the job exit code into an `ExitStatus`.
    fn into_exit_status(self) -> ExitStatus {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt as _;
        #[cfg(windows)]
        use std::os::windows::process::ExitStatusExt as _;

        // See WEXITSTATUS from wait(2) to explain the shift and masks used here
        #[cfg(unix)]
        let status = if self.signal > 0 {
            ExitStatus::from_raw((self.signal as i32) & 0x7F)
        } else {
            ExitStatus::from_raw((self.exit_code as i32) << 8)
        };

        #[cfg(windows)]
        let status = ExitStatus::from_raw(self.exit_code as u32);

        status
    }
}

impl FromStr for JobExitCode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let (exit_code, signal) = s
            .split_once(':')
            .with_context(|| format!("invalid Slurm exit code `{s}`"))?;
        Ok(Self {
            exit_code: exit_code
                .parse()
                .with_context(|| "invalid exit code `{exit_code}`")?,
            signal: signal
                .parse()
                .with_context(|| "invalid signal number `{signal}`")?,
        })
    }
}

impl fmt::Display for JobExitCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.signal > 0 {
            write!(f, "signal number `{signal}`", signal = self.signal)
        } else {
            write!(f, "exit code `{code}`", code = self.exit_code)
        }
    }
}

/// The expected job record output by `sacct`.
#[derive(Debug)]
struct JobRecord<'a> {
    /// The Slurm job identifier.
    job_id: u64,
    /// The current state of the job.
    state: JobState,
    /// The exit code of the job.
    ///
    /// This is `None` if the job has not terminated.
    exit_code: Option<JobExitCode>,
    /// The total (system and user) CPU time used by the job.
    total_cpu: &'a str,
    /// The system CPU time used by the job.
    system_cpu: &'a str,
    /// The user CPU time used by the job.
    user_cpu: &'a str,
    /// The maximum virtual memory size of the job.
    max_vm_size: &'a str,
    /// The average virtual memory size of the job.
    avg_vm_size: &'a str,
}

impl<'a> JobRecord<'a> {
    /// Creates a new job record from a line of `sacct` output.
    ///
    /// This implementation expects a `|` delimited list of fields matching
    /// those returned by the `fields` method.
    ///
    /// Returns `Ok(None)` if the output line is for a job step and not the job
    /// itself.
    fn new(line: &'a str) -> Result<Option<Self>> {
        let mut parts = line.split('|');

        let job_id = parts
            .next()
            .context("`sacct` output is missing job identifier")?;

        // Ignore job identifiers that contain steps
        if job_id.contains('.') {
            return Ok(None);
        }

        // Parse the job id
        let job_id = job_id.parse().context("invalid Slurm job identifier")?;

        // Parse the job state
        let state: JobState = parts
            .next()
            .context("`sacct` output is missing job state")?
            .parse()?;

        // Parse the exit code field (if terminated, otherwise ignore)
        let exit_code = parts
            .next()
            .context("`sacct` output is missing exit code")?;
        let exit_code = if state.terminated() {
            Some(exit_code.parse()?)
        } else {
            None
        };

        // Get the statistics fields
        let total_cpu = parts.next().context("`sacct` output missing total CPU")?;
        let system_cpu = parts.next().context("`sacct` output missing system CPU")?;
        let user_cpu = parts.next().context("`sacct` output missing user CPU")?;
        let max_vm_size = parts
            .next()
            .context("`sacct` output missing maximum virtual memory size")?;
        let avg_vm_size = parts
            .next()
            .context("`sacct` output missing average virtual memory size")?;

        Ok(Some(Self {
            job_id,
            state,
            exit_code,
            total_cpu,
            system_cpu,
            user_cpu,
            max_vm_size,
            avg_vm_size,
        }))
    }

    /// Gets the fields that should be output by `sacct`.
    ///
    /// The field list must be kept in sync with the implementation of the `new`
    /// method.
    fn fields() -> &'static str {
        "JobID,State,ExitCode,TotalCPU,SystemCPU,UserCPU,MaxVMSize,AveVMSize"
    }
}

/// Represents information about a Slurm job for the monitor.
#[derive(Debug)]
struct Job {
    /// The Crankshaft identifier for the job.
    crankshaft_id: u64,
    /// The last known state of the job.
    state: JobState,
    /// The channel to notify of the completion of the job; provides the job's
    /// exit code.
    completed: oneshot::Sender<Result<JobExitCode>>,
}

/// State used by the Slurm task monitor.
#[derive(Debug)]
struct MonitorState {
    /// The name generator for tasks.
    names: GeneratorIterator<UniqueAlphanumeric>,
    /// The map of jobs being monitored.
    ///
    /// The key is the Slurm job identifier.
    jobs: HashMap<u64, Job>,
}

impl MonitorState {
    /// Constructs a new monitor state.
    fn new() -> Self {
        Self {
            names: GeneratorIterator::new(
                UniqueAlphanumeric::default_with_expected_generations(INITIAL_EXPECTED_NAMES),
                INITIAL_EXPECTED_NAMES,
            ),
            jobs: HashMap::new(),
        }
    }

    /// Adds a new job to the monitor state.
    fn add_job(
        &mut self,
        job_id: u64,
        crankshaft_id: u64,
        completed: oneshot::Sender<Result<JobExitCode>>,
    ) {
        let prev = self.jobs.insert(
            job_id,
            Job {
                crankshaft_id,
                state: JobState::Pending,
                completed,
            },
        );

        if prev.is_some() {
            warn!(
                "encountered duplicate Slurm job id `{job_id}`: tasks may not be monitored \
                 correctly"
            );
        }
    }

    /// Update the jobs based on the current output of `sacct`.
    ///
    /// This is also responsible for sending "task started" events.
    fn update_jobs(&mut self, output: &[u8], events: &Events) -> Result<()> {
        let output = str::from_utf8(output).map_err(|_| anyhow!("`sacct` output was not UTF-8"))?;

        for line in output.lines() {
            let Some(record) = JobRecord::new(line)? else {
                continue;
            };
            let Some(job) = self.jobs.get_mut(&record.job_id) else {
                continue;
            };

            if record.state != job.state {
                // If the job state is now running, send the started event
                if record.state == JobState::Running {
                    send_event!(
                        events.crankshaft(),
                        CrankshaftEvent::TaskStarted {
                            id: job.crankshaft_id
                        },
                    );
                }

                if record.state.terminated() {
                    // If the job was not already in a running state, send the
                    // started event now
                    if job.state != JobState::Running {
                        send_event!(
                            events.crankshaft(),
                            CrankshaftEvent::TaskStarted {
                                id: job.crankshaft_id
                            },
                        );
                    }

                    let exit_code = record
                        .exit_code
                        .expect("terminated job should have exit code");

                    debug!(
                        "Slurm job `{job_id}` has exited with {exit_code}: average virtual memory \
                         size `{avg_mem}`, maximum virtual memory size `{max_mem}`, total CPU \
                         used `{total_cpu}`, system CPU time `{system_cpu}`, user CPU time \
                         `{user_cpu}`",
                        job_id = record.job_id,
                        avg_mem = record.avg_vm_size,
                        max_mem = record.max_vm_size,
                        total_cpu = record.total_cpu,
                        system_cpu = record.system_cpu,
                        user_cpu = record.user_cpu,
                    );

                    let job = self.jobs.remove(&record.job_id).unwrap();
                    let _ = job.completed.send(Ok(exit_code));
                    continue;
                } else {
                    debug!(
                        "Slurm job `{id}` is now in the `{state}` state",
                        id = record.job_id,
                        state = record.state
                    );
                }

                job.state = record.state;
            }
        }

        Ok(())
    }

    /// Fails all currently monitored jobs with the given error.
    fn fail_all_jobs(&mut self, error: &anyhow::Error) {
        for (_, job) in self.jobs.drain() {
            let _ = job.completed.send(Err(anyhow!("{error:#}")));
        }
    }
}

/// Represents a submitted Slurm job.
#[derive(Debug)]
struct SubmittedJob {
    /// The identifier for the Slurm job.
    id: u64,
    /// The task name for Crankshaft events.
    ///
    /// Note: this name differs from the job name used in `sbatch`.
    task_name: String,
    /// The receiver for when the job completes.
    completed: oneshot::Receiver<Result<JobExitCode>>,
}

/// The monitor is responsible for periodically querying Slurm for job state and
/// sending task events.
#[derive(Debug, Clone)]
struct Monitor {
    /// The state of the monitor.
    state: Arc<Mutex<MonitorState>>,
    /// A sender for notifying that the monitor has been dropped.
    _drop: Arc<oneshot::Sender<()>>,
}

impl Monitor {
    /// Constructs a new Slurm monitor using the given update interval.
    fn new(interval: Duration, events: Events) -> Self {
        let (tx, rx) = oneshot::channel();
        let state = Arc::new(Mutex::new(MonitorState::new()));
        tokio::spawn(Self::monitor(state.clone(), interval, events, rx));

        Self {
            state,
            _drop: Arc::new(tx),
        }
    }

    /// Submits a new Slurm job with the monitor by spawning `sbatch`.
    ///
    /// Upon success, returns information about the submitted job.
    async fn submit_job(
        &self,
        config: &SlurmApptainerBackendConfig,
        request: &ExecuteTaskRequest<'_>,
        crankshaft_id: u64,
        command_path: &Path,
    ) -> Result<SubmittedJob> {
        let task_name = {
            let mut state = self.state.lock().expect("failed to lock state");

            let task_name = format!(
                "{id}-{generated}",
                id = request.id,
                generated = state
                    .names
                    .next()
                    .expect("generator should never be exhausted")
            );

            task_name
        };

        let mut command = Command::new("sbatch");

        // If a Slurm partition has been configured, specify it. Otherwise, the job will
        // end up on the cluster's default partition.
        if let Some(partition) =
            config.slurm_partition_for_task(request.requirements, request.hints)
        {
            command.arg("--partition").arg(&partition.name);
        }

        // If GPUs are required, use the gpu helper to determine the count and pass it
        // to `sbatch` via `--gpus-per-task`.
        if let Some(gpu_count) =
            requirements::gpu(request.inputs, request.requirements, request.hints)
        {
            command.arg(format!("--gpus-per-task={gpu_count}"));
        }

        // Add any user-configured extra arguments.
        if let Some(args) = &config.extra_sbatch_args {
            command.args(args);
        }

        // Format a name for the Slurm job
        let job_name = format!(
            "{prefix}{sep}{task_name}",
            prefix = config.job_name_prefix.as_deref().unwrap_or(""),
            sep = if config.job_name_prefix.is_some() {
                "-"
            } else {
                ""
            }
        );

        // The path for the Slurm-level stdout and stderr. This primarily contains the
        // job report, as we redirect Apptainer and WDL output separately.
        let slurm_stdout_path = request.attempt_dir.join("slurm.stdout");
        let slurm_stderr_path = request.attempt_dir.join("slurm.stderr");

        command
            .arg("--job-name")
            .arg(&job_name)
            // Send Slurm job stdout and stderr streams to these files. Since we redirect the
            // Apptainer invocation's stdio to separate files, this will typically not contain
            // anything, but can be useful for debugging if the scripts get modified.
            .arg("-o")
            .arg(slurm_stdout_path)
            .arg("-e")
            .arg(slurm_stderr_path)
            // An explicit task count is required for some options
            .arg("--ntasks=1")
            // CPU request is rounded up to the nearest whole CPU
            .arg(format!(
                "--cpus-per-task={}",
                request.constraints.cpu.ceil() as u64
            ))
            // Memory request is specified per node in mebibytes; we round the request up to the
            // next mebibyte.
            //
            // Note that the Slurm documentation says "megabyte" (i.e., the base-10 unit), but
            // the other explanations of the unit suffixes in the first-party documentation show
            // the use of base-2 units, and multiple third-party sources available through
            // online searches back the base-2 interpretation, for example:
            //
            // https://info.nrao.edu/computing/guide/cluster-processing/appendix/memory-options
            // https://wcmscu.atlassian.net/wiki/spaces/WIKI/pages/327731/Using+Slurm
            .arg(format!(
                "--mem={}M",
                (request.constraints.memory as f64 / bytesize::MIB as f64).ceil() as u64
            ))
            .arg(command_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        trace!(?command, "spawning `sbatch` to queue task");

        let child = command.spawn().context("failed to spawn `sbatch`")?;
        let output = child
            .wait_with_output()
            .await
            .context("failed to wait for `sbatch` to exit")?;
        if !output.status.success() {
            bail!(
                "failed to submit Slurm job with `sbatch` ({status})\n{stderr}",
                status = output.status,
                stderr = str::from_utf8(&output.stderr)
                    .unwrap_or("<output not UTF-8>")
                    .trim()
            );
        }

        let stdout =
            str::from_utf8(&output.stdout).map_err(|_| anyhow!("`sbatch` output was not UTF-8"))?;

        let mut job_id = None;
        for line in stdout.lines() {
            if let Some(id) = line.trim().strip_prefix("Submitted batch job ") {
                job_id = Some(
                    id.parse()
                        .context("`sbatch` returned an invalid job identifier")?,
                );
            }
        }

        let job_id = job_id.context("`sbatch` did not output a job identifier")?;

        debug!("task `{task_name}` was queued as Slurm job `{job_id}`");

        let (tx, rx) = oneshot::channel();
        let mut state = self.state.lock().expect("failed to lock state");
        state.add_job(job_id, crankshaft_id, tx);
        drop(state);

        Ok(SubmittedJob {
            id: job_id,
            task_name,
            completed: rx,
        })
    }

    /// Runs the monitoring loop
    async fn monitor(
        state: Arc<Mutex<MonitorState>>,
        interval: Duration,
        events: Events,
        mut drop: oneshot::Receiver<()>,
    ) {
        debug!(
            "Slurm task monitor is starting with polling interval of {interval} seconds",
            interval = interval.as_secs()
        );

        // The timer for reading Slurm job state
        let mut timer = tokio::time::interval(interval);
        timer.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            select! {
                _ = &mut drop => break,
                _ = timer.tick() => {
                    let jobs = {
                        // If there are no jobs to monitor, do nothing
                        let state = state.lock().expect("failed to lock state");
                        if state.jobs.is_empty() {
                            continue;
                        }

                        state.jobs.keys().join(",")
                    };

                    match Self::read_jobs(&jobs).await.context("failed to query jobs with `sacct`") {
                        Ok(output) => {
                            let mut state = state.lock().expect("failed to lock state");
                            if let Err(e) = state.update_jobs(&output, &events) {
                                state.fail_all_jobs(&e);
                            }
                        }
                        Err(e) => {
                            let mut state = state.lock().expect("failed to lock state");
                            state.fail_all_jobs(&e);
                        }
                    }
                }
            }
        }

        debug!("Slurm task monitor has shut down");
    }

    /// Reads the current jobs using `sacct`.
    ///
    /// Returns the stdout of `sacct`.
    async fn read_jobs(jobs: &str) -> Result<Vec<u8>> {
        let mut command = Command::new("sacct");
        let command = command
            .arg("-P") // parseable
            .arg("-n") // no header
            .arg("--format") // JSON output
            .arg(JobRecord::fields())
            .arg("-j")
            .arg(jobs)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        trace!(?command, "spawning `sacct` to monitor tasks");

        let child = command.spawn().context("failed to spawn `sacct` command")?;

        let output = child
            .wait_with_output()
            .await
            .context("failed to wait for `sacct` to exit")?;
        if !output.status.success() {
            bail!(
                "`sacct` failed: {status}: {stderr}",
                status = output.status,
                stderr = str::from_utf8(&output.stderr)
                    .unwrap_or("<output not UTF-8>")
                    .trim()
            );
        }

        Ok(output.stdout)
    }
}

/// The experimental Slurm + Apptainer backend.
///
/// See the module-level documentation for details.
pub struct SlurmApptainerBackend {
    /// The shared engine configuration.
    config: Arc<Config>,
    /// The engine events.
    events: Events,
    /// The evaluation cancellation context.
    cancellation: CancellationContext,
    /// The underlying Apptainer runtime to use.
    apptainer: ApptainerRuntime,
    /// The Slurm task monitor.
    monitor: Monitor,
    /// The permits for `sbatch` and `scancel` operations.
    permits: Semaphore,
}

impl SlurmApptainerBackend {
    /// Create a new backend.
    pub fn new(
        config: Arc<Config>,
        run_root_dir: &Path,
        events: Events,
        cancellation: CancellationContext,
    ) -> Result<Self> {
        // Ensure the configured backend is Slurm Apptainer
        let backend_config = config.backend()?;

        let backend_config = backend_config
            .as_slurm_apptainer()
            .context("configured backend is not Slurm Apptainer")?;

        let monitor = Monitor::new(
            Duration::from_secs(backend_config.interval.unwrap_or(DEFAULT_MONITOR_INTERVAL)),
            events.clone(),
        );

        let permits = Semaphore::new(
            backend_config
                .max_concurrency
                .unwrap_or(DEFAULT_MAX_CONCURRENCY) as usize,
        );

        Ok(Self {
            config,
            events,
            cancellation,
            apptainer: ApptainerRuntime::new(run_root_dir)?,
            monitor,
            permits,
        })
    }

    /// Kills the given Slurm job.
    async fn kill_job(&self, job_id: u64) -> Result<()> {
        let mut command = Command::new("scancel");
        let command = command
            .arg(job_id.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let _permit = self
            .permits
            .acquire()
            .await
            .context("failed to acquire permit for canceling job")?;

        trace!(?command, "spawning `scancel` to cancel task");

        let mut child = command
            .spawn()
            .context("failed to spawn `scancel` command")?;
        let status = child.wait().await.context("failed to wait for `scancel`")?;
        if !status.success() {
            bail!("`scancel` failed: {status}");
        }

        Ok(())
    }
}

impl TaskExecutionBackend for SlurmApptainerBackend {
    fn constraints(
        &self,
        inputs: &TaskInputs,
        requirements: &HashMap<String, Value>,
        hints: &HashMap<String, crate::Value>,
    ) -> Result<TaskExecutionConstraints> {
        let mut required_cpu = requirements::cpu(inputs, requirements);
        let mut required_memory = ByteSize::b(requirements::memory(inputs, requirements)? as u64);

        let backend_config = self.config.backend()?;
        let backend_config = backend_config
            .as_slurm_apptainer()
            .expect("configured backend is not Slurm Apptainer");

        // Determine whether CPU or memory limits are set for this partition, and clamp
        // or deny them as appropriate if the limits are exceeded
        //
        // TODO ACF 2025-10-16: refactor so that we're not duplicating logic here (for
        // the in-WDL `task` values) and below in `spawn` (for the actual
        // resource request)
        if let Some(partition) = backend_config.slurm_partition_for_task(requirements, hints) {
            if let Some(max_cpu) = partition.max_cpu_per_task
                && required_cpu > max_cpu as f64
            {
                let env_specific = if self.config.suppress_env_specific_output {
                    String::new()
                } else {
                    format!(", but the execution backend has a maximum of {max_cpu}",)
                };
                match self.config.task.cpu_limit_behavior {
                    TaskResourceLimitBehavior::TryWithMax => {
                        warn!(
                            "task requires at least {required_cpu} CPU{s}{env_specific}",
                            s = if required_cpu == 1.0 { "" } else { "s" },
                        );
                        // clamp the reported constraint to what's available
                        required_cpu = max_cpu as f64;
                    }
                    TaskResourceLimitBehavior::Deny => {
                        bail!(
                            "task requires at least {required_cpu} CPU{s}{env_specific}",
                            s = if required_cpu == 1.0 { "" } else { "s" },
                        );
                    }
                }
            }
            if let Some(max_memory) = partition.max_memory_per_task
                && required_memory > max_memory
            {
                let env_specific = if self.config.suppress_env_specific_output {
                    String::new()
                } else {
                    format!(
                        ", but the execution backend has a maximum of {max_memory} GiB",
                        max_memory = max_memory.as_u64() as f64 / ONE_GIBIBYTE
                    )
                };
                match self.config.task.memory_limit_behavior {
                    TaskResourceLimitBehavior::TryWithMax => {
                        warn!(
                            "task requires at least {required_memory} GiB of memory{env_specific}",
                            required_memory = required_memory.as_u64() as f64 / ONE_GIBIBYTE
                        );
                        // clamp the reported constraint to what's available
                        required_memory = max_memory;
                    }
                    TaskResourceLimitBehavior::Deny => {
                        bail!(
                            "task requires at least {required_memory} GiB of memory{env_specific}",
                            required_memory = required_memory.as_u64() as f64 / ONE_GIBIBYTE
                        );
                    }
                }
            }
        }

        let container =
            requirements::container(inputs, requirements, self.config.task.container.as_deref());
        if let ContainerSource::Unknown(_) = &container {
            bail!(
                "Slurm Apptainer backend does not support unknown container source `{container:#}`"
            )
        }

        Ok(super::TaskExecutionConstraints {
            container: Some(container),
            // TODO ACF 2025-10-13: populate more meaningful values for these based on the given
            // Slurm partition.
            //
            // sinfo -p <partition> -s --json | jq .sinfo[0].cpus
            // sinfo -p <partition> -s --json | jq .sinfo[0].memory
            cpu: required_cpu,
            memory: required_memory.as_u64(),
            // TODO ACF 2025-10-16: these are almost certainly wrong
            gpu: Default::default(),
            fpga: Default::default(),
            disks: Default::default(),
        })
    }

    fn execute<'a>(
        &'a self,
        _: &'a Arc<dyn Transferer>,
        request: ExecuteTaskRequest<'a>,
    ) -> BoxFuture<'a, Result<Option<TaskExecutionResult>>> {
        async move {
            let backend_config = self.config.backend()?;
            let backend_config = backend_config
                .as_slurm_apptainer()
                .expect("configured backend is not Slurm Apptainer");

            // Create the working directory.
            let work_dir = request.work_dir();
            fs::create_dir_all(&work_dir).await.with_context(|| {
                format!(
                    "failed to create working directory `{path}`",
                    path = work_dir.display()
                )
            })?;

            // Create an empty file for the task's stdout.
            let stdout_path = request.stdout_path();
            let _ = File::create(&stdout_path).await.with_context(|| {
                format!(
                    "failed to create stdout file `{path}`",
                    path = stdout_path.display()
                )
            })?;

            // Create an empty file for the task's stderr.
            let stderr_path = request.stderr_path();
            let _ = File::create(&stderr_path).await.with_context(|| {
                format!(
                    "failed to create stderr file `{path}`",
                    path = stderr_path.display()
                )
            })?;

            // Write the evaluated WDL command section to a host file.
            let command_path = request.command_path();
            fs::write(&command_path, request.command)
                .await
                .with_context(|| {
                    format!(
                        "failed to write command contents to `{path}`",
                        path = command_path.display()
                    )
                })?;

            let Some(apptainer_script) = self
                .apptainer
                .generate_script(
                    &self.config,
                    &request,
                    backend_config
                        .apptainer_config
                        .extra_apptainer_exec_args
                        .as_deref()
                        .unwrap_or_default()
                        .iter()
                        .map(String::as_str),
                    self.cancellation.first(),
                )
                .await?
            else {
                return Ok(None);
            };

            let apptainer_command_path = request.attempt_dir.join(APPTAINER_COMMAND_FILE_NAME);
            fs::write(&apptainer_command_path, apptainer_script)
                .await
                .with_context(|| {
                    format!(
                        "failed to write Apptainer command file `{}`",
                        apptainer_command_path.display()
                    )
                })?;

            // Ensure the command files are executable
            #[cfg(unix)]
            {
                use std::fs::Permissions;
                use std::os::unix::fs::PermissionsExt;

                fs::set_permissions(&command_path, Permissions::from_mode(0o770)).await?;
                fs::set_permissions(&apptainer_command_path, Permissions::from_mode(0o770)).await?;
            }

            let crankshaft_id = crankshaft::events::next_task_id();

            let permit = self
                .permits
                .acquire()
                .await
                .context("failed to acquire permit for submitting job")?;

            let job = self.monitor.submit_job(backend_config, &request, crankshaft_id, &apptainer_command_path).await?;
            drop(permit);

            let name = job.task_name;
            let job_id = job.id;

            // Create a task-specific cancellation token that is independent of the overall
            // cancellation context
            let task_token = CancellationToken::new();
            send_event!(
                self.events.crankshaft(),
                CrankshaftEvent::TaskCreated {
                    id: crankshaft_id,
                    name: name.clone(),
                    tes_id: None,
                    token: task_token.clone(),
                },
            );

            let cancelled = async {
                send_event!(
                    self.events.crankshaft(),
                    CrankshaftEvent::TaskCanceled { id: crankshaft_id },
                );

                self.kill_job(job_id).await
            };

            let token = self.cancellation.second();
            let exit_code = tokio::select! {
                _ = task_token.cancelled() => {
                    if let Err(e) = cancelled.await {
                        error!("failed to cancel task `{name}` (LSF job `{job_id}`): {e:#}");
                    }

                    return Ok(None);
                }
                _ = token.cancelled() => {
                    if let Err(e) = cancelled.await {
                        error!("failed to cancel task `{name}` (LSF job `{job_id}`): {e:#}");
                    }

                    return Ok(None);
                }
                result = job.completed => match result.context("failed to wait for task to complete")? {
                    Ok(exit_code) => {
                        let exit_status = exit_code.into_exit_status();

                        send_event!(
                            self.events.crankshaft(),
                            CrankshaftEvent::TaskCompleted {
                                id: crankshaft_id,
                                exit_statuses: NonEmpty::new(exit_status),
                            }
                        );

                        exit_code.code()
                    },
                    Err(e) => {
                        send_event!(
                            self.events.crankshaft(),
                            CrankshaftEvent::TaskFailed {
                                id: crankshaft_id,
                                message: format!("{e:#}"),
                            },
                        );

                        return Err(e);
                    }
                }
            };

            Ok(Some(TaskExecutionResult {
                exit_code: exit_code as i32,
                work_dir: EvaluationPath::from_local_path(work_dir),
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
            }))

                }
        .boxed()
    }
}
