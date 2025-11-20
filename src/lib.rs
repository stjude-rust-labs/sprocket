//! [The Sprocket command line tool](https://sprocket.bio/).
//!
//! This library crate only exports the items necessary to build the `sprocket`
//! binary crate and associated integration tests. It is not meant to be used by
//! any other crates.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::io::IsTerminal as _;
use std::io::stderr;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use clap::CommandFactory as _;
use clap::Parser as _;
use clap_verbosity_flag::Verbosity;
use clap_verbosity_flag::WarnLevel;
use colored::Colorize as _;
use commands::Commands;
pub use config::Config;
use git_testament::git_testament;
use git_testament::render_testament;
use tracing::trace;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt as _;

mod analysis;
mod commands;
mod config;
pub mod database;
mod diagnostics;
mod eval;
pub mod execution;
mod inputs;
pub mod provenance;
pub mod server;

pub use database::Database;

use crate::execution::RunDirectory;

/// Subdirectory name for workflow execution runs.
const RUNS_DIR: &str = "runs";

/// Subdirectory name for the provenance index.
const INDEX_DIR: &str = "index";

/// Root directory for all workflow outputs and indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDirectory(PathBuf);

impl OutputDirectory {
    /// Create a new output directory.
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self(root.as_ref().to_path_buf())
    }

    /// Get the workflow execution directory for a given workflow name.
    pub fn workflow_run(&self, workflow_name: &str) -> RunDirectory {
        RunDirectory::new(self.clone(), workflow_name)
    }

    /// Constructs a workflow directory and then ensure that it exists.
    pub fn ensure_workflow_run(&self, workflow_name: &str) -> std::io::Result<RunDirectory> {
        let dir = self.workflow_run(workflow_name);
        std::fs::create_dir_all(dir.root())?;
        Ok(dir)
    }

    /// Get the index directory for a given index path.
    pub fn index_dir(&self, index_path: &str) -> PathBuf {
        self.0.join(INDEX_DIR).join(index_path)
    }

    /// Get the index directory and ensure it exists.
    pub fn ensure_index_dir(&self, index_path: &str) -> std::io::Result<PathBuf> {
        let path = self.index_dir(index_path);
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }

    /// Get the root directory.
    pub fn root(&self) -> &Path {
        &self.0
    }

    /// Convert an absolute path to a relative path within the output directory.
    ///
    /// Returns `Some` with a path starting with `./` if the path is within the
    /// output directory, or `None` if the path is not within the output
    /// directory.
    pub fn make_relative_to(&self, path: impl AsRef<Path>) -> Option<String> {
        let path = path.as_ref();
        path.strip_prefix(&self.0)
            .ok()
            .map(|p| format!("./{}", p.display()))
    }
}

/// ignorefile basename to respect.
const IGNORE_FILENAME: &str = ".sprocketignore";

git_testament!(TESTAMENT);

#[derive(clap::Parser, Debug)]
#[command(author, version = render_testament!(TESTAMENT), propagate_version = true, about, long_about = None)]
struct Cli {
    /// The command to execute.
    #[command(subcommand)]
    pub command: Commands,

    /// The verbosity for log messages.
    #[command(flatten)]
    verbosity: Verbosity<WarnLevel>,

    /// Path to the configuration file.
    #[arg(long, short, global = true)]
    config: Vec<PathBuf>,

    /// Skip searching for and loading configuration files.
    ///
    /// Only a configuration file specified as a command line argument will be
    /// used.
    #[arg(long, short, global = true)]
    skip_config_search: bool,
}

async fn inner() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match std::env::var("RUST_LOG") {
        Ok(_) => {
            let indicatif_layer = tracing_indicatif::IndicatifLayer::new();

            let subscriber = tracing_subscriber::fmt::Subscriber::builder()
                .with_env_filter(EnvFilter::from_default_env())
                .with_writer(indicatif_layer.get_stderr_writer())
                .with_ansi(stderr().is_terminal())
                .finish()
                .with(indicatif_layer);

            tracing::subscriber::set_global_default(subscriber)?;
        }
        Err(_) => {
            let indicatif_layer = tracing_indicatif::IndicatifLayer::new();

            let subscriber = tracing_subscriber::fmt()
                .with_max_level(cli.verbosity)
                .with_writer(indicatif_layer.get_stderr_writer())
                .with_ansi(stderr().is_terminal())
                .finish()
                .with(indicatif_layer);

            tracing::subscriber::set_global_default(subscriber)?;
        }
    };

    let mut config = Config::new(
        cli.config.iter().map(PathBuf::as_path),
        cli.skip_config_search,
    )?;
    config
        .validate()
        .with_context(|| "validating provided configuration")?;

    // Write effective configuration to the log
    trace!(
        "effective configuration:\n{}",
        toml::to_string_pretty(&config).unwrap_or_default()
    );

    match cli.command {
        Commands::Analyzer(args) => commands::analyzer::analyzer(args.apply(config)).await,
        Commands::Check(args) => commands::check::check(args.apply(config)).await,
        Commands::Completions(args) => {
            let mut cmd = Cli::command();
            commands::completions::completions(args, &mut cmd).await
        }
        Commands::Config(args) => commands::config::config(args, config),
        Commands::Explain(args) => commands::explain::explain(args),
        Commands::Format(args) => commands::format::format(args.apply(config)).await,
        Commands::Inputs(args) => commands::inputs::inputs(args).await,
        Commands::Lint(args) => commands::check::lint(args.apply(config)).await,
        Commands::Run(args) => commands::run::run(args.apply(config)).await,
        Commands::Server(args) => commands::server::server(args, config).await,
        Commands::Validate(args) => commands::validate::validate(args.apply(config)).await,
        Commands::Dev(commands::DevCommands::Doc(args)) => commands::doc::doc(args).await,
        Commands::Dev(commands::DevCommands::Lock(args)) => commands::lock::lock(args).await,
    }
}

/// The Sprocket command line entrypoint.
pub async fn sprocket_main() {
    if let Err(e) = inner().await {
        eprintln!(
            "{error}: {e:?}",
            error = if std::io::stderr().is_terminal() {
                "error".red().bold()
            } else {
                "error".normal()
            }
        );
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_relative_to_within_output_dir() {
        let output_dir = OutputDirectory::new("/tmp/output");

        // Test path within output directory
        let path = Path::new("/tmp/output/runs/workflow-123");
        assert_eq!(
            output_dir.make_relative_to(path),
            Some("./runs/workflow-123".to_string())
        );

        // Test path at root of output directory
        let path = Path::new("/tmp/output");
        assert_eq!(output_dir.make_relative_to(path), Some("./".to_string()));

        // Test nested path
        let path = Path::new("/tmp/output/index/my-workflow/output.txt");
        assert_eq!(
            output_dir.make_relative_to(path),
            Some("./index/my-workflow/output.txt".to_string())
        );
    }

    #[test]
    fn make_relative_to_outside_output_dir() {
        let output_dir = OutputDirectory::new("/tmp/output");

        // Test path outside output directory
        let path = Path::new("/tmp/other/workflow");
        assert_eq!(output_dir.make_relative_to(path), None);

        // Test path at sibling directory
        let path = Path::new("/tmp/workflows/run");
        assert_eq!(output_dir.make_relative_to(path), None);
    }

    #[test]
    fn ensure_workflow_run_creates_directory() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let output_dir = OutputDirectory::new(temp.path());

        let workflow_name = "my-workflow-123";
        let run_path = output_dir.ensure_workflow_run(workflow_name).unwrap();

        // Should create `runs/my-workflow-123`
        assert!(run_path.exists());
        assert!(run_path.is_dir());
        assert_eq!(
            run_path.root(),
            temp.path().join("runs").join(workflow_name)
        );
    }

    #[test]
    fn ensure_index_dir_creates_nested_path() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let output_dir = OutputDirectory::new(temp.path());

        let nested_index = "project/sample/results";
        let index_path = output_dir.ensure_index_dir(nested_index).unwrap();

        // Should create `index/project/sample/results`
        assert!(index_path.exists());
        assert!(index_path.is_dir());
        assert_eq!(index_path, temp.path().join("index").join(nested_index));
    }

    #[test]
    fn ensure_operations_are_idempotent() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let output_dir = OutputDirectory::new(temp.path());

        // Call `ensure_workflow_run` twice
        let path1 = output_dir.ensure_workflow_run("workflow-1").unwrap();
        let path2 = output_dir.ensure_workflow_run("workflow-1").unwrap();
        assert_eq!(path1, path2);

        // Call `ensure_index_dir` twice
        let path3 = output_dir.ensure_index_dir("index-1").unwrap();
        let path4 = output_dir.ensure_index_dir("index-1").unwrap();
        assert_eq!(path3, path4);
    }

    #[test]
    fn workflow_run_and_index_dir_with_special_characters() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let output_dir = OutputDirectory::new(temp.path());

        // Unicode emoji in workflow name
        let emoji_workflow = "🚀-workflow";
        let emoji_path = output_dir.ensure_workflow_run(emoji_workflow).unwrap();
        assert!(emoji_path.exists());

        // Spaces in index name
        let spaces_index = "my index path";
        let spaces_path = output_dir.ensure_index_dir(spaces_index).unwrap();
        assert!(spaces_path.exists());
    }

    #[test]
    fn make_relative_to_with_symlinks() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let output_dir = OutputDirectory::new(temp.path());

        // Create a real directory inside output dir
        let real_dir = temp.path().join("real");
        std::fs::create_dir(&real_dir).unwrap();

        // Create a symlink to it
        let symlink = temp.path().join("link");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_dir, &symlink).unwrap();

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&real_dir, &symlink).unwrap();

        // Both should work
        assert_eq!(
            output_dir.make_relative_to(&real_dir),
            Some("./real".to_string())
        );
        assert_eq!(
            output_dir.make_relative_to(&symlink),
            Some("./link".to_string())
        );
    }
}
