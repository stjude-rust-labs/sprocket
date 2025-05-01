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

use std::collections::HashSet;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;

use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;
use codespan_reporting::term::Config;
use codespan_reporting::term::termcolor::Buffer;
use colored::Colorize;
use path_clean::clean;
use pretty_assertions::StrComparison;
use wdl_analysis::Analyzer;
use wdl_analysis::DiagnosticsConfig;
use wdl_analysis::Validator;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_lint::Linter;

/// Finds tests for this package.
fn find_tests() -> Vec<PathBuf> {
    // Check for filter arguments consisting of test names
    let mut filter = HashSet::new();
    for arg in std::env::args().skip_while(|a| a != "--").skip(1) {
        if !arg.starts_with('-') {
            filter.insert(arg);
        }
    }

    let mut tests: Vec<PathBuf> = Vec::new();
    for entry in Path::new("tests/lints").read_dir().unwrap() {
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

/// Normalizes a path.
fn normalize(s: &str) -> String {
    // Normalize paths in any error messages
    s.replace('\\', "/").replace("\r\n", "\n")
}

/// Formats diagnostics.
fn format_diagnostics(diagnostics: &[Diagnostic], path: &Path, source: &str) -> String {
    let file = SimpleFile::new(path.as_os_str().to_str().unwrap(), source);
    let mut buffer = Buffer::no_color();
    for diagnostic in diagnostics {
        term::emit(
            &mut buffer,
            &Config::default(),
            &file,
            &diagnostic.to_codespan(()),
        )
        .expect("should emit");
    }

    String::from_utf8(buffer.into_inner()).expect("should be UTF-8")
}

/// Compares a test result.
fn compare_result(path: &Path, result: &str) -> Result<(), String> {
    let result = normalize(result);
    if env::var_os("BLESS").is_some() {
        fs::write(path, &result).map_err(|e| {
            format!(
                "failed to write result file `{path}`: {e}",
                path = path.display()
            )
        })?;
        return Ok(());
    }

    let expected = fs::read_to_string(path)
        .map_err(|e| {
            format!(
                "failed to read result file `{path}`: {e}",
                path = path.display()
            )
        })?
        .replace("\r\n", "\n");

    if expected != result {
        return Err(format!(
            "result from `{path}` is not as expected:\n{diff}",
            path = path.display(),
            diff = StrComparison::new(&expected, &result),
        ));
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    let tests = find_tests();
    println!("\nrunning {} tests\n", tests.len());

    let analyzer = Analyzer::new_with_validator(
        DiagnosticsConfig::except_all(),
        |_, _, _, _| async {},
        || {
            let mut validator = Validator::default();
            validator.add_visitor(Linter::default());
            validator
        },
    );
    for test in &tests {
        analyzer
            .add_directory(test.to_path_buf())
            .await
            .expect("failed to add directory");
    }
    let results = analyzer.analyze(()).await.expect("failed to analyze");

    let mut errors = Vec::new();
    for test in &tests {
        let test_name = test.file_stem().and_then(OsStr::to_str).unwrap();

        let base = clean(absolute(test).unwrap());
        let source_path = base.join("source.wdl");
        let errors_path = base.join("source.errors");

        let result = results
            .iter()
            .find_map(|result| {
                if result.document().uri().to_file_path().unwrap() == source_path {
                    Some(result.clone())
                } else {
                    None
                }
            })
            .expect("failed to find test result");
        match compare_result(
            &errors_path,
            &format_diagnostics(
                result.document().diagnostics(),
                &test.join("source.wdl"),
                &result.document().root().text().to_string(),
            ),
        ) {
            Ok(()) => {
                println!("test {test_name} ... {ok}", ok = "ok".green());
            }
            Err(e) => {
                println!("test {test_name} ... {failed}", failed = "failed".red());
                errors.push((test_name, e));
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

        std::process::exit(1);
    }

    println!("\ntest result: ok. {count} passed\n", count = tests.len());
}
