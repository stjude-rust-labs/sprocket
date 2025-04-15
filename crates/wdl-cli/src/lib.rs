//! Facilities for more elegantly exposing `wdl` crate functionality from the
//! command line.

pub mod analysis;
pub mod eval;
pub mod inputs;

pub use analysis::Analysis;
pub use eval::Evaluator;
pub use inputs::Input;
pub use inputs::Inputs;
