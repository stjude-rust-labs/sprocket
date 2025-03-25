//! The WDL input file tests.
//!
//! This test looks for directories in `tests/inputs`.
//!
//! Each directory is expected to contain:
//!
//! * `source.wdl` - the test input source to analyze; the file is expected to
//!   contain no error diagnostics.
//! * Both of:
//!   * `inputs.json` - The JSON format inputs to the workflow or task.
//!   * `inputs.yaml` - The YAML format inputs to the workflow or task.
//! * `error.txt` - the expected error message (if there is one).
//!
//! Requiring both JSON and YAML variants ensures complete test coverage and
//! consistent behavior across different input formats.
//!
//! An exception is made for the "missing-file" test which intentionally tests
//! the error case of a missing input file.
//!
//! The `error.txt` file may be automatically generated or updated by setting
//! the `BLESS` environment variable when running this test.

use std::borrow::Cow;
use std::collections::HashSet;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::process::exit;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;
use codespan_reporting::term::Config;
use codespan_reporting::term::termcolor::Buffer;
use colored::Colorize;
use path_clean::clean;
use pretty_assertions::StrComparison;
use rayon::prelude::*;
use wdl_analysis::AnalysisResult;
use wdl_analysis::Analyzer;
use wdl_analysis::DiagnosticsConfig;
use wdl_analysis::rules;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_engine::Inputs;

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
    for entry in Path::new("tests/inputs").read_dir().unwrap() {
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

/// Normalizes a result.
fn normalize(s: &str) -> String {
    // Normalize paths in any error messages
    let mut s = s.replace('\\', "/").replace("\r\n", "\n");

    // Handle any OS specific errors messages
    s = s.replace(
        "The system cannot find the file specified. (os error 2)",
        "No such file or directory (os error 2)",
    );

    // Normalize references to YAML files to match JSON baselines
    s = s.replace("inputs.yaml", "inputs.json");

    s
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
fn run_test(test: &Path, result: AnalysisResult, ntests: &AtomicUsize) -> Result<()> {
    let mut buffer = Buffer::no_color();

    let path = result.document().path();
    let diagnostics: Cow<'_, [Diagnostic]> = match result.error() {
        Some(e) => vec![Diagnostic::error(format!("failed to read `{path}`: {e:#}"))].into(),
        None => result.document().diagnostics().into(),
    };

    if let Some(diagnostic) = diagnostics.iter().find(|d| d.severity() == Severity::Error) {
        let source = result.document().root().text().to_string();
        let file = SimpleFile::new(&path, &source);

        term::emit(
            &mut buffer,
            &Config::default(),
            &file,
            &diagnostic.to_codespan(),
        )
        .expect("should emit");

        let diagnostic: String = String::from_utf8(buffer.into_inner()).expect("should be UTF-8");
        bail!("document `{path}` contains at least one diagnostic error:\n{diagnostic}");
    }

    let document = result.document();

    let json_path = test.join("inputs.json");
    let yaml_path = test.join("inputs.yaml");

    // Special case for the "missing-file" test which intentionally tests missing
    // input files
    if test.file_name().unwrap().to_string_lossy() == "missing-file" {
        // Always use the JSON path for consistency across platforms and pass as &Path
        let result = match Inputs::parse(document, &json_path) {
            Ok(_) => String::new(),
            Err(e) => format!("{e:?}"),
        };

        let output = test.join("error.txt");
        compare_result(&output, &result)?;
        ntests.fetch_add(1, Ordering::SeqCst);
        return Ok(());
    }

    // For all other tests, require both JSON and YAML files to ensure complete
    // coverage
    if !json_path.exists() {
        bail!("inputs.json doesn't exist for test, both JSON and YAML formats are required");
    }
    if !yaml_path.exists() {
        bail!("inputs.yaml doesn't exist for test, both JSON and YAML formats are required");
    }

    // Test for each input file format
    for input_path in [&json_path, &yaml_path] {
        let result = match Inputs::parse(document, input_path) {
            Ok(Some((name, inputs))) => match inputs {
                Inputs::Task(inputs) => {
                    match inputs
                        .validate(
                            document,
                            document
                                .task_by_name(&name)
                                .expect("task should be present"),
                            None,
                        )
                        .with_context(|| format!("failed to validate the inputs to task `{name}`"))
                    {
                        Ok(()) => String::new(),
                        Err(e) => format!("{e:?}"),
                    }
                }
                Inputs::Workflow(inputs) => {
                    let workflow = document.workflow().expect("workflow should be present");
                    match inputs.validate(document, workflow, None).with_context(|| {
                        format!(
                            "failed to validate the inputs to workflow `{workflow}`",
                            workflow = workflow.name()
                        )
                    }) {
                        Ok(()) => String::new(),
                        Err(e) => format!("{e:?}"),
                    }
                }
            },
            Ok(None) => String::new(),
            Err(e) => format!("{e:?}"),
        };

        let output = test.join("error.txt");
        compare_result(&output, &result)?;
    }

    ntests.fetch_add(1, Ordering::SeqCst);
    Ok(())
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

    let ntests = AtomicUsize::new(0);
    let errors = tests
        .par_iter()
        .filter_map(|test| {
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

            let result = match results.find_map(|r| {
                let path = r.document().uri().to_file_path().ok()?;
                if path.parent()?.file_name()?.to_str()? == test_name {
                    Some(r.clone())
                } else {
                    None
                }
            }) {
                Some(document) => document,
                None => {
                    return Some((
                        test_name,
                        format!("failed to find analysis result for test `{test_name}`"),
                    ));
                }
            };

            let test_name = test.file_stem().and_then(OsStr::to_str).unwrap();
            match std::panic::catch_unwind(|| {
                match run_test(test, result, &ntests)
                    .map_err(|e| format!("failed to run test `{path}`: {e}", path = test.display()))
                    .err()
                {
                    Some(e) => {
                        println!("test {test_name} ... {failed}", failed = "failed".red());
                        Some((test_name, e))
                    }
                    None => {
                        println!("test {test_name} ... {ok}", ok = "ok".green());
                        None
                    }
                }
            }) {
                Ok(result) => result,
                Err(e) => {
                    println!(
                        "test {test_name} ... {panicked}",
                        panicked = "panicked".red()
                    );
                    Some((
                        test_name,
                        format!(
                            "test panicked: {e:?}",
                            e = e
                                .downcast_ref::<String>()
                                .map(|s| s.as_str())
                                .or_else(|| e.downcast_ref::<&str>().copied())
                                .unwrap_or("no panic message")
                        ),
                    ))
                }
            }
        })
        .collect::<Vec<_>>();

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

    println!(
        "\ntest result: ok. {} passed\n",
        ntests.load(Ordering::SeqCst)
    );
}
