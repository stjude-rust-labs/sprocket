//! Fuzz target for the `wdl-format` crate.

#![no_main]

#[path = "../common.rs"]
mod common;

use libfuzzer_sys::fuzz_target;
use wdl::ast::Node;
use wdl::ast::SupportedVersion;
use wdl::format::Config;
use wdl::format::element::node::AstNodeFormatExt;

fuzz_target!(
    init: {
        if let Err(e) = common::init_corpus_dir("wdl-format") {
            eprintln!("{e}");
            return 1;
        }
    },
    |data: &str| {
        let fallback_version = Some(SupportedVersion::default());
        let (document, diagnostics) = wdl::ast::Document::parse(data, fallback_version);
        if !diagnostics.is_empty() {
            return; // Same as `sprocket format`
        }

        let Some(v1_ast) = document
            .ast_with_version_fallback(fallback_version)
            .into_v1()
        else {
            return;
        };

        let _ = wdl::format::Formatter::new(Config::default())
            .format(&Node::Ast(v1_ast).into_format_element());
    }
);
