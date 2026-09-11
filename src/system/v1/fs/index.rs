//! Index creation and management for run outputs.

use std::fmt;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Context as _;
use anyhow::Result;
use anyhow::anyhow;
use thiserror::Error;
use uuid::Uuid;
use wdl::engine::Outputs;
use wdl::engine::PrimitiveValue;
use wdl::engine::Value;

use crate::system::v1::db::Database;
use crate::system::v1::fs::OutputDirectory;
use crate::system::v1::fs::RunDirectory;

/// Files to always symlink from execution directory to index directory.
const DEFAULT_SYMLINK_FILES: &[&str] = &["outputs.json"];

/// An error encountered while validating a user-supplied index path.
#[derive(Debug, Error)]
pub enum IndexPathError {
    /// The index path did not contain any path components.
    #[error("an index path cannot be empty")]
    Empty,
    /// The index path was absolute.
    #[error(
        "index path `{0}` cannot be absolute; index paths are relative to the `index` directory \
         of the output directory"
    )]
    Absolute(String),
    /// The index path contained a `.` or `..` component.
    #[error("index path `{0}` cannot contain `.` or `..` components")]
    NotNormalized(String),
}

/// A user-supplied path within the index directory of an output directory.
///
/// Run outputs are indexed at `<output directory>/index/<index path>`, so an
/// index path is always relative and never escapes the index directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexPath(PathBuf);

impl IndexPath {
    /// Gets the index path as a [`Path`].
    ///
    /// The returned path is relative to the index directory of an output
    /// directory.
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl FromStr for IndexPath {
    type Err = IndexPathError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut path = PathBuf::new();
        for component in Path::new(s).components() {
            match component {
                Component::Normal(component) => path.push(component),
                Component::Prefix(_) | Component::RootDir => {
                    return Err(IndexPathError::Absolute(s.to_string()));
                }
                Component::CurDir | Component::ParentDir => {
                    return Err(IndexPathError::NotNormalized(s.to_string()));
                }
            }
        }

        if path.as_os_str().is_empty() {
            return Err(IndexPathError::Empty);
        }

        Ok(Self(path))
    }
}

impl fmt::Display for IndexPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{path}", path = self.0.display())
    }
}

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

/// Resolves a run output path to an absolute path.
///
/// Relative paths are joined onto the run directory and symlinks within the
/// parent directory of the path are resolved, which allows the result to be
/// compared against the root of an output directory. The final component is
/// never resolved, so an output that is itself a symlink is indexed where it
/// was produced.
fn resolve_output_path(run_dir: &RunDirectory, path: &Path) -> Result<PathBuf> {
    let path = run_dir.root().join(path);

    let parent = path.parent().ok_or_else(|| {
        anyhow!(
            "path `{path}` has no parent directory",
            path = path.display()
        )
    })?;
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("path `{path}` has no file name", path = path.display()))?;

    let parent = std::fs::canonicalize(parent).map_err(|e| {
        anyhow!(
            "failed to resolve directory `{parent}` ({e})",
            parent = parent.display()
        )
    })?;

    Ok(parent.join(file_name))
}

/// Symlink a single run output into the index directory and log it to the
/// database.
///
/// Both `output_dir` and `target` must be absolute; `relative_target` is
/// `target` relative to `output_dir`.
async fn symlink_and_log(
    db: &dyn Database,
    run_id: Uuid,
    output_dir: &OutputDirectory,
    index_path: &IndexPath,
    target: &Path,
    relative_target: &str,
) -> Result<()> {
    let file_name = target
        .file_name()
        .ok_or_else(|| anyhow!("invalid file path `{target}`", target = target.display()))?;

    let link = output_dir.index_dir(index_path).join(file_name);

    create_or_resymlink(&link, target)?;

    let relative_link = output_dir.make_relative_to(&link).ok_or_else(|| {
        anyhow!(
            "index entry `{link}` is not within the output directory `{root}`",
            link = link.display(),
            root = output_dir.root().display()
        )
    })?;

    db.create_index_log_entry(run_id, &relative_link, relative_target)
        .await?;

    Ok(())
}

/// Create index entries for a completed run.
///
/// Returns the index directory of the run relative to the output directory.
///
/// Outputs that reside outside of the output directory (e.g. a `File` input
/// passed straight through to an output) cannot be recorded relative to the
/// output directory, which the provenance database and the index symlinks
/// require; such outputs are logged and left out of the index.
pub async fn create_index_entries(
    db: &dyn Database,
    run_id: Uuid,
    run_dir: &RunDirectory,
    index_path: &IndexPath,
    outputs: &Outputs,
) -> Result<String> {
    // The root of the output directory may be relative (e.g. `./out`) while the
    // paths of the outputs are absolute, so resolve the root before comparing
    // the two.
    let output_dir = run_dir.output_directory().canonicalize().with_context(|| {
        format!(
            "failed to resolve output directory `{root}`",
            root = run_dir.output_directory().root().display()
        )
    })?;

    let index_dir = output_dir
        .ensure_index_dir(index_path)
        .map_err(|e| anyhow!("failed to create index directory for `{index_path}` ({e})"))?;

    let relative_index_dir = output_dir.make_relative_to(&index_dir).ok_or_else(|| {
        anyhow!(
            "index directory `{index_dir}` is not within the output directory `{root}`",
            index_dir = index_dir.display(),
            root = output_dir.root().display()
        )
    })?;

    let mut files_to_symlink: Vec<PathBuf> =
        DEFAULT_SYMLINK_FILES.iter().map(PathBuf::from).collect();

    for (_, value) in outputs.iter() {
        extract_symlink_paths(value, &mut files_to_symlink);
    }

    let mut had_errors = false;

    for file_path in files_to_symlink {
        let target = match resolve_output_path(run_dir, &file_path) {
            Ok(target) if target.exists() => target,
            Ok(target) => {
                tracing::error!(
                    "failed to create index entry for `{file_path}`: target `{target}` does not \
                     exist",
                    file_path = file_path.display(),
                    target = target.display()
                );
                had_errors = true;
                continue;
            }
            Err(e) => {
                tracing::error!(
                    "failed to create index entry for `{file_path}`: {e}",
                    file_path = file_path.display()
                );
                had_errors = true;
                continue;
            }
        };

        let Some(relative_target) = output_dir.make_relative_to(&target) else {
            tracing::warn!(
                "not indexing `{target}`: the path is outside of the output directory `{root}`",
                target = target.display(),
                root = output_dir.root().display()
            );
            continue;
        };

        if let Err(e) = symlink_and_log(
            db,
            run_id,
            &output_dir,
            index_path,
            &target,
            &relative_target,
        )
        .await
        {
            tracing::error!(
                "failed to create index entry for `{target}`: {e}",
                target = target.display()
            );
            had_errors = true;
        }
    }

    if had_errors {
        return Err(anyhow!("failed to create one or more index entries"));
    }

    Ok(relative_index_dir)
}

/// Extract file and directory paths from a WDL value that should be symlinked.
fn extract_symlink_paths(value: &Value, paths: &mut Vec<PathBuf>) {
    match value {
        Value::Primitive(PrimitiveValue::File(path)) => {
            paths.push(path.into());
        }
        Value::Primitive(PrimitiveValue::Directory(path)) => {
            paths.push(path.into());
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

/// Resolves a path recorded in the provenance database against the root of an
/// output directory.
///
/// Recorded paths are relative to the root of the output directory (e.g.
/// `./index/yak/outputs.json`). Returns `None` if the recorded path is absolute
/// or leaves the output directory, which a database written before index paths
/// were validated may contain.
fn resolve_recorded_path(root: &Path, recorded: &str) -> Option<PathBuf> {
    let mut path = root.to_path_buf();
    for component in Path::new(recorded).components() {
        match component {
            Component::Normal(component) => path.push(component),
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => return None,
        }
    }

    Some(path)
}

/// Rebuild index from database history.
///
/// Entries that do not resolve within the output directory are reported and
/// skipped.
pub async fn rebuild_index(db: &dyn Database, output_directory: &OutputDirectory) -> Result<()> {
    let entries = db.list_latest_index_entries().await?;

    let mut had_errors = false;

    for entry in entries {
        let index_path = PathBuf::from(&entry.link_path);

        let (Some(link), Some(target)) = (
            resolve_recorded_path(output_directory.root(), &entry.link_path),
            resolve_recorded_path(output_directory.root(), &entry.target_path),
        ) else {
            tracing::warn!(
                "skipping index entry for `{link_path}`: the entry does not resolve within the \
                 output directory `{root}`",
                link_path = entry.link_path,
                root = output_directory.root().display()
            );
            continue;
        };

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_path_accepts_relative_paths() {
        let path: IndexPath = "project/2026/sample".parse().unwrap();
        assert_eq!(path.as_path(), Path::new("project/2026/sample"));
        assert_eq!(path.to_string().replace('\\', "/"), "project/2026/sample");
    }

    #[test]
    fn index_path_drops_redundant_separators() {
        let path: IndexPath = "project//sample/".parse().unwrap();
        assert_eq!(path.as_path(), Path::new("project/sample"));
    }

    #[test]
    fn index_path_rejects_empty_paths() {
        assert!(matches!(
            "".parse::<IndexPath>(),
            Err(IndexPathError::Empty)
        ));
    }

    #[test]
    fn index_path_rejects_absolute_paths() {
        assert!(matches!(
            "/project/sample".parse::<IndexPath>(),
            Err(IndexPathError::Absolute(_))
        ));
    }

    #[test]
    fn index_path_rejects_relative_components() {
        assert!(matches!(
            "../escape".parse::<IndexPath>(),
            Err(IndexPathError::NotNormalized(_))
        ));
        assert!(matches!(
            "project/../../escape".parse::<IndexPath>(),
            Err(IndexPathError::NotNormalized(_))
        ));
        assert!(matches!(
            "./project".parse::<IndexPath>(),
            Err(IndexPathError::NotNormalized(_))
        ));
    }
}
