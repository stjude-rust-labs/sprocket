//! Integration tests for the LSP requests.

mod call_hierarchy;
mod completions;
mod diagnostics;
mod find_all_references;
mod folding_range;
mod goto_definition;
mod hover;
mod rename;
mod semantic_tokens;
mod shutdown;
mod signature_help;
mod symbols;

#[path = "../common.rs"]
mod common;
