//! The format file tests.
//!
//! This test looks for directories in `tests/format`.
//!
//! Each directory is expected to contain:
//!
//! * `source.wdl` - the test input source to parse.
//! * `source.formatted.wdl` - the expected formatted output.
//!
//! The `source.formatted.wdl` file may be automatically generated or updated by
//! setting the `BLESS` environment variable when running this test.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use anyhow::Context as _;
use anyhow::bail;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;
use codespan_reporting::term::Config;
use codespan_reporting::term::termcolor::Buffer;
use libtest_mimic::Trial;
use pretty_assertions::StrComparison;
use wdl_ast::Diagnostic;
use wdl_ast::Document;
use wdl_ast::Node;
use wdl_format::Formatter;
use wdl_format::element::FormatElement;
use wdl_format::element::node::AstNodeFormatExt;

/// Normalizes a result.
fn normalize(s: &str) -> String {
    // Just normalize line endings
    s.replace("\r\n", "\n")
}

/// Find all the tests in the `tests/format` directory.
fn find_tests() -> Vec<Trial> {
    Path::new("tests")
        .join("format")
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

/// Format a list of diagnostics.
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

/// Compare the result of a test to the expected result.
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

/// Parses source string into a document FormatElement
fn prepare_document(source: &str, path: &Path) -> Result<FormatElement, anyhow::Error> {
    let (document, diagnostics) = Document::parse(source);

    if !diagnostics.is_empty() {
        bail!(
            "failed to parse `{path}` {e}",
            path = path.display(),
            e = format_diagnostics(&diagnostics, path, source)
        );
    };

    Ok(Node::Ast(document.ast().into_v1().unwrap()).into_format_element())
}

/// Parses and formats source string
fn format(source: &str, path: &Path) -> Result<String, anyhow::Error> {
    let document = prepare_document(source, path)?;
    Formatter::default()
        .format(&document)
        .context("formatting document")
}

/// Run a test.
fn run_test(test: &Path) -> Result<(), anyhow::Error> {
    let path = test.join("source.wdl");
    let formatted_path = path.with_extension("formatted.wdl");
    let source = std::fs::read_to_string(&path).context("reading source file")?;

    let formatted = format(&source, path.as_path())?;
    compare_result(formatted_path.as_path(), &formatted)?;

    // test idempotency by formatting the formatted document
    let twice_formatted = format(&formatted, formatted_path.as_path())?;
    compare_result(formatted_path.as_path(), &twice_formatted)
}

/// Run all the tests.
fn main() {
    let args = libtest_mimic::Arguments::from_args();
    let tests = find_tests();
    libtest_mimic::run(&args, tests).exit();
}
