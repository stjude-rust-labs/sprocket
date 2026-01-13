//! The WDL analysis tests.
//!
//! This test looks for directories in `tests/analysis`.
//!
//! Each directory is expected to contain:
//!
//! * `source.wdl` - the test input source to analyze.
//! * `source.diagnostics` - the expected set of diagnostics across all analyzed
//!   files.
//! * `config.toml` (optional) - the `wdl_analysis::Config` to use to run the
//!   test.
//!
//! The `source.diagnostics` file may be automatically generated or updated by
//! setting the `BLESS` environment variable when running this test.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::absolute;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;
use codespan_reporting::term::Config as CodespanConfig;
use codespan_reporting::term::termcolor::Buffer;
use libtest_mimic::Trial;
use path_clean::PathClean;
use pretty_assertions::StrComparison;
use wdl_analysis::AnalysisResult;
use wdl_analysis::Analyzer;
use wdl_analysis::Config;
use wdl_analysis::path_to_uri;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;

/// These are the tests that require single document analysis as they are
/// sensitive to parse order.
const SINGLE_DOCUMENT_TESTS: &[&str] = &["import-dependency-cycle"];

/// Find tests to run as part of the analysis test suite.
fn find_tests(runtime: &tokio::runtime::Handle) -> Vec<Trial> {
    Path::new("tests")
        .join("analysis")
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
                    .map_err(|e| format!("{e:?}"))?)
            }))
        })
        .collect()
}

/// Normalizes a result.
fn normalize(s: &str, is_error: bool) -> String {
    if is_error {
        // Normalize paths in any error messages
        let s = s.replace('\\', "/").replace("\r\n", "\n");

        // Handle any OS specific errors messages
        let s = s.replace(
            "The system cannot find the file specified. (os error 2)",
            "No such file or directory (os error 2)",
        );
        return s;
    }

    // Otherwise, just normalize line endings
    s.replace("\r\n", "\n")
}

/// Compares a single result.
fn compare_result(path: &Path, result: &str, is_error: bool) -> Result<()> {
    let result = normalize(result, is_error);
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

/// Compares the provided results.
fn compare_results(test: &Path, results: Vec<AnalysisResult>) -> Result<()> {
    let mut buffer = Buffer::no_color();
    for result in results {
        // Attempt to strip the CWD from the result path
        let path = result.document().path();
        let diagnostics = match result.error() {
            Some(e) => vec![Diagnostic::error(format!("failed to read `{path}`: {e:#}"))],
            None => result.document().diagnostics().cloned().collect(),
        };

        if !diagnostics.is_empty() {
            let source = result.document().root().text().to_string();
            let file = SimpleFile::new(path, &source);
            for diagnostic in diagnostics {
                term::emit(
                    &mut buffer,
                    &CodespanConfig::default(),
                    &file,
                    &diagnostic.to_codespan(()),
                )
                .expect("should emit");
            }
        }
    }

    let output = test.join("source.diagnostics");
    compare_result(
        &output,
        &String::from_utf8(buffer.into_inner()).expect("should be UTF-8"),
        true,
    )
}

/// Run a test either in whole-directory or single-document mode based on
/// whether the test name appears in the `SINGLE_DOCUMENT_TESTS` list.
async fn run_test(test: &Path) -> Result<(), anyhow::Error> {
    // Set up a new analyzer for this test, reading in a custom config if present.
    let base = absolute(test).expect("should be made absolute").clean();
    let config_path = base.join("config.toml");
    let config = if config_path.exists() {
        toml::from_str(&std::fs::read_to_string(config_path)?)?
    } else {
        Config::default()
    };
    let analyzer = Analyzer::new(config, |_, _, _, _| async {});

    let results =
        if SINGLE_DOCUMENT_TESTS.contains(&base.file_stem().and_then(OsStr::to_str).unwrap()) {
            // Single-document tests add and analyze only `source.wdl`.
            let document = base.join("source.wdl");
            let uri = path_to_uri(&document).expect("should be valid URI");
            analyzer
                .add_document(path_to_uri(&document).expect("should be valid URI"))
                .await
                .context("adding test document")?;
            analyzer
                .analyze_document((), uri)
                .await
                .context("analyzing document")?
        } else {
            // If it's not specified as a single-document test, add and analyze the whole
            // directory
            analyzer
                .add_directory(&base)
                .await
                .context("adding test directory")?;
            analyzer.analyze(()).await.context("analyzing documents")?
        };
    compare_results(test, results)
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = libtest_mimic::Arguments::from_args();
    let runtime = tokio::runtime::Runtime::new()?;
    let tests = find_tests(runtime.handle());
    libtest_mimic::run(&args, tests).exit();
}
