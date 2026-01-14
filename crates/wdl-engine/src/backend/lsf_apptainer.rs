//! Experimental LSF + Apptainer (aka Singularity) task execution backend.
//!
//! This experimental backend submits each task as an LSF job which invokes
//! Apptainer to provide the appropriate container environment for the WDL
//! command to execute.
//!
//! Due to the proprietary nature of LSF, and limited ability to install
//! Apptainer locally or in CI, this is currently tested by hand; expect (and
//! report) bugs! In follow-up work, we hope to build a limited test suite based
//! on mocking CLI invocations and/or golden testing of generated
//! `bsub`/`apptainer` scripts.

use std::collections::HashMap;
use std::fmt;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt as _;
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt as _;
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
use cloud_copy::Alphanumeric;
use crankshaft::engine::service::name::GeneratorIterator;
use crankshaft::engine::service::name::UniqueAlphanumeric;
use crankshaft::events::Event as CrankshaftEvent;
use crankshaft::events::send_event;
use futures::FutureExt;
use futures::future::BoxFuture;
use nonempty::NonEmpty;
use serde::Deserialize;
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
use tracing::warn;

use super::TaskExecutionBackend;
use crate::CancellationContext;
use crate::EvaluationPath;
use crate::Events;
use crate::ONE_GIBIBYTE;
use crate::PrimitiveValue;
use crate::TaskInputs;
use crate::Value;
use crate::backend::ApptainerRuntime;
use crate::backend::ExecuteTaskRequest;
use crate::backend::INITIAL_EXPECTED_NAMES;
use crate::backend::TaskExecutionConstraints;
use crate::backend::TaskExecutionResult;
use crate::config::Config;
use crate::config::LsfApptainerBackendConfig;
use crate::config::TaskResourceLimitBehavior;
use crate::http::Transferer;
use crate::v1::requirements;
use crate::v1::requirements::ContainerSource;

/// The name of the file where the Apptainer command invocation will be written.
const APPTAINER_COMMAND_FILE_NAME: &str = "apptainer_command";

/// The maximum length of an LSF job name, in *bytes*.
///
/// See <https://www.ibm.com/docs/en/spectrum-lsf/10.1.0?topic=o-j>.
const LSF_JOB_NAME_MAX_LENGTH: usize = 4094;

/// The default monitor interval, in seconds.
const DEFAULT_MONITOR_INTERVAL: u64 = 30;

/// The default maximum concurrency for `bsub` and `bkill` operations.
const DEFAULT_MAX_CONCURRENCY: u32 = 10;

/// The length, in bytes, of the generated monitor tag.
const MONITOR_TAG_LENGTH: usize = 10;

/// Truncates an LSF job name if the size of the job name exceeds the maximum.
///
/// Note: LSF job names do not need to be unique.
fn truncate_job_name(name: &str) -> &str {
    if name.len() < LSF_JOB_NAME_MAX_LENGTH {
        return name;
    }

    // Find the index of the character that won't fit
    let index = name
        .char_indices()
        .find_map(|(i, c)| {
            if (i + c.len_utf8()) >= LSF_JOB_NAME_MAX_LENGTH {
                Some(i)
            } else {
                None
            }
        })
        .expect("should have index");

    &name[0..index]
}

/// Represents an LSF job state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum JobState {
    /// The job is pending.
    Pending,
    /// The job is running.
    Running,
    /// The job is done (i.e. exited zero).
    Done,
    /// The job is suspended.
    Suspended,
    /// The job exited with a non-zero status.
    Exited,
}

impl JobState {
    /// Determines if the job is in a terminated state.
    fn terminated(&self) -> bool {
        matches!(self, Self::Done | Self::Exited)
    }
}

impl fmt::Display for JobState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Running => write!(f, "running"),
            Self::Done => write!(f, "done"),
            Self::Suspended => write!(f, "suspended"),
            Self::Exited => write!(f, "exited"),
        }
    }
}

impl FromStr for JobState {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        // See: https://www.ibm.com/docs/en/spectrum-lsf/10.1.0?topic=execution-about-job-states
        match s {
            "PEND" => Ok(Self::Pending),
            "RUN" => Ok(Self::Running),
            "DONE" => Ok(Self::Done),
            "PSUSP" | "USUSP" | "SSUSP" => Ok(Self::Suspended),
            "EXIT" => Ok(Self::Exited),
            _ => bail!("unknown LSF job state `{s}"),
        }
    }
}

/// The expected job record structure output by `bjobs`.
#[derive(Deserialize)]
struct JobRecord {
    /// The LSF job id.
    #[serde(rename = "JOBID")]
    job_id: String,
    /// The job state.
    #[serde(rename = "STAT")]
    state: String,
    /// The job exit code (may be empty).
    #[serde(rename = "EXIT_CODE")]
    exit_code: String,
}

/// Represents information about an LSF job.
#[derive(Debug)]
struct Job {
    /// The current tick count of the job.
    tick: u64,
    /// The Crankshaft identifier for the job.
    crankshaft_id: u64,
    /// The last known state of the job.
    state: JobState,
    /// The channel to notify of the completion of the job; provides the job's
    /// exit code.
    completed: oneshot::Sender<Result<u8>>,
}

#[derive(Debug)]
struct MonitorState {
    /// The name generator for tasks.
    names: GeneratorIterator<UniqueAlphanumeric>,
    /// The current tick count of the monitor.
    ///
    /// This is used to cheaply keep track of jobs that aren't in `bjobs`
    /// output.
    tick: u64,
    /// The current tag used for grouping tasks being monitored together.
    tag: String,
    /// The map of jobs being monitored.
    ///
    /// The key is the LSF job identifier.
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
            tick: 0,
            tag: String::new(),
            jobs: HashMap::new(),
        }
    }

    /// Gets the current tag to use for jobs.
    ///
    /// If there is no tag currently, a new tag is created.
    fn current_tag(&mut self) -> &str {
        // Create a new tag if there isn't one
        if self.tag.is_empty() {
            self.tag = Alphanumeric::new(MONITOR_TAG_LENGTH).to_string();
        }

        &self.tag
    }

    /// Adds a new job to the monitor state.
    fn add_job(&mut self, job_id: u64, crankshaft_id: u64, completed: oneshot::Sender<Result<u8>>) {
        let tick = self.tick;
        let prev = self.jobs.insert(
            job_id,
            Job {
                tick,
                crankshaft_id,
                state: JobState::Pending,
                completed,
            },
        );

        if prev.is_some() {
            warn!(
                "encountered duplicate LSF job id `{job_id}`: tasks may not be monitored correctly"
            );
        }
    }

    /// Update the jobs based on the current job records.
    ///
    /// This is also responsible for sending "task started" events.
    fn update_jobs(&mut self, records: Vec<JobRecord>, events: &Events) {
        let tick = self.tick;

        for record in records {
            let Ok(job_id) = record.job_id.parse() else {
                warn!(
                    "LSF task monitor encountered invalid job identifier `{id}`",
                    id = record.job_id
                );
                continue;
            };

            let Some(job) = self.jobs.get_mut(&job_id) else {
                // Ignore unknown jobs
                continue;
            };

            match record.state.parse::<JobState>() {
                Ok(job_state) => {
                    if job.state != job_state {
                        debug!("LSF job `{job_id}` is now in the `{job_state}` state");

                        // If the job state is now running, send the started event
                        if job_state == JobState::Running {
                            send_event!(
                                events.crankshaft(),
                                CrankshaftEvent::TaskStarted {
                                    id: job.crankshaft_id
                                },
                            );
                        }

                        if job_state.terminated() {
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

                            let job = self.jobs.remove(&job_id).unwrap();
                            let _ = job
                                .completed
                                .send(Ok(record.exit_code.parse().unwrap_or_default()));
                            continue;
                        }

                        job.state = job_state;
                    }

                    job.tick = tick;
                }
                Err(e) => {
                    let job = self.jobs.remove(&job_id).unwrap();
                    let _ = job.completed.send(Err(e));
                }
            }
        }

        // Every job must have been updated otherwise we cannot monitor it
        for (id, job) in self.jobs.extract_if(|_, j| j.tick != tick) {
            let _ = job.completed.send(Err(anyhow!(
                "LSF job `{id}` was missing from `bjobs` output: cannot monitor associated task"
            )));
        }

        // Reset the current tag if there are no more jobs being monitored
        if self.jobs.is_empty() {
            self.tag.clear();
        }
    }

    /// Fails all currently monitored jobs with the given error.
    fn fail_all_jobs(&mut self, error: &anyhow::Error) {
        for (_, job) in self.jobs.drain() {
            let _ = job.completed.send(Err(anyhow!("{error:#}")));
        }

        // Reset the current tag
        self.tag.clear();
    }
}

/// Represents a submitted LSF job.
#[derive(Debug)]
struct SubmittedJob {
    /// The identifier for the LSF job.
    id: u64,
    /// The task name for Crankshaft events.
    ///
    /// Note: this name differs from the job name used in `bsub`.
    task_name: String,
    /// The reciever for when the job completes.
    completed: oneshot::Receiver<Result<u8>>,
}

/// The monitor is responsible for
#[derive(Debug, Clone)]
struct Monitor {
    /// The state of the monitor.
    state: Arc<Mutex<MonitorState>>,
    /// A sender for notifying that the monitor has been dropped.
    _drop: Arc<oneshot::Sender<()>>,
}

impl Monitor {
    /// Constructs a new LSF monitor using the given update interval.
    fn new(interval: Duration, job_name_prefix: Option<String>, events: Events) -> Self {
        let (tx, rx) = oneshot::channel();
        let state = Arc::new(Mutex::new(MonitorState::new()));
        tokio::spawn(Self::monitor(
            state.clone(),
            interval,
            job_name_prefix,
            events,
            rx,
        ));

        Self {
            state,
            _drop: Arc::new(tx),
        }
    }

    /// Submits a new LSF job with the monitor by spawning `bsub`.
    ///
    /// Upon success, returns information about the submitted job.
    async fn submit_job(
        &self,
        config: &LsfApptainerBackendConfig,
        request: &ExecuteTaskRequest<'_>,
        crankshaft_id: u64,
        command_path: &Path,
    ) -> Result<SubmittedJob> {
        let (task_name, tag) = {
            let mut state = self.state.lock().expect("failed to lock state");

            let task_name = format!(
                "{id}-{generated}",
                id = request.id,
                generated = state
                    .names
                    .next()
                    .expect("generator should never be exhausted")
            );

            (task_name, state.current_tag().to_string())
        };

        let mut command = Command::new("bsub");

        // Set the queue to use if specified in configuration
        if let Some(queue) = config.lsf_queue_for_task(request.requirements, request.hints) {
            command.arg("-q").arg(queue.name());
        }

        // If GPUs are required, pass a basic `-gpu` flag to `bsub`.
        if let Some(gpu) = requirements::gpu(request.inputs, request.requirements, request.hints) {
            command.arg("-gpu").arg(format!("num={gpu}/host"));
        }

        // Add any user-configured extra arguments.
        if let Some(args) = &config.extra_bsub_args {
            command.args(args);
        }

        // Format a name for the LSF job; job names do not have to be unique, but we
        // should not truncate the prefix or tag
        let job_name = format!(
            "{prefix}{sep}{tag}-{task_name}",
            prefix = config.job_name_prefix.as_deref().unwrap_or(""),
            sep = if config.job_name_prefix.is_some() {
                "-"
            } else {
                ""
            }
        );

        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // TODO ACF 2025-09-10: make this configurable; hardcode turning off LSF email spam
            // for now though.
            .env("LSB_JOB_REPORT_MAIL", "N")
            // Set the job name
            .arg("-J")
            .arg(truncate_job_name(&job_name))
            // Send LSF job stdout and stderr streams to these files. Since we redirect the
            // Apptainer invocation's stdio to separate files, this will typically amount to the
            // LSF job report.
            .arg("-oo")
            .arg(request.attempt_dir.join("job.%J.stdout"))
            .arg("-eo")
            .arg(request.attempt_dir.join("job.%J.stderr"))
            // CPU request is rounded up to the nearest whole CPU
            .arg("-R")
            .arg(format!(
                "affinity[cpu({cpu})]",
                cpu = request.constraints.cpu.ceil() as u64
            ))
            // Memory request is specified per job to avoid ambiguity on clusters which may be
            // configured to interpret memory requests as per-core or per-task. We also use an
            // explicit KB unit which LSF appears to interpret as base-2 kibibytes.
            .arg("-R")
            .arg(format!(
                "rusage[mem={memory_kb}KB/job]",
                memory_kb = request.constraints.memory / bytesize::KIB,
            ))
            .arg(command_path);

        debug!(?command, "spawning `bsub` to queue task");

        let child = command.spawn().context("failed to spawn `bsub`")?;
        let output = child
            .wait_with_output()
            .await
            .context("failed to wait for `bsub` to exit")?;
        if !output.status.success() {
            bail!(
                "`bsub` failed: {status}: {stderr}",
                status = output.status,
                stderr = str::from_utf8(&output.stderr)
                    .unwrap_or("<output not UTF-8>")
                    .trim()
            );
        }

        let stdout =
            str::from_utf8(&output.stdout).map_err(|_| anyhow!("`bsub` output was not UTF-8"))?;

        // Parse out the job id from the output, which is surrounded by `<` and `>`
        let job_id: u64 = stdout
            .split(' ')
            .nth(1)
            .and_then(|id| {
                id.trim_start_matches('<')
                    .trim_end_matches('>')
                    .parse()
                    .ok()
            })
            .context("`bsub` output did not contain the job identifier")?;

        debug!("task `{task_name}` was queued as LSF job `{job_id}`");

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
        job_name_prefix: Option<String>,
        events: Events,
        mut drop: oneshot::Receiver<()>,
    ) {
        debug!(
            "LSF task monitor is starting with polling interval of {interval} seconds",
            interval = interval.as_secs()
        );

        // The timer for reading LSF task state
        let mut timer = tokio::time::interval(interval);
        timer.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            select! {
                _ = &mut drop => break,
                _ = timer.tick() => {
                    let search_prefix = {
                        let mut state = state.lock().expect("failed to lock state");

                        // If there are no jobs to monitor, do nothing
                        if state.jobs.is_empty() {
                            continue;
                        }

                        // Increment the tick count
                        state.tick = state.tick.wrapping_add(1);

                        // Format the search prefix for reading the jobs
                        assert!(!state.tag.is_empty(), "tag should not be empty");
                        format!(
                            "{prefix}{sep}{tag}*",
                            prefix = job_name_prefix.as_deref().unwrap_or(""),
                            sep = if job_name_prefix.is_some() {
                                "-"
                            } else {
                                ""
                            },
                            tag = state.tag
                        )
                    };

                    // Read the records using `bjobs` and then update the state
                    let result = Self::read_job_records(&search_prefix).await.context("failed to query job status using `bjobs`");
                    let mut state = state.lock().expect("failed to lock state");
                    match result {
                        Ok(records) => state.update_jobs(records, &events),
                        Err(e) => state.fail_all_jobs(&e),
                    }
                }
            }
        }

        debug!("LSF task monitor has shut down");
    }

    /// Reads the current job records using `bjobs`.
    async fn read_job_records(search_prefix: &str) -> Result<Vec<JobRecord>> {
        /// The expected output of `bjobs`.
        #[derive(Deserialize)]
        struct Output {
            /// The output records.
            #[serde(rename = "RECORDS")]
            records: Vec<JobRecord>,
        }

        let mut command = Command::new("bjobs");
        let command = command
            .arg("-a") // all jobs
            .arg("-J")
            .arg(search_prefix)
            .arg("-json") // JSON output
            .arg("-o") // output specified fields
            .arg("jobid stat exit_code") // output jobid, state, and exit code
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!(?command, "spawning `bjobs` to monitor tasks");

        let child = command.spawn().context("failed to spawn `bjobs` command")?;

        let output = child
            .wait_with_output()
            .await
            .context("failed to wait for `bjobs` to exit")?;
        if !output.status.success() {
            bail!(
                "`bjobs` failed: {status}: {stderr}",
                status = output.status,
                stderr = str::from_utf8(&output.stderr)
                    .unwrap_or("<output not UTF-8>")
                    .trim()
            );
        }

        Ok(serde_json::from_str::<Output>(
            str::from_utf8(&output.stdout).map_err(|_| anyhow!("`bjobs` output was not UTF-8"))?,
        )
        .context("failed to deserialize `bjobs` output")?
        .records)
    }
}

/// The experimental LSF + Apptainer backend.
///
/// See the module-level documentation for details.
pub struct LsfApptainerBackend {
    /// The shared engine configuration.
    config: Arc<Config>,
    /// The engine events.
    events: Events,
    /// The evaluation cancellation context.
    cancellation: CancellationContext,
    /// The Apptainer runtime.
    apptainer: ApptainerRuntime,
    /// The LSF task monitor.
    monitor: Monitor,
    /// The permits for `bsub` and `bkill` operations.
    permits: Semaphore,
}

impl LsfApptainerBackend {
    /// Create a new backend.
    ///
    /// The `run_root_dir` argument should be a directory that exists for the
    /// duration of the entire top-level evaluation. It is used to store
    /// Apptainer images which should only be created once per container per
    /// run.
    pub fn new(
        config: Arc<Config>,
        run_root_dir: &Path,
        events: Events,
        cancellation: CancellationContext,
    ) -> Result<Self> {
        // Ensure the configured backend is LSF Apptainer
        let backend_config = config.backend()?;

        let backend_config = backend_config
            .as_lsf_apptainer()
            .context("configured backend is not LSF Apptainer")?;

        let monitor = Monitor::new(
            Duration::from_secs(backend_config.interval.unwrap_or(DEFAULT_MONITOR_INTERVAL)),
            backend_config.job_name_prefix.clone(),
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
            apptainer: ApptainerRuntime::new(run_root_dir),
            monitor,
            permits,
        })
    }

    /// Kills the given LSF job.
    async fn kill_job(&self, job_id: u64) -> Result<()> {
        let mut command = Command::new("bkill");
        let command = command
            .arg("-C")
            .arg("task was cancelled")
            .arg(job_id.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let _permit = self
            .permits
            .acquire()
            .await
            .context("failed to acquire permit for submitting job")?;

        debug!(?command, "spawning `bkill` to cancel task");

        let mut child = command.spawn().context("failed to spawn `bkill` command")?;
        let status = child.wait().await.context("failed to wait for `bkill`")?;
        if !status.success() {
            bail!("`bkill` failed: {status}");
        }

        Ok(())
    }
}

impl TaskExecutionBackend for LsfApptainerBackend {
    fn constraints(
        &self,
        inputs: &TaskInputs,
        requirements: &HashMap<String, Value>,
        hints: &HashMap<String, Value>,
    ) -> Result<TaskExecutionConstraints> {
        let mut required_cpu = requirements::cpu(inputs, requirements);
        let mut required_memory = ByteSize::b(requirements::memory(inputs, requirements)? as u64);

        let backend_config = self.config.backend()?;
        let backend_config = backend_config
            .as_lsf_apptainer()
            .expect("configured backend is not LSF Apptainer");

        // Determine whether CPU or memory limits are set for this queue, and clamp or
        // deny them as appropriate if the limits are exceeded
        if let Some(queue) = backend_config.lsf_queue_for_task(requirements, hints) {
            if let Some(max_cpu) = queue.max_cpu_per_task()
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

            if let Some(max_memory) = queue.max_memory_per_task()
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
            bail!("LSF Apptainer backend does not support unknown container source `{container:#}`")
        }

        Ok(TaskExecutionConstraints {
            container: Some(container),
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
                .as_lsf_apptainer()
                .expect("configured backend is not LSF Apptainer");

            // Create the host directory that will be mapped to the working directory.
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

            // Create an empty file for the task's stderr
            let stderr_path = request.stderr_path();
            let _ = File::create(&stderr_path).await.with_context(|| {
                format!(
                    "failed to create stderr file `{path}`",
                    path = stderr_path.display()
                )
            })?;

            // Write the evaluated command section to a host file.
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
            debug!("task `{name}` was queued as LSF job `{job_id}`");

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
                        // See WEXITSTATUS from wait(2) to explain the shift
                        #[cfg(unix)]
                        let status = ExitStatus::from_raw((exit_code as i32) << 8);

                        #[cfg(windows)]
                        let status = ExitStatus::from_raw(exit_code as u32);

                        send_event!(
                            self.events.crankshaft(),
                            CrankshaftEvent::TaskCompleted {
                                id: crankshaft_id,
                                exit_statuses: NonEmpty::new(status),
                            }
                        );

                        exit_code
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_name_truncates() {
        let name = "é".repeat(LSF_JOB_NAME_MAX_LENGTH);
        assert_eq!(name.len(), 8188);
        let name = truncate_job_name(&name);
        assert!(name.len() < LSF_JOB_NAME_MAX_LENGTH);
    }
}
