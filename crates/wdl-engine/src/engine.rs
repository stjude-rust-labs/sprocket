//! Implementation of the WDL evaluation engine.

use std::collections::HashMap;
use std::sync::Arc;

use sysinfo::CpuRefreshKind;
use sysinfo::MemoryRefreshKind;
use sysinfo::System;
use wdl_analysis::diagnostics::unknown_type;
use wdl_analysis::document::Document;
use wdl_analysis::types::Type;
use wdl_analysis::types::Types;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;

use crate::TaskExecutionBackend;

/// Represents a cache of imported types for a specific document.
///
/// Maps a document-specific type name to a previously imported type.
#[derive(Debug, Default)]
struct DocumentTypeCache(HashMap<String, Type>);

/// Represents a cache of imported types for all evaluated documents.
///
/// Maps a document identifier to that document's type cache.
#[derive(Debug, Default)]
struct TypeCache(HashMap<Arc<String>, DocumentTypeCache>);

/// Represents an evaluation engine.
pub struct Engine {
    /// The types collection for evaluation.
    types: Types,
    /// The type cache for evaluation.
    cache: TypeCache,
    /// The task execution backend to use.
    backend: Box<dyn TaskExecutionBackend>,
    /// Information about the current system.
    system: System,
}

impl Engine {
    /// Constructs a new engine for the given task execution backend.
    pub fn new<B: TaskExecutionBackend + 'static>(backend: B) -> Self {
        let mut system = System::new();
        system.refresh_cpu_list(CpuRefreshKind::new());
        system.refresh_memory_specifics(MemoryRefreshKind::new().with_ram());

        Self {
            types: Default::default(),
            cache: Default::default(),
            backend: Box::new(backend),
            system,
        }
    }

    /// Gets the engine's type collection.
    pub fn types(&self) -> &Types {
        &self.types
    }

    /// Gets a mutable reference to the engine's type collection.
    pub fn types_mut(&mut self) -> &mut Types {
        &mut self.types
    }

    /// Gets a reference to the task execution backend.
    pub fn backend(&self) -> &dyn TaskExecutionBackend {
        self.backend.as_ref()
    }

    /// Gets information about the system the engine is running on.
    pub fn system(&self) -> &System {
        &self.system
    }

    /// Resolves a type name from a document.
    ///
    /// This function will import the type into the engine's type collection if
    /// not already cached.
    pub(crate) fn resolve_type_name(
        &mut self,
        document: &Document,
        name: &Ident,
    ) -> Result<Type, Diagnostic> {
        let cache = self.cache.0.entry(document.id().clone()).or_default();

        match cache.0.get(name.as_str()) {
            Some(ty) => Ok(*ty),
            None => {
                let ty = document
                    .struct_by_name(name.as_str())
                    .map(|s| s.ty().expect("struct should have type"))
                    .ok_or_else(|| unknown_type(name.as_str(), name.span()))?;

                // Cache the imported type for future expression evaluations
                let ty = self.types.import(document.types(), ty);
                cache.0.insert(name.as_str().into(), ty);
                Ok(ty)
            }
        }
    }
}
