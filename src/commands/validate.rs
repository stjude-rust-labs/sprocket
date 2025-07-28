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
    /// This argument is required if trying to validate a task or workflow
    /// without any inputs.
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
    if let Source::Directory(_) = args.source {
        bail!("directory sources are not supported for the `validate` command");
    }

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

    let inputs = Inputs::coalesce(&args.inputs, args.entrypoint.clone())
        .with_context(|| {
            format!(
                "failed to parse inputs from `{sources}`",
                sources = args.inputs.join("`, `")
            )
        })?
        .into_engine_inputs(document)?;

    let (name, inputs, _) = if let Some(inputs) = inputs {
        inputs
    } else {
        // No inputs provided
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
