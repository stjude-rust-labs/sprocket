//! Lint rules for Workflow Description Language (WDL) documents.
#![doc = include_str!("../RULES.md")]
//! # Examples
//!
//! An example of parsing a WDL document and linting it:
//!
//! ```rust
//! # let source = "version 1.1\nworkflow test {}";
//! use wdl_lint::ast::Document;
//! use wdl_lint::ast::Validator;
//! use wdl_lint::v1::rules;
//!
//! match Document::parse(source).into_result() {
//!     Ok(document) => {
//!         let mut validator = Validator::default();
//!         validator.add_v1_visitors(rules().into_iter().map(|r| r.visitor()));
//!         match validator.validate(&document) {
//!             Ok(_) => {
//!                 // The document was valid WDL and passed all lints
//!             }
//!             Err(diagnostics) => {
//!                 // Handle the failure to validate
//!             }
//!         }
//!     }
//!     Err(diagnostics) => {
//!         // Handle the failure to parse
//!     }
//! }
//! ```

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

mod tags;
pub(crate) mod util;
pub mod v1;

pub use tags::*;
pub use wdl_ast as ast;
