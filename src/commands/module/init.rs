//! `sprocket dev module init`.

use std::ffi::OsString;
use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Context as _;
use clap::Parser;
use serde_json::Map;
use serde_json::Value;
use wdl::ast::SupportedVersion;

use super::manifest::align_temp_permissions;
use crate::commands::CommandResult;
use crate::commands::output::Action;
use crate::commands::output::CommandOutput;

const INITIALIZE: Action = Action::new("Initialized", "initialize");

/// Arguments to `sprocket dev module init`.
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

/// Runs `sprocket dev module init`.
pub async fn init(args: Args, output: CommandOutput) -> CommandResult<()> {
    tracing::trace!(
        has_path = args.path.is_some(),
        has_name = args.name.is_some(),
        has_license = args.license.is_some(),
        no_scaffold = args.no_scaffold,
        "starting `sprocket dev module init`"
    );
    run_init(args, output).map_err(Into::into)
}

fn run_init(args: Args, output: CommandOutput) -> anyhow::Result<()> {
    let current_dir = std::env::current_dir().context("reading current directory")?;
    let target_dir = args.path.map_or_else(
        || current_dir.clone(),
        |path| {
            if path.is_absolute() {
                path
            } else {
                current_dir.join(path)
            }
        },
    );
    let manifest_path = target_dir.join(wdl_modules::MANIFEST_FILENAME);
    tracing::debug!(
        target = %target_dir.display(),
        manifest = %manifest_path.display(),
        "initializing module project"
    );

    let name = args.name.unwrap_or_else(|| infer_name(&target_dir));
    let license = args
        .license
        .unwrap_or_else(|| "Apache-2.0 OR MIT".to_string());
    validate_name_and_license(&name, &license)?;

    ensure_target_directory(&target_dir)?;
    ensure_new_file_path(&manifest_path, "manifest")?;
    if !args.no_scaffold {
        ensure_scaffold_path(&target_dir.join("index.wdl"), "index.wdl")?;
        ensure_scaffold_path(&target_dir.join("README.md"), "README.md")?;
    }

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
    write_new_manifest(&manifest_path, &Value::Object(manifest))?;
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

    output.completed(INITIALIZE, format!("module `{name}`"));
    output.detail("Manifest", manifest_path.display());
    if !args.no_scaffold {
        output.detail("Entrypoint", "index.wdl");
    }

    Ok(())
}

/// Writes a scaffold file, leaving an existing file untouched and warning.
fn write_scaffold_file(path: &Path, label: &str, content: String) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            tracing::warn!(path = %path.display(), label, "skipped existing scaffold file");
            return Ok(());
        }
        Ok(_) => anyhow::bail!("scaffold path `{}` is not a regular file", path.display()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(source).with_context(|| format!("inspecting scaffold `{label}`"));
        }
    }
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .with_context(|| format!("creating scaffold `{label}`"))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("writing scaffold `{label}`"))?;
    file.sync_all()
        .with_context(|| format!("syncing scaffold `{label}`"))?;
    Ok(())
}

/// Creates missing target components and rejects symbolic links.
fn ensure_target_directory(target: &Path) -> anyhow::Result<()> {
    let mut current = target;
    let mut missing = Vec::<OsString>::new();
    let existing = loop {
        match std::fs::symlink_metadata(current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    anyhow::bail!(
                        "module target `{}` is not a regular directory",
                        current.display()
                    );
                }
                break current.to_path_buf();
            }
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                let name = current.file_name().with_context(|| {
                    format!("module target `{}` has no directory name", target.display())
                })?;
                missing.push(name.to_os_string());
                current = current.parent().with_context(|| {
                    format!(
                        "module target `{}` has no existing parent",
                        target.display()
                    )
                })?;
            }
            Err(source) => {
                return Err(source)
                    .with_context(|| format!("inspecting module target `{}`", current.display()));
            }
        }
    };

    let mut directory = existing;
    for component in missing.into_iter().rev() {
        directory.push(component);
        match std::fs::create_dir(&directory) {
            Ok(()) => {}
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = std::fs::symlink_metadata(&directory).with_context(|| {
                    format!("inspecting module target `{}`", directory.display())
                })?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    anyhow::bail!(
                        "module target `{}` is not a regular directory",
                        directory.display()
                    );
                }
            }
            Err(source) => {
                return Err(source)
                    .with_context(|| format!("creating module target `{}`", directory.display()));
            }
        }
    }
    Ok(())
}

/// Ensures initialization will not replace an existing manifest path.
fn ensure_new_file_path(path: &Path, label: &str) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => anyhow::bail!("{label} path `{}` already exists", path.display()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => {
            Err(source).with_context(|| format!("inspecting {label} path `{}`", path.display()))
        }
    }
}

/// Ensures an existing scaffold path is a regular file.
fn ensure_scaffold_path(path: &Path, label: &str) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => anyhow::bail!("scaffold path `{}` is not a regular file", path.display()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(source).with_context(|| format!("inspecting scaffold `{label}`")),
    }
}

/// Writes a validated manifest without replacing a concurrently created file.
fn write_new_manifest(path: &Path, value: &Value) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    wdl_modules::Manifest::parse(&bytes).context("validating generated manifest")?;
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(directory)
        .with_context(|| format!("creating a temporary file in `{}`", directory.display()))?;
    temp.write_all(&bytes)
        .with_context(|| format!("writing `{}`", temp.path().display()))?;
    temp.as_file()
        .sync_all()
        .with_context(|| format!("syncing `{}`", temp.path().display()))?;
    align_temp_permissions(&temp, path)?;
    temp.persist_noclobber(path)
        .with_context(|| format!("creating `{}`", path.display()))?;
    Ok(())
}

/// Validates user-controlled manifest fields before creating a target.
fn validate_name_and_license(name: &str, license: &str) -> anyhow::Result<()> {
    let value = serde_json::json!({
        "name": name,
        "license": license,
    });
    let bytes = serde_json::to_vec(&value)?;
    wdl_modules::Manifest::parse(&bytes).context("validating generated manifest")?;
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
    (!repository.is_empty())
        .then(|| sanitize_repository(repository))
        .flatten()
}

/// Removes credentials and request metadata from an inferred repository URL.
fn sanitize_repository(repository: &str) -> Option<String> {
    let Ok(mut url) = url::Url::parse(repository) else {
        return Some(repository.to_string());
    };
    if matches!(url.scheme(), "http" | "https") {
        url.set_username("").ok()?;
        url.set_password(None).ok()?;
    } else if url.password().is_some() {
        url.set_password(None).ok()?;
    }
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
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

    #[test]
    fn strips_credentials_from_inferred_repository_urls() {
        assert_eq!(
            sanitize_repository(
                "https://token:secret@example.com/owner/repo.git?access_token=secret#fragment"
            )
            .as_deref(),
            Some("https://example.com/owner/repo.git")
        );
        assert_eq!(
            sanitize_repository("ssh://git:secret@example.com/owner/repo.git").as_deref(),
            Some("ssh://git@example.com/owner/repo.git")
        );
        assert_eq!(
            sanitize_repository("git@example.com:owner/repo.git").as_deref(),
            Some("git@example.com:owner/repo.git")
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_target_directory() -> anyhow::Result<()> {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let target = directory.path().join("module");
        symlink(outside.path(), &target)?;

        assert!(ensure_target_directory(&target).is_err());
        assert!(!outside.path().join(wdl_modules::MANIFEST_FILENAME).exists());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rejects_dangling_scaffold_symlink() -> anyhow::Result<()> {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir()?;
        let outside = directory.path().join("outside.wdl");
        let scaffold = directory.path().join("index.wdl");
        symlink(&outside, &scaffold)?;

        assert!(write_scaffold_file(&scaffold, "index.wdl", "version 1.3\n".to_string()).is_err());
        assert!(!outside.exists());
        Ok(())
    }
}
