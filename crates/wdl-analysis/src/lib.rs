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

use wdl_ast::Direction;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::SyntaxToken;

mod analyzer;
pub mod diagnostics;
pub mod document;
pub mod eval;
mod graph;
mod queue;
mod rayon;
mod rules;
pub mod stdlib;
pub mod types;
mod validation;
mod visitor;

pub use analyzer::*;
pub use document::Document;
pub use rules::*;
pub use validation::*;
pub use visitor::*;

/// The prefix of `except` comments.
pub const EXCEPT_COMMENT_PREFIX: &str = "#@ except:";

/// An extension trait for syntax nodes.
pub trait SyntaxNodeExt {
    /// Gets an iterator over the `@except` comments for a syntax node.
    fn except_comments(&self) -> impl Iterator<Item = SyntaxToken> + '_;

    /// Gets the AST node's rule exceptions set.
    ///
    /// The set is the comma-delimited list of rule identifiers that follows a
    /// `#@ except:` comment.
    fn rule_exceptions(&self) -> HashSet<String>;

    /// Determines if a given rule id is excepted for the syntax node.
    fn is_rule_excepted(&self, id: &str) -> bool;
}

impl SyntaxNodeExt for SyntaxNode {
    fn except_comments(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.siblings_with_tokens(Direction::Prev)
            .skip(1)
            .map_while(|s| {
                if s.kind() == SyntaxKind::Whitespace || s.kind() == SyntaxKind::Comment {
                    s.into_token()
                } else {
                    None
                }
            })
            .filter(|t| t.kind() == SyntaxKind::Comment)
    }

    fn rule_exceptions(&self) -> HashSet<String> {
        let mut set = HashSet::default();
        for comment in self.except_comments() {
            if let Some(ids) = comment.text().strip_prefix(EXCEPT_COMMENT_PREFIX) {
                for id in ids.split(',') {
                    let id = id.trim();
                    set.insert(id.to_string());
                }
            }
        }

        set
    }

    fn is_rule_excepted(&self, id: &str) -> bool {
        for comment in self.except_comments() {
            if let Some(ids) = comment.text().strip_prefix(EXCEPT_COMMENT_PREFIX) {
                if ids.split(',').any(|i| i.trim() == id) {
                    return true;
                }
            }
        }

        false
    }
}
