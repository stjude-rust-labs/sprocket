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
use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::process::exit;
use std::sync::LazyLock;
use std::thread::available_parallelism;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use codespan_reporting::diagnostic::Label;
use codespan_reporting::diagnostic::LabelStyle;
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term;
use codespan_reporting::term::Config;
use codespan_reporting::term::termcolor::Buffer;
use colored::Colorize;
use futures::StreamExt;
use futures::stream;
use pretty_assertions::StrComparison;
use regex::Regex;
use serde_json::to_string_pretty;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;
use wdl_analysis::AnalysisResult;
use wdl_analysis::Analyzer;
use wdl_analysis::DiagnosticsConfig;
use wdl_analysis::rules;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_engine::EvaluatedTask;
use wdl_engine::EvaluationError;
use wdl_engine::Inputs;
use wdl_engine::config;
use wdl_engine::config::BackendKind;
use wdl_engine::v1::TaskEvaluator;

/// Regex used to remove both host and guest path prefixes.
static PATH_PREFIX_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(attempts[\/\\]\d+[\/\\]|\/mnt\/inputs\/\d+\/)"#).expect("invalid regex")
});

/// Regex used to replace temporary file names in task command files with
/// consistent names for test baselines.
static TEMP_FILENAME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("tmp[[:alnum:]]{6}").expect("invalid regex"));

/// Finds tests to run as part of the analysis test suite.
fn find_tests() -> Vec<PathBuf> {
    // Check for filter arguments consisting of test names
    let mut filter = HashSet::new();
    for arg in std::env::args().skip_while(|a| a != "--").skip(1) {
        if !arg.starts_with('-') {
            filter.insert(arg);
        }
    }

    let mut tests: Vec<PathBuf> = Vec::new();
    for entry in Path::new("tests").join("tasks").read_dir().unwrap() {
        let entry = entry.expect("failed to read directory");
        let path = entry.path();
        if !path.is_dir()
            || (!filter.is_empty()
                && !filter.contains(entry.file_name().to_str().expect("name should be UTF-8")))
        {
            continue;
        }

        tests.push(path);
    }

    tests.sort();
    tests
}

/// Strips paths from the given string.
fn strip_paths(root: &Path, s: &str) -> String {
    #[cfg(windows)]
    {
        // First try it with a single slash
        let mut pattern = root.to_str().expect("path is not UTF-8").to_string();
        if !pattern.ends_with('\\') {
            pattern.push('\\');
        }

        // Next try with double slashes in case there were escaped backslashes
        let s = s.replace(&pattern, "");
        let pattern = pattern.replace('\\', "\\\\");
        s.replace(&pattern, "")
    }

    #[cfg(unix)]
    {
        let mut pattern = root.to_str().expect("path is not UTF-8").to_string();
        if !pattern.ends_with('/') {
            pattern.push('/');
        }

        s.replace(&pattern, "")
    }
}

/// Normalizes a result.
fn normalize(s: &str) -> String {
    // Normalize paths separation characters first
    s.replace("\\\\", "/")
        .replace("\\", "/")
        .replace("\r\n", "\n")
}

/// Compares a single result.
fn compare_result(path: &Path, result: &str) -> Result<()> {
    let result = normalize(result);
    if env::var_os("BLESS").is_some() {
        fs::write(path, &result).with_context(|| {
            format!(
                "failed to write result file `{path}`",
                path = path.display()
            )
        })?;
        return Ok(());
    }

    let expected = fs::read_to_string(path)
        .with_context(|| {
            format!(
                "failed to read result file `{path}`: expected contents to be `{result}`",
                path = path.display()
            )
        })?
        .replace("\r\n", "\n");

    if expected != result {
        bail!(
            "result from `{path}` is not as expected:\n{diff}",
            path = path.display(),
            diff = StrComparison::new(&expected, &result),
        );
    }

    Ok(())
}

/// Gets the engine configurations to use for the test.
fn configs() -> Vec<config::Config> {
    vec![
        {
            let mut config = config::Config::default();
            config.backend.default = BackendKind::Local;
            config
        },
        // Currently we limit running the Docker backend to Linux as GitHub does not have Docker
        // installed on macOS hosted runners and the Windows hosted runners are configured to use
        // Windows containers
        #[cfg(target_os = "linux")]
        {
            let mut config = config::Config::default();
            config.backend.crankshaft.default = config::CrankshaftBackendKind::Docker;
            config.backend.default = BackendKind::Crankshaft;
            config
        },
    ]
}

/// Runs the test given the provided analysis result.
async fn run_test(test: &Path, result: &AnalysisResult) -> Result<()> {
    let path = result.document().path();
    let diagnostics: Cow<'_, [Diagnostic]> = match result.error() {
        Some(e) => vec![Diagnostic::error(format!("failed to read `{path}`: {e:#}"))].into(),
        None => result.document().diagnostics().into(),
    };

    if let Some(diagnostic) = diagnostics.iter().find(|d| d.severity() == Severity::Error) {
        bail!(eval_error_to_string(&EvaluationError::new(
            result.document().clone(),
            diagnostic.clone()
        )));
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

    // Make any paths specified in the inputs file relative to the test directory
    let task = result
        .document()
        .task_by_name(&name)
        .ok_or_else(|| anyhow!("document does not contain a task named `{name}`"))?;
    inputs.join_paths(task, |_| Ok(&test_dir))?;

    for config in configs() {
        let evaluator = TaskEvaluator::new(config, CancellationToken::new()).await?;
        let dir = TempDir::new().context("failed to create temporary directory")?;
        match evaluator
            .evaluate(result.document(), task, &inputs, dir.path(), |_| async {})
            .await
        {
            Ok(evaluated) => {
                compare_evaluation_results(&test_dir, dir.path(), &evaluated)?;

                match evaluated.into_result() {
                    Ok(outputs) => {
                        let outputs = outputs.with_name(name.clone());
                        let outputs =
                            to_string_pretty(&outputs).context("failed to serialize outputs")?;
                        let outputs = strip_paths(dir.path(), &outputs);
                        compare_result(&test.join("outputs.json"), &outputs)?;
                    }
                    Err(e) => {
                        let error = eval_error_to_string(&e);
                        let error = strip_paths(dir.path(), &error);
                        compare_result(&test.join("error.txt"), &error)?;
                    }
                }
            }
            Err(e) => {
                let error = eval_error_to_string(&e);
                let error = strip_paths(dir.path(), &error);
                compare_result(&test.join("error.txt"), &error)?;
            }
        }
    }

    Ok(())
}

/// Compares the evaluation output files against the baselines.
fn compare_evaluation_results(
    test_dir: &Path,
    temp_dir: &Path,
    evaluated: &EvaluatedTask,
) -> Result<()> {
    let command = fs::read_to_string(evaluated.root().command()).with_context(|| {
        format!(
            "failed to read task command file `{path}`",
            path = evaluated.root().command().display()
        )
    })?;
    let stdout = fs::read_to_string(evaluated.root().stdout()).with_context(|| {
        format!(
            "failed to read task stdout file `{path}`",
            path = evaluated.root().stdout().display()
        )
    })?;
    let stderr = fs::read_to_string(evaluated.root().stderr()).with_context(|| {
        format!(
            "failed to read task stderr file `{path}`",
            path = evaluated.root().stderr().display()
        )
    })?;

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

/// Creates a string from the given evaluation error.
fn eval_error_to_string(e: &EvaluationError) -> String {
    match e {
        EvaluationError::Source(e) => {
            let mut files = SimpleFiles::new();
            let mut map = HashMap::new();

            let file_id = files.add(e.document.path(), e.document.root().text().to_string());

            let diagnostic =
                e.diagnostic
                    .to_codespan(file_id)
                    .with_labels_iter(e.backtrace.iter().map(|l| {
                        let id = l.document.id();
                        let file_id = *map.entry(id).or_insert_with(|| {
                            files.add(l.document.path(), l.document.root().text().to_string())
                        });

                        Label {
                            style: LabelStyle::Secondary,
                            file_id,
                            range: l.span.start()..l.span.end(),
                            message: "called from this location".into(),
                        }
                    }));

            let mut buffer = Buffer::no_color();
            term::emit(&mut buffer, &Config::default(), &files, &diagnostic)
                .expect("failed to emit diagnostic");

            String::from_utf8(buffer.into_inner()).expect("should be UTF-8")
        }
        EvaluationError::Other(e) => format!("{e:?}"),
    }
}

#[tokio::main]
async fn main() {
    let tests = find_tests();
    println!("\nrunning {} tests\n", tests.len());

    // Start with a single analysis pass over all the test files
    let analyzer = Analyzer::new(DiagnosticsConfig::new(rules()), |_, _, _, _| async {});
    for test in &tests {
        analyzer
            .add_directory(test.clone())
            .await
            .expect("should add directory");
    }
    let results = analyzer
        .analyze(())
        .await
        .expect("failed to analyze documents");

    let mut futures = Vec::new();
    let mut errors = Vec::new();
    for test in &tests {
        let test_name = test.file_stem().and_then(OsStr::to_str).unwrap();

        // Find the root source.wdl to evaluate
        let source_path = test.join("source.wdl");
        let result = match results
            .iter()
            .find(|r| Some(r.document().path().as_ref()) == source_path.to_str())
        {
            Some(result) => result,
            None => {
                println!("test {test_name} ... {failed}", failed = "failed".red());
                errors.push((
                    test_name.to_string(),
                    "`source.wdl` was not found in the analysis results`".to_string(),
                ));
                continue;
            }
        };

        futures.push(async { (test_name.to_string(), run_test(test, result).await) });
    }

    let mut stream = stream::iter(futures)
        .buffer_unordered(available_parallelism().map(Into::into).unwrap_or(1));
    while let Some((test_name, result)) = stream.next().await {
        match result {
            Ok(_) => {
                println!("test {test_name} ... {ok}", ok = "ok".green());
            }
            Err(e) => {
                println!("test {test_name} ... {failed}", failed = "failed".red());
                errors.push((test_name, format!("{e:?}")));
            }
        }
    }

    if !errors.is_empty() {
        eprintln!(
            "\n{count} test(s) {failed}:",
            count = errors.len(),
            failed = "failed".red()
        );

        for (name, msg) in errors.iter() {
            eprintln!("{name}: {msg}", msg = msg.red());
        }

        exit(1);
    }

    println!("\ntest result: ok. {count} passed\n", count = tests.len());
}
