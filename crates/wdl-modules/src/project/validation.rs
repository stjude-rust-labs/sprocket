use std::fmt;
use std::path::Path;
use std::path::PathBuf;

use thiserror::Error;

use super::ModuleProject;
use crate::hash::ContentHash;
use crate::hash::HashError;

/// A manifest-referenced file required for a valid module project.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectFileKind {
    /// The module's WDL entrypoint.
    Entrypoint,
    /// The module's readme.
    Readme,
}

impl fmt::Display for ProjectFileKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Entrypoint => f.write_str("entrypoint"),
            Self::Readme => f.write_str("readme"),
        }
    }
}

/// An error validating a loaded module project.
#[derive(Debug, Error)]
pub enum ProjectValidationError {
    /// A required manifest-referenced file does not exist.
    #[error("module {kind} `{path}` does not exist")]
    MissingFile {
        /// The role of the missing file.
        kind: ProjectFileKind,
        /// The resolved path that was missing.
        path: PathBuf,
    },

    /// A required manifest reference does not resolve to a regular file.
    #[error("module {kind} `{path}` is not a regular file")]
    NotRegularFile {
        /// The role of the invalid file.
        kind: ProjectFileKind,
        /// The resolved path that was not a regular file.
        path: PathBuf,
    },

    /// A required manifest reference is omitted from module content hashing.
    #[error("module {kind} `{path}` is excluded from module content hashing")]
    ExcludedFile {
        /// The role of the excluded file.
        kind: ProjectFileKind,
        /// The resolved path that would be omitted from the digest.
        path: PathBuf,
    },

    /// Inspecting a required manifest-referenced file failed.
    #[error("failed to inspect module {kind} `{path}`")]
    Io {
        /// The role of the file.
        kind: ProjectFileKind,
        /// The resolved path that could not be inspected.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Module content violated a tree or hashing invariant.
    #[error(transparent)]
    Content(#[from] HashError),
}

impl ModuleProject {
    /// Validates this project's manifest-referenced files and content tree.
    ///
    /// The returned digest can be reused for signature verification.
    pub fn validate(&self) -> Result<ContentHash, ProjectValidationError> {
        validate_regular_file(
            &self.root,
            self.manifest().entrypoint_filename(),
            ProjectFileKind::Entrypoint,
        )?;
        if let Some(readme) = self.manifest().readme_filename() {
            validate_regular_file(&self.root, readme, ProjectFileKind::Readme)?;
        }

        crate::hash::hash_directory(&self.root).map_err(Into::into)
    }
}

/// Validates that a manifest-referenced path exists as a regular file and is
/// included in module content hashing.
fn validate_regular_file(
    root: &Path,
    relative_path: &Path,
    kind: ProjectFileKind,
) -> Result<(), ProjectValidationError> {
    let path = root.join(relative_path);
    if crate::hash::path_is_excluded_from_hash(relative_path) {
        return Err(ProjectValidationError::ExcludedFile { kind, path });
    }

    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(ProjectValidationError::MissingFile { kind, path });
        }
        Err(source) => {
            return Err(ProjectValidationError::Io { kind, path, source });
        }
    };
    if !metadata.is_file() {
        return Err(ProjectValidationError::NotRegularFile { kind, path });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    fn project(manifest: &str) -> Result<(tempfile::TempDir, ModuleProject), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        let manifest_path = directory.path().join(crate::MANIFEST_FILENAME);
        std::fs::write(&manifest_path, manifest)?;
        let project = ModuleProject::load(manifest_path)?;
        Ok((directory, project))
    }

    #[test]
    fn validates_default_entrypoint_and_readme() -> Result<(), Box<dyn Error>> {
        let (directory, project) = project(r#"{"name":"example","license":"MIT"}"#)?;
        std::fs::write(directory.path().join("index.wdl"), "version 1.3\n")?;
        std::fs::write(directory.path().join("README.md"), "# Example\n")?;

        let checksum = project.validate()?;
        assert_eq!(checksum, crate::hash::hash_directory(directory.path())?);
        Ok(())
    }

    #[test]
    fn validates_custom_references() -> Result<(), Box<dyn Error>> {
        let (directory, project) = project(
            r#"{
                "name":"example",
                "license":"MIT",
                "entrypoint":"src/main.wdl",
                "readme":"docs/guide.md"
            }"#,
        )?;
        std::fs::create_dir_all(directory.path().join("src"))?;
        std::fs::create_dir_all(directory.path().join("docs"))?;
        std::fs::write(directory.path().join("src/main.wdl"), "version 1.3\n")?;
        std::fs::write(directory.path().join("docs/guide.md"), "# Guide\n")?;

        project.validate()?;
        Ok(())
    }

    #[test]
    fn accepts_disabled_readme() -> Result<(), Box<dyn Error>> {
        let (directory, project) = project(r#"{"name":"example","license":"MIT","readme":false}"#)?;
        std::fs::write(directory.path().join("index.wdl"), "version 1.3\n")?;

        project.validate()?;
        Ok(())
    }

    #[test]
    fn reports_missing_entrypoint() -> Result<(), Box<dyn Error>> {
        let (directory, project) = project(r#"{"name":"example","license":"MIT"}"#)?;
        std::fs::write(directory.path().join("README.md"), "# Example\n")?;

        let error = project.validate().unwrap_err();
        assert!(matches!(
            error,
            ProjectValidationError::MissingFile {
                kind: ProjectFileKind::Entrypoint,
                ..
            }
        ));
        assert!(error.to_string().contains("index.wdl"));
        Ok(())
    }

    #[test]
    fn reports_missing_readme() -> Result<(), Box<dyn Error>> {
        let (directory, project) = project(r#"{"name":"example","license":"MIT"}"#)?;
        std::fs::write(directory.path().join("index.wdl"), "version 1.3\n")?;

        let error = project.validate().unwrap_err();
        assert!(matches!(
            error,
            ProjectValidationError::MissingFile {
                kind: ProjectFileKind::Readme,
                ..
            }
        ));
        assert!(error.to_string().contains("README.md"));
        Ok(())
    }

    #[test]
    fn reports_missing_custom_references() -> Result<(), Box<dyn Error>> {
        let (directory, project) = project(
            r#"{
                "name":"example",
                "license":"MIT",
                "entrypoint":"src/main.wdl",
                "readme":"docs/guide.md"
            }"#,
        )?;
        std::fs::create_dir_all(directory.path().join("docs"))?;
        std::fs::write(directory.path().join("docs/guide.md"), "# Guide\n")?;

        let error = project.validate().unwrap_err();
        assert!(matches!(
            error,
            ProjectValidationError::MissingFile {
                kind: ProjectFileKind::Entrypoint,
                ..
            }
        ));
        assert!(error.to_string().contains("src/main.wdl"));

        std::fs::create_dir_all(directory.path().join("src"))?;
        std::fs::write(directory.path().join("src/main.wdl"), "version 1.3\n")?;
        std::fs::remove_file(directory.path().join("docs/guide.md"))?;

        let error = project.validate().unwrap_err();
        assert!(matches!(
            error,
            ProjectValidationError::MissingFile {
                kind: ProjectFileKind::Readme,
                ..
            }
        ));
        assert!(error.to_string().contains("docs/guide.md"));
        Ok(())
    }

    #[test]
    fn rejects_directory_in_place_of_referenced_file() -> Result<(), Box<dyn Error>> {
        let (directory, project) = project(r#"{"name":"example","license":"MIT"}"#)?;
        std::fs::create_dir(directory.path().join("index.wdl"))?;
        std::fs::write(directory.path().join("README.md"), "# Example\n")?;

        let error = project.validate().unwrap_err();
        assert!(matches!(
            error,
            ProjectValidationError::NotRegularFile {
                kind: ProjectFileKind::Entrypoint,
                ..
            }
        ));
        Ok(())
    }

    #[test]
    fn rejects_entrypoint_excluded_from_hash_at_module_root() -> Result<(), Box<dyn Error>> {
        let (directory, project) = project(
            r#"{
                "name":"example",
                "license":"MIT",
                "entrypoint":"module-lock.json"
            }"#,
        )?;
        std::fs::write(directory.path().join(crate::LOCKFILE_FILENAME), "{}\n")?;
        std::fs::write(directory.path().join("README.md"), "# Example\n")?;

        let error = project.validate().unwrap_err();
        assert!(matches!(
            &error,
            ProjectValidationError::ExcludedFile {
                kind: ProjectFileKind::Entrypoint,
                path,
            } if path == &directory.path().join(crate::LOCKFILE_FILENAME)
        ));
        assert!(matches!(
            error,
            ProjectValidationError::ExcludedFile {
                kind: ProjectFileKind::Entrypoint,
                ..
            }
        ));
        assert!(error.to_string().contains(crate::LOCKFILE_FILENAME));
        Ok(())
    }

    #[test]
    fn rejects_readme_excluded_from_hash_in_skipped_directory() -> Result<(), Box<dyn Error>> {
        let (directory, project) = project(
            r#"{
                "name":"example",
                "license":"MIT",
                "readme":".sprocket/README.md"
            }"#,
        )?;
        std::fs::write(directory.path().join("index.wdl"), "version 1.3\n")?;
        std::fs::create_dir(directory.path().join(".sprocket"))?;
        std::fs::write(directory.path().join(".sprocket/README.md"), "# Hidden\n")?;

        let error = project.validate().unwrap_err();
        assert!(matches!(
            &error,
            ProjectValidationError::ExcludedFile {
                kind: ProjectFileKind::Readme,
                path,
            } if path == &directory.path().join(".sprocket/README.md")
        ));
        assert!(matches!(
            error,
            ProjectValidationError::ExcludedFile {
                kind: ProjectFileKind::Readme,
                ..
            }
        ));
        assert!(error.to_string().contains(".sprocket/README.md"));
        Ok(())
    }

    #[test]
    fn preserves_tree_validation_errors() -> Result<(), Box<dyn Error>> {
        let (directory, project) = project(r#"{"name":"example","license":"MIT"}"#)?;
        std::fs::write(directory.path().join("index.wdl"), "version 1.3\n")?;
        std::fs::write(directory.path().join("README.md"), "# Example\n")?;
        std::fs::create_dir(directory.path().join("nested"))?;
        std::fs::write(directory.path().join("nested/module.json"), b"not metadata")?;

        let error = project.validate().unwrap_err();
        assert!(matches!(error, ProjectValidationError::Content(_)));
        assert!(
            error
                .to_string()
                .contains("only permitted at the module root")
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_anywhere_in_module() -> Result<(), Box<dyn Error>> {
        let (directory, project) = project(r#"{"name":"example","license":"MIT"}"#)?;
        std::fs::write(directory.path().join("index.wdl"), "version 1.3\n")?;
        std::fs::write(directory.path().join("README.md"), "# Example\n")?;
        std::fs::write(directory.path().join("real.wdl"), "version 1.3\n")?;
        std::os::unix::fs::symlink(
            directory.path().join("real.wdl"),
            directory.path().join("alias.wdl"),
        )?;

        let error = project.validate().unwrap_err();
        assert!(matches!(error, ProjectValidationError::Content(_)));
        Ok(())
    }
}
