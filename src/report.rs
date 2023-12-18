//! Reporting.

use codespan_reporting::diagnostic::Diagnostic;
use codespan_reporting::diagnostic::Label;
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term;
use codespan_reporting::term::termcolor::StandardStream;
use codespan_reporting::term::Config;
use codespan_reporting::term::DisplayStyle;
use wdl::core::concern::Concern;

/// A reporter for Sprocket.
#[derive(Debug)]
pub(crate) struct Reporter<'a> {
    /// The configuration.
    config: Config,

    /// The stream to write to.
    stream: StandardStream,

    /// The file repository.
    files: &'a SimpleFiles<String, String>,
}

impl<'a> Reporter<'a> {
    /// Creates a new [`Reporter`].
    pub(crate) fn new(
        config: Config,
        stream: StandardStream,
        files: &'a SimpleFiles<String, String>,
    ) -> Self {
        Self {
            config,
            stream,
            files,
        }
    }

    /// Reports a concern to the terminal.
    pub(crate) fn report_concern(&mut self, concern: &Concern, handle: usize) {
        let diagnostic = match concern {
            Concern::LintWarning(warning) => {
                let mut diagnostic = Diagnostic::warning()
                    .with_code(format!(
                        "{}::{}/{:?}",
                        warning.code(),
                        warning.group(),
                        warning.level()
                    ))
                    .with_message(warning.subject());

                for location in warning.locations() {
                    // SAFETY: if `report` is called, then the location **must**
                    // fall within the provided file's contents. As such, it will
                    // never be [`Location::Unplaced`], and this will always unwrap.
                    let byte_range = location.byte_range().unwrap();

                    diagnostic = diagnostic.with_labels(vec![
                        Label::primary(handle, byte_range).with_message(warning.body()),
                    ]);
                }

                if let Some(fix) = warning.fix() {
                    diagnostic = diagnostic.with_notes(vec![format!("fix: {}", fix)]);
                }

                diagnostic
            }
            Concern::ParseError(error) => {
                // SAFETY: if `report` is called, then the location **must**
                // fall within the provided file's contents. As such, it will
                // never be [`Location::Unplaced`], and this will always unwrap.
                let byte_range = error.byte_range().unwrap();

                let diagnostic = match &self.config.display_style {
                    DisplayStyle::Rich => Diagnostic::error()
                        .with_message("parse error")
                        .with_labels(vec![
                            Label::primary(handle, byte_range).with_message(error.message()),
                        ]),
                    _ => Diagnostic::error()
                        .with_message(error.message().to_lowercase())
                        .with_labels(vec![
                            Label::primary(handle, byte_range).with_message(error.message()),
                        ]),
                };

                diagnostic
            }
            Concern::ValidationFailure(failure) => {
                let mut diagnostic = Diagnostic::error()
                    .with_code(failure.code().to_string())
                    .with_message(failure.subject());

                for location in failure.locations() {
                    // SAFETY: if `report` is called, then the location **must**
                    // fall within the provided file's contents. As such, it will
                    // never be [`Location::Unplaced`], and this will always unwrap.
                    let byte_range = location.byte_range().unwrap();

                    diagnostic = diagnostic.with_labels(vec![
                        Label::primary(handle, byte_range).with_message(failure.body()),
                    ]);
                }

                if let Some(fix) = failure.fix() {
                    diagnostic = diagnostic.with_notes(vec![format!("fix: {}", fix)]);
                }

                diagnostic
            }
        };

        // SAFETY: for use on the command line, this should always succeed.
        term::emit(
            &mut self.stream.lock(),
            &self.config,
            self.files,
            &diagnostic,
        )
        .expect("writing diagnostic to stream failed")
    }
}
