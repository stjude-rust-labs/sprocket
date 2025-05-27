//! Implementation of the `doc` command.

use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;

use anyhow::Ok;
use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use wdl::doc::document_workspace;

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
    /// with a `main.css` file. `npm install` should be run in the theme
    /// directory **prior to running this command** to install the dependencies.
    #[arg(long, value_name = "DIR")]
    pub theme: Option<PathBuf>,
}

/// Build a stylesheet for the documentation, given a path to the theme
/// directory.
pub fn build_stylesheet(theme_dir: &Path) -> Result<PathBuf> {
    let theme_dir = absolute(theme_dir)?;
    let output = std::process::Command::new("npx")
        .arg("@tailwindcss/cli")
        .arg("-i")
        .arg("src/main.css")
        .arg("-o")
        .arg("dist/style.css")
        .current_dir(&theme_dir)
        .output()?;
    if !output.status.success() {
        bail!(
            "failed to build stylesheet: {stderr}",
            stderr = String::from_utf8_lossy(&output.stderr)
        );
    }
    let css_path = theme_dir.join("dist/style.css");
    if !css_path.exists() {
        bail!("failed to build stylesheet: no output file found");
    }

    Ok(css_path)
}

/// The default output directory for the generated documentation.
const DEFAULT_OUTPUT_DIR: &str = "docs";

/// Generate documentation for a WDL workspace.
pub async fn doc(args: Args) -> anyhow::Result<()> {
    let css = args
        .theme
        .as_ref()
        .map(|theme| build_stylesheet(theme))
        .transpose()?;

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
