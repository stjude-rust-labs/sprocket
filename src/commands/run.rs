//! Implementation of the run command.

use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::fs;

use anyhow::Result;
use anyhow::anyhow;
use anyhow::Context;
use anyhow::bail;
use clap::Parser;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::emit;
use tracing_log::log;
use url::Url;
use wdl::analysis::path_to_uri;
use wdl::engine::local::LocalTaskExecutionBackend;
use wdl::engine::Engine;
use wdl::engine::Inputs;
use wdl::engine::EvaluationError;
use wdl::engine::v1::TaskEvaluator;

use crate::analyze;
use crate::Mode;
use crate::get_display_config;

/// Arguments for the run command.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct RunArgs {
    /// The path or URL to the WDL document containing the task to run.
    #[arg(value_name = "PATH or URL")]
    pub file: String,

    /// The path to the input JSON file.
    ///
    /// If not provided, an empty set of inputs will be sent to the task.
    #[arg(short, long, value_name = "JSON", conflicts_with = "name")]
    pub inputs: Option<PathBuf>,

    /// The name of the task to run.
    ///
    /// Required if no `inputs` file is provided.
    #[arg(short, long, value_name = "NAME", conflicts_with = "inputs")]
    pub name: Option<String>,

    /// The output directory; defaults to the task name.
    #[arg(short, long, value_name = "DIR")]
    pub output: Option<PathBuf>,

    /// Overwrite the output directory if it exists.
    #[arg(long)]
    pub overwrite: bool,

    /// Disables color output.
    #[arg(long)]
    pub no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, default_value_t, value_name = "MODE")]
    pub report_mode: Mode,
}

/// Runs a task.
pub async fn run(args: RunArgs) -> Result<()> {
    let file = args.file;
    if Path::new(&file).is_dir() {
        anyhow::bail!("expected a WDL document, found a directory");
    }
    let (config, mut stream) = get_display_config(args.report_mode, args.no_color);

    let results = analyze(&file, vec![], false).await?;

    let uri = if let Ok(uri) = Url::parse(&file) {
        uri
    } else {
        path_to_uri(&file).expect("file should be a local path")
    };

    let result = results
        .iter()
        .find(|r| **r.document().uri() == uri)
        .context("failed to find document in analysis results")?;
    let document = result.document();

    let mut engine = Engine::new(LocalTaskExecutionBackend::new());
    let (path, name, inputs) = if let Some(path) = args.inputs {
        let abs_path = absolute(&path).with_context(|| {
            format!(
                "failed to determine the absolute path of `{path}`",
                path = path.display()
            )
        })?;
        match Inputs::parse(engine.types_mut(), document, &abs_path)? {
            Some((name, inputs)) => (Some(path), name, inputs),
            None => bail!(
                "inputs file `{path}` is empty; use the `--name` option to specify the name \
                 of the task or workflow to run",
                path = path.display()
            ),
        }
    } else if let Some(name) = args.name {
        if document.task_by_name(&name).is_some() {
            (None, name, Inputs::Task(Default::default()))
        } else if document.workflow().is_some() {
            (None, name, Inputs::Workflow(Default::default()))
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
            .context(
                "inputs file is empty and the WDL document contains no tasks or workflow",
            )?;

        if iter.next().is_some() {
            bail!("inputs file is empty and the WDL document contains more than one task");
        }

        (None, name, inputs)
    };


    let output_dir_specified = args.output.is_some();
    let output_dir = args
        .output
        .unwrap_or_else(|| Path::new(&format!("sprocket_run-{}-0", &name)).to_path_buf());

    let output_dir = if output_dir.exists() {
        if args.overwrite {
            fs::remove_dir_all(&output_dir).with_context(|| {
                format!(
                    "failed to remove output directory `{dir}`",
                    dir = output_dir.display()
                )
            })?;
            output_dir
        } else if output_dir_specified {
            bail!(
                "output directory `{dir}` already exists; use the `--overwrite` option to overwrite it",
                dir = output_dir.display()
            );
        } else {
            log::warn!(
                "output directory `{dir}` already exists; incrementing the run number",
                dir = output_dir.display()
            );
            let mut run_number: usize = 1;
            let mut new_output_dir = output_dir.clone();
            while new_output_dir.exists() {
                new_output_dir = output_dir.with_file_name(format!(
                    "sprocket_run-{}-{}",
                    &name, run_number
                ));
                run_number += 1;
            }
            new_output_dir
        }
    } else {
        output_dir
    };

    match inputs {
        Inputs::Task(mut inputs) => {
            // Make any paths specified in the inputs absolute
            let task = document
                .task_by_name(&name)
                .ok_or_else(|| anyhow!("document does not contain a task named `{name}`"))?;

            // Ensure all the paths specified in the inputs file are relative to the file's
            // directory
            if let Some(path) = path.as_ref().and_then(|p| p.parent()) {
                inputs.join_paths(engine.types_mut(), document, task, path);
            }

            let mut evaluator = TaskEvaluator::new(&mut engine);
            match evaluator
                .evaluate(document, task, &inputs, &output_dir, &name)
                .await
            {
                Ok(evaluated) => {
                    match evaluated.into_result() {
                        Ok(outputs) => {
                            // Buffer the entire output before writing it out in case there are
                            // errors during serialization.
                            let mut buffer = Vec::new();
                            let mut serializer = serde_json::Serializer::pretty(&mut buffer);
                            outputs.serialize(engine.types(), &mut serializer)?;
                            println!(
                                "{buffer}\n",
                                buffer = std::str::from_utf8(&buffer)
                                    .expect("output should be UTF-8")
                            );
                        }
                        Err(e) => match e {
                            EvaluationError::Source(diagnostic) => {
                                let file = SimpleFile::new(
                                    uri.to_string(),
                                    document.node().syntax().text().to_string(),
                                );
                                emit(&mut stream, &config, &file, &diagnostic.to_codespan())?;

                                bail!("aborting due to task evaluation failure");
                            }
                            EvaluationError::Other(e) => return Err(e),
                        },
                    }
                }
                Err(e) => match e {
                    EvaluationError::Source(diagnostic) => {
                        let file = SimpleFile::new(
                            uri.to_string(),
                            document.node().syntax().text().to_string(),
                        );
                        emit(&mut stream, &config, &file, &diagnostic.to_codespan())?;

                        bail!("aborting due to task evaluation failure");
                    }
                    EvaluationError::Other(e) => return Err(e),
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
                inputs.join_paths(engine.types_mut(), document, workflow, path);
            }

            bail!("running workflows is not yet supported")
        }
    }

    Ok(())
}
