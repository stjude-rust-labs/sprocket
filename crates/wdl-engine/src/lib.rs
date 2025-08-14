//! Execution engine for Workflow Description Language (WDL) documents.

mod backend;
pub mod config;
pub mod diagnostics;
mod eval;
pub(crate) mod http;
mod inputs;
mod outputs;
pub mod path;
mod stdlib;
pub(crate) mod tree;
mod units;
mod value;

use std::sync::LazyLock;

pub use backend::*;
pub use eval::*;
pub use inputs::*;
pub use outputs::*;
use sysinfo::CpuRefreshKind;
use sysinfo::MemoryRefreshKind;
use sysinfo::System;
pub use units::*;
pub use value::*;
use wdl_analysis::Document;
use wdl_analysis::diagnostics::unknown_type;
use wdl_analysis::types::Type;
use wdl_analysis::types::TypeNameResolver;
use wdl_analysis::types::v1::AstTypeConverter;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::TreeNode;

/// One gibibyte (GiB) as a float.
///
/// This is defined as a constant as it's a commonly performed conversion.
const ONE_GIBIBYTE: f64 = 1024.0 * 1024.0 * 1024.0;

/// One megabyte (MB) as a float.
///
/// This is defined as a constant as it's a commonly performed conversion.
const ONE_MEGABYTE: f64 = 1000.0 * 1000.0;

/// Resolves a type name from a document.
///
/// This function will import the type into the type cache if not already
/// cached.
fn resolve_type_name(document: &Document, name: &str, span: Span) -> Result<Type, Diagnostic> {
    document
        .struct_by_name(name)
        .map(|s| s.ty().expect("struct should have type").clone())
        .ok_or_else(|| unknown_type(name, span))
}

/// Converts a V1 AST type to an analysis type.
fn convert_ast_type_v1<N: TreeNode>(
    document: &Document,
    ty: &wdl_ast::v1::Type<N>,
) -> Result<Type, Diagnostic> {
    /// Used to resolve a type name from a document.
    struct Resolver<'a>(&'a Document);

    impl TypeNameResolver for Resolver<'_> {
        fn resolve(&mut self, name: &str, span: Span) -> Result<Type, Diagnostic> {
            resolve_type_name(self.0, name, span)
        }
    }

    AstTypeConverter::new(Resolver(document)).convert_type(ty)
}

/// Cached information about the host system.
static SYSTEM: LazyLock<System> = LazyLock::new(|| {
    let mut system = System::new();
    system.refresh_cpu_list(CpuRefreshKind::nothing());
    system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
    system
});
