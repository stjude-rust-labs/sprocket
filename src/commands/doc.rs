//! Implementation of the `doc` command.

use std::path::PathBuf;
use std::sync::mpsc;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use notify::Event;
use notify::RecursiveMode;
use notify::Result as NotifyResult;
use notify::Watcher;
use notify::recommended_watcher;
use wdl::doc::build_stylesheet;
use wdl::doc::build_web_components;
use wdl::doc::document_workspace;
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

    /// Whether to watch the theme directory for changes.
    ///
    /// Requires the `--theme` argument to be specified.
    #[arg(long)]
    pub watch: bool,
}

/// The default output directory for the generated documentation.
const DEFAULT_OUTPUT_DIR: &str = "docs";

/// Generate documentation for a WDL workspace.
pub async fn doc(args: Args) -> Result<()> {
    if args.install {
        if let Some(theme_path) = &args.theme {
            install_theme(theme_path).with_context(|| {
                format!(
                    "failed to install theme from {}",
                    theme_path.display()
                )
            })?;
        } else {
            bail!("the --install flag requires the --theme argument to be specified");
        }
    }

    if let Some(theme) = &args.theme {
        build_stylesheet(theme).with_context(|| {
            format!(
                "failed to build stylesheet for theme at {}",
                theme.display()
            )
        })?;
        build_web_components(theme).with_context(|| {
            format!(
                "failed to build web components for theme at {}",
                theme.display()
            )
        })?;
    }

    let docs_dir = args
        .output
        .unwrap_or(args.workspace.join(DEFAULT_OUTPUT_DIR));

    if args.overwrite && docs_dir.exists() {
        std::fs::remove_dir_all(&docs_dir)?;
    }

    document_workspace(&args.workspace, &docs_dir, args.homepage.clone(), args.theme.clone()).await.with_context(|| {
        format!(
            "failed to generate documentation for workspace at {}",
            args.workspace.display()
        )
    })?;

    if args.open {
        opener::open(docs_dir.join("index.html"))
            .map_err(|e| anyhow::anyhow!("failed to open documentation: {e}"))?;
    }

    if args.watch {
        if let Some(theme) = &args.theme {
            let (tx, rx) = mpsc::channel::<NotifyResult<Event>>();
            let mut watcher = recommended_watcher(tx)?;

            watcher.watch(&theme.join("src"), RecursiveMode::Recursive)?;

            println!("watching for changes in theme directory...");
            println!("press Ctrl+C to stop watching");

            loop {
                match rx.recv() {
                    Ok(Ok(Event { .. })) => {
                        println!("regenerating documentation...");
                        build_stylesheet(theme).with_context(|| {
                            format!(
                                "failed to build stylesheet for theme at {}",
                                theme.display()
                            )
                        })?;
                        build_web_components(theme).with_context(|| {
                            format!(
                                "failed to build web components for theme at {}",
                                theme.display()
                            )
                        })?;
                        document_workspace(&args.workspace, &docs_dir, args.homepage.clone(), args.theme.clone())
                            .await.with_context(|| {
                                format!(
                                    "failed to regenerate documentation for workspace at {}",
                                    args.workspace.display()
                                )
                            })?;
                        println!("done");
                    }
                    Ok(Err(e)) => eprintln!("watch error: {}", e),
                    Err(e) => eprintln!("watch error: {}", e),
                }
            }
        } else {
            bail!("the --watch flag requires the --theme argument to be specified");
        }
    }

    anyhow::Ok(())
}
