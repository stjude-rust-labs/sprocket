//! A builder for an [`Options`].

use crate::commands::gauntlet::repository::Options;

/// A builder for an [`Options`].
#[derive(Debug)]
pub struct Builder {
    /// Whether or not to hydrate a repository from its remote files.
    hydrate_remote: bool,
}

impl Builder {
    /// Sets whether or not the
    /// [`Repository`](crate::commands::gauntlet::Repository) will hydrate
    /// itself from remote sources (or, in contrast, if it will rely purely on
    /// the local files it already has cached).
    pub fn hydrate_remote(mut self, value: bool) -> Self {
        self.hydrate_remote = value;
        self
    }

    /// Consumes `self` to create a new [`Options`].
    pub fn build(self) -> Options {
        Options {
            hydrate_remote: self.hydrate_remote,
        }
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            hydrate_remote: true,
        }
    }
}
