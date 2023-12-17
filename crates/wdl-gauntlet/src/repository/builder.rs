//! A builder for a [`Repository`].

use std::path::Path;
use std::path::PathBuf;

use log::debug;
use octocrab::Octocrab;

use crate::config::default_config_dir;
use crate::repository::cache;
use crate::repository::cache::Cache;
use crate::repository::options;
use crate::repository::Identifier;
use crate::repository::Options;
use crate::Repository;

/// The environment variables within which a GitHub personal access token can be
/// stored. See [this
/// link](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens)
/// for more details.
const GITHUB_TOKEN_ENV: &[&str] = &["GITHUB_TOKEN", "GH_TOKEN"];

/// An error related to a [`Builder`].
#[derive(Debug)]
pub enum Error {
    /// An error related to the cache.
    Cache(cache::Error),

    /// An repository identifier was never specified.
    MissingRepositoryIdentifier,

    /// An error related to [`octocrab`].
    Octocrab(octocrab::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Cache(err) => write!(f, "cache error: {err}"),
            Error::MissingRepositoryIdentifier => {
                write!(f, "missing repository identifier")
            }
            Error::Octocrab(err) => write!(f, "octocrab error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// A repository of GitHub files.
#[derive(Debug, Default)]
pub struct Builder {
    /// The root location where the GitHub files are cached locally.
    root: Option<PathBuf>,

    /// The name of the [`Repository`] expressed as an [`Identifier`].
    identifier: Option<Identifier>,

    /// The options for operating the [`Repository`].
    options: Option<Options>,
}

impl Builder {
    /// Sets the root path where the [`Repository`] will cache files locally.
    pub fn root(mut self, path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        self.root = Some(path);
        self
    }

    /// Sets the name of the [`Repository`] expressed as an [`Identifier`].
    pub fn identifier(mut self, identifier: Identifier) -> Self {
        self.identifier = Some(identifier);
        self
    }

    /// Sets the options by which the [`Repository`] will operate.
    pub fn options(mut self, options: Options) -> Self {
        self.options = Some(options);
        self
    }

    /// Consumes `self` and attempts to build a [`Repository`]
    pub fn try_build(self) -> Result<Repository> {
        let token = GITHUB_TOKEN_ENV
            .iter()
            .filter_map(|var| match std::env::var(var) {
                Ok(value) => Some(value),
                Err(_) => None,
            })
            .collect::<Vec<_>>()
            .pop();

        let mut builder = Octocrab::builder();

        if let Some(token) = token {
            debug!("GitHub token detected.");
            builder = builder.personal_token(token);
        }

        let client = builder.build().map_err(Error::Octocrab)?;

        let identifier = match self.identifier {
            Some(repository) => repository,
            None => return Err(Error::MissingRepositoryIdentifier),
        };

        let root = self.root.unwrap_or_else(|| {
            let mut path = default_config_dir();
            path.push(identifier.organization());
            path.push(identifier.name());
            path
        });

        let options = self.options.unwrap_or(options::Builder::default().build());

        let cache = Cache::try_from(root.as_ref()).map_err(Error::Cache)?;

        Ok(Repository {
            cache,
            client,
            identifier,
            options,
        })
    }
}
