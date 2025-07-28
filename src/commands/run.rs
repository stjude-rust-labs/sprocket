//! Implementation of the `run` subcommand.

use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use colored::Colorize as _;
use futures::FutureExt as _;
use indexmap::IndexSet;
use indicatif::ProgressStyle;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::Level;
use tracing::error;
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
use wdl::engine::Inputs as EngineInputs;
use wdl::engine::v1::ProgressKind;

use crate::Mode;
use crate::emit_diagnostics;

/// The delay in showing the progress bar.
///
/// This is to prevent the progress bar from flashing on the screen for
/// very short analyses.
const PROGRESS_BAR_DELAY_BEFORE_RENDER: Duration = Duration::from_secs(2);

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
    /// If `entrypoint` is specified, it will be prefixed (with a `.` delimiter)
    /// to all key-value pair inputs on the command line. Keys specified within
    /// files are unchanged by this argument.
    #[clap(short, long, value_name = "NAME")]
    pub entrypoint: Option<String>,

    /// The execution output directory; defaults to the task name if provided,
    /// otherwise, `output`.
    #[clap(short, long, value_name = "OUTPUT_DIR")]
    pub output: Option<PathBuf>,

    /// Overwrites the execution output directory if it exists.
    #[clap(long)]
    pub overwrite: bool,

    /// Disables color output.
    #[arg(long)]
    pub no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,

    /// The engine configuration to use.
    #[clap(skip)]
    pub engine: Option<engine::config::Config>,
}

impl Args {
    /// Applies the configuration to the arguments.
    pub fn apply(mut self, config: crate::config::Config) -> Self {
        self.engine = Some(config.run.engine);
        self.no_color = self.no_color || !config.common.color;
        self.report_mode = match self.report_mode {
            Some(mode) => Some(mode),
            None => Some(config.common.report_mode),
        };
        self
    }
}

/// Helper for displaying task ids.
struct Ids<'a>(&'a IndexSet<String>);

impl std::fmt::Display for Ids<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        /// The maximum number of executing task names to display at a time
        const MAX_TASKS: usize = 10;

        let mut first = true;
        for (i, id) in self.0.iter().enumerate() {
            if i == MAX_TASKS {
                write!(f, "...")?;
                break;
            }

            if first {
                first = false;
            } else {
                write!(f, ", ")?;
            }

            write!(f, "{id}", id = id.magenta().bold())?;
        }

        Ok(())
    }
}

/// Represents state for reporting evaluation progress.
#[derive(Default)]
struct State {
    /// The set of currently executing task identifiers.
    ids: IndexSet<String>,
    /// The number of completed tasks.
    completed: usize,
    /// The number of tasks awaiting execution.
    ready: usize,
    /// The number of currently executing tasks.
    executing: usize,
}

/// A callback for updating state based on engine events.
fn progress(kind: ProgressKind<'_>, pb: &tracing::Span, state: &Mutex<State>) {
    pb.pb_start();

    let message = {
        let mut state = state.lock().expect("failed to lock progress mutex");
        match kind {
            ProgressKind::TaskStarted { .. } | ProgressKind::TaskRetried { .. } => {
                state.ready += 1;
            }
            ProgressKind::TaskExecutionStarted { id } => {
                state.ready -= 1;
                state.executing += 1;
                state.ids.insert(id.to_string());
            }
            ProgressKind::TaskExecutionCompleted { id, .. } => {
                state.executing -= 1;
                state.ids.swap_remove(id);
            }
            ProgressKind::TaskCompleted { .. } => {
                state.completed += 1;
            }
            _ => {}
        }

        format!(
            " - {c} {completed} task{s1}, {r} {ready} task{s2}, {e} {executing} task{s3}: {ids}",
            c = state.completed,
            completed = "completed".cyan(),
            s1 = if state.completed == 1 { "" } else { "s" },
            r = state.ready,
            ready = "ready".cyan(),
            s2 = if state.ready == 1 { "" } else { "s" },
            e = state.executing,
            executing = "executing".cyan(),
            s3 = if state.executing == 1 { "" } else { "s" },
            ids = Ids(&state.ids)
        )
    };

    pb.pb_set_message(&message);
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

    let (name, inputs, origins) = if let Some(inputs) = inputs {
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

    let output_dir = args
        .output
        .as_deref()
        .unwrap_or_else(|| Path::new(&name))
        .to_owned();

    // Check to see if the output directory already exists and if it should be
    // removed.
    if output_dir.exists() {
        if !args.overwrite {
            bail!(
                "output directory `{dir}` exists; use the `--overwrite` option to overwrite its \
                 contents",
                dir = output_dir.display()
            );
        }

        std::fs::remove_dir_all(&output_dir).with_context(|| {
            format!(
                "failed to remove output directory `{dir}`",
                dir = output_dir.display()
            )
        })?;
    }

    let run_kind = match &inputs {
        EngineInputs::Task(_) => "task",
        EngineInputs::Workflow(_) => "workflow",
    };

    span.pb_set_style(
        &ProgressStyle::with_template(&format!(
            "[{{elapsed_precise:.cyan/blue}}] {{spinner:.cyan/blue}} {running} {run_kind} \
             {name}{{msg}}",
            running = "running".cyan(),
            name = name.magenta().bold()
        ))
        .unwrap(),
    );

    let state = Mutex::<State>::default();
    let evaluator = Evaluator::new(
        document,
        &name,
        inputs,
        origins,
        args.engine.unwrap_or_default(),
        &output_dir,
    );
    let token = CancellationToken::new();

    let mut evaluate = evaluator
        .run(token.clone(), move |kind: ProgressKind<'_>| {
            progress(kind, &span, &state);
            async {}
        })
        .boxed();

    select! {
        // Always prefer the CTRL-C signal to the evaluation returning.
        biased;

        _ = tokio::signal::ctrl_c() => {
            error!("execution was interrupted: waiting for evaluation to abort");
            token.cancel();
            evaluate.await.ok();
            bail!("execution was aborted");
        },
        res = &mut evaluate => match res {
            Ok(outputs) => {
                println!("{outputs}", outputs = serde_json::to_string_pretty(&outputs.with_name(&name))?);
                Ok(())
            }
            Err(EvaluationError::Source(e)) => {
                emit_diagnostics(&e.document.path(), e.document.root().text().to_string(), &[e.diagnostic], &e.backtrace, args.report_mode.unwrap_or_default(), args.no_color)?;
                bail!("aborting due to evaluation error");
            }
            Err(EvaluationError::Other(e)) => Err(e)
        },
    }
}
