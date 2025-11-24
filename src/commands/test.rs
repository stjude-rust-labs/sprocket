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

/// A tuple of an input name (`String`) and an input value (as a [`Value`] which has not been converted into a WDL value yet)
type Input = (String, Value);
/// Collection of [`Input`]s which correspond to a single "run" for Sprocket to test with.
/// Should be a complete set of required inputs (potentially with values for optional inputs).
type Run = Vec<Input>;

/// User defined (via test YAML) set of possible inputs for a single WDL input.
#[derive(Debug)]
struct PossibleInputs {
    /// Name of the input.
    pub name: String,
    /// Collection of YAML [`Value`]s that correspond to the named input.
    pub values: Vec<Value>,
}

impl PossibleInputs {
    /// Transform into an iterator of [`Input`]s for this individual input key.
    pub fn into_inputs_iter(self) -> impl Iterator<Item = Input> {
        zip(repeat_n(self.name.clone(), self.values.len()), self.values)
    }
}

/// Inputs which should be zipped and iterated through together.
///
/// An example would be a set of BAM input files and their corresponding BAI files.
#[derive(Debug)]
struct InputsToZip {
    /// Inputs which should be zipped and iterated through together.
    pub sets_of_possible_inputs: Vec<PossibleInputs>,
}

impl InputsToZip {
    /// Transform into an iterator of `N` [`Input`]s which should be iterated through together.
    pub fn into_zip(self) -> Zip<(impl Iterator<Item = Input>,)> {
        multizip((self
            .sets_of_possible_inputs
            .into_iter()
            .flat_map(|set| set.into_inputs_iter()),))
    }
}

/// Collection of [`InputsToZip`] which together define a collection of [`Run`]s.
#[derive(Debug)]
struct AllInputsToEntrypoint {
    /// Collection of all [`InputsToZip`] for a WDL task or workflow.
    pub sets_of_inputs: Vec<InputsToZip>,
}

impl AllInputsToEntrypoint {
    /// Transform into an iterator of [`Run`]s.
    pub fn into_runs(self) -> impl Iterator<Item = Run> {
        // Something with `iproduct!()`?
    }
}

/// Compute an iterator of [`Run`]s from a user provided matrix of inputs.
fn compute_runs_from_matrix(matrix: Vec<IndexMap<String, Value>>) -> impl Iterator<Item = Run> {
    let mut all_inputs = Vec::new();
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
        all_inputs.push(transformed_set);
    }
    let all_inputs = AllInputsToEntrypoint {
        sets_of_inputs: all_inputs,
    };
    all_inputs.into_runs()
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
                    for run in compute_runs_from_matrix(input_matrix) {
                        info!("execution with inputs: {:#?}", run);
                        counter += 1;
                    }
                    // compute_runs_from_matrix(input_matrix);
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
                    for run in compute_runs_from_matrix(input_matrix) {
                        info!("execution with inputs: {:#?}", run);
                        counter += 1;
                    }
                    // compute_runs_from_matrix(input_matrix);
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
