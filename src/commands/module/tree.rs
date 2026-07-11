//! `sprocket dev module tree` and `sprocket dev module list`.

use std::collections::BTreeSet;

use clap::Parser;
use wdl_modules::Lockfile;
use wdl_modules::dependency::GitSelector;
use wdl_modules::lockfile::DependencyMap;
use wdl_modules::lockfile::ResolvedSource;

use crate::commands::CommandResult;
use crate::commands::module::Locator;
use crate::commands::module::discover;
use crate::commands::module::require_lockfile;
use crate::commands::module::trace_project;
use crate::config::Config;

/// Arguments to `sprocket dev module tree`.
#[derive(Parser, Debug)]
pub struct TreeArgs {
    /// Maximum depth to print (`0` prints only the root module).
    #[arg(long)]
    pub depth: Option<usize>,

    /// Shared module locator.
    #[command(flatten)]
    pub locator: Locator,
}

/// Arguments to `sprocket dev module list`.
#[derive(Parser, Debug)]
pub struct ListArgs {
    /// Include transitive dependencies.
    #[arg(long)]
    pub all: bool,

    /// Shared module locator.
    #[command(flatten)]
    pub locator: Locator,
}

/// Runs `sprocket dev module tree`.
pub async fn tree(args: TreeArgs, _config: Config) -> CommandResult<()> {
    tracing::trace!(depth = ?args.depth, "starting `sprocket dev module tree`");
    let project = discover(&args.locator)?;
    trace_project("module tree", &project);
    let lock = require_lockfile(&project)?;
    tracing::debug!(
        dependencies = lock.dependencies.len(),
        "loaded module lockfile for tree"
    );

    println!("{}", project.manifest.name);
    if args.depth == Some(0) {
        tracing::trace!("printed root module only because depth is zero");
        return Ok(());
    }

    print_tree_level(&lock.dependencies, "", 1, args.depth);
    Ok(())
}

/// Runs `sprocket dev module list`.
pub async fn list(args: ListArgs, _config: Config) -> CommandResult<()> {
    tracing::trace!(all = args.all, "starting `sprocket dev module list`");
    let project = discover(&args.locator)?;
    trace_project("module list", &project);
    let lock = require_lockfile(&project)?;

    let rows = if args.all {
        let mut rows = BTreeSet::new();
        collect_all_rows(&lock, &mut rows);
        rows.into_iter().collect::<Vec<_>>()
    } else {
        lock.dependencies
            .iter()
            .map(|(name, entry)| (name.manifest().to_string(), source_desc(&entry.source)))
            .collect::<Vec<_>>()
    };
    tracing::debug!(
        rows = rows.len(),
        all = args.all,
        "printing dependency list"
    );

    print_rows(rows);
    Ok(())
}

fn print_tree_level(entries: &DependencyMap, prefix: &str, depth: usize, max_depth: Option<usize>) {
    if max_depth.is_some_and(|d| depth > d) {
        return;
    }

    let total = entries.len();
    for (index, (name, entry)) in entries.iter().enumerate() {
        let is_last = index + 1 == total;
        let connector = if is_last { "└── " } else { "├── " };
        println!(
            "{prefix}{connector}{} ({})",
            name.manifest(),
            source_desc(&entry.source)
        );

        if max_depth.is_some_and(|d| depth >= d) {
            continue;
        }

        let next_prefix = if is_last {
            format!("{prefix}    ")
        } else {
            format!("{prefix}│   ")
        };
        print_tree_level(&entry.dependencies, &next_prefix, depth + 1, max_depth);
    }
}

fn source_desc(source: &ResolvedSource) -> String {
    match source {
        ResolvedSource::Git {
            git,
            sha,
            path,
            selector,
        } => {
            let short = &sha.as_str()[..7.min(sha.as_str().len())];
            let selector = selector_text(selector);
            let mut parts = vec![
                format!("source: {git}"),
                format!("selector: {selector} @{short}"),
            ];
            if let Some(path) = path {
                parts.push(format!("path: {path}"));
            }
            format!("({})", parts.join(", "))
        }
        ResolvedSource::Path { path } => format!("(source: {})", path.display()),
    }
}

fn selector_text(selector: &GitSelector) -> String {
    match selector {
        GitSelector::Version(requirement) => format!("version `{requirement}`"),
        GitSelector::Tag(tag) => format!("tag `{tag}`"),
        GitSelector::Branch(branch) => format!("branch `{branch}`"),
        GitSelector::Commit(commit) => format!("commit `{commit}`"),
    }
}

fn collect_all_rows(lock: &Lockfile, rows: &mut BTreeSet<(String, String)>) {
    collect_rows_from_map(&lock.dependencies, rows);
}

fn collect_rows_from_map(entries: &DependencyMap, rows: &mut BTreeSet<(String, String)>) {
    for (name, entry) in entries {
        rows.insert((name.manifest().to_string(), source_desc(&entry.source)));
        collect_rows_from_map(&entry.dependencies, rows);
    }
}

fn print_rows(rows: Vec<(String, String)>) {
    let alias_header = "name";
    let source_header = "source";
    let alias_width = rows
        .iter()
        .map(|(alias, _)| alias.len())
        .max()
        .unwrap_or(0)
        .max(alias_header.len());

    println!("{:<alias_width$}  {}", alias_header, source_header);
    for (alias, source) in rows {
        println!("{:<alias_width$}  {}", alias, source);
    }
}
