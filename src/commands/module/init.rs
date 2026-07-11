//! `sprocket module init`.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Context as _;
use clap::Parser;
use serde_json::Map;
use serde_json::Value;
use wdl::ast::SupportedVersion;

use crate::commands::CommandResult;
use crate::commands::module::ActionColor;
use crate::commands::module::print_action;
use crate::commands::module::write_manifest_value;
use crate::config::Config;

/// Arguments to `sprocket module init`.
#[derive(Parser, Debug)]
#[command(disable_version_flag = true)]
pub struct Args {
    /// Path to initialize the module in. Defaults to the current directory.
    pub path: Option<PathBuf>,

    /// Explicit module name. Defaults to the target directory name.
    #[arg(long)]
    pub name: Option<String>,

    /// SPDX license expression to write.
    #[arg(long)]
    pub license: Option<String>,

    /// Skip creating scaffold files (`index.wdl`, `README.md`).
    #[arg(long)]
    pub no_scaffold: bool,
}

/// Runs `sprocket module init`.
pub async fn init(args: Args, _config: Config, colorize: bool) -> CommandResult<()> {
    tracing::trace!(
        has_path = args.path.is_some(),
        has_name = args.name.is_some(),
        has_license = args.license.is_some(),
        no_scaffold = args.no_scaffold,
        "starting `sprocket module init`"
    );
    run_init(args, colorize).map_err(Into::into)
}

fn run_init(args: Args, colorize: bool) -> anyhow::Result<()> {
    let target_dir = args
        .path
        .unwrap_or(std::env::current_dir().context("reading current directory")?);
    let manifest_path = target_dir.join(wdl_modules::MANIFEST_FILENAME);
    tracing::debug!(
        target = %target_dir.display(),
        manifest = %manifest_path.display(),
        "initializing module project"
    );

    if manifest_path.exists() {
        tracing::debug!(manifest = %manifest_path.display(), "manifest already exists");
        anyhow::bail!("`{}` already exists", manifest_path.display());
    }

    let name = args.name.unwrap_or_else(|| infer_name(&target_dir));
    let license = args
        .license
        .unwrap_or_else(|| "Apache-2.0 OR MIT".to_string());

    // Build the manifest as an ordered object. `entrypoint` is omitted: it
    // defaults to `index.wdl`, which is exactly what the scaffold writes.
    // `authors` and `repository` are included only when they can be inferred.
    let mut manifest = Map::new();
    manifest.insert("name".to_string(), Value::String(name.clone()));
    manifest.insert(
        "description".to_string(),
        Value::String(format!("The `{name}` WDL module.")),
    );
    if let Some(author) = infer_author(&target_dir) {
        tracing::trace!("inferred module author from Git config");
        manifest.insert(
            "authors".to_string(),
            Value::Array(vec![Value::String(author)]),
        );
    }
    manifest.insert("license".to_string(), Value::String(license));
    if let Some(repository) = infer_repository(&target_dir) {
        tracing::trace!("inferred module repository from Git config");
        manifest.insert("repository".to_string(), Value::String(repository));
    }
    write_manifest_value(&manifest_path, &Value::Object(manifest))?;
    tracing::debug!(manifest = %manifest_path.display(), "wrote module manifest");

    if !args.no_scaffold {
        tracing::debug!("writing module scaffold files");
        write_scaffold_file(
            &target_dir.join("index.wdl"),
            "index.wdl",
            format!("version {}\n", SupportedVersion::default()),
        )?;
        write_scaffold_file(
            &target_dir.join("README.md"),
            "README.md",
            format!("# {name}\n"),
        )?;
    } else {
        tracing::debug!("skipped module scaffold files");
    }

    print_action(
        "Created",
        format!("module `{name}`"),
        colorize,
        ActionColor::Green,
    );

    Ok(())
}

/// Writes a scaffold file, leaving an existing file untouched and warning.
fn write_scaffold_file(path: &Path, label: &str, content: String) -> anyhow::Result<()> {
    if path.exists() {
        tracing::warn!(path = %path.display(), label, "skipped existing scaffold file");
        return Ok(());
    }
    std::fs::write(path, content).with_context(|| format!("writing scaffold `{label}`"))?;
    Ok(())
}

fn infer_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "module".to_string())
}

/// Infers an author entry from the local or global Git identity, formatted as
/// `Name <email>` (or just the name when no email is configured).
fn infer_author(dir: &Path) -> Option<String> {
    let name = git_config(dir, "user.name");
    let email = git_config(dir, "user.email");
    match (name, email) {
        (Some(name), Some(email)) => Some(format!("{name} <{email}>")),
        (Some(name), None) => Some(name),
        _ => None,
    }
}

/// Reads a single Git config value, returning `None` when it is unset or Git is
/// unavailable.
fn git_config(dir: &Path, key: &str) -> Option<String> {
    let output = Command::new("git")
        .arg("config")
        .arg("--get")
        .arg(key)
        .current_dir(dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn infer_repository(path: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("remote")
        .arg("get-url")
        .arg("origin")
        .current_dir(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let repository = String::from_utf8(output.stdout).ok()?;
    let repository = repository.trim();
    if repository.is_empty() {
        None
    } else {
        Some(repository.to_string())
    }
}

#[cfg(test)]
mod tests {
    use tracing_test::traced_test;

    use super::*;

    #[traced_test]
    #[test]
    fn existing_scaffold_file_warns_and_is_preserved() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("index.wdl");
        std::fs::write(&path, "version 1.0\n")?;

        write_scaffold_file(&path, "index.wdl", "version 1.3\n".to_string())?;

        assert_eq!(std::fs::read_to_string(&path)?, "version 1.0\n");
        assert!(logs_contain("skipped existing scaffold file"));
        assert!(logs_contain("index.wdl"));
        Ok(())
    }
}
