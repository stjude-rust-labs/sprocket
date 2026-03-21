//! The parser validation tests.
//!
//! This test looks for directories in `tests/validation`.
//!
//! Each directory is expected to contain:
//!
//! * `source.wdl` - the test input source to parse.
//! * `source.errors` - the expected set of validation errors.
//! * `config.toml` (optional) - the `wdl_analysis::Config` to use to run the
//!   test.
//!
//! The `source.errors` file may be automatically generated or updated by
//! setting the `BLESS` environment variable when running this test.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::absolute;

use anyhow::anyhow;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;
use codespan_reporting::term::Config as CodespanConfig;
use codespan_reporting::term::termcolor::Buffer;
use libtest_mimic::Trial;
use path_clean::PathClean;
use pretty_assertions::StrComparison;
use tracing_subscriber::EnvFilter;
use wdl_analysis::Analyzer;
use wdl_analysis::Config;
use wdl_analysis::DiagnosticsConfig;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;

/// Find tests for grammar validation.
fn find_tests(runtime: &tokio::runtime::Handle) -> Vec<Trial> {
    Path::new("tests")
        .join("validation")
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
        return s.replace('\\', "/").replace("\r\n", "\n");
    }

    // Otherwise, just normalize line endings
    s.replace("\r\n", "\n")
}

/// Formats diagnostics.
fn format_diagnostics<'a>(
    diagnostics: impl Iterator<Item = &'a Diagnostic>,
    path: &Path,
    source: &str,
) -> String {
    let file = SimpleFile::new(path.as_os_str().to_str().unwrap(), source);
    let mut buffer = Buffer::no_color();
    for diagnostic in diagnostics {
        term::emit_to_write_style(
            &mut buffer,
            &CodespanConfig::default(),
            &file,
            &diagnostic.to_codespan(()),
        )
        .expect("should emit");
    }

    String::from_utf8(buffer.into_inner()).expect("should be UTF-8")
}

/// Compares a single result.
fn compare_result(path: &Path, result: &str, is_error: bool) -> Result<(), anyhow::Error> {
    let result = normalize(result, is_error);
    if env::var_os("BLESS").is_some() {
        fs::write(path, &result).map_err(|e| {
            anyhow!(
                "failed to write result file `{path}`: {e}",
                path = path.display()
            )
        })?;
        return Ok(());
    }

    let expected = fs::read_to_string(path)
        .map_err(|e| {
            anyhow!(
                "failed to read result file `{path}`: {e}",
                path = path.display()
            )
        })?
        .replace("\r\n", "\n");

    if expected != result {
        return Err(anyhow!(
            "result from `{path}` is not as expected:\n{diff}",
            path = path.display(),
            diff = StrComparison::new(&expected, &result),
        ));
    }

    Ok(())
}

/// Run a single test.
async fn run_test(test: &Path) -> Result<(), anyhow::Error> {
    // Add this test's directory to a new analyzer, reading in a custom config if
    // present.
    let base = absolute(test).expect("should be made absolute").clean();
    let source_path = base.join("source.wdl");
    let errors_path = base.join("source.errors");
    let config_path = base.join("config.toml");

    let config = if config_path.exists() {
        toml::from_str(&std::fs::read_to_string(config_path)?)?
    } else {
        Config::default().with_diagnostics_config(DiagnosticsConfig::except_all())
    };
    let analyzer = Analyzer::new(config, |_, _, _, _| async {});
    analyzer
        .add_directory(base)
        .await
        .expect("should add directory");
    let results = analyzer
        .analyze(())
        .await
        .expect("failed to analyze documents");

    // Find the result for this test's `source.wdl`
    let result = results
        .into_iter()
        .find_map(|result| {
            if result.document().uri().to_file_path().unwrap() == source_path {
                Some(result.clone())
            } else {
                None
            }
        })
        .expect("should find result for test");
    compare_result(
        &errors_path,
        &format_diagnostics(
            result.document().diagnostics(),
            &test.join("source.wdl"),
            &result.document().root().text().to_string(),
        ),
        true,
    )
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = libtest_mimic::Arguments::from_args();
    let runtime = tokio::runtime::Runtime::new()?;
    let tests = find_tests(runtime.handle());
    libtest_mimic::run(&args, tests).exit();
}
