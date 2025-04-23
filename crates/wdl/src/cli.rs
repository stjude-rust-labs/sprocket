//! Entry point functions for the command-line interface.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::io::IsTerminal;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use codespan_reporting::diagnostic::Label;
use codespan_reporting::diagnostic::LabelStyle;
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term::emit;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;
use colored::Colorize;
use futures::FutureExt;
use indexmap::IndexSet;
use indicatif::ProgressStyle;
use serde_json::to_string_pretty;
use tokio::select;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::error;
use tracing_indicatif::span_ext::IndicatifSpanExt;
use url::Url;
use wdl_analysis::AnalysisResult;
use wdl_analysis::Analyzer;
use wdl_analysis::DiagnosticsConfig;
use wdl_analysis::document::Document;
use wdl_analysis::path_to_uri;
use wdl_analysis::rules as analysis_rules;
use wdl_ast::AstNode;
use wdl_engine::CallLocation;
use wdl_engine::EvaluatedTask;
use wdl_engine::EvaluationError;
use wdl_engine::EvaluationResult;
use wdl_engine::Inputs;
use wdl_engine::Outputs;
use wdl_engine::config::Config;
use wdl_engine::v1::ProgressKind;
use wdl_engine::v1::TaskEvaluator;
use wdl_engine::v1::WorkflowEvaluator;
use wdl_grammar::Diagnostic;
use wdl_grammar::Severity;
use wdl_lint::rules as lint_rules;

/// The delay in showing the progress bar.
///
/// This is to prevent the progress bar from flashing on the screen for
/// very short analyses.
const PROGRESS_BAR_DELAY_BEFORE_RENDER: Duration = Duration::from_secs(2);

/// The maximum number of call locations to print for evaluation errors.
const MAX_CALL_LOCATIONS: usize = 10;

/// Emits the given diagnostics to the output stream.
///
/// The use of color is determined by the presence of a terminal.
///
/// In the future, we might want the color choice to be a CLI argument.
pub fn emit_diagnostics(
    path: &str,
    source: String,
    diagnostics: &[Diagnostic],
    backtrace: &[CallLocation],
) -> Result<usize> {
    use codespan_reporting::term::Config;

    let mut map = HashMap::new();
    let mut files = SimpleFiles::new();

    let mut stream = StandardStream::stdout(if std::io::stdout().is_terminal() {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    });

    let file_id = files.add(Cow::Borrowed(path), source);
    let mut errors = 0;
    for diagnostic in diagnostics {
        if diagnostic.severity() == Severity::Error {
            errors += 1;
        }

        let diagnostic = diagnostic.to_codespan(file_id).with_labels_iter(
            backtrace.iter().take(MAX_CALL_LOCATIONS).map(|l| {
                let id = l.document.id();
                let file_id = *map.entry(id).or_insert_with(|| {
                    files.add(l.document.path(), l.document.root().text().to_string())
                });

                Label {
                    style: LabelStyle::Secondary,
                    file_id,
                    range: l.span.start()..l.span.end(),
                    message: "called from this location".into(),
                }
            }),
        );

        emit(&mut stream, &Config::default(), &files, &diagnostic)
            .context("failed to emit diagnostic")?;

        if backtrace.len() > MAX_CALL_LOCATIONS {
            writeln!(
                &mut stream,
                "  and {count} more call{s}...",
                count = backtrace.len() - MAX_CALL_LOCATIONS,
                s = if backtrace.len() - MAX_CALL_LOCATIONS == 1 {
                    ""
                } else {
                    "s"
                }
            )
            .unwrap();
        }
    }

    Ok(errors)
}

/// Analyze the document or directory, returning [`AnalysisResult`]s.
pub async fn analyze(
    file: &str,
    exceptions: Vec<String>,
    lint: bool,
    shellcheck: bool,
) -> Result<Vec<AnalysisResult>> {
    let rules = analysis_rules();
    let rules = rules
        .iter()
        .filter(|rule| !exceptions.iter().any(|e| e.eq_ignore_ascii_case(rule.id())));
    let rules_config = DiagnosticsConfig::new(rules);

    let pb = tracing::warn_span!("progress");
    pb.pb_set_style(
        &ProgressStyle::with_template(
            "[{elapsed_precise:.cyan/blue}] {bar:40.cyan/blue} {msg} {pos}/{len}",
        )
        .unwrap(),
    );

    let start = Instant::now();
    let analyzer = Analyzer::new_with_validator(
        rules_config,
        move |_: (), kind, completed, total| {
            let pb = pb.clone();
            async move {
                if start.elapsed() < PROGRESS_BAR_DELAY_BEFORE_RENDER {
                    return;
                }

                if completed == 0 {
                    pb.pb_start();
                    pb.pb_set_length(total.try_into().unwrap());
                    pb.pb_set_message(&format!("{kind}"));
                }

                pb.pb_set_position(completed.try_into().unwrap());
            }
        },
        move || {
            let mut validator = wdl_ast::Validator::default();

            if lint {
                let visitor =
                    wdl_lint::LintVisitor::new(lint_rules().into_iter().filter_map(|rule| {
                        if exceptions.iter().any(|e| e.eq_ignore_ascii_case(rule.id())) {
                            None
                        } else {
                            Some(rule)
                        }
                    }));
                validator.add_visitor(visitor);

                if shellcheck {
                    let rule: Vec<Box<dyn wdl_lint::Rule>> =
                        vec![Box::<wdl_lint::rules::ShellCheckRule>::default()];
                    let visitor = wdl_lint::LintVisitor::new(rule);
                    validator.add_visitor(visitor);
                }
            }

            validator
        },
    );

    if let Ok(url) = Url::parse(file) {
        analyzer.add_document(url).await?;
    } else if Path::new(file).is_dir() {
        analyzer.add_directory(file.into()).await?;
    } else if let Some(url) = path_to_uri(file) {
        if !Path::new(file).is_file() {
            bail!("source file `{file}` does not exist");
        }

        analyzer.add_document(url).await?;
    } else {
        bail!("failed to convert `{file}` to a URI", file = file)
    }

    let results = analyzer.analyze(()).await?;
    Ok(results)
}

/// Parses the inputs for a task or workflow.
///
/// Returns the absolute path to the inputs file (if a path was provided), the
/// name of the workflow or task referenced by the inputs, and the inputs to the
/// workflow or task. If no `inputs` file is provided, the resulting [`Inputs`]
/// will be empty.
pub fn parse_inputs(
    document: &Document,
    name: Option<&str>,
    inputs: Option<&Path>,
) -> Result<(Option<PathBuf>, String, Inputs)> {
    if let Some(path) = inputs {
        // If a inputs file path was provided, parse the inputs from the file
        match Inputs::parse(document, path)? {
            Some((name, inputs)) => {
                // Make the inputs file path absolute so that we treat any file/directory inputs
                // in the file as relative to the inputs file itself
                let path = absolute(path).with_context(|| {
                    format!(
                        "failed to determine the absolute path of `{path}`",
                        path = path.display()
                    )
                })?;
                Ok((Some(path), name, inputs))
            }
            None => bail!("inputs file `{path}` is empty", path = path.display()),
        }
    } else if let Some(name) = name {
        // Otherwise, if a name was provided, look for a task or workflow with that
        // name
        if document.task_by_name(name).is_some() {
            Ok((None, name.to_string(), Inputs::Task(Default::default())))
        } else if document.workflow().is_some() {
            if name != document.workflow().unwrap().name() {
                bail!("document does not contain a workflow named `{name}`");
            }
            Ok((None, name.to_string(), Inputs::Workflow(Default::default())))
        } else {
            bail!("document does not contain a task or workflow named `{name}`");
        }
    } else {
        // Neither an inputs file or name was provided, look for a workflow in the
        // document; failing that, find at most one task in the document
        let (name, inputs) = document
            .workflow()
            .map(|w| Ok((w.name().to_string(), Inputs::Workflow(Default::default()))))
            .unwrap_or_else(|| {
                let mut iter = document.tasks();
                let (name, inputs) = iter
                    .next()
                    .map(|t| (t.name().to_string(), Inputs::Task(Default::default())))
                    .context(
                        "inputs file is empty and the document contains no workflow or task",
                    )?;

                if iter.next().is_some() {
                    bail!("inputs file is empty and the document contains more than one task");
                }

                Ok((name, inputs))
            })?;

        Ok((None, name, inputs))
    }
}

/// Validates the inputs for a task or workflow.
pub async fn validate_inputs(document: &str, inputs: &Path) -> Result<Option<Diagnostic>> {
    let results = analyze(document, vec![], false, false).await?;

    let uri = Url::parse(document)
        .unwrap_or_else(|_| path_to_uri(document).expect("file should be a local path"));

    let result = results
        .iter()
        .find(|r| **r.document().uri() == uri)
        .context("failed to find document in analysis results")?;
    let document = result.document();

    let (_path, name, inputs) = parse_inputs(document, None, Some(inputs))?;

    match inputs {
        Inputs::Task(inputs) => {
            inputs.validate(document, document.task_by_name(&name).unwrap(), None)?
        }
        Inputs::Workflow(inputs) => {
            inputs.validate(document, document.workflow().unwrap(), None)?
        }
    }

    Ok(None)
}

/// Evaluates a WDL task or workflow.
async fn evaluate(
    document: &Document,
    path: Option<&Path>,
    name: &str,
    config: Config,
    inputs: Inputs,
    output_dir: &Path,
    token: CancellationToken,
) -> EvaluationResult<Outputs> {
    /// Helper for displaying task ids
    struct Ids<'a>(&'a IndexSet<String>);

    impl fmt::Display for Ids<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

    /// Represents state for reporting progress
    #[derive(Default)]
    struct State {
        /// The set of currently executing task identifiers
        ids: IndexSet<String>,
        /// The number of completed tasks
        completed: usize,
        /// The number of tasks awaiting execution.
        ready: usize,
        /// The number of currently executing tasks
        executing: usize,
    }

    fn progress(kind: ProgressKind<'_>, pb: &tracing::Span, state: &Mutex<State>) {
        pb.pb_start();

        let message = {
            let mut state = state.lock().expect("failed to lock progress mutex");
            match kind {
                ProgressKind::TaskStarted { .. } => {
                    state.ready += 1;
                }
                ProgressKind::TaskExecutionStarted { id, attempt } => {
                    // If this is the first attempt, remove it from the ready set
                    if attempt == 0 {
                        state.ready -= 1;
                    }

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
                " - {c} {completed} task{s1}, {r} {ready} task{s2}, {e} {executing} task{s3}: \
                 {ids}",
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

    let run_kind = match &inputs {
        Inputs::Task(_) => "task",
        Inputs::Workflow(_) => "workflow",
    };

    let pb = tracing::warn_span!("progress");
    pb.pb_set_style(
        &ProgressStyle::with_template(&format!(
            "[{{elapsed_precise:.cyan/blue}}] {{spinner:.cyan/blue}} {running} {run_kind} \
             {name}{{msg}}",
            running = "running".cyan(),
            name = name.magenta().bold()
        ))
        .unwrap(),
    );

    let state = Mutex::<State>::default();
    match inputs {
        Inputs::Task(mut inputs) => {
            // Make any paths specified in the inputs absolute
            let task = document
                .task_by_name(name)
                .ok_or_else(|| anyhow!("document does not contain a task named `{name}`"))?;

            // Ensure all the paths specified in the inputs file are relative to the file's
            // directory
            if let Some(path) = path.as_ref().and_then(|p| p.parent()) {
                inputs.join_paths(task, path)?;
            }

            let evaluator = TaskEvaluator::new(config, token).await?;
            evaluator
                .evaluate(document, task, &inputs, output_dir, move |kind| {
                    progress(kind, &pb, &state);
                    async {}
                })
                .await
                .and_then(EvaluatedTask::into_result)
        }
        Inputs::Workflow(mut inputs) => {
            let workflow = document
                .workflow()
                .ok_or_else(|| anyhow!("document does not contain a workflow"))?;
            if workflow.name() != name {
                return Err(anyhow!("document does not contain a workflow named `{name}`").into());
            }

            // Ensure all the paths specified in the inputs file are relative to the file's
            // directory
            if let Some(path) = path.as_ref().and_then(|p| p.parent()) {
                inputs.join_paths(workflow, path)?;
            }

            let evaluator = WorkflowEvaluator::new(config, token).await?;
            evaluator
                .evaluate(document, inputs, output_dir, move |kind| {
                    progress(kind, &pb, &state);
                    async {}
                })
                .await
        }
    }
}

/// Runs a WDL task or workflow.
pub async fn run(
    document: &Document,
    path: Option<&Path>,
    name: &str,
    config: Config,
    inputs: Inputs,
    output_dir: &Path,
) -> Result<()> {
    let token = CancellationToken::new();
    let mut evaluate = evaluate(
        document,
        path,
        name,
        config,
        inputs,
        output_dir,
        token.clone(),
    )
    .boxed();

    select! {
        _ = signal::ctrl_c() => {
            error!("execution was interrupted: waiting for evaluation to abort");
            token.cancel();
            evaluate.await.ok();
            bail!("execution was aborted");
        },
        res = &mut evaluate => match res {
            Ok(outputs) => {
                println!("{outputs}", outputs = to_string_pretty(&outputs)?);
                Ok(())
            }
            Err(EvaluationError::Source(e)) => {
                emit_diagnostics(&e.document.path(), e.document.root().text().to_string(), &[e.diagnostic], &e.backtrace)?;
                bail!("aborting due to evaluation error");
            }
            Err(EvaluationError::Other(e)) => Err(e)
        },
    }
}
