//! The lint file tests.
//!
//! This test looks for directories in `tests/lints`.
//!
//! Each directory is expected to contain:
//!
//! * `source.wdl` - the test input source to parse; the first line in the file
//!   must be a comment with the lint rule name to run.
//! * `source.errors` - the expected set of lint diagnostics.
//!
//! The `source.errors` file may be automatically generated or updated by
//! setting the `BLESS` environment variable when running this test.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::absolute;

use anyhow::Context as _;
use anyhow::bail;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;
use codespan_reporting::term::Config as CodespanConfig;
use codespan_reporting::term::termcolor::Buffer;
use libtest_mimic::Trial;
use path_clean::PathClean;
use pretty_assertions::StrComparison;
use wdl_analysis::Analyzer;
use wdl_analysis::Config as AnalysisConfig;
use wdl_analysis::DiagnosticsConfig;
use wdl_analysis::Validator;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_lint::Config;
use wdl_lint::Linter;
use wdl_lint::rules;

/// Finds tests for this package.
fn find_tests(runtime: &tokio::runtime::Handle) -> Vec<Trial> {
    Path::new("tests")
        .join("lints")
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

/// Normalizes a path.
fn normalize(s: &str) -> String {
    // Normalize paths in any error messages
    s.replace('\\', "/").replace("\r\n", "\n")
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
        term::emit_to_io_write(
            &mut buffer,
            &CodespanConfig::default(),
            &file,
            &diagnostic.to_codespan(()),
        )
        .expect("should emit");
    }

    String::from_utf8(buffer.into_inner()).expect("should be UTF-8")
}

/// Compares a test result.
fn compare_result(path: &Path, result: &str) -> Result<(), anyhow::Error> {
    let result = normalize(result);
    if env::var_os("BLESS").is_some() {
        fs::write(path, &result).context("writing result file")?;
        return Ok(());
    }

    let expected = fs::read_to_string(path)
        .context("reading result file")?
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

/// Runs a lint test.
async fn run_test(test: &Path) -> Result<(), anyhow::Error> {
    let config_path = test.join("config.toml");
    if config_path.exists() {
        let config_str = fs::read_to_string(&config_path)?;
        let config = toml::from_str(&config_str)?;

        run_test_inner(test, "source.errors.default", Config::default()).await?;
        run_test_inner(test, "source.errors", config).await?;
    } else {
        run_test_inner(test, "source.errors", Config::default()).await?;
    }

    Ok(())
}

/// Runs a lint test with the specified [`Config`]
async fn run_test_inner(
    test: &Path,
    errors_path: &str,
    config: Config,
) -> Result<(), anyhow::Error> {
    let analyzer = Analyzer::new_with_validator(
        AnalysisConfig::default().with_diagnostics_config(DiagnosticsConfig::except_all()),
        |_, _, _, _| async {},
        move || {
            let mut validator = Validator::default();
            validator.add_visitor(Linter::new(rules(&config)));
            validator
        },
    );
    analyzer
        .add_directory(test)
        .await
        .context("adding directory")?;
    let results = analyzer.analyze(()).await.context("running analysis")?;

    let base = absolute(test)?.clean();
    let source_path = base.join("source.wdl");
    let errors_path = base.join(errors_path);

    let Some(result) = results
        .into_iter()
        .find(|result| result.document().uri().to_file_path().unwrap() == source_path)
    else {
        bail!("failed to find test result");
    };
    compare_result(
        &errors_path,
        &format_diagnostics(
            result.document().diagnostics(),
            &test.join("source.wdl"),
            &result.document().root().text().to_string(),
        ),
    )
}

fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = libtest_mimic::Arguments::from_args();
    let runtime = tokio::runtime::Runtime::new()?;
    let tests = find_tests(runtime.handle());
    libtest_mimic::run(&args, tests).exit();
}
