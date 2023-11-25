//! Workflow Description Language (WDL) document parsing and linting.

#![warn(missing_docs)]

#[cfg(feature = "ast")]
#[doc(inline)]
pub use wdl_ast as ast;
#[cfg(feature = "core")]
#[doc(inline)]
pub use wdl_core as core;
#[cfg(feature = "grammar")]
#[doc(inline)]
pub use wdl_grammar as grammar;
