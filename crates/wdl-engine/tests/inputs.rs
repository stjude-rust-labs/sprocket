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

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;
use codespan_reporting::term::Config;
use codespan_reporting::term::termcolor::Buffer;
use libtest_mimic::Trial;
use pretty_assertions::StrComparison;
use wdl_analysis::Analyzer;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_engine::Inputs;

/// Find tests to run.
fn find_tests(runtime: &tokio::runtime::Handle) -> Vec<Trial> {
    Path::new("tests")
        .join("inputs")
        .read_dir()
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.expect("failed to read directory");
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }

            let test_name = path
                .file_stem()
                .map(OsStr::to_string_lossy)
                .unwrap()
                .into_owned();
            let test_runtime = runtime.clone();
            Some(Trial::test(test_name, move || {
                Ok(test_runtime
                    .block_on(run_test(&path))
                    .map_err(|e| format!("{e:#}"))?)
            }))
        })
        .collect()
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

/// Runs the test.
async fn run_test(test: &Path) -> Result<()> {
    let analyzer = Analyzer::default();
    analyzer
        .add_directory(test)
        .await
        .context("adding directory")?;
    let results = analyzer.analyze(()).await.context("running analysis")?;

    // Find the analysis result specific to this test
    let test_name = test.file_stem().and_then(OsStr::to_str).unwrap();
    let Some(result) = results.into_iter().find_map(|r| {
        let path = r.document().uri().to_file_path().ok()?;
        if path.parent()?.file_name()?.to_str()? == test_name {
            Some(r)
        } else {
            None
        }
    }) else {
        bail!("failed to find analysis result for test `{test_name}`")
    };

    let mut buffer = Buffer::no_color();

    let path = result.document().path();
    let diagnostics = match result.error() {
        Some(e) => vec![Diagnostic::error(format!("failed to read `{path}`: {e:#}"))],
        None => result.document().diagnostics().cloned().collect(),
    };

    if let Some(diagnostic) = diagnostics.iter().find(|d| d.severity() == Severity::Error) {
        let source = result.document().root().text().to_string();
        let file = SimpleFile::new(&path, &source);

        term::emit_to_io_write(
            &mut buffer,
            &Config::default(),
            &file,
            &diagnostic.to_codespan(()),
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
    if test_name == "missing-file" {
        // Always use the JSON path for consistency across platforms and pass as &Path
        let result = match Inputs::parse(document, &json_path) {
            Ok(_) => String::new(),
            Err(e) => format!("{e:#}"),
        };

        let output = test.join("error.txt");
        return compare_result(&output, &result);
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
                        Err(e) => format!("{e:#}"),
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
                        Err(e) => format!("{e:#}"),
                    }
                }
            },
            Ok(None) => String::new(),
            Err(e) => format!("{e:#}"),
        };

        let output = test.join("error.txt");
        compare_result(&output, &result)?;
    }

    Ok(())
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = libtest_mimic::Arguments::from_args();
    let runtime = tokio::runtime::Runtime::new()?;
    let tests = find_tests(runtime.handle());
    libtest_mimic::run(&args, tests).exit();
}
