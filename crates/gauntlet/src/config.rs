//! Configuration.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use tracing::debug;
use tracing::trace;

pub mod inner;

pub use inner::Inner;

/// The default directory name for the `wdl-gauntlet` configuration file
const DEFAULT_CONFIG_DIR: &str = "wdl-gauntlet";

/// The default name for the `wdl-gauntlet` configuration file.
const DEFAULT_CONFIG_FILE: &str = "Gauntlet.toml";

/// The default name for the `wdl-gauntlet --arena` configuration file.
const DEFAULT_ARENA_CONFIG_FILE: &str = "Arena.toml";

/// An error related to a [`Config`].
#[derive(Debug)]
pub enum Error {
    /// An error serializing TOML.
    DeserializeToml(toml::de::Error),

    /// An input/output error.
    InputOutput(std::io::Error),

    /// Attempted to save a config without a backing `path` (i.e., an anonymous
    /// configuration file that is only meant to be used for testing).
    SaveOnAnonymousConfig,

    /// An error serializing TOML.
    SerializeToml(toml::ser::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::DeserializeToml(err) => {
                write!(f, "deserialize toml error: {err}")
            }
            Error::InputOutput(err) => write!(f, "i/e error: {err}"),
            Error::SaveOnAnonymousConfig => {
                write!(f, "attempted to save an anonymous config")
            }
            Error::SerializeToml(err) => {
                write!(f, "serialize toml error: {err}")
            }
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
#[derive(Debug, Default)]
pub struct Config {
    /// The path to the configuration file.
    path: Option<PathBuf>,

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
    /// * Presence of the `--arena` flag will change the default configuration
    ///   file name.
    ///
    /// **Note:** the file may not actually exist—it is up to the consumer to
    /// check if the file exists before acting on it.
    pub fn default_path(arena: bool) -> PathBuf {
        let mut path = std::env::current_dir().expect("cannot locate working directory");
        let filename = if arena {
            DEFAULT_ARENA_CONFIG_FILE
        } else {
            DEFAULT_CONFIG_FILE
        };
        path.push(filename);
        if path.exists() {
            return path;
        }

        let mut path = default_config_dir();
        path.push(filename);
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
    /// intended location (should [`Config::save()`] be called).
    pub fn load_or_new(path: PathBuf) -> Result<Self> {
        if !path.exists() {
            debug!(
                "no configuration exists at {}, creating new configuration.",
                path.display()
            );
            return Ok(Self {
                path: Some(path),
                inner: Inner::default(),
            });
        }

        debug!("loading from {}.", path.display());
        let contents = std::fs::read_to_string(&path).map_err(Error::InputOutput)?;
        let mut inner: Inner = toml::from_str(&contents).map_err(Error::DeserializeToml)?;
        inner.sort();

        let result = Self {
            path: Some(path),
            inner,
        };

        trace!("Loaded configuration file with the following:");
        trace!("  -> {} repositories.", result.inner().repositories().len());
        trace!(
            "  -> {} ignored diagnostics.",
            result.inner().diagnostics().len()
        );

        Ok(result)
    }

    /// Gets the [`Inner`] configuration by reference.
    pub fn inner(&self) -> &Inner {
        &self.inner
    }

    /// Gets the [`Inner`] configuration by mutable reference.
    pub fn inner_mut(&mut self) -> &mut Inner {
        &mut self.inner
    }

    /// Attempts to save the contents of the [`Config`] (in particular, the
    /// [`Self::inner`] stored within the [`Config`]) to the path backing the
    /// [`Config`].
    pub fn save(&self) -> Result<()> {
        if let Some(ref path) = self.path {
            if path.exists() {
                debug!("overwriting configuration at {}", path.display());
            } else {
                debug!("saving configuration to {}", path.display());
            }

            let mut file = File::create(path).map_err(Error::InputOutput)?;
            let contents = toml::to_string_pretty(&self.inner).map_err(Error::SerializeToml)?;

            write!(file, "{}", contents).map_err(Error::InputOutput)
        } else {
            Err(Error::SaveOnAnonymousConfig)
        }
    }
}

impl std::str::FromStr for Config {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let inner = toml::from_str(s).map_err(Error::DeserializeToml)?;
        Ok(Self { path: None, inner })
    }
}

/// Gets the default configuration directory for this crate.
///
/// **Note::** this function also ensure that the directory exists.
pub fn default_config_dir() -> PathBuf {
    // SAFETY: for all of our use cases, this should always unwrap.
    let mut path = dirs::home_dir().expect("cannot locate home directory");
    path.push(".config");
    path.push(DEFAULT_CONFIG_DIR);

    // SAFETY: for all of our use cases, this should always unwrap.
    std::fs::create_dir_all(&path).expect("could not create config directory");

    path
}
