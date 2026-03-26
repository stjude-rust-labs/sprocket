//! Analysis of Workflow Description Language (WDL) documents.
//!
//! An analyzer can be used to implement the [Language Server Protocol (LSP)](https://microsoft.github.io/language-server-protocol/).
//!
//! # Examples
//!
//! ```no_run
//! use url::Url;
//! use wdl_analysis::Analyzer;
//!
//! #[tokio::main]
//! async fn main() {
//!     let analyzer = Analyzer::default();
//!     // Add a docuement to the analyzer
//!     analyzer
//!         .add_document(Url::parse("file:///path/to/file.wdl").unwrap())
//!         .await
//!         .unwrap();
//!     let results = analyzer.analyze(()).await.unwrap();
//!     // Process the results
//!     for result in results {
//!         // Do something
//!     }
//! }
//! ```
#![doc = include_str!("../RULES.md")]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::collections::HashSet;

use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Direction;
use wdl_ast::Directive;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;

mod analyzer;
pub mod config;
pub mod diagnostics;
pub mod document;
pub mod eval;
mod graph;
pub mod handlers;
mod queue;
mod rayon;
mod rules;
pub mod stdlib;
pub mod types;
mod validation;
mod visitor;

pub use analyzer::*;
pub use config::Config;
pub use config::DiagnosticsConfig;
pub use config::FeatureFlags;
pub use document::Document;
pub use rules::*;
pub use validation::*;
pub use visitor::*;

/// An extension trait for syntax nodes.
pub trait Exceptable {
    /// Gets the AST node's rule exceptions set.
    ///
    /// The set is the comma-delimited list of rule identifiers that follows a
    /// `#@ except:` comment.
    fn rule_exceptions(&self) -> HashSet<String> {
        HashSet::new()
    }

    /// Determines if a given rule id is excepted for the syntax node.
    fn is_rule_excepted(&self, _id: &str) -> bool {
        true
    }
}

impl Exceptable for SyntaxNode {
    fn rule_exceptions(&self) -> HashSet<String> {
        self.siblings_with_tokens(Direction::Prev)
            .skip(1) // self is included with siblings
            .map_while(|s| {
                if s.kind() == SyntaxKind::Whitespace || s.kind() == SyntaxKind::Comment {
                    s.into_token()
                } else {
                    None
                }
            })
            .filter_map(Comment::cast)
            .filter_map(|c| c.directive())
            .flat_map(|d| match d {
                Directive::Except(e) => e,
            })
            .collect()
    }

    fn is_rule_excepted(&self, id: &str) -> bool {
        self.rule_exceptions().contains(id)
    }
}
