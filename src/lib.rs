//! Package manager for Workflow Description Language (WDL) files.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::time::Duration;

use anyhow::bail;
use url::Url;
use tokio::fs;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use clap::ValueEnum;
use codespan_reporting::term::Config;
use codespan_reporting::term::DisplayStyle;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;
use wdl::analysis::AnalysisResult;
use wdl::analysis::Analyzer;
use wdl::analysis::path_to_uri;
use wdl::lint::LintVisitor;
use wdl::ast::Validator;

pub mod commands;

/// The delay in showing the progress bar.
const PROGRESS_BAR_DELAY: Duration = Duration::from_secs(2);

/// The diagnostic mode to use for reporting diagnostics.
#[derive(Clone, Copy, Debug, Default, ValueEnum, PartialEq, Eq)]
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

/// Gets the display config to use for reporting diagnostics.
fn get_display_config(report_mode: Mode, no_color: bool) -> (Config, StandardStream) {
    let display_style = match report_mode {
        Mode::Full => DisplayStyle::Rich,
        Mode::OneLine => DisplayStyle::Short,
    };

    let config = Config {
        display_style,
        ..Default::default()
    };

    let color_choice = if no_color {
        ColorChoice::Never
    } else {
        ColorChoice::Always
    };

    let writer = StandardStream::stderr(color_choice);

    (config, writer)
}

/// Analyze the document or directory, returning [`AnalysisResult`]s.
pub async fn analyze(
    file: &str,
    exceptions: &[String],
    lint: bool,
) -> anyhow::Result<Vec<AnalysisResult>> {
    let rules = wdl::analysis::rules().iter().filter_map(|rule| {
        if exceptions.iter().any(|e| e == rule.id()) {
            None
        } else {
            Some(rule)
        }
    });

    let analyzer = Analyzer::new_with_validator(
        rules,
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
                    if exceptions.iter().any(|e| e == rule.id()) {
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

    if let Ok(url) = Url::parse(&file) {
        analyzer.add_document(url).await?;
    } else if fs::metadata(&file)
        .await?
        .is_dir()
    {
        analyzer.add_directory(file.clone().into()).await?;
    } else if let Some(url) = path_to_uri(&file) {
        analyzer.add_document(url).await?;
    } else {
        bail!("failed to convert `{file}` to a URI", file = file)
    }

    let bar = ProgressBar::new(0);
    bar.set_style(
        ProgressStyle::with_template("[{elapsed_precise}] {bar:40.cyan/blue} {msg} {pos}/{len}")
            .unwrap(),
    );

    let results = analyzer
        .analyze(bar.clone())
        .await?;

    // Drop (hide) the progress bar before emitting any diagnostics
    drop(bar);

    anyhow::Ok(results)
}
