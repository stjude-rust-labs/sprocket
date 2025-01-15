//! Entry point functions for the command-line interface.

use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use colored::Colorize;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use serde_json::to_string_pretty;
use tokio::fs;
use url::Url;
use wdl_analysis::AnalysisResult;
use wdl_analysis::Analyzer;
use wdl_analysis::DiagnosticsConfig;
use wdl_analysis::document::Document;
use wdl_analysis::path_to_uri;
use wdl_analysis::rules as analysis_rules;
use wdl_engine::EvaluationError;
use wdl_engine::Inputs;
use wdl_engine::local::LocalTaskExecutionBackend;
use wdl_engine::v1::TaskEvaluator;
use wdl_engine::v1::WorkflowEvaluator;
use wdl_grammar::Diagnostic;
use wdl_lint::rules as lint_rules;

/// The delay in showing the progress bar.
///
/// This is to prevent the progress bar from flashing on the screen for
/// very short analyses.
const PROGRESS_BAR_DELAY_BEFORE_RENDER: Duration = Duration::from_secs(2);

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
        .filter(|rule| !exceptions.iter().any(|e| e == rule.id()));
    let rules_config = DiagnosticsConfig::new(rules);

    let analyzer = Analyzer::new_with_validator(
        rules_config,
        move |bar: ProgressBar, kind, completed, total| async move {
            if bar.elapsed() < PROGRESS_BAR_DELAY_BEFORE_RENDER {
                return;
            }

            if completed == 0 || bar.length() == Some(0) {
                bar.set_length(total.try_into().unwrap());
                bar.set_message(format!("{kind}"));
            }

            bar.set_position(completed.try_into().unwrap());
        },
        move || {
            let mut validator = wdl_ast::Validator::default();

            if lint {
                let visitor =
                    wdl_lint::LintVisitor::new(lint_rules().into_iter().filter_map(|rule| {
                        if exceptions.iter().any(|e| e == rule.id()) {
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
    } else if fs::metadata(&file).await?.is_dir() {
        analyzer.add_directory(file.into()).await?;
    } else if let Some(url) = path_to_uri(file) {
        analyzer.add_document(url).await?;
    } else {
        bail!("failed to convert `{file}` to a URI", file = file)
    }

    let bar = ProgressBar::new(0);
    bar.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise:.cyan/blue}] {bar:40.cyan/blue} {msg} {pos}/{len}",
        )
        .unwrap(),
    );

    let results = analyzer.analyze(bar.clone()).await?;

    anyhow::Ok(results)
}

/// Parses the inputs for a task or workflow.
pub fn parse_inputs(
    document: &Document,
    name: Option<&str>,
    inputs: Option<&Path>,
) -> Result<(Option<PathBuf>, String, Inputs)> {
    let (path, name, inputs) = if let Some(path) = inputs {
        let abs_path = absolute(path).with_context(|| {
            format!(
                "failed to determine the absolute path of `{path}`",
                path = path.display()
            )
        })?;
        match Inputs::parse(document, &abs_path)? {
            Some((name, inputs)) => (Some(path.to_path_buf()), name, inputs),
            None => bail!("inputs file `{path}` is empty", path = path.display()),
        }
    } else if let Some(name) = name {
        if document.task_by_name(name).is_some() {
            (None, name.to_string(), Inputs::Task(Default::default()))
        } else if document.workflow().is_some() {
            if name != document.workflow().unwrap().name() {
                bail!("document does not contain a workflow named `{name}`");
            }
            (None, name.to_string(), Inputs::Workflow(Default::default()))
        } else {
            bail!("document does not contain a task or workflow named `{name}`");
        }
    } else {
        let mut iter = document.tasks();
        let (name, inputs) = iter
            .next()
            .map(|t| (t.name().to_string(), Inputs::Task(Default::default())))
            .or_else(|| {
                document
                    .workflow()
                    .map(|w| (w.name().to_string(), Inputs::Workflow(Default::default())))
            })
            .context("inputs file is empty and the WDL document contains no tasks or workflow")?;

        if iter.next().is_some() {
            bail!("inputs file is empty and the WDL document contains more than one task");
        }

        (None, name, inputs)
    };

    anyhow::Ok((path, name, inputs))
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

    anyhow::Ok(None)
}

/// Run a WDL task or workflow.
pub async fn run(
    document: &Document,
    path: Option<&Path>,
    name: &str,
    inputs: Inputs,
    output_dir: &Path,
) -> Result<Option<Diagnostic>> {
    let bar = ProgressBar::new_spinner();
    bar.set_message(format!(
        "{running} {kind} {name}",
        running = "running".cyan(),
        kind = match &inputs {
            Inputs::Task(_) => "task",
            Inputs::Workflow(_) => "workflow",
        },
        name = name.magenta().bold()
    ));
    bar.enable_steady_tick(Duration::from_millis(100));
    bar.set_style(
        ProgressStyle::with_template("[{elapsed_precise:.cyan/blue}] {spinner:.cyan/blue} {msg}")
            .unwrap(),
    );

    match inputs {
        Inputs::Task(mut inputs) => {
            // Make any paths specified in the inputs absolute
            let task = document
                .task_by_name(name)
                .ok_or_else(|| anyhow!("document does not contain a task named `{name}`"))?;

            // Ensure all the paths specified in the inputs file are relative to the file's
            // directory
            if let Some(path) = path.as_ref().and_then(|p| p.parent()) {
                inputs.join_paths(task, path);
            }

            let backend = LocalTaskExecutionBackend::new(None);
            let mut evaluator = TaskEvaluator::new(&backend);
            match evaluator
                .evaluate(document, task, &inputs, output_dir, name)
                .await
            {
                Ok(evaluated) => match evaluated.into_result() {
                    Ok(outputs) => {
                        drop(bar);
                        let s = to_string_pretty(&outputs)?;
                        println!("{s}\n");
                        Ok(None)
                    }
                    Err(e) => match e {
                        EvaluationError::Source(diagnostic) => Ok(Some(diagnostic)),
                        EvaluationError::Other(e) => Err(e),
                    },
                },
                Err(e) => match e {
                    EvaluationError::Source(diagnostic) => Ok(Some(diagnostic)),
                    EvaluationError::Other(e) => Err(e),
                },
            }
        }
        Inputs::Workflow(mut inputs) => {
            let workflow = document
                .workflow()
                .ok_or_else(|| anyhow!("document does not contain a workflow"))?;
            if workflow.name() != name {
                bail!("document does not contain a workflow named `{name}`");
            }

            // Ensure all the paths specified in the inputs file are relative to the file's
            // directory
            if let Some(path) = path.as_ref().and_then(|p| p.parent()) {
                inputs.join_paths(workflow, path);
            }

            let mut evaluator =
                WorkflowEvaluator::new(Arc::new(LocalTaskExecutionBackend::new(None)));
            match evaluator.evaluate(document, &inputs, output_dir).await {
                Ok(outputs) => {
                    drop(bar);
                    let s = to_string_pretty(&outputs)?;
                    println!("{s}\n");
                    Ok(None)
                }
                Err(e) => match e {
                    EvaluationError::Source(diagnostic) => Ok(Some(diagnostic)),
                    EvaluationError::Other(e) => Err(e),
                },
            }
        }
    }
}
