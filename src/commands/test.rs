//! Implementation of the `test` subcommand.

use std::collections::HashSet;
use std::fs::read;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use clap::Parser;
use indexmap::IndexMap;
use nonempty::NonEmpty;
use path_clean::PathClean;
use serde_json::Value as JsonValue;
use tokio::fs::remove_dir_all;
use tokio::task::JoinSet;
use tracing::debug;
use tracing::info;
use tracing::warn;
use wdl::engine::CancellationContext;
use wdl::engine::EvaluationError;
use wdl::engine::Events;
use wdl::engine::FailedTask;
use wdl::engine::Inputs as EngineInputs;
use wdl::engine::Outputs;

use crate::analysis::Analysis;
use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::commands::run::EVENTS_CHANNEL_CAPACITY;
use crate::eval::Evaluator;
use crate::inputs::OriginPaths;
use crate::test::Assertions;
use crate::test::TestDefinition;

const TEST_DIR: &str = "test";
const FIXTURES_DIR: &str = "fixtures";
const RUNS_DIR: &str = "runs";

/// Arguments for the `test` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
    /// Local path to a WDL document or workspace to unit test.
    pub source: Option<Source>,
    /// Root workspace where test fixtures are relative to.
    #[clap(short, long)]
    pub workspace: Option<PathBuf>,
    /// Specific test tag that should be run.
    ///
    /// Can be repeated multiple times.
    #[clap(short='t', long, value_name = "TAG",
        action = clap::ArgAction::Append,
        num_args = 1,
        conflicts_with="filter_tag",
    )]
    pub include_tag: Vec<String>,
    /// Filter out any tests with a matching tag.
    ///
    /// Can be repeated multiple times.
    #[clap(short, long, value_name = "TAG",
        action = clap::ArgAction::Append,
        num_args = 1,
    )]
    pub filter_tag: Vec<String>,
    /// The engine configuration to use.
    ///
    /// This is not exposed via [`clap`] and is not settable by users.
    /// It will always be overwritten by the engine config provided by the user
    /// (which will be set with `Default::default()` if the user does not
    /// explicitly set `run` config values).
    #[clap(skip)]
    pub engine: wdl::engine::config::Config,
}

impl Args {
    pub fn apply(mut self, config: crate::config::Config) -> Self {
        self.engine = config.run.engine;
        self
    }
}

const NESTED_TEST_DIR_NAME: &str = "test";

fn find_yaml(wdl_path: &Path) -> Result<Option<PathBuf>> {
    let mut result: Option<PathBuf> = None;
    let mut inner = |path: &Path| {
        for ext in ["yaml", "yml"] {
            let yaml = path.with_extension(ext);
            match (yaml.exists(), &result) {
                (true, Some(previous)) => bail!(
                    "test file `{path}` conflicts with test file `{previous}`",
                    path = yaml.display(),
                    previous = previous.display()
                ),
                (true, None) => {
                    result = Some(yaml);
                }
                _ => {}
            }
        }

        Ok(())
    };
    inner(wdl_path)?;

    let parent = wdl_path.parent().expect("should have parent");
    let nested = parent
        .join(NESTED_TEST_DIR_NAME)
        .join(wdl_path.file_name().expect("should have filename"));
    inner(&nested)?;
    Ok(result)
}

fn filter_test(
    test: &TestDefinition,
    include_tags: &HashSet<String>,
    filter_tags: &HashSet<String>,
) -> bool {
    if !include_tags.is_empty() && !test.tags.iter().any(|t| include_tags.contains(t)) {
        return true;
    }
    if test.tags.iter().any(|t| filter_tags.contains(t)) {
        return true;
    }
    false
}

fn evaluate_test_result(
    assertions: &Assertions,
    result: std::result::Result<Outputs, EvaluationError>,
) -> Result<(bool, Option<FailedTask>)> {
    match result {
        Ok(_outputs) => {
            if assertions.exit_code == 0 && !assertions.should_fail {
                Ok((true, None))
            } else {
                Ok((false, None))
            }
        }
        Err(eval_err) => match eval_err {
            EvaluationError::Canceled => Err(anyhow!("test was cancelled")),
            EvaluationError::FailedTask(task_err) => {
                let task = task_err.failed_task;
                if task.exit_code == assertions.exit_code {
                    Ok((true, None))
                } else {
                    Ok((false, Some(task)))
                }
            }
            EvaluationError::Source(source_err) => Err(anyhow!("source error!: {:#?}", source_err)),
            EvaluationError::Other(other_err) => Err(anyhow!("other error: {:#?}", other_err)),
        },
    }
}

/// Performs the `test` command.
pub async fn test(args: Args) -> CommandResult<()> {
    let source = args.source.unwrap_or_default();
    let (source, workspace) = match (&source, args.workspace) {
        (Source::Remote(_), _) => {
            return Err(anyhow!("the `test` subcommand does not accept remote sources").into());
        }
        (Source::Directory(_), Some(_)) => {
            return Err(anyhow!("arg conflict").into());
        }
        (Source::Directory(source_dir), None) => (source.clone(), source_dir.to_path_buf()),
        (Source::File(_), Some(supplied_dir)) => (source, supplied_dir),
        (Source::File(_), None) => (
            source,
            std::env::current_dir().context("failed to get current directory")?,
        ),
    };
    let workspace = absolute(&workspace)
        .with_context(|| {
            format!(
                "resolving absolute path to workspace: `{}`",
                workspace.display()
            )
        })?
        .clean();

    let analysis_results = Analysis::default()
        .add_source(source.clone())
        .run()
        .await
        .map_err(CommandError::from)?;

    // Find and parse all YAML before beginning any executions.
    // This is so that any totally invalid YAML is caught up-front before we start
    // testing. Smaller issues with test definitions will later be collected and
    // reported on after all tests execute.
    let mut documents = Vec::new();
    for analysis in analysis_results.filter(&[&source]) {
        let document = analysis.document();
        let wdl_path = PathBuf::from(document.path().as_ref());
        let yaml_path = match find_yaml(&wdl_path)? {
            Some(p) => p,
            None => {
                debug!(
                    "no test YAML found for WDL document `{}`",
                    wdl_path.display()
                );
                continue;
            }
        };
        let document_tests: crate::test::DocumentTests = serde_yaml_ng::from_slice(
            &read(&yaml_path)
                .with_context(|| format!("reading file: `{}`", yaml_path.display()))?,
        )
        .with_context(|| format!("parsing YAML: `{}`", yaml_path.display()))?;
        documents.push((analysis, document_tests));
        info!(
            "found tests for WDL `{}` in `{}`",
            wdl_path.display(),
            yaml_path.display()
        )
    }

    let test_dir = workspace.join(TEST_DIR);
    let fixture_origins = OriginPaths::Single(wdl::engine::path::EvaluationPath::Local(
        test_dir.join(FIXTURES_DIR),
    ));
    let events = Events::new(EVENTS_CHANNEL_CAPACITY);

    let include_tags = HashSet::from_iter(args.include_tag.into_iter());
    let filter_tags = HashSet::from_iter(args.filter_tag.into_iter());
    let mut errors = Vec::new();
    let mut all_results = IndexMap::new();
    for (analysis, test_definitions) in documents {
        let wdl_document = analysis.document();
        info!("testing WDL document `{}`", wdl_document.path());
        let mut document_results = IndexMap::new();
        for (entrypoint, tests) in test_definitions.entrypoints.iter() {
            let mut entrypoint_results = IndexMap::new();
            let found_entrypoint = match (
                wdl_document.task_by_name(entrypoint),
                wdl_document.workflow(),
            ) {
                (Some(_), _) => true,
                (None, Some(wf)) if wf.name() == entrypoint => true,
                (..) => false,
            };
            if !found_entrypoint {
                errors.push(Arc::new(anyhow!(
                    "no entrypoint named `{}` in `{}`",
                    entrypoint,
                    wdl_document.path()
                )));
                continue;
            }
            info!("testing entrypoint `{}`", entrypoint);
            for test in tests {
                if filter_test(test, &include_tags, &filter_tags) {
                    info!("skipping `{}`", test.name);
                    continue;
                }
                let matrix = match test
                    .parse_inputs()
                    .with_context(|| format!("parsing input matrix of `{}`", test.name))
                {
                    Ok(res) => res,
                    Err(e) => {
                        errors.push(Arc::new(e));
                        warn!(
                            "skipping test `{}` due to problem with input matrix",
                            test.name
                        );
                        continue;
                    }
                };
                info!("running `{}`", test.name);
                let run_root = test_dir.join(RUNS_DIR).join(entrypoint).join(&test.name);
                if run_root.exists() {
                    remove_dir_all(&run_root)
                        .await
                        .with_context(|| "removing prior run dir")?;
                }
                let mut futures = JoinSet::new();
                for (test_num, run_inputs) in matrix.cartesian_product().enumerate() {
                    let inputs = match run_inputs
                        .map(|(key, yaml_val)| match serde_json::to_value(yaml_val) {
                            Ok(json_val) => Ok((format!("{entrypoint}.{key}"), json_val)),
                            Err(e) => Err(anyhow!(e)),
                        })
                        .collect::<Result<serde_json::Map<String, JsonValue>>>()
                        .with_context(|| "converting YAML inputs to a JSON map")
                    {
                        Ok(res) => res,
                        Err(e) => {
                            errors.push(Arc::new(e));
                            warn!(
                                "skipping test `{}` due to problem with input matrix",
                                test.name
                            );
                            continue;
                        }
                    };

                    let engine_inputs = EngineInputs::parse_object(wdl_document, inputs)
                        .with_context(|| "converting to WDL inputs")?;

                    let Some((_derived_ep, wdl_inputs)) = engine_inputs else {
                        todo!("handle empty inputs");
                    };
                    let run_dir = run_root.join(test_num.to_string());
                    let fixtures = fixture_origins.clone();
                    let engine = args.engine.clone();
                    let events = events.clone();
                    let entrypoint = entrypoint.to_string();
                    let assertions = test.assertions.clone();
                    let document = wdl_document.clone();
                    futures.spawn(async move {
                        let evaluator = Evaluator::new(
                            &document,
                            &entrypoint,
                            wdl_inputs,
                            &fixtures,
                            engine,
                            &run_dir,
                        );
                        let cancellation = CancellationContext::new(args.engine.failure_mode);
                        (
                            test_num,
                            evaluator.run(cancellation, &events).await,
                            assertions,
                        )
                    });
                }
                entrypoint_results.insert(test.name.clone(), futures);
            }
            document_results.insert(entrypoint.clone(), entrypoint_results);
        }
        all_results.insert(wdl_document.uri().path().to_string(), document_results);
    }

    for (doc, ep_results) in all_results {
        println!("evaluating document: `{doc}`");
        for (ep, results) in ep_results {
            println!("evaluating entrypoint: `{ep}`");
            for (test, mut test_results) in results {
                println!("evaluating test: `{test}`");
                let mut success_counter = 0;
                let mut fail_counter = 0;
                let mut err_counter = 0;

                while let Some(res) = test_results.join_next().await {
                    let (test_num, result, assertions) = res.expect("error joining");
                    let result = evaluate_test_result(&assertions, result);
                    match result {
                        Ok((true, _)) => {
                            success_counter += 1;
                        }
                        Ok((false, task)) => {
                            fail_counter += 1;
                            if let Some(task) = task {
                                errors.push(Arc::new(anyhow!(
                                    "test iteration #{test_num} of `{test}` failed with exit code \
                                     `{}` but expected exit code `{}`",
                                    task.exit_code,
                                    assertions.exit_code
                                )));
                            } else {
                                errors.push(Arc::new(anyhow!(
                                    "test iteration #{test_num} of `{test}` was expected to fail \
                                     but it succeeded"
                                )));
                            }
                        }
                        Err(e) => {
                            errors.push(Arc::new(e));
                            err_counter += 1;
                        }
                    }
                }
                if err_counter > 0 {
                    println!(
                        "☠️ `{test}` had errors: {err_counter} errors out of {total} executions",
                        total = err_counter + fail_counter + success_counter
                    );
                } else if fail_counter > 0 {
                    println!(
                        "❌ `{test}` failed: {fail_counter} executions failed assertions (out of \
                         {total} executions)",
                        total = fail_counter + success_counter
                    )
                } else {
                    println!("✅ `{test}` success! ({success_counter} successful executions)");
                }
            }
        }
    }

    if let Some(errors) = NonEmpty::from_vec(errors) {
        return Err(CommandError::from(errors));
    };

    Ok(())
}
