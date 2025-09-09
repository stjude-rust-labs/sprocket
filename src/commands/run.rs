//! Implementation of the `run` subcommand.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use colored::Colorize as _;
use crankshaft::events::Event;
use futures::FutureExt as _;
use indexmap::IndexSet;
use indicatif::ProgressStyle;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;
use tracing::Level;
use tracing::error;
use tracing::warn;
use tracing_indicatif::span_ext::IndicatifSpanExt as _;
use wdl::ast::AstNode as _;
use wdl::ast::Severity;
use wdl::cli::Analysis;
use wdl::cli::Evaluator;
use wdl::cli::Inputs;
use wdl::cli::analysis::AnalysisResults;
use wdl::cli::analysis::Source;
use wdl::cli::inputs::OriginPaths;
use wdl::engine;
use wdl::engine::EvaluationError;
use wdl::engine::Events;
use wdl::engine::Inputs as EngineInputs;

use crate::Mode;
use crate::emit_diagnostics;

/// The delay in showing the progress bar.
///
/// This is to prevent the progress bar from flashing on the screen for
/// very short analyses.
const PROGRESS_BAR_DELAY_BEFORE_RENDER: Duration = Duration::from_secs(2);

/// The capacity for the Crankshaft events channel.
///
/// This is the number of events to buffer in the channel before receivers
/// become lagged.
///
/// The value of `100` was chosen simply as a reasonable default.
const EVENTS_CHANNEL_CAPACITY: usize = 100;

/// The name of the default "runs" directory.
pub(crate) const DEFAULT_RUNS_DIR: &str = "runs";

/// The name for the "latest" symlink.
#[cfg(not(target_os = "windows"))]
const LATEST: &str = "_latest";

/// Arguments to the `run` subcommand.
#[derive(Parser, Debug)]
#[clap(disable_version_flag = true)]
pub struct Args {
    /// A source WDL file or URL.
    #[clap(value_name = "PATH or URL")]
    pub source: Source,

    /// The inputs for the task or workflow.
    ///
    /// These inputs can be either paths to files containing inputs or key-value
    /// pairs passed in on the command line.
    pub inputs: Vec<String>,

    /// The name of the task or workflow to run.
    ///
    /// This argument is required if trying to run a task or workflow without
    /// any inputs.
    ///
    /// If `entrypoint` is not specified, all inputs (from both files and
    /// key-value pairs) are expected to be prefixed with the name of the
    /// workflow or task being run.
    ///
    /// If `entrypoint` is specified, it will be appended with a `.` delimiter
    /// and then prepended to all key-value pair inputs on the command line.
    /// Keys specified within files are unchanged by this argument.
    #[clap(short, long, value_name = "NAME")]
    pub entrypoint: Option<String>,

    /// The root "runs" directory; defaults to `./runs/`.
    ///
    /// Individual invocations of `sprocket run` will nest their execution
    /// directories beneath this root directory at the path
    /// `<entrypoint name>/<timestamp>/`. On Unix systems, the latest `run`
    /// invocation will be symlinked at `<entrypoint name>/_latest`.
    #[clap(short, long, value_name = "ROOT_DIR")]
    pub runs_dir: Option<PathBuf>,

    /// The execution directory.
    ///
    /// If this argument is supplied, the default output behavior of nesting
    /// execution directories using the entrypoint and timestamp will be
    /// disabled.
    #[clap(long, conflicts_with = "runs_dir", value_name = "OUTPUT_DIR")]
    pub output: Option<PathBuf>,

    /// Overwrites the execution directory if it exists.
    #[clap(long, conflicts_with = "runs_dir")]
    pub overwrite: bool,

    /// Disables color output.
    #[arg(long)]
    pub no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,

    /// The engine configuration to use.
    ///
    /// This is not exposed via [`clap`] and is not settable by users.
    /// It will always be overwritten by the engine config provided by the user
    /// (which will be set with `Default::default()` if the user does not
    /// explicitly set `run` config values).
    #[clap(skip)]
    pub engine: engine::config::Config,
}

impl Args {
    /// Applies the configuration to the arguments.
    pub fn apply(mut self, config: crate::config::Config) -> Self {
        self.engine = config.run.engine;
        if self.runs_dir.is_none() {
            self.runs_dir = Some(config.run.runs_dir);
        }
        self.no_color = self.no_color || !config.common.color;
        if self.report_mode.is_none() {
            self.report_mode = Some(config.common.report_mode);
        }
        self
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

/// Represents state for reporting evaluation progress.
#[derive(Default)]
struct State {
    /// The map of task identifiers to names.
    tasks: HashMap<u64, Arc<String>>,
    /// The set of currently executing tasks.
    executing: IndexSet<Arc<String>>,
    /// The number of failed tasks.
    failed: usize,
    /// The number of completed tasks.
    completed: usize,
}

/// Displays evaluation progress.
async fn progress(
    mut events: broadcast::Receiver<Event>,
    pb: tracing::Span,
    state: Arc<Mutex<State>>,
) {
    /// Helper for formatting the progress bar
    fn message(state: &State) -> String {
        let executing = state.executing.len();
        let ready = state.tasks.len() - executing;
        format!(
            " - {c} {completed} task{s1}, {r} {ready} task{s2}, {e} {executing} \
             task{s3}{sep}{tasks}",
            c = state.completed,
            completed = "completed".cyan(),
            s1 = if state.completed == 1 { "" } else { "s" },
            r = ready,
            ready = "ready".cyan(),
            s2 = if ready == 1 { "" } else { "s" },
            e = executing,
            executing = "executing".cyan(),
            s3 = if executing == 1 { "" } else { "s" },
            sep = if executing == 0 { "" } else { ": " },
            tasks = Tasks(&state.executing)
        )
    }

    let mut warned = false;
    message(&state.lock().expect("failed to lock state"));

    loop {
        match events.recv().await {
            Ok(event) => {
                pb.pb_start();

                let message = match event {
                    Event::TaskCreated { id, name, .. } => {
                        let mut state = state.lock().expect("failed to lock state");
                        state.tasks.insert(id, name.into());
                        message(&state)
                    }
                    Event::TaskStarted { id } => {
                        let mut state = state.lock().expect("failed to lock state");
                        if let Some(name) = state.tasks.get(&id).cloned() {
                            state.executing.insert(name);
                        }
                        message(&state)
                    }
                    Event::TaskCompleted { id, .. } => {
                        let mut state = state.lock().expect("failed to lock state");
                        if let Some(name) = state.tasks.remove(&id) {
                            state.executing.swap_remove(&name);
                        }
                        state.completed += 1;
                        message(&state)
                    }
                    Event::TaskFailed { id, .. } => {
                        let mut state = state.lock().expect("failed to lock state");
                        if let Some(name) = state.tasks.remove(&id) {
                            state.executing.swap_remove(&name);
                        }
                        state.failed += 1;
                        message(&state)
                    }
                    Event::TaskCanceled { id } | Event::TaskPreempted { id } => {
                        let mut state = state.lock().expect("failed to lock state");
                        if let Some(name) = state.tasks.remove(&id) {
                            state.executing.swap_remove(&name);
                        }
                        state.failed += 1;
                        message(&state)
                    }
                    _ => continue,
                };

                pb.pb_set_message(&message);
            }
            Err(RecvError::Closed) => break,
            Err(RecvError::Lagged(_)) => {
                if !warned {
                    warned = true;
                    warn!("event stream is lagging: task progress reporting may be incorrect");
                }
            }
        }
    }
}

/// Determines the timestamped execution directory and performs any necessary
/// staging prior to execution.
///
/// Notably, this function does not actually create the execution directory at
/// the returned path, as that is handled by execution itself.
///
/// If running on a Unix system, a symlink to the returned path will be created
/// at `<root>/<entrypoint>/_latest`.
pub fn setup_run_dir(root: &Path, entrypoint: &str) -> Result<PathBuf> {
    let root = root.join(entrypoint);
    std::fs::create_dir_all(&root)
        .with_context(|| format!("failed to create directory: `{dir}`", dir = root.display()))?;

    let timestamp = chrono::Utc::now();

    let output = root.join(timestamp.format("%F_%H%M%S%f").to_string());

    if output.exists() {
        bail!(
            "timestamped execution directory `{dir}` existed before execution began",
            dir = output.display()
        );
    }

    #[cfg(not(target_os = "windows"))]
    {
        let latest = root.join(LATEST);
        let _ = std::fs::remove_file(&latest);
        if std::os::unix::fs::symlink(output.file_name().expect("should have basename"), &latest)
            .is_err()
        {
            tracing::warn!("failed to create latest symlink: continuing with run")
        };
    }

    Ok(output)
}

/// The main function for the `run` subcommand.
pub async fn run(args: Args) -> Result<()> {
    if let Source::Directory(_) = args.source {
        bail!("directory sources are not supported for the `run` command");
    }

    let style = ProgressStyle::with_template(
        "[{elapsed_precise:.cyan/blue}] {bar:40.cyan/blue} {msg} {pos}/{len}",
    )
    .unwrap();

    let span = tracing::span!(Level::WARN, "progress");
    let start = std::time::Instant::now();

    let results = match Analysis::default()
        .add_source(args.source.clone())
        .init({
            let span = span.clone();
            Box::new(move || {
                span.pb_set_style(&style);
            })
        })
        .progress({
            let span = span.clone();
            move |kind, completed, total| {
                let span = span.clone();
                async move {
                    if start.elapsed() < PROGRESS_BAR_DELAY_BEFORE_RENDER {
                        return;
                    }

                    if completed == 0 {
                        span.pb_start();
                        span.pb_set_length(total.try_into().unwrap());
                        span.pb_set_message(&format!("{kind}"));
                    }

                    span.pb_set_position(completed.try_into().unwrap());
                }
                .boxed()
            }
        })
        .run()
        .await
    {
        Ok(results) => results.into_inner(),
        Err(errors) => {
            // SAFETY: this is a non-empty, so it must always have a first
            // element.
            bail!(errors.into_iter().next().unwrap())
        }
    };

    // Emits diagnostics for all analyzed documents
    let mut errors = 0;
    for result in &results {
        let diagnostics = result.document().diagnostics();
        if !diagnostics.is_empty() {
            let path = result.document().path().to_string();
            let source = result.document().root().text().to_string();

            errors += diagnostics
                .iter()
                .filter(|d| d.severity() == Severity::Error)
                .count();

            emit_diagnostics(
                &path,
                source,
                diagnostics,
                &[],
                args.report_mode.unwrap_or_default(),
                args.no_color,
            )
            .context("failed to emit diagnostics")?;
        }
    }

    if errors > 0 {
        bail!(
            "aborting due to previous {errors} error{s}",
            s = if errors == 1 { "" } else { "s" }
        );
    }

    // SAFETY: this must exist, as we added it as the only source to be analyzed
    // above.
    let results = AnalysisResults::try_new(results).unwrap();
    let document = results.filter(&[&args.source]).next().unwrap().document();

    let inputs = Inputs::coalesce(&args.inputs, args.entrypoint.clone())
        .with_context(|| {
            format!(
                "failed to parse inputs from `{sources}`",
                sources = args.inputs.join("`, `")
            )
        })?
        .into_engine_inputs(document)?;

    let (entrypoint, inputs, origins) = if let Some(inputs) = inputs {
        inputs
    } else {
        // No inputs were provided
        let origins =
            OriginPaths::from(std::env::current_dir().context("failed to get current directory")?);

        if let Some(name) = args.entrypoint {
            match (document.task_by_name(&name), document.workflow()) {
                (Some(_), _) => (name, EngineInputs::Task(Default::default()), origins),
                (None, Some(workflow)) => {
                    if workflow.name() == name {
                        (name, EngineInputs::Workflow(Default::default()), origins)
                    } else {
                        bail!(
                            "no task or workflow with name `{name}` was found in document `{path}`",
                            path = document.path()
                        );
                    }
                }
                (None, None) => bail!(
                    "no task or workflow with name `{name}` was found in document `{path}`",
                    path = document.path()
                ),
            }
        } else {
            bail!("the `--entrypoint` option is required if no inputs are provided")
        }
    };

    let output_dir = if let Some(supplied_dir) = args.output {
        if supplied_dir.exists() {
            if !args.overwrite {
                bail!(
                    "output directory `{dir}` exists; use the `--overwrite` option to overwrite \
                     its contents",
                    dir = supplied_dir.display()
                );
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
        setup_run_dir(
            &args.runs_dir.unwrap_or(DEFAULT_RUNS_DIR.into()),
            &entrypoint,
        )?
    };

    tracing::info!(
        "`{dir}` will be used as the execution directory",
        dir = output_dir.display()
    );

    let run_kind = match &inputs {
        EngineInputs::Task(_) => "task",
        EngineInputs::Workflow(_) => "workflow",
    };

    span.pb_set_style(
        &ProgressStyle::with_template(&format!(
            "[{{elapsed_precise:.cyan/blue}}] {{spinner:.cyan/blue}} {running} {run_kind} \
             {name}{{msg}}",
            running = "running".cyan(),
            name = entrypoint.magenta().bold()
        ))
        .unwrap(),
    );

    let state = Arc::new(Mutex::<State>::default());
    let token = CancellationToken::new();
    let events = Events::all(EVENTS_CHANNEL_CAPACITY);
    let transfer_progress = tokio::spawn(cloud_copy::cli::handle_events(
        events.subscribe_transfer(),
        token.clone(),
    ));
    let crankshaft_progress = tokio::spawn(progress(events.subscribe_crankshaft(), span, state));

    let evaluator = Evaluator::new(
        document,
        &entrypoint,
        inputs,
        origins,
        args.engine,
        &output_dir,
    );

    let mut evaluate = evaluator.run(token.clone(), events).boxed();

    select! {
        // Always prefer the CTRL-C signal to the evaluation returning.
        biased;

        _ = tokio::signal::ctrl_c() => {
            error!("execution was interrupted: waiting for evaluation to abort");
            token.cancel();
            evaluate.await.ok();
            transfer_progress.await.ok();
            crankshaft_progress.await.ok();
            bail!("execution was aborted");
        },
        res = &mut evaluate => {
            transfer_progress.await.ok();
            crankshaft_progress.await.ok();

            match res {
                Ok(outputs) => {
                    println!("{outputs}", outputs = serde_json::to_string_pretty(&outputs.with_name(&entrypoint))?);
                    Ok(())
                }
                Err(EvaluationError::Source(e)) => {
                    emit_diagnostics(&e.document.path(), e.document.root().text().to_string(), &[e.diagnostic], &e.backtrace, args.report_mode.unwrap_or_default(), args.no_color)?;
                    bail!("aborting due to evaluation error");
                }
                Err(EvaluationError::Other(e)) => Err(e)
            }
        },
    }
}
