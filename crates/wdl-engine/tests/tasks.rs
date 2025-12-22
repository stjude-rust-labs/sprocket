//! The WDL task file tests.
//!
//! This test looks for directories in `tests/tasks`.
//!
//! Each directory is expected to contain:
//!
//! * `source.wdl` - the test input source to evaluate; the file is expected to
//!   contain no static analysis errors, but may fail at evaluation time.
//! * `error.txt` - the expected evaluation error, if any.
//! * `inputs.json` - the inputs to the task.
//! * `outputs.json` - the expected outputs from the task, if the task runs
//!   successfully.
//! * `stdout` - the expected stdout from the task.
//! * `stderr` - the expected stderr from the task.
//! * `files` - a directory containing any expected files written by the task.
//!
//! The expected files may be automatically generated or updated by setting the
//! `BLESS` environment variable when running this test.

use std::borrow::Cow;
use std::env;
use std::fs;
use std::path::Path;
use std::path::absolute;
use std::sync::LazyLock;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use common::TestConfig;
use common::compare_result;
use common::find_tests;
use common::strip_paths;
use futures::FutureExt;
use futures::future::BoxFuture;
use regex::Regex;
use serde_json::to_string_pretty;
use tempfile::TempDir;
use tracing::debug;
use tracing::info;
use tracing::level_filters::LevelFilter;
use walkdir::WalkDir;
use wdl_analysis::Analyzer;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_engine::EvaluatedTask;
use wdl_engine::EvaluationError;
use wdl_engine::Events;
use wdl_engine::Inputs;
use wdl_engine::path::EvaluationPath;
use wdl_engine::v1::TopLevelEvaluator;

mod common;

/// Regex used to remove both host and guest path prefixes.
static PATH_PREFIX_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(attempts[\/\\]\d+[\/\\]|\/mnt\/task\/inputs\/\d+\/)"#).expect("invalid regex")
});

/// Regex used to replace temporary file names in task command files with
/// consistent names for test baselines.
static TEMP_FILENAME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(tmp[\/\\])?tmp[[:alnum:]]{6}"#).expect("invalid regex"));

/// Runs a single test.
fn run_test(test: &Path, config: TestConfig) -> BoxFuture<'_, Result<()>> {
    async move {
        debug!(test = %test.display(), ?config, "running test");

        let analyzer = Analyzer::new(config.analysis, |(), _, _, _| async {});
        analyzer
            .add_directory(test)
            .await
            .context("adding directory")?;

        let results = analyzer.analyze(()).await.context("running analysis")?;

        // Find the root source.wdl to evaluate
        let source_path = test.join("source.wdl");
        let Some(result) = results
            .iter()
            .find(|r| Some(r.document().path().as_ref()) == source_path.to_str())
        else {
            bail!("`source.wdl` was not found in the analysis results");
        };
        if let Some(e) = result.error() {
            bail!("parsing failed: {e:#}");
        }
        if result.document().has_errors() {
            let errors: Vec<_> = result
                .document()
                .diagnostics()
                .filter(|d| d.severity() == Severity::Error)
                .collect();
            bail!(
                "test WDL contains {} error(s):\n{}",
                errors.len(),
                errors
                    .iter()
                    .map(|d| format!("  - {:?}", d))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }

        let path = result.document().path();
        let diagnostics = match result.error() {
            Some(e) => vec![Diagnostic::error(format!("failed to read `{path}`: {e:#}"))],
            None => result.document().diagnostics().cloned().collect(),
        };

        if let Some(diagnostic) = diagnostics.iter().find(|d| d.severity() == Severity::Error) {
            bail!(EvaluationError::new(result.document().clone(), diagnostic.clone()).to_string());
        }

        let (name, mut inputs) = match Inputs::parse(result.document(), test.join("inputs.json"))? {
            Some((name, Inputs::Task(inputs))) => (name, inputs),
            Some((_, Inputs::Workflow(_))) => {
                bail!("`inputs.json` contains inputs for a workflow, not a task")
            }
            None => {
                let mut iter = result.document().tasks();
                let name = iter
                    .next()
                    .context("inputs file is empty and the WDL document contains no tasks")?
                    .name()
                    .to_string();
                if iter.next().is_some() {
                    bail!("inputs file is empty and the WDL document contains more than one task");
                }

                (name, Default::default())
            }
        };

        let test_dir = absolute(test).expect("failed to get absolute directory");
        let test_dir_path = EvaluationPath::Local(test_dir.clone());

        // Make any paths specified in the inputs file relative to the test directory
        let task = result
            .document()
            .task_by_name(&name)
            .ok_or_else(|| anyhow!("document does not contain a task named `{name}`"))?;
        inputs.join_paths(task, |_| Ok(&test_dir_path)).await?;

        let mut dir = TempDir::new_in(env!("CARGO_TARGET_TMPDIR"))
            .context("failed to create temporary directory")?;
        if env::var_os("SPROCKET_TEST_KEEP_TMPDIRS").is_some() {
            dir.disable_cleanup(true);
            info!(dir = %dir.path().display(), "test temp dir created (will be kept)");
        } else {
            info!(dir = %dir.path().display(), "test temp dir created");
        }

        let evaluator = TopLevelEvaluator::new(
            dir.path(),
            config.engine,
            Default::default(),
            Events::disabled(),
        )
        .await?;
        match evaluator
            .evaluate_task(result.document(), task, &inputs, dir.path())
            .await
        {
            Ok(evaluated) => {
                compare_evaluation_results(&test_dir, dir.path(), &evaluated)?;

                match evaluated.into_outputs() {
                    Ok(outputs) => {
                        let outputs = outputs.with_name(name.clone());
                        let outputs =
                            to_string_pretty(&outputs).context("failed to serialize outputs")?;
                        let outputs = strip_paths(dir.path(), &outputs);
                        compare_result(&test.join("outputs.json"), &outputs)?;
                    }
                    Err(e) => {
                        let error = e.to_string();
                        let error = strip_paths(dir.path(), &error);
                        compare_result(&test.join("error.txt"), &error)?;
                    }
                }
            }
            Err(e) => {
                let error = e.to_string();
                let error = strip_paths(dir.path(), &error);
                compare_result(&test.join("error.txt"), &error)?;
            }
        }

        Ok(())
    }
    .boxed()
}

/// Compares the evaluation output files against the baselines.
fn compare_evaluation_results(
    test_dir: &Path,
    temp_dir: &Path,
    evaluated: &EvaluatedTask,
) -> Result<()> {
    let command_path = evaluated
        .work_dir()
        .join("../command")
        .unwrap()
        .unwrap_local();
    let command = fs::read_to_string(&command_path).with_context(|| {
        format!(
            "failed to read task command file `{path}`",
            path = command_path.display()
        )
    })?;

    let stdout_path = evaluated.stdout().as_file().unwrap();
    let stdout = fs::read_to_string(stdout_path.as_str())
        .with_context(|| format!("failed to read task stdout file `{stdout_path}`"))?;

    let stderr_path = evaluated.stderr().as_file().unwrap();
    let stderr = fs::read_to_string(stderr_path.as_str())
        .with_context(|| format!("failed to read task stderr file `{stderr_path}`"))?;

    // Strip both temp paths and test dir (input file) paths from the outputs
    let command = strip_paths(temp_dir, &command);
    let command = strip_paths(test_dir, &command);
    let mut command = PATH_PREFIX_REGEX.replace_all(&command, "");

    // Replace any temporary file names in the command
    for i in 0..usize::MAX {
        match TEMP_FILENAME_REGEX.replace(&command, format!("tmp{i}")) {
            Cow::Borrowed(_) => break,
            Cow::Owned(s) => command = s.into(),
        }
    }

    compare_result(&test_dir.join("command"), &command)?;

    let stdout = strip_paths(temp_dir, &stdout);
    let stdout = strip_paths(test_dir, &stdout);
    compare_result(&test_dir.join("stdout"), &stdout)?;

    let stderr = strip_paths(temp_dir, &stderr);
    let stderr = strip_paths(test_dir, &stderr);
    compare_result(&test_dir.join("stderr"), &stderr)?;

    // Compare expected output files
    let mut had_files = false;
    let files_dir = test_dir.join("files");
    for entry in WalkDir::new(
        evaluated
            .work_dir()
            .as_local()
            .expect("work dir should be local"),
    ) {
        let entry = entry.with_context(|| {
            format!(
                "failed to read directory `{path}`",
                path = evaluated.work_dir().display()
            )
        })?;
        let metadata = entry.metadata().with_context(|| {
            format!(
                "failed to read metadata of `{path}`",
                path = entry.path().display()
            )
        })?;
        if !metadata.is_file() {
            continue;
        }

        had_files = true;

        let contents = fs::read_to_string(entry.path()).with_context(|| {
            format!(
                "failed to read file `{path}`",
                path = entry.path().display()
            )
        })?;
        let expected_path = files_dir.join(
            entry
                .path()
                .strip_prefix(
                    evaluated
                        .work_dir()
                        .as_local()
                        .expect("should be local path"),
                )
                .unwrap_or(entry.path()),
        );
        fs::create_dir_all(
            expected_path
                .parent()
                .expect("should have parent directory"),
        )
        .context("failed to create output file directory")?;
        compare_result(&expected_path, &contents)?;
    }

    // Look for missing output files
    if files_dir.exists() {
        for entry in WalkDir::new(&files_dir) {
            let entry = entry.with_context(|| {
                format!(
                    "failed to read directory `{path}`",
                    path = files_dir.display()
                )
            })?;
            let metadata = entry.metadata().with_context(|| {
                format!(
                    "failed to read metadata of `{path}`",
                    path = entry.path().display()
                )
            })?;
            if !metadata.is_file() {
                continue;
            }

            let relative_path = entry
                .path()
                .strip_prefix(&files_dir)
                .unwrap_or(entry.path());
            let expected_path = evaluated
                .work_dir()
                .join(relative_path.to_str().unwrap())?
                .unwrap_local();
            if !expected_path.is_file() {
                bail!(
                    "task did not produce expected output file `{path}`",
                    path = relative_path.display()
                );
            }
        }
    } else if had_files {
        bail!(
            "task generated files in the working directory that are not present in a `files` \
             subdirectory"
        );
    }

    Ok(())
}

fn main() -> Result<(), anyhow::Error> {
    // Default log level to off as some tests are designed to fail and we don't want
    // to log errors during the test
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::OFF.into())
                .from_env_lossy(),
        )
        .init();

    let args = libtest_mimic::Arguments::from_args();
    let runtime = tokio::runtime::Runtime::new()?;
    let tests = find_tests(
        run_test,
        &Path::new("tests").join("tasks"),
        runtime.handle(),
    )?;
    libtest_mimic::run(&args, tests).exit();
}
