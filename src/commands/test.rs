//! Implementation of the `test` subcommand.

use std::fs::read;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::anyhow;
use clap::Parser;
use nonempty::NonEmpty;
use tracing::info;
use tracing::trace;
use tracing::warn;

use crate::analysis::Analysis;
use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;

/// Arguments for the `test` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
    /// Local path to a WDL document or workspace to unit test.
    pub source: Option<Source>,
}

/// Performs the `test` command.
pub async fn test(args: Args) -> CommandResult<()> {
    let source = args.source.unwrap_or_default();
    let source = match &source {
        Source::Remote(_) => {
            return Err(anyhow!("the `test` subcommand does not accept remote sources").into());
        }
        Source::Directory(_) | Source::File(_) => source,
    };
    let results = Analysis::default()
        .add_source(source.clone())
        .run()
        .await
        .map_err(CommandError::from)?;

    let mut errors = Vec::new();
    for result in results.filter(&[&source]) {
        let document = result.document();
        let wdl_path = PathBuf::from(document.path().as_ref());
        let mut yaml_path = wdl_path.with_extension("yaml");
        if !yaml_path.exists() {
            yaml_path = wdl_path.with_extension("yml");
            if !yaml_path.exists() {
                trace!("no tests found for WDL document: `{}`", wdl_path.display());
                continue;
            }
        }
        info!("---------NEW WDL DOCUMENT----------");
        println!(
            "found tests in `{}` for WDL document `{}`",
            yaml_path.display(),
            wdl_path.display()
        );
        let document_tests: crate::test::DocumentTests = serde_yaml_ng::from_slice(
            &read(&yaml_path)
                .with_context(|| format!("reading file: `{}`", yaml_path.display()))?,
        )
        .with_context(|| format!("parsing YAML: `{}`", yaml_path.display()))?;
        for (entrypoint, tests) in document_tests.entrypoints.iter() {
            if let Some(task) = document.task_by_name(entrypoint) {
                info!("-------NEW TASK-------");
                println!("found tests for task: `{}`", task.name());
                for test in tests {
                    let assertions = test.parse_assertions();
                    info!("---NEW TEST---");
                    println!("test name: `{}`", &test.name);
                    info!("assertions: {:#?}", &assertions);
                    info!("logging each individual execution defined by test matrix");
                    let mut counter = 0;
                    let matrix = match test.parse_inputs().with_context(|| {
                        format!("parsing test `{}` in `{}`", test.name, yaml_path.display())
                    }) {
                        Ok(matrix) => matrix,
                        Err(e) => {
                            errors.push(Arc::new(e));
                            continue;
                        }
                    };
                    for run in matrix.cartesian_product() {
                        info!("execution with inputs: {:#?}", run.collect::<Vec<_>>());
                        counter += 1;
                    }
                    println!("computed {counter} executions");
                }
            } else if let Some(workflow) = document.workflow()
                && workflow.name() == entrypoint
            {
                info!("-------NEW WORKFLOW-------");
                println!("found tests for workflow: `{}`", workflow.name());
                for test in tests {
                    let assertions = test.parse_assertions();
                    info!("---NEW TEST---");
                    println!("test name: `{}`", &test.name);
                    info!("assertions: {:#?}", &assertions);
                    info!("logging each individual execution defined by test matrix");
                    let mut counter = 0;
                    let matrix = match test.parse_inputs().with_context(|| {
                        format!("parsing test `{}` in `{}`", test.name, yaml_path.display())
                    }) {
                        Ok(matrix) => matrix,
                        Err(e) => {
                            errors.push(Arc::new(e));
                            continue;
                        }
                    };
                    for run in matrix.cartesian_product() {
                        info!("execution with inputs: {:#?}", run.collect::<Vec<_>>());
                        counter += 1;
                    }
                    println!("computed {counter} executions");
                }
            } else {
                warn!(
                    "no task or workflow named `{entrypoint}` found in `{}`",
                    wdl_path.display()
                );
            }
        }
    }

    if let Some(errors) = NonEmpty::from_vec(errors) {
        return Err(CommandError::from(errors));
    };

    Ok(())
}
