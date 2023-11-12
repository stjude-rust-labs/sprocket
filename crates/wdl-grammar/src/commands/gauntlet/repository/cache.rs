//! A cache of local repository files.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

mod entry;
pub mod registry;

pub use entry::Entry;
use log::debug;
pub use registry::Registry;

/// An error related to a [`Cache`].
#[derive(Debug)]
pub enum Error {
    /// An input/output error.
    InputOutput(std::io::Error),

    /// The provided path is missing.
    MissingFile(PathBuf),

    /// The provided path is missing a parent.
    MissingParent(PathBuf),

    /// A registry error
    Registry(registry::Error),

    /// The root path is a file.
    RootIsFile(PathBuf),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InputOutput(err) => write!(f, "i/o error: {err}"),
            Error::MissingFile(path) => write!(f, "missing file: {}", path.display()),
            Error::MissingParent(path) => write!(f, "missing parent: {}", path.display()),
            Error::Registry(err) => write!(f, "registry error: {err}"),
            Error::RootIsFile(root) => write!(f, "root is file: {}", root.display()),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// A cache of local repository files.
#[derive(Debug)]
pub struct Cache {
    /// The root directory.
    root: PathBuf,

    /// The [registry file](Registry).
    registry: Registry,
}

impl Cache {
    /// Gets a path within the [`Cache`].
    fn path(&self, path: impl AsRef<str>) -> PathBuf {
        let mut result = self.root.clone();
        result.push(path.as_ref());
        result
    }

    /// Gets the [registry file](Registry) for this [`Cache`] by reference.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Attempts to get an entry within the [`Cache`] if it exists.
    pub fn get(&self, path: impl AsRef<str>) -> Result<Option<Entry>> {
        let path = path.as_ref();

        let etag = match self.registry.entries().get(path) {
            Some(etag) => etag.clone(),
            None => return Ok(None),
        };

        let path = self.path(path);

        if !path.exists() {
            return Err(Error::MissingFile(path));
        }

        let contents = std::fs::read_to_string(path).map_err(Error::InputOutput)?;
        Ok(Some(Entry::new(etag, contents)))
    }

    /// Inserts a file and its associated `etag` into the [`Cache`] at the
    /// provided `path`.
    pub fn insert(
        &mut self,
        path: impl AsRef<str>,
        etag: impl AsRef<str>,
        contents: impl AsRef<str>,
    ) -> Result<()> {
        self.registry
            .try_insert_etag(&path, etag)
            .map_err(Error::Registry)?;

        let path = self.path(&path);

        match path.parent() {
            Some(parent) => std::fs::create_dir_all(parent).map_err(Error::InputOutput)?,
            None => return Err(Error::MissingParent(path.clone())),
        };

        let mut file = File::create(path).map_err(Error::InputOutput)?;
        file.write_all(contents.as_ref().as_bytes())
            .map_err(Error::InputOutput)?;

        self.registry.save().map_err(Error::Registry)?;

        Ok(())
    }
}

impl TryFrom<&Path> for Cache {
    type Error = Error;

    fn try_from(root: &Path) -> std::result::Result<Self, Self::Error> {
        let root = root.to_path_buf();

        let mut registry_path = root.clone();
        registry_path.push("Registry.toml");
        let registry = Registry::try_from(registry_path).map_err(Error::Registry)?;

        if root.is_file() {
            return Err(Error::RootIsFile(root));
        }

        debug!("Creating cache at {}", root.display());

        if !root.exists() {
            std::fs::create_dir_all(&root).map_err(Error::InputOutput)?;
        }

        registry.save().map_err(Error::Registry)?;

        Ok(Self { root, registry })
    }
}
