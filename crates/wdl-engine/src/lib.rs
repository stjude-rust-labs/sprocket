//! Execution engine for Workflow Description Language (WDL) documents.

mod backend;
pub mod config;
pub mod diagnostics;
mod eval;
mod inputs;
mod outputs;
mod stdlib;
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
use wdl_analysis::diagnostics::unknown_type;
use wdl_analysis::document::Document;
use wdl_analysis::types::Type;
use wdl_analysis::types::TypeNameResolver;
use wdl_analysis::types::v1::AstTypeConverter;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;

/// Resolves a type name from a document.
///
/// This function will import the type into the type cache if not already
/// cached.
fn resolve_type_name(document: &Document, name: &Ident) -> Result<Type, Diagnostic> {
    document
        .struct_by_name(name.as_str())
        .map(|s| s.ty().expect("struct should have type").clone())
        .ok_or_else(|| unknown_type(name.as_str(), name.span()))
}

/// Converts a V1 AST type to an analysis type.
fn convert_ast_type_v1(document: &Document, ty: &wdl_ast::v1::Type) -> Result<Type, Diagnostic> {
    /// Used to resolve a type name from a document.
    struct Resolver<'a> {
        /// The document containing the type name to resolve.
        document: &'a Document,
    }

    impl TypeNameResolver for Resolver<'_> {
        fn resolve(&mut self, name: &Ident) -> Result<Type, Diagnostic> {
            resolve_type_name(self.document, name)
        }
    }

    AstTypeConverter::new(Resolver { document }).convert_type(ty)
}

/// Cached information about the host system.
static SYSTEM: LazyLock<System> = LazyLock::new(|| {
    let mut system = System::new();
    system.refresh_cpu_list(CpuRefreshKind::nothing());
    system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
    system
});
