//! Serialized module project mutations backed by the library state root.

use std::path::Path;
use std::path::PathBuf;

use wdl_modules::project::LockedModuleProject;
pub(super) use wdl_modules::project::ProjectUpdate;

use super::project::Project;

/// Chooses the root directory under which global module mutation state is
/// stored, preferring the Sprocket configuration directory, then the
/// platform cache directory, then the system temporary directory.
fn select_mutation_state_root(config: Option<&Path>, cache: Option<&Path>, temp: &Path) -> PathBuf {
    if let Some(config) = config {
        return config.join("locks").join("module-mutations");
    }
    if let Some(cache) = cache {
        return cache
            .join("sprocket")
            .join("locks")
            .join("module-mutations");
    }
    temp.join("sprocket").join("locks").join("module-mutations")
}

/// Returns the global module mutation state root for the current platform.
fn mutation_state_root() -> PathBuf {
    select_mutation_state_root(
        crate::config::config_root().as_deref(),
        dirs::cache_dir().as_deref(),
        &std::env::temp_dir(),
    )
}

/// A refreshed module project held under its exclusive global mutation lock.
#[derive(Debug)]
pub(super) struct LockedProject(LockedModuleProject);

impl LockedProject {
    /// Acquires the project mutation lock, recovers interrupted work, and
    /// reloads the manifest under the lock.
    pub(super) fn acquire(project: Project) -> anyhow::Result<Self> {
        LockedModuleProject::acquire(project, &mutation_state_root())
            .map(Self)
            .map_err(anyhow::Error::from)
    }

    /// Returns the refreshed project snapshot protected by this lock.
    pub(super) fn project(&self) -> &Project {
        self.0.project()
    }

    /// Atomically applies a non-empty manifest and/or lockfile update.
    pub(super) fn commit(&self, update: ProjectUpdate<'_>) -> anyhow::Result<()> {
        self.0.commit(update).map_err(anyhow::Error::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_mutation_state_root_by_available_base() {
        let config = Path::new("/config");
        let cache = Path::new("/cache");
        let temp = Path::new("/tmp");

        assert_eq!(
            select_mutation_state_root(Some(config), Some(cache), temp),
            config.join("locks").join("module-mutations")
        );
        assert_eq!(
            select_mutation_state_root(None, Some(cache), temp),
            cache
                .join("sprocket")
                .join("locks")
                .join("module-mutations")
        );
        assert_eq!(
            select_mutation_state_root(None, None, temp),
            temp.join("sprocket").join("locks").join("module-mutations")
        );
    }
}
