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
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;
use codespan_reporting::term::Config;
use codespan_reporting::term::termcolor::Buffer;
use colored::Colorize;
use futures::StreamExt;
use futures::stream;
use path_clean::clean;
use pretty_assertions::StrComparison;
use regex::Regex;
use tempfile::TempDir;
use walkdir::WalkDir;
use wdl_analysis::AnalysisResult;
use wdl_analysis::Analyzer;
use wdl_analysis::document::Document;
use wdl_analysis::rules;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_engine::Engine;
use wdl_engine::EvaluatedTask;
use wdl_engine::EvaluationError;
use wdl_engine::Inputs;
use wdl_engine::local::LocalTaskExecutionBackend;
use wdl_engine::v1::TaskEvaluator;

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
    for entry in Path::new("tests/tasks").read_dir().unwrap() {
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
        .with_context(|| format!("failed to read result file `{path}`", path = path.display()))?
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

/// Runs the test given the provided analysis result.
async fn run_test(test: &Path, result: AnalysisResult) -> Result<()> {
    let cwd = std::env::current_dir().expect("must have a CWD");
    // Attempt to strip the CWD from the result path
    let path = result.document().uri().to_file_path();
    let path: Cow<'_, str> = match &path {
        // Strip the CWD from the path
        Ok(path) => path.strip_prefix(&cwd).unwrap_or(path).to_string_lossy(),
        // Use the id itself if there is no path
        Err(_) => result.document().uri().as_str().into(),
    };

    let diagnostics: Cow<'_, [Diagnostic]> = match result.error() {
        Some(e) => vec![Diagnostic::error(format!("failed to read `{path}`: {e:#}"))].into(),
        None => result.document().diagnostics().into(),
    };

    if let Some(diagnostic) = diagnostics.iter().find(|d| d.severity() == Severity::Error) {
        bail!(diagnostic_to_string(result.document(), &path, diagnostic));
    }

    let mut engine = Engine::new(LocalTaskExecutionBackend::new());
    let (name, mut inputs) = match Inputs::parse(
        engine.types_mut(),
        result.document(),
        test.join("inputs.json"),
    )? {
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
    inputs.join_paths(engine.types_mut(), result.document(), task, &test_dir);

    let dir = TempDir::new().context("failed to create temporary directory")?;
    let mut evaluator = TaskEvaluator::new(&mut engine);
    match evaluator
        .evaluate(result.document(), task, &inputs, dir.path(), &name)
        .await
    {
        Ok(evaluated) => {
            compare_evaluation_results(&test_dir, dir.path(), &evaluated)?;

            match evaluated.into_result() {
                Ok(outputs) => {
                    let outputs = outputs.with_name(name);
                    let mut buffer = Vec::new();
                    let mut serializer = serde_json::Serializer::pretty(&mut buffer);
                    outputs.serialize(engine.types(), &mut serializer)?;
                    let outputs = String::from_utf8(buffer).expect("output should be UTF-8");
                    let outputs = strip_paths(dir.path(), &outputs);
                    compare_result(&test.join("outputs.json"), &outputs)?;
                }
                Err(e) => {
                    let error = match e {
                        EvaluationError::Source(diagnostic) => {
                            diagnostic_to_string(result.document(), &path, &diagnostic)
                        }
                        EvaluationError::Other(e) => format!("{e:?}"),
                    };
                    let error = strip_paths(dir.path(), &error);
                    compare_result(&test.join("error.txt"), &error)?;
                }
            }
        }
        Err(e) => {
            let error = match e {
                EvaluationError::Source(diagnostic) => {
                    diagnostic_to_string(result.document(), &path, &diagnostic)
                }
                EvaluationError::Other(e) => format!("{e:?}"),
            };
            let error = strip_paths(dir.path(), &error);
            compare_result(&test.join("error.txt"), &error)?;
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
    let command = fs::read_to_string(evaluated.command()).with_context(|| {
        format!(
            "failed to read task command file `{path}`",
            path = evaluated.command().display()
        )
    })?;
    let stdout =
        fs::read_to_string(evaluated.stdout().as_file().unwrap().as_str()).with_context(|| {
            format!(
                "failed to read task stdout file `{path}`",
                path = evaluated.stdout().as_file().unwrap()
            )
        })?;
    let stderr =
        fs::read_to_string(evaluated.stderr().as_file().unwrap().as_str()).with_context(|| {
            format!(
                "failed to read task stderr file `{path}`",
                path = evaluated.stderr().as_file().unwrap()
            )
        })?;

    // Strip both temp paths and test dir (input file) paths from the outputs
    let command = strip_paths(temp_dir, &command);
    let mut command = strip_paths(test_dir, &command);

    // Replace any temporary file names in the command
    for i in 0..usize::MAX {
        match TEMP_FILENAME_REGEX.replace(&command, format!("tmp{i}")) {
            Cow::Borrowed(_) => break,
            Cow::Owned(s) => command = s,
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
    for entry in WalkDir::new(evaluated.work_dir()) {
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
                .strip_prefix(evaluated.work_dir())
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
            let expected_path = evaluated.work_dir().join(relative_path);
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

/// Creates a string from the given diagnostic.
fn diagnostic_to_string(document: &Document, path: &str, diagnostic: &Diagnostic) -> String {
    let source = document.node().syntax().text().to_string();
    let file = SimpleFile::new(path, &source);

    let mut buffer = Buffer::no_color();
    term::emit(
        &mut buffer,
        &Config::default(),
        &file,
        &diagnostic.to_codespan(),
    )
    .expect("should emit");

    String::from_utf8(buffer.into_inner()).expect("should be UTF-8")
}

#[tokio::main]
async fn main() {
    let tests = find_tests();
    println!("\nrunning {} tests\n", tests.len());

    // Start with a single analysis pass over all the test files
    let analyzer = Analyzer::new(rules(), |_, _, _, _| async {});
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

        // Discover the results that are relevant only to this test
        let base = clean(absolute(test).expect("should be made absolute"));

        let mut results = results.iter().filter_map(|r| {
            if r.document().uri().to_file_path().ok()?.starts_with(&base) {
                Some(r.clone())
            } else {
                None
            }
        });

        let result = results.next().expect("should have a result");
        if results.next().is_some() {
            println!("test {test_name} ... {failed}", failed = "failed".red());
            errors.push((
                test_name.to_string(),
                "more than one WDL file was in the test directory".to_string(),
            ));
            continue;
        }

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
