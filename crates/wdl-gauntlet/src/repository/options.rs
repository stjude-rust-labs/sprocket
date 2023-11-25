//! Options for operating a [`Repository`](super::Repository).

mod builder;

pub use builder::Builder;

/// Options for operating a [`Repository`](super::Repository).
#[derive(Debug)]
pub struct Options {
    /// Whether or not to hydrate a repository from its remote files.
    pub hydrate_remote: bool,
}
