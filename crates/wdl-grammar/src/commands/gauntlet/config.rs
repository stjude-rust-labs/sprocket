//! Configuration.

use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use grammar::Version;
use log::debug;
use log::log_enabled;
use log::trace;
use wdl_grammar as grammar;

pub mod inner;

pub use inner::Inner;

use crate::commands::gauntlet::repository;

/// The default directory name for the `wdl-grammar` configuration file and
/// cache.
const DEFAULT_CONFIG_DIR: &str = "wdl-grammar";

/// The default name for the `wdl-grammar` configuration file.
const DEFAULT_CONFIG_FILE: &str = "Gauntlet.toml";

/// An error related to a [`Config`].
#[derive(Debug)]
pub enum Error {
    /// An error serializing TOML.
    DeserializeToml(toml::de::Error),

    /// An input/output error.
    InputOutput(std::io::Error),

    /// An error serializing TOML.
    SerializeToml(toml::ser::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::DeserializeToml(err) => write!(f, "deserialize toml error: {err}"),
            Error::InputOutput(err) => write!(f, "i/e error: {err}"),
            Error::SerializeToml(err) => write!(f, "serialize toml error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// A configuration outer object.
///
/// This struct holds both (a) the path to the configuration file and (b) the
/// configuration itself. Notably, the path to the configuration file should
/// _not_ be part of the serialized configuration value. Thus, I split the
/// concept of the path and the actual configuration into two different structs.
pub struct Config {
    /// The path to the configuration file.
    path: PathBuf,

    /// The inner configuration values.
    inner: Inner,
}

impl Config {
    /// Gets the default path for the configuration file.
    ///
    /// * If there exists a file matching the default configuration file name
    ///   within the current working directory, that is returned.
    /// * Otherwise, the default configuration directory is searched for a file
    ///   matching the default configuration file name.
    ///
    /// **Note:** the file may not actually exist—it is up to the consumer to
    /// check if the file exists before acting on it.
    pub fn default_path() -> PathBuf {
        let mut path = std::env::current_dir().expect("cannot locate working directory");
        path.push(DEFAULT_CONFIG_FILE);
        if path.exists() {
            return path;
        }

        let mut path = default_config_dir();
        path.push(DEFAULT_CONFIG_FILE);
        path
    }

    /// Attempts to load configuration values from the provided `path`.
    ///
    /// * If the `path` exists, the contents of the file will be read and
    ///   deserialized to the [`Config`] object (pending any errors in
    ///   deserialization, of course).
    /// * If the `path` does _not_ exist, a new, default [`Config`] will be
    ///   created and returned.
    ///
    /// In both cases, the `path` will be stored within the [`Config`]. This has
    /// the effect of ensuring the value loaded here will be saved to the
    /// inteded location (should [`Config::save()`] be called).
    pub fn load_or_new(path: PathBuf, version: grammar::Version) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                path,
                inner: Inner::from(version),
            });
        }

        debug!("loading from {}.", path.display());
        let contents = std::fs::read_to_string(&path).map_err(Error::InputOutput)?;
        let inner = toml::from_str(&contents).map_err(Error::DeserializeToml)?;

        let result = Self { path, inner };

        if log_enabled!(log::Level::Trace) {
            trace!("Loaded configuration file with the following:");
            trace!("  -> {} repositories.", result.repositories().len());
            let num_ignored_errors = result.ignored_errors().len();
            trace!("  -> {} ignored errors.", num_ignored_errors);
        }

        Ok(result)
    }

    /// Gets the [`Version`] from the [`Config`] by reference.
    pub fn version(&self) -> &Version {
        &self.inner.version
    }

    /// Gets the [`inner::Repositories`] from the [`Config`] by reference.
    pub fn repositories(&self) -> &inner::Repositories {
        &self.inner.repositories
    }

    /// Gets the [`inner::Repositories`] from the [`Config`] by mutable
    /// reference.
    pub fn repositories_mut(&mut self) -> &mut HashSet<repository::Identifier> {
        &mut self.inner.repositories
    }

    /// Gets the [`inner::Errors`] from the [`Config`] by reference.
    pub fn ignored_errors(&self) -> &inner::Errors {
        &self.inner.ignored_errors
    }

    /// Gets the [`inner::Errors`] from the [`Config`] by mutable reference.
    pub fn ignored_errors_mut(&mut self) -> &mut inner::Errors {
        &mut self.inner.ignored_errors
    }

    /// Attempts to save the contents of the [`Config`] (in particular, the
    /// [`Self::inner`] stored within the [`Config`]) to the path pointed to
    /// [`Self::path`].
    pub fn save(&self) -> Result<()> {
        if log_enabled!(log::Level::Debug) {
            if self.path.exists() {
                debug!("overwriting configuration at {}", self.path.display());
            } else {
                debug!("saving configuration to {}", self.path.display());
            }
        }

        let mut file = File::create(&self.path).map_err(Error::InputOutput)?;
        let contents = toml::to_string_pretty(&self.inner).map_err(Error::SerializeToml)?;

        write!(file, "{}", contents).map_err(Error::InputOutput)
    }
}

/// Gets the default configuration directory for this crate.
///
/// **NOTE:** this function also ensure that the directory exists.
pub fn default_config_dir() -> PathBuf {
    // SAFETY: for all of our use cases, this should always unwrap.
    let mut path = dirs::home_dir().expect("cannot locate home directory");
    path.push(".config");
    path.push(DEFAULT_CONFIG_DIR);

    // SAFETY: for all of our use cases, this should always unwrap.
    std::fs::create_dir_all(&path).expect("could not create config directory");

    path
}
