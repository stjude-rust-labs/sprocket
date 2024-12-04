//! The WDL input file tests.
//!
//! This test looks for directories in `tests/inputs`.
//!
//! Each directory is expected to contain:
//!
//! * `source.wdl` - the test input source to analyze; the file is expected to
//!   contain no error diagnostics.
//! * `inputs.json` - the inputs to the workflow or task.
//! * `error.txt` - the expected error message (if there is one).
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
use wdl_analysis::AnalysisResult;
use wdl_analysis::Analyzer;
use wdl_analysis::rules;
use wdl_ast::Diagnostic;
use wdl_ast::Severity;
use wdl_ast::SyntaxNode;
use wdl_engine::Engine;
use wdl_engine::InputsFile;

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
    let s = s.replace('\\', "/").replace("\r\n", "\n");

    // Handle any OS specific errors messages
    s.replace(
        "The system cannot find the file specified. (os error 2)",
        "No such file or directory (os error 2)",
    )
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
            "result is not as expected:\n{}",
            StrComparison::new(&expected, &result),
        );
    }

    Ok(())
}

/// Runts the test given the provided analysis result.
fn run_test(test: &Path, result: AnalysisResult) -> Result<()> {
    let cwd = std::env::current_dir().expect("must have a CWD");
    let mut buffer = Buffer::no_color();

    // Attempt to strip the CWD from the result path
    let path = result.uri().to_file_path();
    let path: Cow<'_, str> = match &path {
        // Strip the CWD from the path
        Ok(path) => path.strip_prefix(&cwd).unwrap_or(path).to_string_lossy(),
        // Use the id itself if there is no path
        Err(_) => result.uri().as_str().into(),
    };

    let diagnostics: Cow<'_, [Diagnostic]> = match result.parse_result().error() {
        Some(e) => vec![Diagnostic::error(format!("failed to read `{path}`: {e:#}"))].into(),
        None => result.diagnostics().into(),
    };

    if let Some(diagnostic) = diagnostics.iter().find(|d| d.severity() == Severity::Error) {
        let source = result
            .parse_result()
            .root()
            .map(|n| SyntaxNode::new_root(n.clone()).text().to_string())
            .unwrap_or_default();
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

    let mut engine = Engine::default();
    let document = result.document();
    let result = match InputsFile::parse(engine.types_mut(), document, test.join("inputs.json")) {
        Ok(inputs) => {
            if let Some((task, inputs)) = inputs.as_task_inputs() {
                match inputs
                    .validate(
                        engine.types_mut(),
                        document,
                        document.task_by_name(task).expect("task should be present"),
                    )
                    .with_context(|| format!("failed to validate the inputs to task `{task}`"))
                {
                    Ok(()) => String::new(),
                    Err(e) => format!("{e:?}"),
                }
            } else if let Some(inputs) = inputs.as_workflow_inputs() {
                let workflow = document.workflow().expect("workflow should be present");
                match inputs
                    .validate(engine.types_mut(), document, workflow)
                    .with_context(|| {
                        format!(
                            "failed to validate the inputs to workflow `{workflow}`",
                            workflow = workflow.name()
                        )
                    }) {
                    Ok(()) => String::new(),
                    Err(e) => format!("{e:?}"),
                }
            } else {
                panic!("expected either a task input or a workflow input");
            }
        }
        Err(e) => format!("{e:?}"),
    };

    let output = test.join("error.txt");
    compare_result(&output, &result)
}

#[tokio::main]
async fn main() {
    let tests = find_tests();
    println!("\nrunning {} tests\n", tests.len());

    // Start with a single analysis pass over all the test files
    let analyzer = Analyzer::new(rules(), |_, _, _, _| async {});
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

    let mut errors = Vec::new();
    for test in &tests {
        let test_name = test.file_stem().and_then(OsStr::to_str).unwrap();

        // Discover the results that are relevant only to this test
        let base = clean(absolute(test).expect("should be made absolute"));

        let mut results = results.iter().filter_map(|r| {
            if r.uri().to_file_path().ok()?.starts_with(&base) {
                Some(r.clone())
            } else {
                None
            }
        });

        let result = results.next().expect("should have a result");
        if results.next().is_some() {
            println!("test {test_name} ... {failed}", failed = "failed".red());
            errors.push((
                test_name,
                "more than one WDL file was in the test directory".to_string(),
            ));
            continue;
        }

        match run_test(test, result) {
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
}
