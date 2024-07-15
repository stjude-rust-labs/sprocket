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
use std::process::exit;
use std::sync::Arc;

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;
use codespan_reporting::term::termcolor::Buffer;
use codespan_reporting::term::Config;
use colored::Colorize;
use futures::stream::FuturesOrdered;
use futures::StreamExt;
use futures::TryStreamExt;
use pretty_assertions::StrComparison;
use wdl_analysis::AnalysisEngine;
use wdl_analysis::AnalysisResult;
use wdl_ast::Diagnostic;
use wdl_ast::SyntaxNode;

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

fn normalize(s: &str, is_error: bool) -> String {
    if is_error {
        // Normalize paths in any error messages
        return s.replace('\\', "/").replace("\r\n", "\n");
    }

    // Otherwise, just normalize line endings
    s.replace("\r\n", "\n")
}

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
        .with_context(|| format!("failed to read result file `{path}`", path = path.display()))?
        .replace("\r\n", "\n");

    if expected != result {
        bail!(
            "result is not as expected:\n{}",
            StrComparison::new(&expected, &result),
        );
    }

    Ok(())
}

fn compare_results(test: &Path, results: Vec<AnalysisResult>) -> Result<()> {
    let mut buffer = Buffer::no_color();
    let cwd = std::env::current_dir().expect("must have a CWD");
    for result in results {
        let path = result.id().path();

        // Attempt to strip the CWD from the result path
        let path: Cow<'_, str> = match &path {
            // Strip the CWD from the path
            Some(path) => path.strip_prefix(&cwd).unwrap_or(path).to_string_lossy(),
            // Use the id itself if there is no path
            None => result.id().to_str(),
        };

        let diagnostics: Cow<'_, [Diagnostic]> = match result.error() {
            Some(e) => vec![Diagnostic::error(format!("failed to read `{path}`: {e:#}"))].into(),
            None => result.diagnostics().into(),
        };

        if !diagnostics.is_empty() {
            let source = result
                .root()
                .map(|n| SyntaxNode::new_root(n.clone()).text().to_string())
                .unwrap_or(String::new());
            let file = SimpleFile::new(path, &source);
            for diagnostic in diagnostics.as_ref() {
                term::emit(
                    &mut buffer,
                    &Config::default(),
                    &file,
                    &diagnostic.to_codespan(),
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
async fn main() {
    let tests = find_tests();
    println!("\nrunning {} tests\n", tests.len());

    let engine = Arc::new(AnalysisEngine::new().expect("failed to create analysis engine"));

    let tests_engine = engine.clone();
    let mut tasks = tests
        .iter()
        .map(move |test| {
            let engine = tests_engine.clone();
            let source = test.join("source.wdl");
            tokio::spawn(async move { engine.analyze(&source).await })
        })
        .collect::<FuturesOrdered<_>>()
        .into_stream()
        .enumerate();

    let mut errors = Vec::new();
    while let Some((index, result)) = tasks.next().await {
        let test_name = tests[index].file_stem().and_then(OsStr::to_str).unwrap();
        match result {
            Ok(results) => match compare_results(&tests[index], results) {
                Ok(_) => {
                    println!("test {test_name} ... {ok}", ok = "ok".green());
                }
                Err(e) => {
                    println!("test {test_name} ... {failed}", failed = "failed".red());
                    errors.push((test_name, e.to_string()));
                }
            },
            Err(e) => {
                println!(
                    "test {test_name} ... {panicked}",
                    panicked = "panicked".red()
                );
                errors.push((test_name, e.to_string()));
            }
        }
    }

    Arc::into_inner(engine)
        .expect("should be last engine reference")
        .shutdown()
        .await;

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
