//! Index creation and management for run outputs.

use std::path::Path;
use std::path::PathBuf;

use anyhow::Result;
use anyhow::anyhow;
use uuid::Uuid;
use wdl::engine::Outputs;
use wdl::engine::PrimitiveValue;
use wdl::engine::Value;

use crate::system::v1::fs::OutputDirectory;
use crate::system::v1::db::Database;
use crate::system::v1::fs::RunDirectory;

/// Files to always symlink from execution directory to index directory.
const DEFAULT_SYMLINK_FILES: &[&str] = &["outputs.json"];

/// Create or replace a symlink using relative paths for portability.
pub fn create_or_resymlink(link: &Path, target: &Path) -> Result<()> {
    if link.exists() && link.is_symlink() {
        std::fs::remove_file(link)
            .or_else(|_| std::fs::remove_dir_all(link))
            .map_err(|e| {
                anyhow!(
                    "failed to remove existing symlink `{}` ({})",
                    link.display(),
                    e
                )
            })?;
    }

    let link_parent = link
        .parent()
        .ok_or_else(|| anyhow!("link path `{}` has no parent directory", link.display()))?;

    let relative_target = pathdiff::diff_paths(target, link_parent).ok_or_else(|| {
        anyhow!(
            "cannot create relative path from `{}` to `{}`",
            target.display(),
            link.display()
        )
    })?;

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&relative_target, link).map_err(|e| {
            anyhow!(
                "failed to create symlink `{}` -> `{}` ({})",
                link.display(),
                relative_target.display(),
                e
            )
        })?;
    }

    #[cfg(windows)]
    {
        if target.is_dir() {
            std::os::windows::fs::symlink_dir(&relative_target, link).map_err(|e| {
                anyhow!(
                    "failed to create directory symlink `{}` -> `{}` ({})",
                    link.display(),
                    relative_target.display(),
                    e
                )
            })?;
        } else {
            std::os::windows::fs::symlink_file(&relative_target, link).map_err(|e| {
                anyhow!(
                    "failed to create file symlink `{}` -> `{}` ({})",
                    link.display(),
                    relative_target.display(),
                    e
                )
            })?;
        }
    }

    Ok(())
}

/// Symlink a single file and log it to the database.
async fn symlink_and_log(
    db: &dyn Database,
    run_id: Uuid,
    run_dir: &RunDirectory,
    index_path: &str,
    file_path: &Path,
) -> Result<()> {
    let file_name = file_path
        .file_name()
        .ok_or_else(|| anyhow!("invalid file path `{}`", file_path.display()))?;

    let target = run_dir.root().join(file_path);
    let link = run_dir
        .output_directory()
        .index_dir(index_path)
        .join(file_name);

    if !target.exists() {
        return Err(anyhow!("target `{}` does not exist", target.display()));
    }

    create_or_resymlink(&link, &target)?;

    let relative_link = run_dir
        .output_directory()
        .make_relative_to(&link)
        .expect("link should be within output directory");

    let relative_target = run_dir
        .output_directory()
        .make_relative_to(&target)
        .expect("target should be within output directory");

    db.create_index_log_entry(run_id, &relative_link, &relative_target)
        .await?;

    Ok(())
}

/// Create index entries for a completed run.
pub async fn create_index_entries(
    db: &dyn Database,
    run_id: Uuid,
    run_dir: &RunDirectory,
    index_path: &str,
    outputs: &Outputs,
) -> Result<()> {
    run_dir
        .output_directory()
        .ensure_index_dir(index_path)
        .map_err(|e| {
            anyhow!(
                "failed to create index directory for `{}` ({})",
                index_path,
                e
            )
        })?;

    let mut files_to_symlink: Vec<PathBuf> =
        DEFAULT_SYMLINK_FILES.iter().map(PathBuf::from).collect();

    for (_, value) in outputs.iter() {
        extract_symlink_paths(value, &mut files_to_symlink);
    }

    let mut had_errors = false;

    for file_path in files_to_symlink {
        if let Err(e) = symlink_and_log(db, run_id, run_dir, index_path, &file_path).await {
            tracing::error!(
                "failed to create index entry for `{}`: {}",
                file_path.display(),
                e
            );
            had_errors = true;
        }
    }

    if had_errors {
        return Err(anyhow!("failed to create one or more index entries"));
    }

    Ok(())
}

/// Extract file and directory paths from a WDL value that should be symlinked.
fn extract_symlink_paths(value: &Value, paths: &mut Vec<PathBuf>) {
    match value {
        Value::Primitive(PrimitiveValue::File(path)) => {
            paths.push(path.to_path_buf());
        }
        Value::Primitive(PrimitiveValue::Directory(path)) => {
            paths.push(path.to_path_buf());
        }
        Value::Compound(compound) => {
            if let Some(array) = compound.as_array() {
                for item in array.as_slice() {
                    extract_symlink_paths(item, paths);
                }
            }
        }
        _ => {}
    }
}

/// Rebuild index from database history.
pub async fn rebuild_index(db: &dyn Database, output_directory: &OutputDirectory) -> Result<()> {
    let entries = db.list_latest_index_entries().await?;

    let mut had_errors = false;

    for entry in entries {
        let index_path = PathBuf::from(&entry.link_path);
        let target_path = PathBuf::from(&entry.target_path);

        let link = output_directory.root().join(&index_path);
        let target = output_directory.root().join(&target_path);

        // Create parent directory for link if needed
        if let Some(parent) = link.parent()
            && let Err(e) = std::fs::create_dir_all(parent).map_err(|e| {
                anyhow!(
                    "failed to create parent directory `{}` ({})",
                    parent.display(),
                    e
                )
            })
        {
            tracing::error!(
                "failed to rebuild index entry for `{}`: {}",
                index_path.display(),
                e
            );
            had_errors = true;
            continue;
        }

        // Check if target exists
        if !target.exists() {
            tracing::warn!(
                "skipping index entry for `{}`: target `{}` does not exist",
                index_path.display(),
                target.display()
            );
            continue;
        }

        // Create or replace symlink
        if let Err(e) = create_or_resymlink(&link, &target) {
            tracing::error!(
                "failed to rebuild index entry for `{}`: {}",
                index_path.display(),
                e
            );
            had_errors = true;
            continue;
        }
    }

    if had_errors {
        return Err(anyhow!("failed to rebuild one or more index entries"));
    }

    Ok(())
}
