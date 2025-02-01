//! The WDL workflow file tests.
//!
//! This test looks for directories in `tests/workflows`.
//!
//! Each directory is expected to contain:
//!
//! * `source.wdl` - the test input source to evaluate; the file is expected to
//!   contain no static analysis errors, but may fail at evaluation time.
//! * `error.txt` - the expected evaluation error, if any.
//! * `inputs.json` - the inputs to the workflow.
//! * `outputs.json` - the expected outputs from the workflow, if the workflow
//!   runs successfully.
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
use std::thread::available_parallelism;

use anyhow::Context;
use anyhow::Result;
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
use serde_json::to_string_pretty;
use tempfile::TempDir;
use wdl_analysis::AnalysisResult;
use wdl_analysis::Analyzer;
use wdl_analysis::DiagnosticsConfig;
use wdl_analysis::document::Document;
use wdl_analysis::rules;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_engine::EvaluationError;
use wdl_engine::Inputs;
use wdl_engine::config;
use wdl_engine::config::Backend;
use wdl_engine::v1::WorkflowEvaluator;

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
    for entry in Path::new("tests/workflows").read_dir().unwrap() {
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

    let mut inputs = match Inputs::parse(result.document(), test.join("inputs.json"))? {
        Some((_, Inputs::Task(_))) => {
            bail!("`inputs.json` contains inputs for a task, not a workflow")
        }
        Some((_, Inputs::Workflow(inputs))) => inputs,
        None => Default::default(),
    };

    let test_dir = absolute(test).expect("failed to get absolute directory");

    // Make any paths specified in the inputs file relative to the test directory
    let workflow = result
        .document()
        .workflow()
        .context("document does not contain a workflow")?;
    inputs.join_paths(workflow, &test_dir);

    let dir = TempDir::new().context("failed to create temporary directory")?;

    let mut config = config::Config::default();
    config.backend.default = Backend::Local;
    let mut evaluator = WorkflowEvaluator::new(config)?;
    match evaluator
        .evaluate(result.document(), inputs, dir.path(), |_| async {})
        .await
    {
        Ok(outputs) => {
            let outputs = outputs.with_name(workflow.name());
            let outputs = to_string_pretty(&outputs).context("failed to serialize outputs")?;
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
