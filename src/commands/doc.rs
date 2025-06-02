//! Implementation of the `doc` command.

use std::path::PathBuf;

use anyhow::bail;
use anyhow::Ok;
use anyhow::Result;
use clap::Parser;
use wdl::doc::document_workspace;
use wdl::doc::build_stylesheet;
use wdl::doc::build_web_components;
use wdl::doc::install_theme;

/// Arguments for the `doc` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Path to the local WDL workspace to document.
    pub workspace: PathBuf,
    /// Path to a Markdown file to embed in the `<output>/index.html` file.
    #[arg(long, value_name = "MARKDOWN FILE")]
    pub homepage: Option<PathBuf>,
    /// Output directory for the generated documentation.
    /// If not specified, the documentation will be generated in
    /// `<workspace>/docs`.
    #[arg(long, value_name = "DIR")]
    pub output: Option<PathBuf>,
    /// Overwrite any existing documentation.
    ///
    /// If specified, any existing files in the output directory will be
    /// deleted. Otherwise, the command will ignore existing files.
    /// Regardless of this flag, the command will overwrite any existing
    /// files which conflict with the generated documentation.
    #[arg(long)]
    pub overwrite: bool,
    /// Open the generated documentation in the default web browser.
    #[arg(long)]
    pub open: bool,
    /// An optional path to a custom theme directory.
    ///
    /// The theme directory is expected to contain a `package.json` file with a
    /// dependency for `"@tailwindcss/cli": "^4.0.0"` and a `src` directory
    /// with a `main.css` file. It should also have a `build` script that
    /// compiles web components into a `dist/index.js` file.
    #[arg(long, value_name = "DIR")]
    pub theme: Option<PathBuf>,

    /// Install the theme if it is not already installed.
    ///
    /// Requires the `--theme` argument to be specified.
    #[arg(long)]
    pub install: bool,
}

/// The default output directory for the generated documentation.
const DEFAULT_OUTPUT_DIR: &str = "docs";

/// Generate documentation for a WDL workspace.
pub async fn doc(args: Args) -> Result<()> {
    if args.install {
        if let Some(theme_path) = &args.theme {
            install_theme(theme_path)?;
        } else {
            bail!("the --install flag requires the --theme argument to be specified");
        }
    }

    let css = args
        .theme
        .as_ref()
        .map(|theme| build_stylesheet(theme))
        .transpose()?;

    if let Some(theme) = &args.theme {
        build_web_components(theme)?;
    }

    let docs_dir = args
        .output
        .unwrap_or(args.workspace.join(DEFAULT_OUTPUT_DIR));

    document_workspace(args.workspace, &docs_dir, css, args.homepage).await?;

    if args.open {
        opener::open(docs_dir.join("index.html"))
            .map_err(|e| anyhow::anyhow!("failed to open documentation: {e}"))?;
    }

    Ok(())
}
