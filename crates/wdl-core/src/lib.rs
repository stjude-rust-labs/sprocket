//! Common functionality used across the `wdl` family of crates.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

pub mod concern;
pub mod display;
pub mod fs;
pub mod parse;
pub mod str;
mod version;

pub use concern::Concern;
pub use version::Version;
