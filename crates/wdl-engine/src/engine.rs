//! Implementation of the WDL evaluation engine.

use sysinfo::CpuRefreshKind;
use sysinfo::MemoryRefreshKind;
use sysinfo::System;
use wdl_analysis::diagnostics::unknown_type;
use wdl_analysis::document::Document;
use wdl_analysis::types::Type;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;

use crate::TaskExecutionBackend;

/// Represents an evaluation engine.
pub struct Engine {
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
            backend: Box::new(backend),
            system,
        }
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
        document
            .struct_by_name(name.as_str())
            .map(|s| s.ty().expect("struct should have type").clone())
            .ok_or_else(|| unknown_type(name.as_str(), name.span()))
    }
}
