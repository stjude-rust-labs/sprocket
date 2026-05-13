//! Dependency names and sources for `module.json`.

mod name;
mod source;

pub use name::DependencyName;
pub use name::DependencyNameError;
pub use source::DependencySource;
pub use source::DependencySourceError;
pub use source::GitSelector;
