//! Implementation of the check and lint commands.

use std::borrow::Cow;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use anyhow::bail;
use clap::Parser;
use clap::ValueEnum;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::Config;
use codespan_reporting::term::DisplayStyle;
use codespan_reporting::term::emit;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use wdl::analysis::Analyzer;
use wdl::ast::Diagnostic;
use wdl::ast::Severity;
use wdl::ast::SyntaxNode;
use wdl::ast::Validator;
use wdl::lint::LintVisitor;

/// The delay in showing the progress bar.
const PROGRESS_BAR_DELAY: Duration = Duration::from_secs(2);

/// The diagnostic mode to use for reporting diagnostics.
#[derive(Clone, Debug, Default, ValueEnum)]
pub enum Mode {
    /// Prints diagnostics as multiple lines.
    #[default]
    Full,

    /// Prints diagnostics as one line.
    OneLine,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Full => write!(f, "full"),
            Mode::OneLine => write!(f, "one-line"),
        }
    }
}

/// Common arguments for the `check` and `lint` subcommands.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Common {
    /// The files or directories to check.
    #[arg(required = true)]
    paths: Vec<PathBuf>,

    /// Lint rules to except from running.
    #[arg(short, long, value_name = "RULE")]
    except: Vec<String>,

    /// Disables color output.
    #[arg(long)]
    no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, default_value_t, value_name = "MODE")]
    report_mode: Mode,
}

/// Arguments for the `check` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct CheckArgs {
    /// The common command line arguments.
    #[command(flatten)]
    common: Common,

    /// Perform lint checks in addition to checking for errors.
    #[arg(short, long)]
    lint: bool,

    /// Causes the command to fail if warnings were reported.
    #[clap(long)]
    deny_warnings: bool,

    /// Causes the command to fail if notes were reported.
    #[clap(long)]
    deny_notes: bool,
}

/// Arguments for the `lint` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct LintArgs {
    /// The command command line arguments.
    #[command(flatten)]
    common: Common,

    /// Causes the command to fail if warnings were reported.
    #[clap(long)]
    deny_warnings: bool,

    /// Causes the command to fail if notes were reported.
    #[clap(long)]
    deny_notes: bool,
}

/// Checks WDL source files for diagnostics.
pub async fn check(args: CheckArgs) -> anyhow::Result<()> {
    if !args.lint && !args.common.except.is_empty() {
        bail!("cannot specify `--except` without `--lint`");
    }

    let (config, mut stream) = get_display_config(&args.common);

    let lint = args.lint;
    let except_rules = args.common.except;
    let analyzer = Analyzer::new_with_validator(
        move |bar: ProgressBar, kind, completed, total| async move {
            if bar.elapsed() < PROGRESS_BAR_DELAY {
                return;
            }

            if completed == 0 || bar.length() == Some(0) {
                bar.set_length(total.try_into().unwrap());
                bar.set_message(format!("{kind}"));
            }

            bar.set_position(completed.try_into().unwrap());
        },
        move || {
            let mut validator = Validator::empty();

            if lint {
                let visitor = LintVisitor::new(wdl::lint::rules().into_iter().filter_map(|rule| {
                    if except_rules.contains(&rule.id().to_string()) {
                        None
                    } else {
                        Some(rule)
                    }
                }));
                validator.add_visitor(visitor);
            }

            validator
        },
    );

    analyzer.add_documents(args.common.paths).await?;

    let bar = ProgressBar::new(0);
    bar.set_style(
        ProgressStyle::with_template("[{elapsed_precise}] {bar:40.cyan/blue} {msg} {pos}/{len}")
            .unwrap(),
    );

    let results = analyzer
        .analyze(bar.clone())
        .await
        .context("failed to analyze documents")?;

    // Drop (hide) the progress bar before emitting any diagnostics
    drop(bar);

    let cwd = std::env::current_dir().ok();
    let mut error_count = 0;
    let mut warning_count = 0;
    let mut note_count = 0;
    for result in &results {
        let path = result.uri().to_file_path().ok();

        // Attempt to strip the CWD from the result path
        let path = match (&cwd, &path) {
            // Only display diagnostics for local files.
            (_, None) => continue,
            // Use just the path if there's no CWD
            (None, Some(path)) => path.to_string_lossy(),
            // Strip the CWD from the path
            (Some(cwd), Some(path)) => path.strip_prefix(cwd).unwrap_or(path).to_string_lossy(),
        };

        let diagnostics: Cow<'_, [Diagnostic]> = match result.parse_result().error() {
            Some(e) => vec![Diagnostic::error(format!("failed to read `{path}`: {e:#}"))].into(),
            None => result.diagnostics().into(),
        };

        if !diagnostics.is_empty() {
            let source = result
                .parse_result()
                .root()
                .map(|n| SyntaxNode::new_root(n.clone()).text().to_string())
                .unwrap_or(String::new());
            let file = SimpleFile::new(path, source);
            for diagnostic in diagnostics.iter() {
                match diagnostic.severity() {
                    Severity::Error => error_count += 1,
                    Severity::Warning => warning_count += 1,
                    Severity::Note => note_count += 1,
                }

                emit(&mut stream, &config, &file, &diagnostic.to_codespan())
                    .context("failed to emit diagnostic")?;
            }
        }
    }

    if error_count > 0 {
        bail!(
            "aborting due to previous {error_count} error{s}",
            s = if error_count == 1 { "" } else { "s" }
        );
    } else if args.deny_warnings && warning_count > 0 {
        bail!(
            "aborting due to previous {warning_count} warning{s} (`--deny-warnings` was specified)",
            s = if warning_count == 1 { "" } else { "s" }
        );
    } else if args.deny_notes && note_count > 0 {
        bail!(
            "aborting due to previous {note_count} note{s} (`--deny-notes` was specified)",
            s = if note_count == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

/// Lints WDL source files.
pub async fn lint(args: LintArgs) -> anyhow::Result<()> {
    check(CheckArgs {
        common: args.common,
        lint: true,
        deny_warnings: args.deny_warnings,
        deny_notes: args.deny_notes,
    })
    .await
}

/// Gets the display config to use for reporting diagnostics.
fn get_display_config(args: &Common) -> (Config, StandardStream) {
    let display_style = match args.report_mode {
        Mode::Full => DisplayStyle::Rich,
        Mode::OneLine => DisplayStyle::Short,
    };

    let config = Config {
        display_style,
        ..Default::default()
    };

    let color_choice = if args.no_color {
        ColorChoice::Never
    } else {
        ColorChoice::Always
    };

    let writer = StandardStream::stderr(color_choice);

    (config, writer)
}
