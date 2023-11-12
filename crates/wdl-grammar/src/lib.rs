//! A crate for lexing and parsing the Workflow Description Language
//! (WDL) using [`pest`](https://pest.rs).

#![feature(let_chains)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

#[cfg(feature = "binaries")]
use clap::ValueEnum;
use serde::Deserialize;
use serde::Serialize;

pub mod v1;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "binaries", derive(ValueEnum))]
#[serde(rename_all = "lowercase")]
pub enum Version {
    /// Version 1.x of the WDL specification.
    #[default]
    V1,
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Version::V1 => write!(f, "WDL v1.x"),
        }
    }
}
