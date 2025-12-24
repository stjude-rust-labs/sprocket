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
use nonempty::NonEmpty;
use path_clean::PathClean;
use serde_json::Value as JsonValue;
use tokio::fs::remove_dir_all;
use tokio::task::JoinSet;
use tracing::debug;
use tracing::info;
use tracing::warn;
use wdl::engine::CancellationContext;
use wdl::engine::EvaluatedTask;
use wdl::engine::EvaluationError;
use wdl::engine::Events;
use wdl::engine::Inputs as EngineInputs;
use wdl::engine::Outputs;
use wdl::engine::config::CallCachingMode;
use wdl::engine::config::FailureMode;
use wdl::engine::config::TaskResourceLimitBehavior;

use crate::analysis::Analysis;
use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::commands::run::DEFAULT_RUNS_DIR;
use crate::eval::Evaluator;
use crate::inputs::OriginPaths;
use crate::test::Assertions;
use crate::test::TestDefinition;

/// Test definitions may appear either sibling to their source WDL, or nested
/// under this directory.
const DEFINITIONS_TEST_DIR: &str = "test";
/// There may be a directory named "test" at the root of each WDL workspace.
const WORKSPACE_TEST_DIR: &str = "test";
/// Test fixtures are located at `$WORKSPACE_TEST_DIR/$FIXTURES_DIR`
const FIXTURES_DIR: &str = "fixtures";

/// Arguments for the `test` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
    /// Local path to a WDL document or workspace to unit test.
    pub source: Option<Source>,
    /// Root of the workspace where test fixtures are relative to.
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
        self.engine.task.cache = CallCachingMode::Off;
        self.engine.task.cpu_limit_behavior = TaskResourceLimitBehavior::TryWithMax;
        self.engine.task.memory_limit_behavior = TaskResourceLimitBehavior::TryWithMax;
        self
    }
}

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
        .join(DEFINITIONS_TEST_DIR)
        .join(wdl_path.file_name().expect("should have filename"));
    inner(&nested)?;
    Ok(result)
}

/// Returns `true` if the test should be filtered.
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

#[derive(Debug)]
enum RunResult {
    Workflow(Result<Outputs, EvaluationError>),
    Task(Box<Result<EvaluatedTask, EvaluationError>>),
}

#[derive(Debug)]
struct TestIteration {
    name: Arc<String>,
    iteration_num: usize,
    result: RunResult,
    assertions: Arc<Assertions>,
    run_dir: PathBuf,
}

impl TestIteration {
    pub fn evaluate(&self) -> Result<IterationResult> {
        match &self.result {
            RunResult::Workflow(result) => match result {
                Ok(_outputs) => {
                    if self.assertions.should_fail {
                        Ok(IterationResult::Fail(anyhow!(
                            "test iteration #{num} of `{name}` succeeded but was expected to \
                             fail: see `{dir}`",
                            num = self.iteration_num,
                            name = self.name,
                            dir = self.run_dir.display(),
                        )))
                    } else {
                        Ok(IterationResult::Success)
                    }
                }
                Err(_eval_err) => {
                    if self.assertions.should_fail {
                        Ok(IterationResult::Success)
                    } else {
                        Ok(IterationResult::Fail(anyhow!(
                            "test iteration #{num} of `{name}` failed but workflow was expected \
                             to succeed: see `{dir}`",
                            num = self.iteration_num,
                            name = self.name,
                            dir = self.run_dir.display(),
                        )))
                    }
                }
            },
            RunResult::Task(result) => match &**result {
                Ok(evaled_task) => {
                    if evaled_task.exit_code() == self.assertions.exit_code {
                        Ok(IterationResult::Success)
                    } else {
                        Ok(IterationResult::Fail(anyhow!(
                            "test iteration #{num} of `{name}` exited with code `{actual}` but \
                             test expected exit code `{expected}`: see `{dir}`",
                            num = self.iteration_num,
                            name = self.name,
                            actual = evaled_task.exit_code(),
                            expected = self.assertions.exit_code,
                            dir = evaled_task.work_dir(),
                        )))
                    }
                }
                Err(eval_err) => Err(anyhow!(
                    "unexpected evaluation error: {}",
                    eval_err.to_string()
                )),
            },
        }
    }
}

enum IterationResult {
    Success,
    Fail(anyhow::Error),
}

/// Performs the `test` command.
pub async fn test(args: Args) -> CommandResult<()> {
    let source = args.source.unwrap_or_default();
    let (source, workspace) = match (&source, args.workspace) {
        (Source::Remote(_), _) => {
            return Err(anyhow!("the `test` subcommand does not accept remote sources").into());
        }
        (Source::Directory(_), Some(workspace)) => (source, workspace),
        (Source::Directory(source_dir), None) => (source.clone(), source_dir.to_path_buf()),
        (Source::File(_), Some(workspace)) => (source, workspace),
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
        info!(
            "found tests for WDL `{}` in `{}`",
            wdl_path.display(),
            yaml_path.display()
        );
        documents.push((analysis, document_tests));
    }

    let test_dir = workspace.join(WORKSPACE_TEST_DIR);
    let fixture_origins = Arc::new(OriginPaths::Single(
        wdl::engine::path::EvaluationPath::Local(test_dir.join(FIXTURES_DIR)),
    ));
    let engine = Arc::new(args.engine);

    let include_tags = HashSet::from_iter(args.include_tag.into_iter());
    let filter_tags = HashSet::from_iter(args.filter_tag.into_iter());
    let mut errors = Vec::new();
    let mut all_results = Vec::new();
    for (analysis, test_definitions) in documents {
        let wdl_document = analysis.document();
        info!("testing WDL document `{}`", wdl_document.path());
        let mut document_results = Vec::new();
        for (entrypoint, definitions) in test_definitions.entrypoints {
            let entrypoint = Arc::new(entrypoint);
            let mut entrypoint_results = Vec::new();
            let is_workflow = match (
                wdl_document.task_by_name(&entrypoint),
                wdl_document.workflow(),
            ) {
                (Some(_), _) => false,
                (None, Some(wf)) if wf.name() == *entrypoint => true,
                (..) => {
                    errors.push(Arc::new(anyhow!(
                        "no entrypoint named `{}` in `{}`",
                        entrypoint,
                        wdl_document.path()
                    )));
                    continue;
                }
            };
            info!("testing entrypoint `{}`", entrypoint);
            for test in definitions {
                let test_name = Arc::new(test.name.clone());
                let assertions = Arc::new(test.assertions.clone());
                if filter_test(&test, &include_tags, &filter_tags) {
                    info!("skipping `{}` due to tag selection", test.name);
                    continue;
                }
                let matrix = match test.parse_inputs().with_context(|| {
                    format!(
                        "parsing input matrix of test `{}` in tests for WDL document `{}`",
                        test.name,
                        wdl_document.path()
                    )
                }) {
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
                let run_root = test_dir
                    .join(DEFAULT_RUNS_DIR)
                    .join(entrypoint.as_ref())
                    .join(test_name.as_ref());
                if run_root.exists() {
                    remove_dir_all(&run_root).await.with_context(|| {
                        format!("removing prior test dir: `{}`", run_root.display())
                    })?;
                }
                let mut futures = JoinSet::new();
                for (test_num, run_inputs) in matrix.cartesian_product().enumerate() {
                    let inputs = match run_inputs
                        .map(|(key, yaml_val)| match serde_json::to_value(yaml_val) {
                            Ok(json_val) => Ok((format!("{entrypoint}.{key}"), json_val)),
                            Err(e) => Err(anyhow!(e)),
                        })
                        .collect::<Result<serde_json::Map<String, JsonValue>>>()
                        .with_context(|| {
                            format!(
                                "converting YAML inputs to a JSON map for test `{}` for WDL \
                                 document `{}`",
                                test_name,
                                wdl_document.path()
                            )
                        }) {
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
                        .with_context(|| {
                            format!(
                                "converting to WDL inputs for test `{}` for WDL document `{}`",
                                test_name,
                                wdl_document.path()
                            )
                        })?;

                    let (_, wdl_inputs) = match engine_inputs {
                        Some(inputs) => inputs,
                        None => (
                            String::new(),
                            if is_workflow {
                                EngineInputs::Workflow(Default::default())
                            } else {
                                EngineInputs::Task(Default::default())
                            },
                        ),
                    };
                    let run_dir = run_root.join(test_num.to_string());
                    let events = Events::disabled();
                    let name = test_name.clone();
                    let fixtures = fixture_origins.clone();
                    let engine = engine.clone();
                    let entrypoint = entrypoint.clone();
                    let assertions = assertions.clone();
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
                        let cancellation = CancellationContext::new(FailureMode::Fast);
                        TestIteration {
                            name,
                            iteration_num: test_num,
                            result: if is_workflow {
                                RunResult::Workflow(evaluator.run(cancellation, events).await)
                            } else {
                                RunResult::Task(Box::new(
                                    evaluator.evaluate_task(cancellation, events).await,
                                ))
                            },
                            assertions,
                            run_dir,
                        }
                    });
                }
                entrypoint_results.push((test_name, futures));
            }
            document_results.push((entrypoint, entrypoint_results));
        }
        all_results.push((wdl_document.uri().path().to_string(), document_results));
    }

    for (document_name, entrypoint_results) in all_results {
        info!("evaluating document: `{document_name}`");
        for (entrypoint_name, results) in entrypoint_results {
            info!("evaluating entrypoint: `{entrypoint_name}`");
            for (test_name, mut test_results) in results {
                info!("evaluating test: `{test_name}`");
                let mut success_counter = 0;
                let mut fail_counter = 0;
                let mut err_counter = 0;

                while let Some(result) = test_results.join_next().await {
                    let test_iteration = result.with_context(|| "joining futures")?;
                    match test_iteration.evaluate() {
                        Ok(IterationResult::Success) => {
                            success_counter += 1;
                        }
                        Ok(IterationResult::Fail(e)) => {
                            fail_counter += 1;
                            errors.push(Arc::new(e));
                        }
                        Err(e) => {
                            err_counter += 1;
                            errors.push(Arc::new(e));
                        }
                    }
                }
                if err_counter > 0 {
                    println!(
                        "☠️ `{test_name}` had errors: {err_counter} errors out of {total} \
                         executions",
                        total = err_counter + fail_counter + success_counter
                    );
                } else if fail_counter > 0 {
                    println!(
                        "❌ `{test_name}` failed: {fail_counter} executions failed assertions \
                         (out of {total} executions)",
                        total = fail_counter + success_counter
                    )
                } else {
                    println!("✅ `{test_name}` success! ({success_counter} successful executions)");
                }
            }
        }
    }

    if let Some(errors) = NonEmpty::from_vec(errors) {
        return Err(CommandError::from(errors));
    };

    Ok(())
}
