//! The experimental parser validation tests.
//!
//! This test looks for directories in `tests/validation`.
//!
//! Each directory is expected to contain:
//!
//! * `source.wdl` - the test input source to parse.
//! * `source.errors` - the expected set of validation errors.
//!
//! The `source.errors` file may be automatically generated or updated by
//! setting the `BLESS` environment variable when running this test.
//!
//! This test currently requires the `experimental` feature to run.

#[cfg(feature = "experimental")]
mod test {
    use std::collections::HashSet;
    use std::env;
    use std::ffi::OsStr;
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;
    use std::process::exit;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use codespan_reporting::diagnostic::Diagnostic;
    use codespan_reporting::diagnostic::Label;
    use codespan_reporting::diagnostic::LabelStyle;
    use codespan_reporting::files::SimpleFiles;
    use codespan_reporting::term;
    use codespan_reporting::term::termcolor::Buffer;
    use codespan_reporting::term::Config;
    use colored::Colorize;
    use pretty_assertions::StrComparison;
    use rayon::prelude::*;
    use wdl_ast::experimental::Document;
    use wdl_ast::experimental::Validator;

    fn find_tests() -> Vec<PathBuf> {
        // Check for filter arguments consisting of test names
        let mut filter = HashSet::new();
        for arg in std::env::args().skip_while(|a| a != "--").skip(1) {
            if !arg.starts_with('-') {
                filter.insert(arg);
            }
        }

        let mut tests: Vec<PathBuf> = Vec::new();
        for entry in Path::new("tests/validation").read_dir().unwrap() {
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

    /// Converts a `miette::LabelSpan` to a `Label`.
    fn to_label(l: miette::LabeledSpan) -> Label<usize> {
        let mut label = Label::new(
            if l.primary() {
                LabelStyle::Primary
            } else {
                LabelStyle::Secondary
            },
            0,
            l.offset()..l.offset() + l.len(),
        );

        if let Some(message) = l.label() {
            label = label.with_message(message);
        }

        label
    }

    /// Converts a `miette::Diagnostic` to a `Diagnostic`.
    fn to_diagnostic(d: &dyn miette::Diagnostic) -> Diagnostic<usize> {
        let mut diagnostic = match d.severity().unwrap_or(miette::Severity::Error) {
            miette::Severity::Advice => Diagnostic::help(),
            miette::Severity::Warning => Diagnostic::warning(),
            miette::Severity::Error => Diagnostic::error(),
        };

        if let Some(code) = d.code() {
            diagnostic = diagnostic.with_code(code.to_string());
        }

        if let Some(mut labels) = d.labels() {
            diagnostic = diagnostic.with_labels(labels.by_ref().map(to_label).collect());
        }

        diagnostic = match (d.help(), d.url()) {
            (Some(help), Some(url)) => {
                diagnostic.with_notes(vec![help.to_string(), format!("see: {url}")])
            }
            (Some(help), None) => diagnostic.with_notes(vec![help.to_string()]),
            (None, Some(url)) => diagnostic.with_notes(vec![format!("see: {url}")]),
            (None, None) => diagnostic,
        };

        diagnostic = diagnostic.with_message(d.to_string());

        diagnostic
    }

    fn format_errors<'a>(
        errors: impl Iterator<Item = &'a dyn miette::Diagnostic>,
        path: &Path,
        source: &str,
    ) -> String {
        let mut files = SimpleFiles::new();
        files.add(path.as_os_str().to_str().unwrap(), source);

        let mut buffer = Buffer::no_color();
        for error in errors {
            term::emit(
                &mut buffer,
                &Config::default(),
                &files,
                &to_diagnostic(error),
            )
            .expect("should emit");
        }

        String::from_utf8(buffer.into_inner()).expect("should be UTF-8")
    }

    fn compare_result(path: &Path, result: &str, is_error: bool) -> Result<(), String> {
        let result = normalize(result, is_error);
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
                "result is not as expected:\n{}",
                StrComparison::new(&expected, &result),
            ));
        }

        Ok(())
    }

    fn run_test(test: &Path, ntests: &AtomicUsize) -> Result<(), String> {
        let path = test.join("source.wdl");
        let source = std::fs::read_to_string(&path)
            .map_err(|e| {
                format!(
                    "failed to read source file `{path}`: {e}",
                    path = path.display()
                )
            })?
            .replace("\r\n", "\n");
        match Document::parse(&source).into_result() {
            Ok(document) => {
                let validator = Validator::default();
                let errors = match validator.validate(&document) {
                    Ok(()) => String::new(),
                    Err(errors) => format_errors(
                        errors.iter().map(|e| e.as_ref() as &dyn miette::Diagnostic),
                        &path,
                        &source,
                    ),
                };
                compare_result(&path.with_extension("errors"), &errors, true)?;
            }
            Err(errors) => {
                compare_result(
                    &path.with_extension("errors"),
                    &format_errors(
                        errors.iter().map(|e| e as &dyn miette::Diagnostic),
                        &path,
                        &source,
                    ),
                    true,
                )?;
            }
        }
        ntests.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    pub fn main() {
        let tests = find_tests();
        println!("\nrunning {} tests\n", tests.len());

        let ntests = AtomicUsize::new(0);
        let errors = tests
            .par_iter()
            .filter_map(|test| {
                let test_name = test.file_stem().and_then(OsStr::to_str).unwrap();
                match std::panic::catch_unwind(|| {
                    match run_test(test, &ntests)
                        .map_err(|e| {
                            format!("failed to run test `{path}`: {e}", path = test.display())
                        })
                        .err()
                    {
                        Some(e) => {
                            println!("test {test_name} ... {failed}", failed = "failed".red());
                            Some((test_name, e))
                        }
                        None => {
                            println!("test {test_name} ... {ok}", ok = "ok".green());
                            None
                        }
                    }
                }) {
                    Ok(result) => result,
                    Err(e) => {
                        println!(
                            "test {test_name} ... {panicked}",
                            panicked = "panicked".red()
                        );
                        Some((
                            test_name,
                            format!(
                                "test panicked: {e:?}",
                                e = e
                                    .downcast_ref::<String>()
                                    .map(|s| s.as_str())
                                    .or_else(|| e.downcast_ref::<&str>().copied())
                                    .unwrap_or("no panic message")
                            ),
                        ))
                    }
                }
            })
            .collect::<Vec<_>>();

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

        println!(
            "\ntest result: ok. {} passed\n",
            ntests.load(Ordering::SeqCst)
        );
    }
}

fn main() {
    #[cfg(feature = "experimental")]
    test::main();
}
