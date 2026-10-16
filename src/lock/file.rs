//! The sprocket lock file structure.

use anyhow::Context;
use anyhow::bail;
use path_clean::PathClean;
use toml_spanner::Arena;
use toml_spanner::Failed;
use toml_spanner::FromToml;
use toml_spanner::Item;
use toml_spanner::Key;
use toml_spanner::Table;
use toml_spanner::ToToml;
use toml_spanner::ToTomlError;
use wdl::engine::images::ContainerImageOverrides;
use wdl::engine::images::ImageOverrideMap;

use crate::analysis::Source;
use crate::commands::lock::LOCK_FILE;
use crate::config::CONFIG_FILENAME;

/// The version of a [`LockFile`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum LockFileVersion {
    /// Version 1.
    #[default]
    V1 = 1,
}

impl ToToml for LockFileVersion {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        let version = match self {
            Self::V1 => &1,
        };

        version.to_toml(arena)
    }
}

impl<'de> FromToml<'de> for LockFileVersion {
    fn from_toml(ctx: &mut toml_spanner::Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        let Some(version) = item.as_i64() else {
            ctx.report_custom_error("invalid lock file version", item);
            return Err(Failed);
        };

        match version {
            1 => Ok(LockFileVersion::V1),
            _ => {
                ctx.report_custom_error("invalid lock file version", item);
                Err(Failed)
            }
        }
    }
}

/// The versioned lock file.
#[derive(Debug)]
enum VersionedLockFile {
    /// [`LockFileVersion::V1`] lock file.
    V1(v1::LockFile),
}

/// A sprocket lock file.
#[derive(Debug)]
pub struct LockFile(VersionedLockFile);

impl LockFile {
    /// Get the version of this lockfile.
    pub fn version(&self) -> LockFileVersion {
        match self.0 {
            VersionedLockFile::V1(_) => LockFileVersion::V1,
        }
    }

    /// Create a new v1 lockfile.
    pub fn new_v1(images: ImageOverrideMap) -> Self {
        Self(VersionedLockFile::V1(v1::LockFile::new(images)))
    }

    /// Get the container images specified in the lock file.
    pub fn images(&self) -> &ImageOverrideMap {
        match &self.0 {
            VersionedLockFile::V1(file) => &file.images,
        }
    }

    /// Convert this `LockFile` into a [`ContainerImageOverrides`], if any
    /// overrides are specified.
    pub fn as_container_overrides(&self) -> Option<ContainerImageOverrides> {
        let overrides = self.images();
        if overrides.is_empty() {
            return None;
        }

        Some(ContainerImageOverrides::new(
            overrides.iter().map(|(k, v)| (k.clone(), v.clone())),
        ))
    }
}

impl Default for LockFile {
    fn default() -> Self {
        Self(VersionedLockFile::V1(v1::LockFile::default()))
    }
}

impl ToToml for LockFile {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        let version = match self.0 {
            VersionedLockFile::V1(_) => &LockFileVersion::V1,
        };
        let version_item = version.to_toml(arena)?;
        let file = match &self.0 {
            VersionedLockFile::V1(file) => file.to_toml(arena)?,
        };
        // SAFETY: The file structs always serialize into tables
        let file_table = file
            .into_table()
            .expect("invalid ToToml impl for `LockFile`");
        let Some(mut table) = Table::try_with_capacity(1 + file_table.len(), arena) else {
            return Err(ToTomlError::from("Table capacity exceeded maximum"));
        };
        table.insert_unique(Key::new("version"), version_item, arena);
        for (key, val) in file_table {
            table.insert_unique(key, val, arena);
        }
        Ok(table.into_item())
    }
}
impl<'de> FromToml<'de> for LockFile {
    fn from_toml(ctx: &mut toml_spanner::Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        let Ok(table) = item.require_table(ctx) else {
            return Err(Failed);
        };
        let Some((_key, version)) = table.get_key_value("version") else {
            ctx.report_custom_error("lock file is missing a version", table.as_item());
            return Err(Failed);
        };

        let version = LockFileVersion::from_toml(ctx, version)?;
        let kind = match version {
            LockFileVersion::V1 => VersionedLockFile::V1(v1::LockFile::from_toml(ctx, item)?),
        };
        Ok(Self(kind))
    }
}

impl LockFile {
    /// Attempt to discover and load the applicable container lock for the given
    /// [`Source`].
    ///
    /// Searches `source`'s parent directory and ancestors up to the repository
    /// root.
    ///
    /// Returns `Ok(None)` if no lock file is found.
    pub(crate) fn locate(source: &Source) -> anyhow::Result<Option<Self>> {
        let start_dir = match source.local_start_dir() {
            Some(dir) => dir,
            None => std::env::current_dir().context("failed to get current directory")?,
        };

        let start_dir = start_dir.clean();
        for dir in start_dir.ancestors() {
            let candidate = dir.join(LOCK_FILE);
            if candidate.is_file() {
                let lock_file = match std::fs::read_to_string(&candidate) {
                    Ok(lock_file) => lock_file,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                    Err(e) => bail!("failed to read `{}`: {e}", candidate.display()),
                };

                return toml_spanner::from_str(&lock_file)
                    .context("failed to parse lock file")
                    .map(Some);
            }

            if dir.join(".git").is_dir() || dir.join(CONFIG_FILENAME).is_file() {
                return Ok(None);
            }
        }

        Ok(None)
    }
}

/// Lock file version 1.
mod v1 {
    use toml_spanner::Failed;
    use toml_spanner::FromToml;
    use toml_spanner::Item;
    use toml_spanner::Toml;
    use wdl::engine::images::ImageOverrideMap;

    use crate::lock::file::LockFileVersion;

    /// Represents the lock file structure.
    #[derive(Debug, Default, PartialEq, Eq, Toml)]
    #[toml(ToToml, deny_unknown_fields)]
    pub(super) struct LockFile {
        /// See [`ImageOverrideMap`].
        pub(super) images: ImageOverrideMap,
    }

    impl LockFile {
        /// Create a new v1 `LockFile`.
        pub(super) fn new(images: ImageOverrideMap) -> Self {
            Self { images }
        }
    }

    impl<'de> FromToml<'de> for LockFile {
        fn from_toml(
            ctx: &mut toml_spanner::Context<'de>,
            item: &Item<'de>,
        ) -> Result<Self, Failed> {
            #[derive(Debug, Toml)]
            #[toml(Toml, deny_unknown_fields)]
            struct LockFileInner {
                version: LockFileVersion,
                #[toml(default)]
                images: ImageOverrideMap,
            }
            let inner = LockFileInner::from_toml(ctx, item)?;
            Ok(Self {
                images: inner.images,
            })
        }
    }
}
