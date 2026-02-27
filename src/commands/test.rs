//! Implementation of the `test` subcommand.

use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::read;
use std::fs::read_to_string;
use std::fs::remove_dir;
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
use regex::Regex;
use serde_json::Value as JsonValue;
use tokio::fs::remove_dir_all;
use tokio::task::JoinSet;
use tracing::debug;
use tracing::info;
use tracing::level_filters::LevelFilter;
use wdl::analysis::AnalysisResult;
use wdl::engine::CancellationContext;
use wdl::engine::EvaluatedTask;
use wdl::engine::EvaluationError;
use wdl::engine::EvaluationPath;
use wdl::engine::Events;
use wdl::engine::Inputs as EngineInputs;
use wdl::engine::Outputs;
use wdl::engine::config::CallCachingMode;
use wdl::engine::config::FailureMode;
use wdl::engine::config::TaskResourceLimitBehavior;

use crate::Config;
use crate::FilterReloadHandle;
use crate::analysis::Analysis;
use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::eval::Evaluator;
use crate::system::v1::fs::RUNS_DIR;
use crate::test::DocumentTests;
use crate::test::OutputAssertion;
use crate::test::ParsedAssertions;
use crate::test::TestDefinition;

/// Test definitions may appear either sibling to their source WDL, or nested
/// under this directory.
const DEFINITIONS_TEST_DIR: &str = "test";
/// Directory which is located at the root of a WDL workspace.
///
/// At a minimum, this directory will contain a `runs/` directory where tests
/// are executed.
const WORKSPACE_TEST_DIR: &str = "test";
/// Test fixtures are located at `$WORKSPACE_TEST_DIR/$FIXTURES_DIR`
const FIXTURES_DIR: &str = "fixtures";

/// Arguments for the `test` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
    /// Local path to a WDL document or workspace to unit test.
    ///
    /// If not specified, this defaults to the current working directory.
    pub source: Option<Source>,
    /// Root of the workspace where the `test/` directory will be located. Test
    /// fixtures will be loaded from `<workspace>/test/fixtures/` if it is
    /// present.
    ///
    /// If a `<workspace>/test/` directory does not exist, one will be created
    /// and it will contain a `runs/` directory for test executions.
    ///
    /// If not specified and the `source` argument is a directory, it's assumed
    /// that directory is also the workspace. This can be specified in addition
    /// to a source directory if they are different.
    ///
    /// If not specified and the `source` argument is a file, it's assumed that
    /// the current working directory is the workspace. This can be specified in
    /// addition to a source file if the CWD is not the right workspace.
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
    /// Do not clean the file system of successful tests.
    ///
    /// The default behavior is to remove directories of successful tests,
    /// leaving only failed and errored run directories on the file system.
    #[clap(long, conflicts_with = "clean_all")]
    pub no_clean: bool,
    /// Clean all exectuion directories, even for tests that failed or errored.
    #[clap(long)]
    pub clean_all: bool,
    /// The number of test executions to run in parallel.
    #[clap(short, long)]
    pub parallelism: Option<usize>,
    /// Do not print results as tests complete.
    #[clap(long)]
    pub no_status: bool,
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

/// Checks that the contents of the given file path match every given regular
/// expression.
///
/// Returns `Ok(None)` if the file's contents match every regular expression.
/// Returns `Ok(Some(regex))` upon the first unmatched regular expression.
fn file_matches<'a>(path: &str, regexs: &'a [Regex]) -> Result<Option<&'a str>> {
    let contents = read_to_string(path).with_context(|| format!("failed to read file `{path}`"))?;
    for re in regexs {
        if !re.is_match(&contents) {
            return Ok(Some(re.as_str()));
        }
    }
    Ok(None)
}

fn evaluate_outputs(
    assertions: &HashMap<String, Vec<OutputAssertion>>,
    outputs: &wdl::engine::Outputs,
) -> Result<()> {
    for (name, fns) in assertions {
        let output = outputs
            .get(name)
            .expect("output should have been validated");
        for func in fns {
            func.evaluate(output)
                .with_context(|| format!("evaluating WDL output with name `{name}`"))?
        }
    }
    Ok(())
}

#[derive(Debug)]
enum RunResult {
    Workflow(Result<Outputs, EvaluationError>),
    Task(Box<Result<EvaluatedTask, EvaluationError>>),
}

#[derive(Debug)]
struct TestIdentifier {
    doc_name: Arc<String>,
    target: Arc<String>,
    test_name: Arc<String>,
    iteration_num: usize,
}

#[derive(Debug)]
struct TestIteration {
    id: TestIdentifier,
    result: RunResult,
    assertions: Arc<ParsedAssertions>,
    run_dir: PathBuf,
}

impl TestIteration {
    pub async fn evaluate(self, clean: bool, quiet: bool) -> Result<IterationResult> {
        let id = format!(
            "{doc}::{target}::{test} (iteration #{num})",
            doc = self.id.doc_name,
            target = self.id.target,
            test = self.id.test_name,
            num = self.id.iteration_num,
        );
        let inner = async || match self.result {
            RunResult::Workflow(result) => match result {
                Ok(outputs) => {
                    if self.assertions.should_fail {
                        Ok(IterationResult::Fail(anyhow!(
                            "{id} succeeded but was expected to fail: see `{dir}`",
                            dir = self.run_dir.display(),
                        )))
                    } else if let Err(e) = evaluate_outputs(&self.assertions.outputs, &outputs)
                        .with_context(|| {
                            format!(
                                "{id} failed output assertions: see `{dir}`",
                                dir = self.run_dir.display()
                            )
                        })
                    {
                        Ok(IterationResult::Fail(e))
                    } else {
                        Ok(IterationResult::Success)
                    }
                }
                Err(eval_err) => {
                    if self.assertions.should_fail {
                        Ok(IterationResult::Success)
                    } else {
                        Ok(IterationResult::Fail(anyhow!(
                            "{id} failed but was expected to succeed: see `{dir}`: {err}",
                            dir = self.run_dir.display(),
                            err = eval_err.to_string(),
                        )))
                    }
                }
            },
            RunResult::Task(result) => match *result {
                Ok(evaled_task) => {
                    if evaled_task.exit_code() == self.assertions.exit_code {
                        if let Some(regexes) = &self.assertions.stdout {
                            let stdout_path = evaled_task
                                .stdout()
                                .as_file()
                                .expect("stdout should be `File`");
                            match file_matches(stdout_path.as_str(), regexes.as_slice()) {
                                Ok(None) => {}
                                Ok(Some(re)) => {
                                    return Ok(IterationResult::Fail(anyhow!(
                                        "{id} stdout did not contain `{re}`: see `{dir}`",
                                        dir = self.run_dir.display(),
                                    )));
                                }
                                Err(e) => return Err(e),
                            }
                        }
                        if let Some(regexes) = &self.assertions.stderr {
                            let stderr_path = evaled_task
                                .stderr()
                                .as_file()
                                .expect("stderr should be `File`");
                            match file_matches(stderr_path.as_str(), regexes.as_slice()) {
                                Ok(None) => {}
                                Ok(Some(re)) => {
                                    return Ok(IterationResult::Fail(anyhow!(
                                        "{id} stderr did not contain `{re}`: see `{dir}`",
                                        dir = self.run_dir.display(),
                                    )));
                                }
                                Err(e) => return Err(e),
                            }
                        }
                        let res = evaled_task.into_outputs();
                        let outputs = match res {
                            Ok(outputs) => outputs,
                            Err(eval_err) => {
                                if self.assertions.exit_code == 0 {
                                    return Err(anyhow!(
                                        "unexpected evaluation error: {}",
                                        eval_err.to_string()
                                    ));
                                }
                                return Ok(IterationResult::Success);
                            }
                        };
                        if let Err(e) = evaluate_outputs(&self.assertions.outputs, &outputs)
                            .with_context(|| {
                                format!(
                                    "{id} failed output assertions: see `{dir}`",
                                    dir = self.run_dir.display()
                                )
                            })
                        {
                            Ok(IterationResult::Fail(e))
                        } else {
                            Ok(IterationResult::Success)
                        }
                    } else {
                        Ok(IterationResult::Fail(anyhow!(
                            "{id} exited with code `{actual}` but test expected exit code \
                             `{expected}`: see `{dir}`",
                            actual = evaled_task.exit_code(),
                            expected = self.assertions.exit_code,
                            dir = self.run_dir.display(),
                        )))
                    }
                }
                Err(eval_err) => Err(anyhow!(
                    "unexpected evaluation error: {}",
                    eval_err.to_string()
                )),
            },
        };
        let result = inner().await;
        if !quiet {
            match &result {
                Ok(IterationResult::Success) => {
                    println!("{id}: ✅")
                }
                Ok(IterationResult::Fail(_)) => {
                    println!("{id}: ❌")
                }
                Err(_) => {
                    println!("{id}: ☠️")
                }
            }
        }
        if clean && matches!(result, Ok(IterationResult::Success)) {
            let _ = remove_dir_all(self.run_dir).await;
        }
        result
    }
}

enum IterationResult {
    Success,
    Fail(anyhow::Error),
}

type TestResults = Vec<Result<IterationResult>>;
type TargetResults = IndexMap<String, TestResults>;
type DocumentResults = IndexMap<String, TargetResults>;

struct Runner {
    root: PathBuf,
    fixtures: Arc<EvaluationPath>,
    engine_config: Arc<wdl::engine::Config>,
    log_handle: FilterReloadHandle,
    permits: usize,
}

impl Runner {
    async fn run(
        &self,
        documents: Vec<(&AnalysisResult, DocumentTests)>,
        should_filter: impl Fn(&TestDefinition) -> bool,
        clean: bool,
        quiet: bool,
        errors: &mut Vec<Arc<anyhow::Error>>,
    ) -> Result<IndexMap<String, DocumentResults>> {
        let current_filter = self.log_handle.clone_current().expect("should have filter");
        self.log_handle
            .reload(LevelFilter::OFF)
            .expect("should reload");

        let mut permits = self.permits;
        let mut futures = JoinSet::new();
        let mut all_results = IndexMap::new();
        for (analysis, tests) in documents {
            let wdl_document = analysis.document();
            let doc_name = Path::new(&wdl_document.path().to_string())
                .with_extension("")
                .file_name()
                .expect("basename")
                .to_string_lossy()
                .to_string();
            let doc_name = Arc::new(doc_name);
            let mut document_results = IndexMap::new();
            for (target, definitions) in tests.targets {
                let target = Arc::new(target);
                let (is_workflow, outputs) =
                    match (wdl_document.task_by_name(&target), wdl_document.workflow()) {
                        (Some(task), _) => (false, task.outputs()),
                        (None, Some(wf)) if wf.name() == *target => (true, wf.outputs()),
                        (..) => {
                            errors.push(Arc::new(anyhow!(
                                "no target named `{target}` in `{path}`",
                                path = wdl_document.path()
                            )));
                            continue;
                        }
                    };
                let mut target_results = IndexMap::new();
                for test in definitions {
                    if should_filter(&test) {
                        continue;
                    }
                    let matrix = match test.parse_inputs().with_context(|| {
                        format!(
                            "parsing input matrix of test `{name}` for WDL document `{path}`",
                            name = test.name,
                            path = wdl_document.path()
                        )
                    }) {
                        Ok(res) => res,
                        Err(e) => {
                            errors.push(Arc::new(e));
                            continue;
                        }
                    };
                    let run_root = self.root.join(target.as_ref()).join(&test.name);
                    if run_root.exists() {
                        remove_dir_all(&run_root).await.with_context(|| {
                            format!("removing prior test dir: `{}`", run_root.display())
                        })?;
                    }
                    let test_name = Arc::new(test.name.clone());
                    let assertions =
                        match test
                            .assertions
                            .parse(is_workflow, outputs)
                            .with_context(|| {
                                format!(
                                    "parsing assertions of test `{name}` for WDL document `{path}`",
                                    name = test.name,
                                    path = wdl_document.path()
                                )
                            }) {
                            Ok(res) => Arc::new(res),
                            Err(e) => {
                                errors.push(Arc::new(e));
                                continue;
                            }
                        };
                    let mut test_iterations = Vec::new();
                    for (test_num, run_inputs) in matrix.cartesian_product().enumerate() {
                        let test_num = test_num + 1; // start count at 1
                        let inputs = match run_inputs
                            .map(|(key, yaml_val)| match serde_json::to_value(yaml_val) {
                                Ok(json_val) => Ok((format!("{target}.{key}"), json_val)),
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
                                continue;
                            }
                        };

                        let engine_inputs =
                            match EngineInputs::parse_json_object(wdl_document, inputs)
                                .with_context(|| {
                                    format!(
                                        "converting to WDL inputs for test `{}` for WDL document \
                                         `{}`",
                                        test_name,
                                        wdl_document.path()
                                    )
                                }) {
                                Ok(res) => res,
                                Err(e) => {
                                    errors.push(Arc::new(e));
                                    continue;
                                }
                            };

                        let wdl_inputs = match engine_inputs {
                            Some((_, inputs)) => inputs,
                            None => {
                                if is_workflow {
                                    EngineInputs::Workflow(Default::default())
                                } else {
                                    EngineInputs::Task(Default::default())
                                }
                            }
                        };
                        let id = TestIdentifier {
                            doc_name: doc_name.clone(),
                            test_name: test_name.clone(),
                            target: target.clone(),
                            iteration_num: test_num,
                        };
                        let assertions = assertions.clone();
                        let document = wdl_document.clone();
                        if permits > 0 {
                            permits -= 1;
                            self.spawn_future(
                                &mut futures,
                                &run_root,
                                id,
                                assertions,
                                document,
                                wdl_inputs,
                            )
                            .await;
                        } else {
                            let result = futures
                                .join_next()
                                .await
                                .expect("futures should not be exhausted");
                            let prior_test_iteration = result.with_context(|| "joining futures")?;
                            all_results
                                .get_mut(prior_test_iteration.id.doc_name.as_str())
                                .unwrap_or(&mut document_results)
                                .get_mut(prior_test_iteration.id.target.as_str())
                                .unwrap_or(&mut target_results)
                                .get_mut(prior_test_iteration.id.test_name.as_str())
                                .unwrap_or(&mut test_iterations)
                                .push(prior_test_iteration.evaluate(clean, quiet).await);
                            self.spawn_future(
                                &mut futures,
                                &run_root,
                                id,
                                assertions,
                                document,
                                wdl_inputs,
                            )
                            .await;
                        }
                    }
                    target_results.insert(test_name.to_string(), test_iterations);
                }
                document_results.insert(target.to_string(), target_results);
            }
            all_results.insert(doc_name.to_string(), document_results);
        }
        while let Some(result) = futures.join_next().await {
            let test_iteration = result.with_context(|| "joining futures")?;
            all_results
                .get_mut(test_iteration.id.doc_name.as_str())
                .unwrap()
                .get_mut(test_iteration.id.target.as_str())
                .unwrap()
                .get_mut(test_iteration.id.test_name.as_str())
                .unwrap()
                .push(test_iteration.evaluate(clean, quiet).await);
        }

        self.log_handle
            .reload(current_filter)
            .expect("should reload");
        Ok(all_results)
    }

    async fn spawn_future(
        &self,
        futures: &mut JoinSet<TestIteration>,
        root: &Path,
        id: TestIdentifier,
        assertions: Arc<ParsedAssertions>,
        document: wdl::analysis::Document,
        inputs: EngineInputs,
    ) {
        let is_workflow = matches!(inputs, EngineInputs::Workflow(_));
        let fixtures = self.fixtures.clone();
        let engine = self.engine_config.clone();
        let run_dir = root.join(id.iteration_num.to_string());
        let events = Events::disabled();
        let target = id.target.clone();
        futures.spawn(async move {
            let evaluator = Evaluator::new(&document, &target, inputs, &fixtures, engine, &run_dir);
            let cancellation = CancellationContext::new(FailureMode::Fast);
            TestIteration {
                id,
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
}

async fn summarize_results(
    results: IndexMap<String, DocumentResults>,
    root: &Path,
    clean: bool,
    errors: &mut Vec<Arc<anyhow::Error>>,
) {
    println!("Sprocket test result summary:");

    let mut any_results = false;
    for (document_name, target_results) in results {
        for (target_name, results) in target_results {
            let target_dir = root.join(&target_name);
            for (test_name, test_results) in results {
                let mut success_counter = 0usize;
                let mut fail_counter = 0usize;
                let mut err_counter = 0usize;

                for result in test_results {
                    any_results = true;
                    match result {
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
                if clean && err_counter == 0 && fail_counter == 0 {
                    let test_dir = target_dir.join(&test_name);
                    let _ = remove_dir_all(&test_dir).await;
                }
                let id = format!("{document_name}::{target_name}::{test_name}");
                if err_counter > 0 {
                    let total = err_counter + fail_counter + success_counter;
                    println!(
                        "☠️ `{id}` had errors: {err_counter} execution{err_plural} errored (out \
                         of {total} test execution{total_plural})",
                        err_plural = if err_counter > 1 { "s" } else { "" },
                        total_plural = if total > 1 { "s" } else { "" },
                    );
                } else if fail_counter > 0 {
                    let total = fail_counter + success_counter;
                    println!(
                        "❌ `{id}` failed: {fail_counter} execution{fail_plural} failed \
                         assertions (out of {total} execution{total_plural})",
                        fail_plural = if fail_counter > 1 { "s" } else { "" },
                        total_plural = if total > 1 { "s" } else { "" },
                    )
                } else {
                    println!(
                        "✅ `{id}` success! ({success_counter} successful test execution{plural})",
                        plural = if success_counter > 1 { "s" } else { "" }
                    );
                }
            }
            // If the target directory is empty, remove it; otherwise leave it.
            let _ = remove_dir(root.join(&target_name));
        }
    }
    if !any_results {
        println!("☠️ no tests executed ☠️")
    }
}

/// Performs the `test` command.
pub async fn test(args: Args, config: Config, handle: FilterReloadHandle) -> CommandResult<()> {
    let source = args.source.unwrap_or_default();
    let parallelism = args.parallelism.unwrap_or(config.test.parallelism);
    let (source, workspace) = match (&source, args.workspace) {
        (Source::Url(_), _) => {
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
        .fallback_version(config.common.wdl.fallback_version)
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
        let wdl_path = PathBuf::from(Into::<String>::into(document.path()));
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
    let fixture_origins = EvaluationPath::from(test_dir.join(FIXTURES_DIR).as_path());
    let engine = {
        let mut engine = config.run.engine;
        engine.task.cache = CallCachingMode::Off;
        engine.task.cpu_limit_behavior = TaskResourceLimitBehavior::TryWithMax;
        engine.task.memory_limit_behavior = TaskResourceLimitBehavior::TryWithMax;
        engine
    };

    let runner = Runner {
        root: test_dir.join(RUNS_DIR),
        fixtures: fixture_origins.into(),
        engine_config: engine.into(),
        log_handle: handle,
        permits: parallelism,
    };

    let include_tags = HashSet::from_iter(args.include_tag.into_iter());
    let filter_tags = HashSet::from_iter(args.filter_tag.into_iter());
    let should_filter = |test: &TestDefinition| filter_test(test, &include_tags, &filter_tags);
    let mut errors = Vec::new();
    let results = runner
        .run(
            documents,
            should_filter,
            !args.no_clean,
            args.no_status,
            &mut errors,
        )
        .await?;

    summarize_results(results, &runner.root, !args.no_clean, &mut errors).await;

    if args.clean_all {
        remove_dir_all(runner.root)
            .await
            .context("cleaning the file system of all test executions")?;
    }

    if let Some(errors) = NonEmpty::from_vec(errors) {
        return Err(CommandError::from(errors));
    };

    Ok(())
}
