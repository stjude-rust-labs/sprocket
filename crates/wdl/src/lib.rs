//! Workflow Description Language (WDL) document parsing and linting.
//!
//! There are three top-level modules to this crate:
//!
//! * `grammar` - used to parse WDL source into a Concrete Syntax Tree (CST).
//! * `ast` - used to parse a WDL document into an Abstract Syntax Tree (AST).
//! * `lint` - provides additional lint rules that can be used in a validation
//!   pass over a document.
//!
//! The above are re-exports of the individual `wdl-grammar`, `wdl-ast`, and
//! `wdl-lint` crates, respectively.
//!
//! The CST is based on the `rowan` crate and represents an immutable red-green
//! tree. Mutations to the tree require creating a new tree where unaffected
//! nodes are shared between the old and new trees; the cost of editing a node
//! of the tree depends solely on the depth of the node, as it must update the
//! parent chain to produce a new tree root.
//!
//! Note that in this implementation, the AST is a facade over the CST; each AST
//! representation internally holds a CST node or token. As a result, the AST is
//! very cheaply constructed and may be cheaply cloned at any level.
//!
//! # Examples
//!
//! An example of parsing WDL source into a CST and printing the tree:
//!
//! ```rust
//! use wdl::grammar::SyntaxTree;
//!
//! let (tree, diagnostics) = SyntaxTree::parse("version 1.1");
//! assert!(diagnostics.is_empty());
//! println!("{tree:#?}");
//! ```
//!
//! An example of parsing a WDL document into an AST and validating it:
//!
//! ```rust
//! # let source = "version 1.1\nworkflow test {}";
//! use wdl::ast::Document;
//! use wdl::ast::Validator;
//!
//! let (document, diagnostics) = Document::parse(source);
//! if !diagnostics.is_empty() {
//!     // Handle the failure to parse
//! }
//!
//! let mut validator = Validator::default();
//! if let Err(diagnostics) = validator.validate(&document) {
//!     // Handle the failure to validate
//! }
//! ```
//!
//! An example of parsing a WDL document and linting it:
//!
//! ```rust
//! # let source = "version 1.1\nworkflow test {}";
//! use wdl::ast::Document;
//! use wdl::ast::Validator;
//! use wdl::lint::LintVisitor;
//!
//! let (document, diagnostics) = Document::parse(source);
//! if !diagnostics.is_empty() {
//!     // Handle the failure to parse
//! }
//!
//! let mut validator = Validator::default();
//! validator.add_visitor(LintVisitor::default());
//! if let Err(diagnostics) = validator.validate(&document) {
//!     // Handle the failure to validate
//! }
//! ```

#![warn(missing_docs)]

#[cfg(feature = "analysis")]
#[doc(inline)]
pub use wdl_analysis as analysis;
#[cfg(feature = "ast")]
#[doc(inline)]
pub use wdl_ast as ast;
#[cfg(feature = "grammar")]
#[doc(inline)]
pub use wdl_grammar as grammar;
#[cfg(feature = "lint")]
#[doc(inline)]
pub use wdl_lint as lint;
#[cfg(feature = "lsp")]
#[doc(inline)]
pub use wdl_lsp as lsp;

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    /// This is a test for checking that the reserved rules in `wdl-lint` match
    /// those from `wdl-analysis`.
    #[cfg(all(feature = "analysis", feature = "lint"))]
    #[test]
    fn reserved_rule_ids() {
        let rules: HashSet<_> = wdl_analysis::rules().iter().map(|r| r.id()).collect();
        let reserved: HashSet<_> = wdl_lint::RESERVED_RULE_IDS.iter().map(|id| *id).collect();

        for id in &reserved {
            if !rules.contains(id) {
                panic!("analysis rule `{id}` is not in the reservation set");
            }
        }

        for id in &rules {
            if !reserved.contains(id) {
                panic!("reserved rule `{id}` is not an analysis rule");
            }
        }
    }
}
