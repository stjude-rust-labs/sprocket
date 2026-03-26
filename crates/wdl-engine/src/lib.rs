//! Execution engine for Workflow Description Language (WDL) documents.

use std::sync::LazyLock;

use num_enum::IntoPrimitive;
use sysinfo::CpuRefreshKind;
use sysinfo::MemoryRefreshKind;
use sysinfo::System;
use wdl_analysis::Document;
use wdl_analysis::diagnostics::unknown_type;
use wdl_analysis::types::Type;
use wdl_analysis::types::TypeNameResolver;
use wdl_analysis::types::v1::AstTypeConverter;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::TreeNode;

mod backend;
mod cache;
pub mod config;
mod diagnostics;
mod digest;
mod eval;
mod http;
mod inputs;
mod outputs;
mod path;
mod stdlib;
mod tree;
mod units;
mod value;

pub use config::Config;
pub use eval::*;
pub use inputs::*;
pub use outputs::*;
pub use path::*;
use units::*;
pub use value::*;

use crate::cache::Hashable;

/// One gibibyte (GiB) as a float.
///
/// This is defined as a constant as it's a commonly performed conversion.
const ONE_GIBIBYTE: f64 = 1024.0 * 1024.0 * 1024.0;

/// Resolves a type name from a document.
///
/// This function will import the type into the type cache if not already
/// cached.
fn resolve_type_name(document: &Document, name: &str, span: Span) -> Result<Type, Diagnostic> {
    document
        .struct_by_name(name)
        .map(|s| s.ty().expect("struct should have type").clone())
        .or_else(|| {
            document
                .enum_by_name(name)
                .map(|e| e.ty().expect("enum should have type").clone())
        })
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

/// Represents either file or directory content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, IntoPrimitive)]
#[repr(u8)]
enum ContentKind {
    /// The content is a single file.
    File,
    /// The content is a directory.
    Directory,
}

impl Hashable for ContentKind {
    fn hash(&self, hasher: &mut blake3::Hasher) {
        hasher.update(&[(*self).into()]);
    }
}

impl From<ContentKind> for crankshaft::engine::task::input::Type {
    fn from(value: ContentKind) -> Self {
        match value {
            ContentKind::File => Self::File,
            ContentKind::Directory => Self::Directory,
        }
    }
}
