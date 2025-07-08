//! The WDL analysis tests.
//!
//! This test looks for directories in `tests/analysis`.
//!
//! Each directory is expected to contain:
//!
//! * `source.wdl` - the test input source to analyze.
//! * `source.diagnostics` - the expected set of diagnostics across all analyzed
//!   files.
//!
//! The `source.diagnostics` file may be automatically generated or updated by
//! setting the `BLESS` environment variable when running this test.

use std::borrow::Cow;
use std::collections::HashSet;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::process::exit;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;
use codespan_reporting::term::Config as CodespanConfig;
use codespan_reporting::term::termcolor::Buffer;
use colored::Colorize;
use path_clean::clean;
use pretty_assertions::StrComparison;
use wdl_analysis::AnalysisResult;
use wdl_analysis::Analyzer;
use wdl_analysis::Config;
use wdl_analysis::path_to_uri;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;

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
    for entry in Path::new("tests/analysis").read_dir().unwrap() {
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
        let diagnostics: Cow<'_, [Diagnostic]> = match result.error() {
            Some(e) => vec![Diagnostic::error(format!("failed to read `{path}`: {e:#}"))].into(),
            None => result.document().diagnostics().into(),
        };

        if !diagnostics.is_empty() {
            let source = result.document().root().text().to_string();
            let file = SimpleFile::new(path, &source);
            for diagnostic in diagnostics.as_ref() {
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // These are the tests that require single document analysis as they are
    // sensitive to parse order
    /// The tests that require single document analysis.
    const SINGLE_DOCUMENT_TESTS: &[&str] = &["import-dependency-cycle"];

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let tests = find_tests();
    println!("\nrunning {} tests\n", tests.len());

    let mut errors = Vec::new();
    let mut single_file = Vec::new();
    for test in &tests {
        let test_name = test.file_stem().and_then(OsStr::to_str).unwrap();
        if SINGLE_DOCUMENT_TESTS.contains(&test_name) {
            single_file.push(test_name);
            continue;
        }

        // Add this test's directory to a new analyzer, reading in a custom config if
        // present.
        let base = clean(absolute(test).expect("should be made absolute"));
        let config_path = base.join("config.toml");
        let config = if config_path.exists() {
            toml::from_str(&std::fs::read_to_string(config_path)?)?
        } else {
            Config::default()
        };
        let analyzer = Analyzer::new(config, |_, _, _, _| async {});
        analyzer
            .add_directory(&base)
            .await
            .expect("should add directory");
        let results = analyzer
            .analyze(())
            .await
            .expect("failed to analyze documents");

        // Discover the results that are relevant only to this test
        //
        // NOTE: clippy appears to be incorrect that this can be modified to use
        // `filter_map`. Perhaps this should be revisited in the future.
        #[allow(clippy::filter_map_bool_then)]
        let results = results
            .iter()
            .filter_map(|r| {
                r.document()
                    .uri()
                    .to_file_path()
                    .ok()?
                    .starts_with(&base)
                    .then(|| r.clone())
            })
            .collect();
        match compare_results(test, results) {
            Ok(_) => {
                println!("test {test_name} ... {ok}", ok = "ok".green());
            }
            Err(e) => {
                println!("test {test_name} ... {failed}", failed = "failed".red());
                errors.push((test_name, e.to_string()));
            }
        }
    }

    // Some tests are sensitive to the order in which files are parsed (e.g.
    // detecting cycles) For those, use a new analyzer and analyze the
    // `source.wdl` directly
    for test_name in single_file {
        let test = Path::new("tests/analysis").join(test_name);
        let config_path = test.join("config.toml");
        let config = if config_path.exists() {
            toml::from_str(&std::fs::read_to_string(config_path)?)?
        } else {
            Config::default()
        };
        let analyzer = Analyzer::new(config, |_, _, _, _| async {});
        let document = test.join("source.wdl");
        let uri = path_to_uri(&document).expect("should be valid URI");
        analyzer
            .add_document(path_to_uri(&document).expect("should be valid URI"))
            .await
            .expect("should add document");
        let results = analyzer
            .analyze_document((), uri)
            .await
            .expect("failed to analyze document");
        match compare_results(&test, results) {
            Ok(_) => {
                println!("test {test_name} ... {ok}", ok = "ok".green());
            }
            Err(e) => {
                println!("test {test_name} ... {failed}", failed = "failed".red());
                errors.push((test_name, e.to_string()));
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

    Ok(())
}
