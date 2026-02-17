//! The parser file tests.
//!
//! This test looks for directories in `tests/parsing`.
//!
//! Each directory is expected to contain:
//!
//! * `source.wdl` - the test input source to parse.
//! * `source.tree` - the expected CST representation of the source.
//! * `source.errors` - the expected set of parser errors encountered during the
//!   parse.
//!
//! Both `source.tree` and `source.errors` may be automatically generated or
//! updated by setting the `BLESS` environment variable when running this test.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use anyhow::Context as _;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;
use codespan_reporting::term::Config;
use codespan_reporting::term::termcolor::Buffer;
use libtest_mimic::Trial;
use pretty_assertions::StrComparison;
use wdl_grammar::Diagnostic;
use wdl_grammar::SyntaxTree;

/// Finds tests for this package.
fn find_tests() -> Vec<Trial> {
    Path::new("tests")
        .join("parsing")
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
            Some(Trial::test(test_name, move || Ok(run_test(&path)?)))
        })
        .collect()
}

/// Normalizes a path.
fn normalize(s: &str, is_error: bool) -> String {
    if is_error {
        // Normalize paths in any error messages
        return s.replace('\\', "/").replace("\r\n", "\n");
    }

    // Otherwise, just normalize line endings
    s.replace("\r\n", "\n")
}

/// Formats diagnostics.
fn format_diagnostics(diagnostics: &[Diagnostic], path: &Path, source: &str) -> String {
    let file = SimpleFile::new(path.as_os_str().to_str().unwrap(), source);
    let mut buffer = Buffer::no_color();
    for diagnostic in diagnostics {
        term::emit_to_write_style(
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
fn compare_result(path: &Path, result: &str, is_error: bool) -> Result<(), anyhow::Error> {
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
        .with_context(|| format!("failed to read result file `{path}`", path = path.display()))?
        .replace("\r\n", "\n");

    if expected != result {
        anyhow::bail!(
            "result from `{path}` is not as expected:\n{diff}",
            path = path.display(),
            diff = StrComparison::new(&expected, &result),
        );
    }

    Ok(())
}

/// Runs a test.
fn run_test(test: &Path) -> Result<(), anyhow::Error> {
    let path = test.join("source.wdl");
    let source = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read source file `{path}`", path = path.display()))?
        .replace("\r\n", "\n");
    let (tree, diagnostics) = SyntaxTree::parse(&source);
    compare_result(&path.with_extension("tree"), &format!("{tree:#?}"), false)?;
    compare_result(
        &path.with_extension("errors"),
        &format_diagnostics(&diagnostics, &path, &source),
        true,
    )?;
    Ok(())
}

fn main() {
    let args = libtest_mimic::Arguments::from_args();
    let tests = find_tests();
    libtest_mimic::run(&args, tests).exit();
}
