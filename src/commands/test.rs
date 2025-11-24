//! Implementation of the `test` subcommand.

use std::fs::read;
use std::iter::zip;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::anyhow;
use clap::Parser;
use indexmap::IndexMap;
use itertools::Zip;
use itertools::iproduct;
use itertools::izip;
use itertools::multizip;
use itertools::repeat_n;
use serde_yaml_ng::Value;
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
    /// Path to the local WDL workspace to document.
    pub source: Option<Source>,
}

type Input = (String, Value);
type Run = Vec<Input>;

#[derive(Debug)]
struct PossibleInputs {
    pub name: String,
    pub values: Vec<Value>,
}

impl PossibleInputs {
    pub fn into_inputs_iter(self) -> impl Iterator<Item = Input> {
        zip(repeat_n(self.name.clone(), self.values.len()), self.values)
            .collect::<Vec<_>>()
            .into_iter()
    }
}

#[derive(Debug)]
struct InputsToZip {
    pub sets_of_possible_inputs: Vec<PossibleInputs>,
}

impl InputsToZip {
    pub fn into_zip(self) -> Zip<(impl Iterator<Item = Input>,)> {
        multizip((self
            .sets_of_possible_inputs
            .into_iter()
            .flat_map(|set| set.into_inputs_iter()),))
    }
}

#[derive(Debug)]
struct InputsToMultiply {
    pub sets_of_runs: Vec<InputsToZip>,
}

// impl InputsToMultiply {
//     pub fn into_runs(self) -> impl IntoIterator<Item = Run> {
//         iproduct!(
//             self.sets_of_runs
//                 .into_iter()
//                 .flat_map(|to_be_zipped| to_be_zipped.into_zip()),
//         )
//     }
// }

fn compute_runs_from_matrix(matrix: Vec<IndexMap<String, Value>>) {
    let mut transformed_matrix = Vec::new();
    for set in matrix {
        let mut inputs_to_zip = vec![];
        for (key, val) in set {
            let Some(seq) = val.as_sequence() else {
                warn!("expected sequence of values, found `{:#?}`", val);
                continue;
            };
            let mut possible_inputs = vec![];
            for val in seq {
                possible_inputs.push(val.clone());
            }
            inputs_to_zip.push(PossibleInputs {
                name: key,
                values: possible_inputs,
            });
        }
        let transformed_set = InputsToZip {
            sets_of_possible_inputs: inputs_to_zip,
        };
        transformed_matrix.push(transformed_set);
    }
    let mut outer = InputsToMultiply {
        sets_of_runs: transformed_matrix,
    };
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
        let document_tests: crate::test::DocumentTests = serde_yaml_ng::from_slice(
            &read(&yaml_path)
                .with_context(|| format!("reading file: `{}`", yaml_path.display()))?,
        )
        .with_context(|| format!("parsing YAML: `{}`", yaml_path.display()))?;
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
                    info!("assertions: {:#?}", &assertions);
                    info!("logging each individual execution defined by test matrix");
                    let mut counter = 0;
                    // for run in compute_runs_from_matrix(input_matrix) {
                    //     info!("execution with inputs: {:#?}", run);
                    //     counter += 1;
                    // }
                    compute_runs_from_matrix(input_matrix);
                    info!("computed {counter} executions");
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
                    info!("assertions: {:#?}", &assertions);
                    let mut counter = 0;
                    // for run in compute_runs_from_matrix(input_matrix) {
                    //     info!("execution with inputs: {:#?}", run);
                    //     counter += 1;
                    // }
                    compute_runs_from_matrix(input_matrix);
                    info!("computed {counter} executions");
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
