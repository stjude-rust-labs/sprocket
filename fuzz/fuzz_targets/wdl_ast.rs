//! Fuzz target for the `wdl-ast` crate.

#![no_main]

#[path = "../common.rs"]
mod common;

use libfuzzer_sys::fuzz_target;
use wdl::ast::SupportedVersion;

fuzz_target!(
    init: {
        if let Err(e) = common::init_corpus_dir("wdl-ast") {
            eprintln!("{e}");
            return 1;
        }
    },
    |data: &str| {
        wdl::ast::Document::parse(data, Some(SupportedVersion::default()));
    }
);
