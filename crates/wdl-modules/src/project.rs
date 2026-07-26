//! Module project loading and upward discovery.

mod document;

use std::path::Path;
use std::path::PathBuf;

use thiserror::Error;

pub use self::document::ManifestDocument;
pub use self::document::ManifestDocumentError;
use crate::Lockfile;
use crate::Manifest;
use crate::lockfile::LockfileError;

/// An error loading or discovering a module project.
#[derive(Debug, Error)]
pub enum ProjectError {
    /// Reading a project file from disk failed.
    #[error("i/o error at `{path}`")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The manifest bytes were not a valid module manifest document.
    #[error("invalid module manifest at `{path}`")]
    Manifest {
        /// The manifest path.
        path: PathBuf,
        /// The underlying manifest-document error.
        #[source]
        source: ManifestDocumentError,
    },

    /// The lockfile bytes were not a valid module lockfile.
    #[error("invalid module lockfile at `{path}`")]
    Lockfile {
        /// The lockfile path.
        path: PathBuf,
        /// The underlying lockfile error.
        #[source]
        source: LockfileError,
    },
}

/// A loaded module project rooted at a `module.json` manifest.
#[derive(Clone, Debug)]
pub struct ModuleProject {
    root: PathBuf,
    manifest_path: PathBuf,
    lockfile_path: PathBuf,
    document: ManifestDocument,
}

impl ModuleProject {
    /// Loads a project from the exact `module.json` path.
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
    /// Discovery stops at the first `.git` directory boundary.
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

    /// Returns the project root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the exact manifest path that loaded this project.
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    /// Returns the sibling lockfile path for this project.
    pub fn lockfile_path(&self) -> &Path {
        &self.lockfile_path
    }

    /// Returns the lossless manifest document.
    pub fn document(&self) -> &ManifestDocument {
        &self.document
    }

    /// Returns the validated manifest view of the current document.
    pub fn manifest(&self) -> &Manifest {
        self.document.manifest()
    }

    /// Reloads the manifest document from disk.
    pub fn reload(&mut self) -> Result<(), ProjectError> {
        *self = Self::load(self.manifest_path.clone())?;
        Ok(())
    }

    /// Loads the sibling `module-lock.json` when it exists.
    pub fn load_lockfile(&self) -> Result<Option<Lockfile>, ProjectError> {
        let bytes = match std::fs::read(&self.lockfile_path) {
            Ok(bytes) => bytes,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(source) => {
                return Err(ProjectError::Io {
                    path: self.lockfile_path.clone(),
                    source,
                });
            }
        };
        Lockfile::parse(&bytes)
            .map(Some)
            .map_err(|source| ProjectError::Lockfile {
                path: self.lockfile_path.clone(),
                source,
            })
    }
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
}
