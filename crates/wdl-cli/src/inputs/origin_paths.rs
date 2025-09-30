//! The set of origin paths for each read input argument.
//!
//! An origin path is the associated directory for which each parsed command
//! line argument is relative to. So, for example, arguments read in from files
//! are relative to the directory the file lives within, whereas arguments
//! provided on the command line are relative to the current working directory.
//!
//! This mechanism ensures that, when files or directories are specified as
//! inputs, we know the prefix to join to those paths to resolve the final
//! location of each path.

use indexmap::IndexMap;
use wdl_engine::path::EvaluationPath;

/// An associated set of path origins for a set of input keys.
///
/// This is useful when, for example, resolving all paths within an
/// [`Inputs`](super::Inputs) to be relative to the input file from whence they
/// originated.
#[derive(Debug)]
pub enum OriginPaths {
    /// A single origin path for all inputs.
    Single(EvaluationPath),
    /// A dynamic mapping of input keys to origin paths.
    Map(IndexMap<String, EvaluationPath>),
}

impl OriginPaths {
    /// Attempts to retrieve the origin path for an input key.
    pub fn get(&self, key: &str) -> Option<&EvaluationPath> {
        match self {
            OriginPaths::Single(path) => Some(path),
            OriginPaths::Map(paths) => paths.get(key),
        }
    }
}
