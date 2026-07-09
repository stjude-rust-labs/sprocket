//! Fuzz target for the `wdl-lint` crate.

#![no_main]

#[path = "../common.rs"]
mod common;

use std::io::Write;
use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;
use tempfile::NamedTempFile;
use tokio::runtime::Runtime;
use url::Url;
use wdl::analysis::Analyzer;
use wdl::analysis::Config;
use wdl::analysis::ResolutionContext;
use wdl::analysis::Validator;
use wdl::ast::SupportedVersion;
use wdl::lint::Linter;

#[derive(Debug)]
struct Context {
    runtime: Runtime,
    analyzer: Analyzer<()>,
}

static CONTEXT: OnceLock<Context> = OnceLock::new();

fuzz_target!(
    init: {
        if let Err(e) = common::init_corpus_dir("wdl-lint") {
            eprintln!("{e}");
            return 1;
        }

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let analyzer = runtime.block_on(async move {
            Analyzer::new_with_validator(
                Config::default(),
                |_, _, _, _| async move {},
                || {
                    let mut validator = Validator::default();
                    validator.add_visitor(Linter::default());
                    validator
                },
            )
        });

        CONTEXT.set(Context { runtime, analyzer }).expect("should be first initialization");
    },
    |data: &str| {
        let fallback_version = Some(SupportedVersion::default());
        let (_, diagnostics) = wdl::ast::Document::parse(data, fallback_version);
        if !diagnostics.is_empty() {
            return;
        }

        let mut file = NamedTempFile::new().expect("failed to create temp file");
        file.write_all(data.as_bytes()).expect("failed to write to temp file");

        let ctx = CONTEXT.get().expect("context should be initialized");
        ctx.runtime.block_on(async {
            let url = Url::from_file_path(file.path()).unwrap();
            if ctx.analyzer.add_document(url.clone()).await.is_err() {
                return;
            }
            let _ = ctx.analyzer
                .analyze_document((), url)
                .await;
        });
    }
);
