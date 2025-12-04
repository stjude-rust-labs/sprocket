//! Implementation of the `test` subcommand.

use std::fs::read;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::anyhow;
use clap::Parser;
use indexmap::IndexMap;
use itertools::Itertools;
use itertools::enumerate;
use serde_yaml_ng::Value;
use tracing::info;
use tracing::trace;
use tracing::warn;

use crate::analysis::Analysis;
use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::test::InputMapping;

/// Arguments for the `test` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
    /// Local path to a WDL document or workspace to unit test.
    pub source: Option<Source>,
}

/// A tuple of an input name (`String`) and an input value (as a
/// [`serde_yaml_ng::Value`] which has not been converted into a WDL value yet)
type Input = (String, Value);
/// A map of input keys to values which correspond to a single "run" or
/// execution for Sprocket to test with. Should be a complete set of required
/// inputs (potentially with values for optional inputs).
///
/// e.g.
/// {
///   "bams": ["$FIXTURES/test1.bam", "$FIXTURES/test2.bam"],
///   "prefix": "test.merged",
/// }
type Run = IndexMap<String, Value>;

/// "zip" may not be the most technically accurate term for this operation,
/// but it transforms a `Vec<Vec<Input>>` into a different shape.
fn zip_inputs(inputs_to_zip: Vec<Vec<Input>>) -> Vec<Vec<Input>> {
    let mut result = Vec::new();
    let mut initial_len = None;
    for (outer_index, individual_input_with_possible_values) in enumerate(inputs_to_zip) {
        if let Some(prev_len) = initial_len
            && prev_len != individual_input_with_possible_values.len()
        {
            panic!("dimensions of input matrix are inconsistent")
        }
        if initial_len.is_none() {
            initial_len = Some(individual_input_with_possible_values.len());
        }
        for (inner_index, possibility) in enumerate(individual_input_with_possible_values) {
            if outer_index == 0 {
                result.push(vec![possibility]);
            } else {
                result[inner_index].push(possibility);
            }
        }
    }
    result
}

/// Compute an iterator of [`Run`]s from a user provided matrix of inputs.
///
/// Steps to complete transformation:
/// 1. distribute (by duplication) inner input keys to each "possible value" for
///    that key.
///     - this converts `IndexMap<String, Vec<Value>>` to `Vec<(String, Value)>`
/// 2. zip groups of possible inputs with [`zip_inputs`]
///     - this reshapes a `Vec<Vec<(String, Value)>>` (the type signature
///       remains the same, but the nested structure changes)
/// 3. take the cartesian product of all zipped groups
///     - this yields items of type `Vec<Vec<(String, Value)>>`
/// 4. flatten each item yielded
///     - results in a `Vec<(String, value)>`
/// 5. convert each item into a [`Run`] (which is a type alias for
///    `IndexMap<String, Value>`)
fn compute_runs_from_matrix(
    matrix: impl Iterator<Item = InputMapping>,
) -> impl Iterator<Item = Run> {
    matrix
        .map(|input_mapping| {
            let mut inputs_to_zip = vec![];
            for (key, vals) in input_mapping {
                let mut possible_inputs = Vec::new();
                // step 1
                for possible_val in vals {
                    possible_inputs.push((key.clone(), possible_val));
                }
                inputs_to_zip.push(possible_inputs);
            }

            let mut run_subsets = Vec::new();
            // step 2
            for run_subset in zip_inputs(inputs_to_zip) {
                run_subsets.push(run_subset);
            }
            run_subsets
        })
        .multi_cartesian_product() // step 3
        .map(|product| product.into_iter().flatten().collect()) // step 4 and 5
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
                    for run in compute_runs_from_matrix(test.parse_inputs()) {
                        info!("execution with inputs: {:#?}", run);
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
                    for run in compute_runs_from_matrix(test.parse_inputs()) {
                        info!("execution with inputs: {:#?}", run);
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

    Ok(())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn zip_1_by_2() {
        let start = vec![vec![
            ("output_singletons".to_string(), Value::Bool(true)),
            ("output_singletons".to_string(), Value::Bool(false)),
        ]];
        let end = zip_inputs(start);
        assert_eq!(
            end,
            vec![
                vec![("output_singletons".to_string(), Value::Bool(true)),],
                vec![("output_singletons".to_string(), Value::Bool(false)),]
            ]
        )
    }

    #[test]
    fn zip_2_by_2() {
        let start = vec![
            vec![
                ("bam".to_string(), Value::String("test1.bam".to_string())),
                ("bam".to_string(), Value::String("test2.bam".to_string())),
            ],
            vec![
                (
                    "bam_index".to_string(),
                    Value::String("test1.bam.bai".to_string()),
                ),
                (
                    "bam_index".to_string(),
                    Value::String("test2.bam.bai".to_string()),
                ),
            ],
        ];
        let end = zip_inputs(start);
        assert_eq!(
            end,
            vec![
                vec![
                    ("bam".to_string(), Value::String("test1.bam".to_string())),
                    (
                        "bam_index".to_string(),
                        Value::String("test1.bam.bai".to_string())
                    ),
                ],
                vec![
                    ("bam".to_string(), Value::String("test2.bam".to_string())),
                    (
                        "bam_index".to_string(),
                        Value::String("test2.bam.bai".to_string())
                    ),
                ],
            ]
        )
    }

    #[test]
    fn zip_2_by_3() {
        let start = vec![
            vec![
                (
                    "bam".to_string(),
                    Value::String("test.hg19.bam".to_string()),
                ),
                (
                    "bam".to_string(),
                    Value::String("test.GRCh38.bam".to_string()),
                ),
            ],
            vec![
                (
                    "bam_index".to_string(),
                    Value::String("test.hg19.bam.bai".to_string()),
                ),
                (
                    "bam_index".to_string(),
                    Value::String("test.GRCh38.bam.bai".to_string()),
                ),
            ],
            vec![
                (
                    "ref_fasta".to_string(),
                    Value::String("hg19.fasta".to_string()),
                ),
                (
                    "ref_fasta".to_string(),
                    Value::String("GRCh38.fasta".to_string()),
                ),
            ],
        ];
        let end = zip_inputs(start);
        assert_eq!(
            end,
            vec![
                vec![
                    (
                        "bam".to_string(),
                        Value::String("test.hg19.bam".to_string())
                    ),
                    (
                        "bam_index".to_string(),
                        Value::String("test.hg19.bam.bai".to_string())
                    ),
                    (
                        "ref_fasta".to_string(),
                        Value::String("hg19.fasta".to_string())
                    ),
                ],
                vec![
                    (
                        "bam".to_string(),
                        Value::String("test.GRCh38.bam".to_string())
                    ),
                    (
                        "bam_index".to_string(),
                        Value::String("test.GRCh38.bam.bai".to_string())
                    ),
                    (
                        "ref_fasta".to_string(),
                        Value::String("GRCh38.fasta".to_string())
                    ),
                ],
            ]
        )
    }
}
