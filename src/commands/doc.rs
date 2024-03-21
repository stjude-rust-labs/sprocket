use std::path::PathBuf;

use clap::Parser;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;
use codespan_reporting::term::Config;
use codespan_reporting::term::DisplayStyle;
use sprocket::file::Repository;
use tracing::info;
use tracing::warn;

/// Arguments for the `doc` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The output directory for the documentation.
    #[arg(short, long, default_value = "./docs")]
    output: PathBuf,

    /// Whether to overwrite the output directory if it already exists.
    #[arg(short, long, default_value = "false")]
    force: bool,
}

pub fn doc(args: Args) -> anyhow::Result<()> {
    // Set up the configuration for the terminal.
    let config = Config {
        display_style: DisplayStyle::Short,
        ..Default::default()
    };
    let writer = StandardStream::stderr(ColorChoice::Auto);

    // Create a new repository and report any concerns.
    // This will include lint warnings, although we don't care about them here.
    // If any errors are encountered, we'll exit with a non-zero status code.
    // (So far, this is the same as the `lint` subcommand, except we've hardcoded
    // the `display_style` to `Short`.)
    info!("Attempting to create a new repository.");
    let mut repo = Repository::try_new(vec![PathBuf::from("./")], vec!["wdl".to_string()])?;
    info!("Reporting any concerns.");
    if repo.report_concerns(config, writer)? {
        warn!("Can't document a repository with errors!");
        warn!("Please fix the errors above and try again. (Any lint warnings can be ignored.)");
        warn!("You can use the `lint` subcommand for more detailed reporting.");
        std::process::exit(1);
    }

    // From here on out, we can assume that the repo will be error-free.
    // We do not care about lint warnings, so we can ignore all concerns.
    info!("Generating documentation.");
    repo.generate_docs(&args.output, args.force)?;

    Ok(())
}
