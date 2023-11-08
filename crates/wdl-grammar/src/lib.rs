//! A crate for lexing and parsing the Workflow Description Language
//! (WDL) using [`pest`](https://pest.rs).

#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![deny(rustdoc::broken_intra_doc_links)]

use clap::ValueEnum;
use serde::Serialize;

pub mod v1;

#[derive(Clone, Debug, Default, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Version {
    /// Version 1.x of the WDL specification.
    #[default]
    V1,
}
