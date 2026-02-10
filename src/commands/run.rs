//! Implementation of the `run` subcommand.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use clap::Parser;
use colored::Colorize as _;
use crankshaft::events::Event as CrankshaftEvent;
use futures::FutureExt as _;
use indexmap::IndexSet;
use indicatif::ProgressStyle;
use tokio::select;
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;
use tracing::Level;
use tracing::error;
use tracing_indicatif::span_ext::IndicatifSpanExt as _;
use tracing_subscriber::fmt::layer;
use wdl::ast::AstNode as _;
use wdl::ast::Severity;
use wdl::engine::CancellationContext;
use wdl::engine::CancellationContextState;
use wdl::engine::Config as EngineConfig;
use wdl::engine::EngineEvent;
use wdl::engine::EvaluationError;
use wdl::engine::Events;
use wdl::engine::Inputs as EngineInputs;
use wdl::engine::config::CallCachingMode;
use wdl::engine::config::SecretString;

use crate::Config;
use crate::FileLoggingReloadHandle;
use crate::analysis::Analysis;
use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::diagnostics::Mode;
use crate::diagnostics::emit_diagnostics;
use crate::eval::Evaluator;
use crate::inputs::Invocation;
use crate::inputs::OriginPaths;

/// The delay in showing the progress bar.
///
/// This is to prevent the progress bar from flashing on the screen for
/// very short analyses.
const PROGRESS_BAR_DELAY_BEFORE_RENDER: Duration = Duration::from_secs(2);

/// The capacity for the events channels.
///
/// This is the number of events to buffer in the events channel before
/// receivers become lagged.
///
/// As `tokio::sync::broadcast` channels are used to support multiple receivers,
/// an event is only dropped from the channel once *all* receivers have read it.
///
/// If the senders are sending events faster than all receivers can read the
/// events, the channel buffer will eventually reach capacity.
///
/// When this happens, the oldest events in the buffer are dropped and receivers
/// are notified via an error on the next read that they are lagging behind.
///
/// If the capacity is reached, Sprocket will stop displaying progress
/// statistics.
///
/// The value of `5000` was chosen as a reasonable amount to make reaching
/// capacity unlikely without allocating too much space unnecessarily.
const DEFAULT_EVENTS_CHANNEL_CAPACITY: usize = 5000;

/// The name of the default "runs" directory.
pub(crate) const DEFAULT_RUNS_DIR: &str = "runs";

/// The name for the "latest" symlink.
#[cfg(not(target_os = "windows"))]
const LATEST: &str = "_latest";

/// The log file in the output directory for writing `sprocket` output to
const LOG_FILE_NAME: &str = "output.log";

/// Arguments to the `run` subcommand.
#[derive(Parser, Debug)]
#[clap(disable_version_flag = true)]
pub struct Args {
    /// The WDL source file to run.
    ///
    /// The source file may be specified by either a local file path or a URL.
    #[clap(value_name = "SOURCE")]
    pub source: Source,

    /// The inputs for the task or workflow.
    ///
    /// An input can be either a local file path or URL to an input file or
    /// key-value pairs passed in on the command line.
    pub inputs: Vec<String>,

    /// The name of the task or workflow to run.
    ///
    /// This argument is required if trying to run a task or workflow without
    /// any inputs.
    ///
    /// If `target` is not specified, all inputs (from both files and
    /// key-value pairs) are expected to be prefixed with the name of the
    /// workflow or task being run.
    ///
    /// If `target` is specified, it will be appended with a `.` delimiter
    /// and then prepended to all key-value pair inputs on the command line.
    /// Keys specified within files are unchanged by this argument.
    #[clap(short, long, value_name = "NAME")]
    pub target: Option<String>,

    /// The root "runs" directory; defaults to `./runs/`.
    ///
    /// Individual sessions of `sprocket run` will nest their execution
    /// directories beneath this root directory at the path
    /// `<target name>/<timestamp>/`. On Unix systems, the latest `run`
    /// session will be symlinked at `<target name>/_latest`.
    #[clap(short, long, value_name = "ROOT_DIR")]
    pub runs_dir: Option<PathBuf>,

    /// The execution directory.
    ///
    /// If this argument is supplied, the default output behavior of nesting
    /// execution directories using the target and timestamp will be
    /// disabled.
    #[clap(long, conflicts_with = "runs_dir", value_name = "OUTPUT_DIR")]
    pub output: Option<PathBuf>,

    /// Overwrites the execution directory if it exists.
    #[clap(long, conflicts_with = "runs_dir")]
    pub overwrite: bool,

    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,

    /// The Azure Storage account name to use.
    #[clap(long, env, value_name = "NAME", requires = "azure_access_key")]
    pub azure_account_name: Option<String>,

    /// The Azure Storage access key to use.
    #[clap(
        long,
        env,
        hide_env_values(true),
        value_name = "KEY",
        requires = "azure_account_name"
    )]
    pub azure_access_key: Option<SecretString>,

    /// The AWS Access Key ID to use; overrides configuration.
    #[clap(long, env, value_name = "ID", requires = "aws_secret_access_key")]
    pub aws_access_key_id: Option<String>,

    /// The AWS Secret Access Key to use; overrides configuration.
    #[clap(
        long,
        env,
        hide_env_values(true),
        value_name = "KEY",
        requires = "aws_access_key_id"
    )]
    pub aws_secret_access_key: Option<SecretString>,

    /// The default AWS region; overrides configuration.
    #[clap(long, env, value_name = "REGION")]
    pub aws_default_region: Option<String>,

    /// The Google Cloud Storage HMAC access key to use; overrides
    /// configuration.
    #[clap(long, env, value_name = "KEY", requires = "google_hmac_secret")]
    pub google_hmac_access_key: Option<String>,

    /// The Google Cloud Storage HMAC secret to use; overrides configuration.
    #[clap(
        long,
        env,
        hide_env_values(true),
        value_name = "SECRET",
        requires = "google_hmac_access_key"
    )]
    pub google_hmac_secret: Option<SecretString>,

    /// Disables the use of the call cache for this run.
    #[clap(long)]
    pub no_call_cache: bool,
}

impl Args {
    /// Applies the given configuration to the CLI arguments.
    fn apply(&mut self, config: &Config) {
        if self.runs_dir.is_none() {
            self.runs_dir = Some(config.run.runs_dir.clone());
        }

        if self.report_mode.is_none() {
            self.report_mode = Some(config.common.report_mode);
        }
    }

    /// Applies the CLI arguments to the given engine configuration.
    fn apply_engine_config(&self, config: &mut EngineConfig) {
        // Apply the Azure auth to the engine config
        if self.azure_account_name.is_some() || self.azure_access_key.is_some() {
            let auth = config.storage.azure.auth.get_or_insert_default();
            if let Some(key) = &self.azure_account_name {
                auth.account_name = key.clone();
            }

            if let Some(access_key) = &self.azure_access_key {
                auth.access_key = access_key.clone();
            }
        }

        // Apply the AWS default region to the engine config
        if let Some(region) = &self.aws_default_region {
            config.storage.s3.region = Some(region.clone());
        }

        // Apply the AWS auth to the engine config
        if self.aws_access_key_id.is_some() || self.aws_secret_access_key.is_some() {
            let auth = config.storage.s3.auth.get_or_insert_default();
            if let Some(key) = &self.aws_access_key_id {
                auth.access_key_id = key.clone();
            }

            if let Some(secret) = &self.aws_secret_access_key {
                auth.secret_access_key = secret.clone();
            }
        }

        // Apply the Google auth to the engine config
        if self.google_hmac_access_key.is_some() || self.google_hmac_secret.is_some() {
            let auth = config.storage.google.auth.get_or_insert_default();
            if let Some(key) = &self.google_hmac_access_key {
                auth.access_key = key.clone();
            }

            if let Some(secret) = &self.google_hmac_secret {
                auth.secret = secret.clone();
            }
        }

        // Disable the call cache if requested
        if self.no_call_cache {
            config.task.cache = CallCachingMode::Off;
        }
    }
}

/// Helper for displaying task names.
struct Tasks<'a>(&'a IndexSet<Arc<String>>);

impl std::fmt::Display for Tasks<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        /// The maximum number of executing task names to display at a time
        const MAX_TASKS: usize = 10;

        let mut first = true;
        for (i, name) in self.0.iter().enumerate() {
            if i == MAX_TASKS {
                write!(f, "...")?;
                break;
            }

            if first {
                first = false;
            } else {
                write!(f, ", ")?;
            }

            write!(f, "{name}", name = name.magenta().bold())?;
        }

        Ok(())
    }
}

/// Represents information about a Crankshaft task.
struct Task {
    /// The name of the task.
    name: Arc<String>,
    /// The per-task cancellation token.
    ///
    /// This is used to cancel Crankshaft tasks that haven't yet executed.
    token: CancellationToken,
}

/// Represents state for reporting evaluation progress.
#[derive(Default)]
struct State {
    /// The map of task identifiers to names.
    tasks: HashMap<u64, Task>,
    /// The set of currently executing tasks.
    executing: IndexSet<Arc<String>>,
    /// The number of failed tasks.
    failed: usize,
    /// The number of completed tasks.
    completed: usize,
    /// The number of canceled tasks.
    canceled: usize,
    /// The number of parked tasks.
    parked: usize,
    /// The number of task results reused from the cache.
    cached: usize,
}

/// Displays evaluation progress.
async fn progress(
    progress_bar: tracing::Span,
    mut crankshaft: Receiver<CrankshaftEvent>,
    mut engine: Receiver<EngineEvent>,
    token: CancellationToken,
) {
    /// Helper for formatting the progress bar
    fn message(state: &State) -> String {
        fn append(message: &mut String, count: usize, kind: impl std::fmt::Display) {
            if count > 0 {
                let comma = if message.is_empty() {
                    message.push_str(" -");
                    false
                } else {
                    true
                };

                let _ = write!(
                    message,
                    "{comma} {count} {kind} task{s}",
                    comma = if comma { "," } else { "" },
                    s = if count == 1 { "" } else { "s" }
                );
            }
        }

        let mut message = String::new();
        append(&mut message, state.completed, "completed".green());
        append(&mut message, state.cached, "cached".green());
        append(&mut message, state.failed, "failed".red());
        append(&mut message, state.canceled, "canceled".red());
        append(
            &mut message,
            (state.tasks.len() - state.executing.len()) + state.parked,
            "waiting".yellow(),
        );
        append(&mut message, state.executing.len(), "executing".cyan());

        if !state.executing.is_empty() {
            let _ = write!(&mut message, ": {tasks}", tasks = Tasks(&state.executing));
        }

        message
    }

    let mut state = State::default();
    let mut lagged = false;
    let mut tasks_canceled = false;

    progress_bar.pb_set_message(&message(&state));
    progress_bar.pb_start();

    loop {
        tokio::select! {
            _ = token.cancelled(), if !tasks_canceled => {
                // Upon the initial cancellation, immediately cancel any Crankshaft task that is not executing.
                for task in state.tasks.values().filter(|t| !state.executing.contains(&t.name)) {
                    task.token.cancel();
                }

                tasks_canceled = true;
            }
            r = crankshaft.recv() => match r {
                Ok(event) if !lagged => {
                    let removed = match event {
                        CrankshaftEvent::TaskCreated { id, name, token: task_token, .. } => {
                            // If there has already been an initial cancellation, immediately signal the new task to cancel
                            if token.is_cancelled() {
                                task_token.cancel();
                            }

                            state.tasks.insert(id, Task { name: name.into(), token: task_token });
                            None
                        }
                        CrankshaftEvent::TaskStarted { id } => {
                            if let Some(task) = state.tasks.get(&id) {
                                state.executing.insert(task.name.clone());
                            }

                            None
                        }
                        CrankshaftEvent::TaskCompleted { id, .. } => {
                            state.completed += 1;
                            Some(id)
                        }
                        CrankshaftEvent::TaskFailed { id, .. } | CrankshaftEvent::TaskPreempted { id } => {
                            state.failed += 1;
                            Some(id)
                        }
                        CrankshaftEvent::TaskCanceled { id } => {
                            state.canceled += 1;
                            Some(id)
                        }
                        CrankshaftEvent::TaskContainerCreated { .. }
                        | CrankshaftEvent::TaskContainerExited { .. }
                        | CrankshaftEvent::TaskStdout { .. }
                        | CrankshaftEvent::TaskStderr { .. } => continue,
                    };

                    if let Some(id) = removed && let Some(task) = state.tasks.remove(&id) {
                        state.executing.swap_remove(&task.name);
                    }

                    progress_bar.pb_set_message(&message(&state));
                }
                Ok(_) => continue,
                Err(RecvError::Closed) => break,
                Err(RecvError::Lagged(_)) => {
                    lagged = true;
                    progress_bar.pb_set_message(" - progress is unavailable due to missed events");
                }
            },
            r = engine.recv() => match r {
                Ok(event) if !lagged => {
                    match event {
                        EngineEvent::ReusedCachedExecutionResult { .. } => {
                            state.cached += 1;
                        }
                        EngineEvent::TaskParked => {
                            state.parked += 1;
                        }
                        EngineEvent::TaskUnparked { canceled } => {
                            state.parked = state.parked.saturating_sub(1);

                            if canceled {
                                state.canceled += 1;
                            }
                        }
                    };

                    progress_bar.pb_set_message(&message(&state));
                }
                Ok(_) => continue,
                Err(RecvError::Closed) => break,
                Err(RecvError::Lagged(_)) => {
                    lagged = true;
                    progress_bar.pb_set_message(" - progress is unavailable due to missed events");
                }
            }
        }
    }
}

/// Determines the timestamped execution directory and performs any necessary
/// staging prior to execution.
///
/// Staging includes writing a `.sprocketignore` file with contents `*` in the
/// `root` if an existing ignorefile is not found.
///
/// Notably, this function does not actually create the execution directory at
/// the returned path, as that is handled by execution itself.
///
/// If running on a Unix system, a symlink to the returned path will be created
/// at `<root>/<target>/_latest`.
pub fn setup_run_dir(root: &Path, target: &str) -> Result<PathBuf> {
    // Create the target root directory
    let target_root = root.join(target);
    fs::create_dir_all(&target_root).with_context(|| {
        format!(
            "failed to create target directory `{path}`",
            path = target_root.display()
        )
    })?;

    // Create an ignore file at the root if one doesn't exist
    let ignore_path = root.join(crate::IGNORE_FILENAME);
    if !ignore_path.exists() {
        fs::write(&ignore_path, "*").with_context(|| {
            format!(
                "failed to write ignorefile `{path}`",
                path = ignore_path.display()
            )
        })?;
    }

    // Format an output directory path
    let timestamp = chrono::Utc::now().format("%F_%H%M%S%f").to_string();
    let output = target_root.join(&timestamp);
    if output.exists() {
        bail!(
            "timestamped execution directory `{dir}` existed before execution began",
            dir = output.display()
        );
    }

    // Replace the latest symlink to the new output path
    #[cfg(not(target_os = "windows"))]
    {
        let latest = target_root.join(LATEST);
        let _ = fs::remove_file(&latest);
        if let Err(e) = std::os::unix::fs::symlink(&timestamp, &latest) {
            tracing::warn!("failed to create latest run symlink: {e}")
        }
    }

    Ok(output)
}

/// The main function for the `run` subcommand.
pub async fn run(
    mut args: Args,
    mut config: Config,
    colorize: bool,
    handle: FileLoggingReloadHandle,
) -> CommandResult<()> {
    if let Source::Directory(_) = args.source {
        return Err(anyhow!("directory sources are not supported for the `run` command").into());
    }

    args.apply(&config);
    args.apply_engine_config(&mut config.run.engine);

    let template = if colorize {
        "[{elapsed_precise:.cyan/blue}] {bar:40.cyan/blue} {msg} {pos}/{len}"
    } else {
        "[{elapsed_precise}] {bar:40} {msg} {pos}/{len}"
    };

    let style = ProgressStyle::with_template(template).unwrap();

    let progress_bar = tracing::span!(Level::WARN, "progress");
    let start = std::time::Instant::now();

    let results = Analysis::default()
        .add_source(args.source.clone())
        .init({
            let progress_bar = progress_bar.clone();
            Box::new(move || {
                progress_bar.pb_set_style(&style);
            })
        })
        .progress({
            let progress_bar = progress_bar.clone();
            move |kind, completed, total| {
                let progress_bar = progress_bar.clone();
                async move {
                    if start.elapsed() < PROGRESS_BAR_DELAY_BEFORE_RENDER {
                        return;
                    }

                    if completed == 0 {
                        progress_bar.pb_start();
                        progress_bar.pb_set_length(total.try_into().unwrap());
                        progress_bar.pb_set_message(&format!("{kind}"));
                    }

                    progress_bar.pb_set_position(completed.try_into().unwrap());
                }
                .boxed()
            }
        })
        .run()
        .await
        .map_err(CommandError::from)?;

    // Emits diagnostics for all analyzed documents
    let mut errors = 0;
    for result in results.as_slice() {
        let mut diagnostics = result.document().diagnostics().peekable();
        if diagnostics.peek().is_some() {
            let path = result.document().path().to_string();
            let source = result.document().root().text().to_string();

            errors += diagnostics
                .filter(|d| d.severity() == Severity::Error)
                .count();

            emit_diagnostics(
                &path,
                source,
                result.document().diagnostics(),
                &[],
                args.report_mode.unwrap_or_default(),
                colorize,
            )
            .context("failed to emit diagnostics")?;
        }
    }

    if errors > 0 {
        return Err(anyhow!(
            "aborting due to previous {errors} error{s}",
            s = if errors == 1 { "" } else { "s" }
        )
        .into());
    }

    let document = results.filter(&[&args.source]).next().unwrap().document();

    let inputs = Invocation::coalesce(&args.inputs, args.target.clone())
        .await
        .with_context(|| {
            format!(
                "failed to parse inputs from `{sources}`",
                sources = args.inputs.join("`, `")
            )
        })?
        .into_engine_invocation(document)?;

    let (target, inputs, origins) = if let Some(inputs) = inputs {
        inputs
    } else {
        // No inputs were provided
        let origins = OriginPaths::Single(
            std::env::current_dir()
                .context("failed to get current directory")?
                .as_path()
                .into(),
        );

        if let Some(name) = args.target {
            match (document.task_by_name(&name), document.workflow()) {
                (Some(_), _) => (name, EngineInputs::Task(Default::default()), origins),
                (None, Some(workflow)) if workflow.name() == name => {
                    (name, EngineInputs::Workflow(Default::default()), origins)
                }
                _ => {
                    return Err(anyhow!(
                        "no task or workflow with name `{name}` was found in document `{path}`",
                        path = document.path()
                    )
                    .into());
                }
            }
        } else {
            return Err(
                anyhow!("the `--target` option is required if no inputs are provided").into(),
            );
        }
    };

    let output_dir = if let Some(supplied_dir) = args.output {
        if supplied_dir.exists() {
            if !args.overwrite {
                return Err(anyhow!(
                    "output directory `{dir}` exists; use the `--overwrite` option to overwrite \
                     its contents",
                    dir = supplied_dir.display()
                )
                .into());
            }

            std::fs::remove_dir_all(&supplied_dir).with_context(|| {
                format!(
                    "failed to remove output directory `{dir}`",
                    dir = supplied_dir.display()
                )
            })?;
        }
        supplied_dir
    } else {
        setup_run_dir(&args.runs_dir.unwrap_or(DEFAULT_RUNS_DIR.into()), &target)?
    };

    // Now that the output directory is calculated, initialize file logging
    initialize_file_logging(handle, &output_dir)?;

    tracing::info!(
        "`{dir}` will be used as the execution directory",
        dir = output_dir.display()
    );

    let run_kind = match &inputs {
        EngineInputs::Task(_) => "task",
        EngineInputs::Workflow(_) => "workflow",
    };

    let template = if colorize {
        format!(
            "[{{elapsed_precise:.cyan/blue}}] {{spinner:.cyan/blue}} {running} {run_kind} \
             {target}{{msg}}",
            running = "running".cyan(),
            target = target.magenta().bold()
        )
    } else {
        format!("[{{elapsed_precise}}] {{spinner}} running {run_kind} {target}{{msg}}",)
    };

    progress_bar.pb_set_style(&ProgressStyle::with_template(&template).unwrap());

    let cancellation = CancellationContext::new(config.run.engine.failure_mode);
    let events = Events::new(
        config
            .run
            .events_capacity
            .unwrap_or(DEFAULT_EVENTS_CHANNEL_CAPACITY),
    );
    let transfer_progress = tokio::spawn(cloud_copy::cli::handle_events(
        events
            .subscribe_transfer()
            .expect("should have transfer events"),
        cancellation.first(),
    ));
    let crankshaft_progress = tokio::spawn(progress(
        progress_bar,
        events
            .subscribe_crankshaft()
            .expect("should have Crankshaft events"),
        events
            .subscribe_engine()
            .expect("should have engine events"),
        cancellation.first(),
    ));

    let evaluator = Evaluator::new(
        document,
        &target,
        inputs,
        &origins,
        config.run.engine.into(),
        &output_dir,
    );

    let mut evaluate = evaluator.run(cancellation.clone(), events).boxed();

    loop {
        select! {
            // Always prefer the CTRL-C signal to the evaluation returning.
            biased;

            _ = tokio::signal::ctrl_c() => {
                // If we've already been waiting for executing tasks to cancel, immediately bail out
                if cancellation.state() == CancellationContextState::Canceling {
                    return Err(anyhow!("evaluation was interrupted").into());
                }

                // Log the message indicating whether we're waiting on completion or waiting on cancellation
                match cancellation.cancel() {
                    CancellationContextState::NotCanceled => unreachable!("should be canceled"),
                    CancellationContextState::Waiting => {
                        error!("waiting for executing tasks to complete: use Ctrl-C to cancel executing tasks");
                    },
                    CancellationContextState::Canceling => {
                        error!("waiting for executing tasks to cancel: use Ctrl-C to immediately terminate Sprocket");
                    },
                }
            },
            res = &mut evaluate => {
                let _ = transfer_progress.await;
                let _ = crankshaft_progress.await;

                return match res {
                    Ok(outputs) => {
                        println!("{}", serde_json::to_string_pretty(&outputs.with_name(&target)).context("failed to serialize outputs")?);
                        Ok(())
                    }
                    Err(EvaluationError::Canceled) => Err(anyhow!("evaluation was interrupted").into()),
                    Err(EvaluationError::Source(e)) => {
                        emit_diagnostics(
                            &e.document.path(),
                            e.document.root().text().to_string(),
                            &[e.diagnostic],
                            &e.backtrace,
                            args.report_mode.unwrap_or_default(),
                            colorize
                        )?;
                        Err(anyhow!("aborting due to evaluation error").into())
                    }
                    Err(EvaluationError::Other(e)) => Err(e.into())
                };
            },
        }
    }
}

/// Initializes logging to `output.log` in the given output directory.
fn initialize_file_logging(handle: FileLoggingReloadHandle, output_dir: &PathBuf) -> Result<()> {
    fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "failed to create directory `{path}`",
            path = output_dir.display()
        )
    })?;

    let log_file_path = output_dir.join(LOG_FILE_NAME);
    let log_file = File::create(&log_file_path).with_context(|| {
        format!(
            "failed to create log file `{path}`",
            path = log_file_path.display()
        )
    })?;

    handle
        .reload(layer().with_ansi(false).with_writer(log_file))
        .context("failed to initialize file logging")
}
