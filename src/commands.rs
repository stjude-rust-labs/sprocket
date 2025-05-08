//! Implementation of sprocket CLI commands.

use clap::Subcommand;

pub mod analyzer;
pub mod check;
pub mod completions;
pub mod config;
pub mod explain;
pub mod format;
pub mod run;
pub mod validate;

/// Represents the available commands for the Sprocket CLI.
#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Commands {
    /// Checks a WDL document (or a directory containing WDL documents) and
    /// reports diagnostics.
    Check(check::CheckArgs),

    /// Lints Workflow Description Language files.
    Lint(check::LintArgs),

    /// Explains a rule.
    Explain(explain::Args),

    /// Runs the analyzer LSP server.
    Analyzer(analyzer::AnalyzerArgs),

    /// Formats a WDL document.
    #[clap(alias = "fmt")]
    Format(format::FormatArgs),

    /// Validates an input JSON or YAML file against a task or workflow input
    /// schema.
    ///
    /// This ensures that every required input is supplied, every supplied input
    /// is correctly typed, that no extraneous inputs are provided, and that any
    /// provided `File` or `Directory` inputs exist.
    ///
    /// It will not catch potential runtime errors that
    /// may occur when running the task or workflow.
    ValidateInputs(validate::ValidateInputsArgs),

    /// Display the effective configuration.
    Config(config::ConfigArgs),

    /// Generates shell completions.
    Completions(commands::completions::Args),
}
