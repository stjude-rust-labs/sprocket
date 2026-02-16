//! Implementation of the `validate` subcommand.

use anyhow::Context;
use anyhow::anyhow;
use clap::Parser;
use wdl::engine::Inputs as EngineInputs;

use crate::Config;
use crate::analysis::Analysis;
use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::diagnostics::Mode;
use crate::inputs::Invocation;
use crate::inputs::OriginPaths;

/// Arguments for the `validate` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The path or URL to a document containing the task or workflow to
    /// validate inputs against.
    #[clap(value_name = "SOURCE")]
    pub source: Source,

    /// The name of the task or workflow to validate inputs against.
    ///
    /// This argument is required if trying to validate a task or workflow
    /// without any inputs.
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

    /// The inputs for the task or workflow.
    ///
    /// These inputs can be either paths to files containing inputs or key-value
    /// pairs passed in on the command line.
    pub inputs: Vec<String>,

    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,
}

/// The main function for the `validate` subcommand.
pub async fn validate(args: Args, config: Config) -> CommandResult<()> {
    if let Source::Directory(_) = args.source {
        return Err(
            anyhow!("directory sources are not supported for the `validate` command").into(),
        );
    }

    let results = Analysis::default()
        .add_source(args.source.clone())
        .fallback_version(config.common.wdl.fallback_version)
        .run()
        .await
        .map_err(CommandError::from)?;

    // SAFETY: this must exist, as we added it as the only source to be analyzed
    // above.
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

    let (name, inputs, origins) = if let Some(inputs) = inputs {
        inputs
    } else {
        // No inputs provided
        let origins = OriginPaths::Single(
            std::env::current_dir()
                .context("failed to get current directory")?
                .as_path()
                .into(),
        );

        if let Some(name) = args.target {
            match (document.task_by_name(&name), document.workflow()) {
                (Some(_), _) => (name, EngineInputs::Task(Default::default()), origins),
                (None, Some(workflow)) => {
                    if workflow.name() == name {
                        (name, EngineInputs::Workflow(Default::default()), origins)
                    } else {
                        return Err(anyhow!(
                            "no task or workflow with name `{name}` was found in document `{path}`",
                            path = document.path()
                        )
                        .into());
                    }
                }
                (None, None) => {
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

    match inputs {
        EngineInputs::Task(mut inputs) => {
            // SAFETY: we wouldn't have a task inputs if a task didn't exist
            // that matched the user's criteria.
            let task = document.task_by_name(&name).unwrap();
            inputs
                .join_paths(task, |key| {
                    origins
                        .get(key)
                        .ok_or(anyhow!("unable to find origin path for key `{key}`"))
                })
                .await?;
            inputs.validate(document, task, None)?
        }
        EngineInputs::Workflow(mut inputs) => {
            // SAFETY: we wouldn't have a workflow inputs if a workflow didn't
            // exist that matched the user's criteria.
            let workflow = document.workflow().unwrap();
            inputs
                .join_paths(workflow, |key| {
                    origins
                        .get(key)
                        .ok_or(anyhow!("unable to find origin path for key `{key}`"))
                })
                .await?;
            inputs.validate(document, workflow, None)?
        }
    }

    Ok(())
}
