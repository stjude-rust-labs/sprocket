//! Index creation and management for workflow outputs.

use std::path::Path;
use std::path::PathBuf;

use anyhow::Result;
use anyhow::anyhow;
use indexmap::IndexMap;
use uuid::Uuid;
use wdl::engine::PrimitiveValue;
use wdl::engine::Value;

use crate::OutputDirectory;
use crate::database::Database;

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
    workflow_id: Uuid,
    output_directory: &OutputDirectory,
    workflow_name: &str,
    index_path: &str,
    file_path: &Path,
) -> Result<()> {
    let file_name = file_path
        .file_name()
        .ok_or_else(|| anyhow!("invalid file path `{}`", file_path.display()))?;

    let target = output_directory.workflow_run(workflow_name).join(file_path);
    let link = output_directory.index_dir(index_path).join(file_name);

    if !target.exists() {
        return Err(anyhow!("target `{}` does not exist", target.display()));
    }

    create_or_resymlink(&link, &target)?;

    // SAFETY: both `link` and `target` are constructed from
    // `output_directory.root()`, so `strip_prefix` will always succeed.
    let relative_link = link
        .strip_prefix(output_directory.root())
        .unwrap()
        .to_str()
        .ok_or_else(|| anyhow!("path `{}` contains invalid UTF-8", link.display()))?
        .to_string();

    let relative_target = target
        .strip_prefix(output_directory.root())
        .unwrap()
        .to_str()
        .ok_or_else(|| anyhow!("path `{}` contains invalid UTF-8", target.display()))?
        .to_string();

    db.create_index_log_entry(workflow_id, relative_link, relative_target)
        .await?;

    Ok(())
}

/// Create index entries for a completed workflow.
pub async fn create_index_entries(
    db: &dyn Database,
    workflow_id: Uuid,
    output_directory: &OutputDirectory,
    workflow_name: &str,
    index_path: &str,
    outputs: &IndexMap<String, Value>,
) -> Result<()> {
    // Ensure index directory exists
    output_directory.ensure_index_dir(index_path).map_err(|e| {
        anyhow!(
            "failed to create index directory for `{}` ({})",
            index_path,
            e
        )
    })?;

    let mut files_to_symlink: Vec<PathBuf> =
        DEFAULT_SYMLINK_FILES.iter().map(PathBuf::from).collect();

    for (_, value) in outputs {
        extract_symlink_paths(value, &mut files_to_symlink);
    }

    let mut had_errors = false;

    for file_path in files_to_symlink {
        if let Err(e) = symlink_and_log(
            db,
            workflow_id,
            output_directory,
            workflow_name,
            index_path,
            &file_path,
        )
        .await
        {
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
        let index_path = PathBuf::from(&entry.index_path);
        let target_path = PathBuf::from(&entry.target_path);

        let link = output_directory.root().join(&index_path);
        let target = output_directory.root().join(&target_path);

        let result: Result<()> = {
            // Create parent directory for link if needed
            if let Some(parent) = link.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    anyhow!(
                        "failed to create parent directory `{}` ({})",
                        parent.display(),
                        e
                    )
                })?;
            }

            // Check if target exists
            if !target.exists() {
                return Err(anyhow!("target `{}` does not exist", target.display()));
            }

            // Create or replace symlink
            create_or_resymlink(&link, &target)?;

            Ok(())
        };

        if let Err(e) = result {
            tracing::error!(
                "failed to rebuild index entry for `{}`: {}",
                index_path.display(),
                e
            );
            had_errors = true;
        }
    }

    if had_errors {
        return Err(anyhow!("failed to rebuild one or more index entries"));
    }

    Ok(())
}
