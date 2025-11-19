//! Implementation of the `test` subcommand.

use std::fs::read;
use std::path::PathBuf;

use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use indexmap::IndexMap;
use serde_yaml_ng::Value;
use tracing::info;
use tracing::trace;
use tracing::warn;

use crate::analysis::Analysis;
use crate::analysis::Source;

/// Arguments for the `test` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
    /// Path to the local WDL workspace to document.
    pub source: Option<Source>,
}

fn compute_runs_from_matrix(matrix: &Vec<IndexMap<String, Value>>) {
    for set in matrix {
        let mut transformed_set = IndexMap::new();
        for (key, val) in set.into_iter() {
            let Some(seq) = val.as_sequence() else {
                warn!("expected sequence of values, found `{:#?}`", val);
                continue;
            };
            transformed_set.insert(
                key.clone(),
                seq.iter().map(|v| v.clone()).collect::<Vec<_>>(),
            );
        }
        info!("processing the input set: `{:#?}`", transformed_set);
    }
}

/// Performs the `test` command.
pub async fn test(args: Args) -> Result<()> {
    let source = args.source.unwrap_or_default();
    let source = match &source {
        Source::Remote(_) => {
            bail!("the `test` subcommand does not accept remote sources")
        }
        Source::Directory(_) | Source::File(_) => source,
    };
    let results = match Analysis::default().add_source(source.clone()).run().await {
        Ok(results) => results,
        Err(errors) => {
            // SAFETY: this is a non-empty, so it must always have a first
            // element.
            bail!(errors.into_iter().next().unwrap())
        }
    };

    for result in results.filter(&[&source]) {
        let document = result.document();
        let wdl_path = PathBuf::from(document.path().as_ref());
        let yaml_path = wdl_path.with_extension("yaml");
        if !yaml_path.exists() {
            trace!("no tests found for WDL document: `{}`", wdl_path.display());
            continue;
        }
        info!("---------NEW WDL DOCUMENT----------");
        info!(
            "found tests in `{}` for WDL document `{}`",
            yaml_path.display(),
            wdl_path.display()
        );
        let document_tests: crate::test::DocumentTests =
            serde_yaml_ng::from_slice(&read(yaml_path)?)?;
        for (entrypoint, tests) in document_tests.entrypoints.iter() {
            if let Some(task) = document.task_by_name(entrypoint) {
                info!("-------NEW TASK-------");
                info!("found tests for task: `{}`", task.name());
                info!(
                    "task `{}` has the following input specification {:#?}",
                    task.name(),
                    task.inputs()
                );
                for test in tests {
                    let input_matrix = test.parse_inputs();
                    let assertions = test.parse_assertions();
                    info!("---NEW TEST---");
                    info!("test name: `{}`", &test.name);
                    compute_runs_from_matrix(&input_matrix);
                    info!("assertions: {:#?}", &assertions);
                }
            } else if let Some(workflow) = document.workflow()
                && workflow.name() == entrypoint
            {
                info!("-------NEW WORKFLOW-------");
                info!("found tests for workflow: `{}`", workflow.name());
                info!(
                    "workflow `{}` has the following input specification {:#?}",
                    workflow.name(),
                    workflow.inputs()
                );
                for test in tests {
                    let input_matrix = test.parse_inputs();
                    let assertions = test.parse_assertions();
                    info!("---NEW TEST---");
                    info!("test name: `{}`", &test.name);
                    compute_runs_from_matrix(&input_matrix);
                    info!("assertions: {:#?}", &assertions);
                }
            } else {
                warn!(
                    "no task or workflow named `{entrypoint}` found in `{}`",
                    wdl_path.display()
                );
            }
        }
    }

    Ok(())
}
