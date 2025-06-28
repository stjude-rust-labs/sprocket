//! Implementation of the `validate` subcommand.

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use wdl::cli::Analysis;
use wdl::cli::Inputs;
use wdl::cli::analysis::Source;
use wdl::cli::inputs::OriginPaths;
use wdl::engine::Inputs as EngineInputs;

use crate::Mode;

/// Arguments for the `validate` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The path or URL to a document containing the task or workflow to
    /// validate inputs against.
    #[clap(value_name = "PATH or URL")]
    pub source: Source,

    /// The name of the task or workflow to validate inputs against.
    ///
    /// If inputs are provided, this will be attempted to be inferred from the
    /// prefixed names of the inputs (e.g, `<name>.<input-name>`).
    ///
    /// If no inputs are provided and this argument is not provided, it will be
    /// assumed you're trying to validate the workflow present in the specified
    /// document.
    #[clap(short, long, value_name = "NAME")]
    pub name: Option<String>,

    /// The inputs for the task or workflow.
    ///
    /// These inputs can be either paths to files containing inputs or key-value
    /// pairs passed in on the command line.
    pub inputs: Vec<String>,

    /// Disables color output.
    #[arg(long)]
    pub no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,
}

impl Args {
    /// Applies the configuration to the arguments.
    pub fn apply(mut self, config: crate::config::Config) -> Self {
        self.no_color = self.no_color || !config.common.color;
        self.report_mode = match self.report_mode {
            Some(mode) => Some(mode),
            None => Some(config.common.report_mode),
        };
        self
    }
}

/// The main function for the `validate` subcommand.
pub async fn validate(args: Args) -> Result<()> {
    let results = match Analysis::default()
        .add_source(args.source.clone())
        .run()
        .await
    {
        Ok(results) => results,
        Err(errors) => {
            // SAFETY: this is a non-empty, so it must always have a first
            // element.
            bail!(errors.into_iter().next().unwrap())
        }
    };

    // SAFETY: this must exist, as we added it as the only source to be analyzed
    // above.
    let document = results.filter(&[&args.source]).next().unwrap().document();

    let inferred = Inputs::coalesce(&args.inputs)
        .with_context(|| {
            format!(
                "failed to parse inputs from `{sources}`",
                sources = args.inputs.join("`, `")
            )
        })?
        .into_engine_inputs(document)?;

    let (name, inputs, _) = if let Some(inputs) = inferred {
        inputs
    } else {
        let origins =
            OriginPaths::from(std::env::current_dir().context("failed to get current directory")?);

        if let Some(name) = args.name {
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
        } else if let Some(workflow) = document.workflow() {
            (
                workflow.name().to_owned(),
                EngineInputs::Workflow(Default::default()),
                origins,
            )
        } else {
            let mut tasks = document.tasks();
            let first = tasks.next();
            if tasks.next().is_some() {
                bail!(
                    "document `{path}` contains more than one task: use the `--name` option to \
                     refer to a specific task by name",
                    path = document.path()
                )
            } else if let Some(task) = first {
                (
                    task.name().to_string(),
                    EngineInputs::Task(Default::default()),
                    origins,
                )
            } else {
                bail!(
                    "document `{path}` contains no workflow or task",
                    path = document.path()
                );
            }
        }
    };

    match inputs {
        EngineInputs::Task(inputs) => {
            // SAFETY: we wouldn't have a task inputs if a task didn't exist
            // that matched the user's criteria.
            inputs.validate(document, document.task_by_name(&name).unwrap(), None)?
        }
        EngineInputs::Workflow(inputs) => {
            // SAFETY: we wouldn't have a workflow inputs if a workflow didn't
            // exist that matched the user's criteria.
            inputs.validate(document, document.workflow().unwrap(), None)?
        }
    }

    Ok(())
}
