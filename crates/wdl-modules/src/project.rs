//! Module project loading, upward discovery, and paired persistence.
//!
//! [`ModuleProject`] loads an exact `module.json` path and remembers the
//! sibling `module-lock.json` path beside it. [`ModuleProject::discover`]
//! walks upward from a caller-supplied start path until it finds `module.json`
//! or reaches a `.git` boundary, so project discovery returns `Ok(None)` when
//! no ancestor project exists. [`ManifestDocument`] preserves unknown manifest
//! extension fields and validates each edit before it lands.

/// Lossless `module.json` document editing helpers.
mod document;
/// Advisory locking for the project lockfile.
mod lockfile;
/// Project validation helpers for manifest-referenced files and content
/// hashing.
mod validation;

use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;

use thiserror::Error;

pub use self::document::ManifestDocument;
pub use self::document::ManifestDocumentError;
pub use self::lockfile::LockedLockfile;
pub use self::validation::ProjectFileKind;
pub use self::validation::ProjectValidationError;
use crate::Lockfile;
use crate::Manifest;
use crate::lockfile::LockfileError;

/// An error loading, discovering, or reloading a module project.
#[derive(Debug, Error)]
pub enum ProjectError {
    /// Reading or inspecting a project file on disk failed.
    #[error("i/o error at `{path}`")]
    Io {
        /// The manifest candidate or sibling project path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The bytes at `path` were not a valid `module.json` document.
    #[error("invalid module manifest at `{path}`")]
    Manifest {
        /// The exact `module.json` path that failed validation.
        path: PathBuf,
        /// The underlying manifest-document error.
        #[source]
        source: ManifestDocumentError,
    },

    /// The bytes at `path` were not a valid `module-lock.json` document.
    #[error("invalid module lockfile at `{path}`")]
    Lockfile {
        /// The sibling `module-lock.json` path that failed validation.
        path: PathBuf,
        /// The underlying lockfile error.
        #[source]
        source: LockfileError,
    },
}

/// A loaded module project rooted at an exact `module.json` path.
///
/// The project keeps the caller-selected manifest path, exposes the sibling
/// `module-lock.json` path even when the lockfile is absent, and keeps the
/// manifest in a lossless [`ManifestDocument`] so extension fields survive
/// future edits.
#[derive(Clone, Debug)]
pub struct ModuleProject {
    /// Directory containing the loaded `module.json`.
    root: PathBuf,
    /// Exact `module.json` path used for loads and reloads.
    manifest_path: PathBuf,
    /// Sibling `module-lock.json` path beside `manifest_path`.
    lockfile_path: PathBuf,
    /// Lossless in-memory `module.json` document for this project.
    document: ManifestDocument,
}

impl ModuleProject {
    /// Loads a project from the exact `module.json` path.
    ///
    /// The returned value reloads this same path on later reads and derives
    /// the sibling `module-lock.json` path beside it.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, ProjectError> {
        let manifest_path = path.into();
        let bytes = std::fs::read(&manifest_path).map_err(|source| ProjectError::Io {
            path: manifest_path.clone(),
            source,
        })?;
        let document =
            ManifestDocument::parse(&bytes).map_err(|source| ProjectError::Manifest {
                path: manifest_path.clone(),
                source,
            })?;
        let root = manifest_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let lockfile_path = manifest_path.with_file_name(crate::LOCKFILE_FILENAME);
        Ok(Self {
            root,
            manifest_path,
            lockfile_path,
            document,
        })
    }

    /// Discovers the nearest ancestor project starting from `start`.
    ///
    /// This walks each ancestor directory, checking for `module.json`, and
    /// stops at the first `.git` directory boundary. It returns `Ok(None)`
    /// when no ancestor project exists before that boundary.
    pub fn discover(start: &Path) -> Result<Option<Self>, ProjectError> {
        for directory in start.ancestors() {
            let manifest_path = directory.join(crate::MANIFEST_FILENAME);
            match std::fs::symlink_metadata(&manifest_path) {
                Ok(_) => return Self::load(manifest_path).map(Some),
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(ProjectError::Io {
                        path: manifest_path,
                        source,
                    });
                }
            }
            if directory.join(".git").exists() {
                break;
            }
        }
        Ok(None)
    }

    /// Returns the directory containing the loaded `module.json`.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the exact `module.json` path that loaded this project.
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    /// Returns the sibling `module-lock.json` path for this project.
    ///
    /// The path is stable even when no lockfile exists yet.
    pub fn lockfile_path(&self) -> &Path {
        &self.lockfile_path
    }

    /// Returns the lossless `module.json` document loaded from
    /// [`Self::manifest_path`].
    pub fn document(&self) -> &ManifestDocument {
        &self.document
    }

    /// Returns the validated manifest view of the current `module.json`
    /// document.
    pub fn manifest(&self) -> &Manifest {
        self.document.manifest()
    }

    /// Reloads the exact `module.json` path from disk.
    ///
    /// This keeps the same project root and sibling lockfile path while
    /// replacing the in-memory manifest document with the latest bytes.
    pub fn reload(&mut self) -> Result<(), ProjectError> {
        let bytes = std::fs::read(&self.manifest_path).map_err(|source| ProjectError::Io {
            path: self.manifest_path.clone(),
            source,
        })?;
        self.document =
            ManifestDocument::parse(&bytes).map_err(|source| ProjectError::Manifest {
                path: self.manifest_path.clone(),
                source,
            })?;
        Ok(())
    }

    /// Loads the sibling `module-lock.json` when it exists.
    ///
    /// See [`LockedLockfile::read`] for the locking and empty-file behavior.
    pub fn load_lockfile(&self) -> Result<Option<Lockfile>, ProjectError> {
        LockedLockfile::read(&self.lockfile_path)
    }

    /// Writes `document` to this project's `module.json`.
    ///
    /// The manifest is replaced by an atomic rename, so a reader sees either
    /// the previous file or the complete new one. No lock is taken, because a
    /// text editor or another tool may rewrite the manifest at any time; the
    /// last writer wins.
    pub fn write_manifest(&self, document: &ManifestDocument) -> Result<(), ProjectError> {
        let bytes = document
            .to_bytes()
            .map_err(|source| ProjectError::Manifest {
                path: self.manifest_path.clone(),
                source,
            })?;
        write_atomically(&self.manifest_path, &bytes)
    }
}

/// Replaces `path` with `bytes` through an atomic rename.
///
/// The temporary file is created in the destination directory so the rename
/// stays on one filesystem, and its permissions are aligned with the
/// destination before the rename.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp =
        tempfile::NamedTempFile::new_in(directory).map_err(|source| ProjectError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
    temp.write_all(bytes).map_err(|source| ProjectError::Io {
        path: temp.path().to_path_buf(),
        source,
    })?;
    align_temp_permissions(&temp, path)?;
    temp.persist(path).map_err(|e| ProjectError::Io {
        path: path.to_path_buf(),
        source: e.error,
    })?;
    Ok(())
}

/// Aligns a temporary file's permissions with the destination before rename.
fn align_temp_permissions(temp: &tempfile::NamedTempFile, path: &Path) -> Result<(), ProjectError> {
    if let Ok(metadata) = std::fs::metadata(path) {
        temp.as_file()
            .set_permissions(metadata.permissions())
            .map_err(|source| ProjectError::Io {
                path: temp.path().to_path_buf(),
                source,
            })?;
        return Ok(());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        temp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o644))
            .map_err(|source| ProjectError::Io {
                path: temp.path().to_path_buf(),
                source,
            })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &[u8] = br#"{"name":"example","license":"MIT"}"#;

    #[test]
    fn load_uses_exact_manifest_and_sibling_lockfile_paths() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(crate::MANIFEST_FILENAME);
        std::fs::write(&path, MANIFEST).unwrap();

        let project = ModuleProject::load(&path).unwrap();

        assert_eq!(project.root(), directory.path());
        assert_eq!(project.manifest_path(), path);
        assert_eq!(
            project.lockfile_path(),
            directory.path().join(crate::LOCKFILE_FILENAME)
        );
        assert_eq!(project.manifest().name, "example");
    }

    #[test]
    fn discover_finds_nearest_ancestor_manifest() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join(crate::MANIFEST_FILENAME), MANIFEST).unwrap();
        let nested = directory.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();

        let project = ModuleProject::discover(&nested).unwrap().unwrap();
        assert_eq!(project.root(), directory.path());
    }

    #[test]
    fn discover_stops_after_git_boundary() {
        let outer = tempfile::tempdir().unwrap();
        std::fs::write(outer.path().join(crate::MANIFEST_FILENAME), MANIFEST).unwrap();
        let repository = outer.path().join("repo");
        let nested = repository.join("nested");
        std::fs::create_dir_all(repository.join(".git")).unwrap();
        std::fs::create_dir_all(&nested).unwrap();

        assert!(ModuleProject::discover(&nested).unwrap().is_none());
    }

    #[test]
    fn missing_lockfile_loads_as_none() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(crate::MANIFEST_FILENAME);
        std::fs::write(&path, MANIFEST).unwrap();
        let project = ModuleProject::load(path).unwrap();

        assert!(project.load_lockfile().unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn write_atomically_gives_new_files_mode_0644()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir()?;
        let path = directory.path().join(crate::MANIFEST_FILENAME);

        write_atomically(&path, b"{}\n")?;

        assert_eq!(
            std::fs::metadata(&path)?.permissions().mode() & 0o777,
            0o644
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn write_manifest_preserves_an_existing_mode()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir()?;
        let path = directory.path().join(crate::MANIFEST_FILENAME);
        std::fs::write(&path, MANIFEST)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        let project = ModuleProject::load(&path)?;

        project.write_manifest(project.document())?;

        assert_eq!(
            std::fs::metadata(&path)?.permissions().mode() & 0o777,
            0o600
        );
        Ok(())
    }
}
