//! Reporting.

use anyhow::Context;
use anyhow::Result;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::emit;
use codespan_reporting::term::termcolor::StandardStream;
use codespan_reporting::term::Config;
use wdl::grammar::Diagnostic;

/// A reporter for Sprocket.
#[derive(Debug)]
pub(crate) struct Reporter {
    /// The configuration.
    config: Config,

    /// The stream to write to.
    stream: StandardStream,
}

impl Reporter {
    /// Creates a new [`Reporter`].
    pub(crate) fn new(config: Config, stream: StandardStream) -> Self {
        Self { config, stream }
    }

    /// Reports diagnostics to the terminal.
    pub(crate) fn emit_diagnostics(
        &mut self,
        file: SimpleFile<String, String>,
        diagnostics: &[Diagnostic],
    ) -> Result<()> {
        for diagnostic in diagnostics.iter() {
            emit(
                &mut self.stream,
                &self.config,
                &file,
                &diagnostic.to_codespan(),
            )
            .context("failed to emit diagnostic")?;
        }

        Ok(())
    }
}
