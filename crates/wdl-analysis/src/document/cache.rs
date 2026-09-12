//! Caching layer for WDL document analysis.

mod hash;
#[cfg(test)]
mod tests;

use std::cmp::Ordering;
use std::hash::Hash;
use std::sync::Arc;

use indexmap::IndexMap;
use petgraph::prelude::DiGraphMap;
use sha2::Digest;
use sha2::Sha256;
use url::Url;
use wdl_ast::TreeNode;
use wdl_ast::v1::Ast;
use wdl_ast::v1::DocumentItem;
use wdl_ast::v1::EnumDefinition;
use wdl_ast::v1::ImportStatement;
use wdl_ast::v1::StructDefinition;
use wdl_grammar::Diagnostic;
use wdl_grammar::Span;
use wdl_grammar::SyntaxKind;

use crate::AppliedEdit;
use crate::Diagnostics;
use crate::Exceptable;
use crate::document::Enum;
use crate::document::ImportedEnum;
use crate::document::ImportedStruct;
use crate::document::ImportedTask;
use crate::document::ImportedWorkflow;
use crate::document::Input;
use crate::document::Namespace;
use crate::document::Output;
use crate::document::Struct;
use crate::document::Task;
use crate::document::Workflow;
use crate::document::cache::hash::HashableCallable;
use crate::document::cache::hash::HashableItem;
use crate::types::Type;

/// The kind of an [`LocalItem`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ItemKind {
    /// A struct.
    Struct,
    /// An enum.
    Enum,
    /// A task.
    Task,
    /// A workflow.
    Workflow,
    /// An import.
    Import,
}

/// The import merges its contents directly into the document's scope.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct MergingImport {
    /// Tasks imported via wildcard or selected import.
    pub(in crate::document) imported_tasks: IndexMap<String, ImportedTask>,
    /// Workflows imported via wildcard or selected import.
    pub(in crate::document) imported_workflows: IndexMap<String, ImportedWorkflow>,
    /// Structs imported via wildcard or selected import.
    ///
    /// NOTE: While this is separated from the [`Document`], imported
    /// structs/enums are copied into the document's scope and should
    /// be treated as though they were defined in the document.
    pub(in crate::document) imported_structs: IndexMap<String, ImportedStruct>,
    /// Enums imported via wildcard or selected import.
    pub(in crate::document) imported_enums: IndexMap<String, ImportedEnum>,
}

impl MergingImport {
    /// Gets all of the items that this import brings into scope.
    pub(crate) fn items(&self) -> impl Iterator<Item = ImportedItem<'_>> {
        self.imported_tasks
            .values()
            .map(ImportedItem::Task)
            .chain(self.imported_workflows.values().map(ImportedItem::Workflow))
            .chain(self.imported_structs.values().map(ImportedItem::Struct))
            .chain(self.imported_enums.values().map(ImportedItem::Enum))
    }
}

/// An import in a document.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Import {
    /// The import introduces a new namespace in the document.
    Namespace(Namespace),
    /// The import merges its contents directly into the document's scope.
    Merging(MergingImport),
}

impl Import {
    /// Get the [`MergingImport`] contents of the import if it's a merging
    /// import.
    pub(crate) fn merging(&self) -> Option<&MergingImport> {
        match self {
            Import::Merging(i) => Some(i),
            _ => None,
        }
    }

    /// Get the namespace of the import, if it has one.
    pub(crate) fn namespace(&self) -> Option<&Namespace> {
        match self {
            Import::Namespace(n) => Some(n),
            _ => None,
        }
    }

    /// Get a mutable reference to the namespace of the import, if it has one.
    fn namespace_mut(&mut self) -> Option<&mut Namespace> {
        match self {
            Import::Namespace(n) => Some(n),
            _ => None,
        }
    }

    /// Gets all of the structs introduced by this import.
    fn structs(&self) -> impl Iterator<Item = &ImportedStruct> {
        match self {
            Import::Namespace(n) => n.imported_structs.values(),
            Import::Merging(m) => m.imported_structs.values(),
        }
    }

    /// Add a struct to this import.
    pub(in crate::document) fn add_struct(&mut self, s: ImportedStruct) {
        let _ = match self {
            Import::Namespace(n) => n.imported_structs.insert(s.local_name.clone(), s),
            Import::Merging(m) => m.imported_structs.insert(s.local_name.clone(), s),
        };
    }

    /// Gets all of the enums introduced by this import.
    fn enums(&self) -> impl Iterator<Item = &ImportedEnum> {
        match self {
            Import::Namespace(n) => n.imported_enums.values(),
            Import::Merging(m) => m.imported_enums.values(),
        }
    }

    /// Add an enum to this import.
    pub(in crate::document) fn add_enum(&mut self, e: ImportedEnum) {
        let _ = match self {
            Import::Namespace(n) => n.imported_enums.insert(e.local_name.clone(), e),
            Import::Merging(m) => m.imported_enums.insert(e.local_name.clone(), e),
        };
    }
}

/// A reference to an externally defined item.
pub(crate) enum ImportedItem<'a> {
    /// A struct.
    Struct(&'a ImportedStruct),
    /// An enum.
    Enum(&'a ImportedEnum),
    /// A task.
    Task(&'a ImportedTask),
    /// A workflow.
    Workflow(&'a ImportedWorkflow),
}

impl<'a> ImportedItem<'a> {
    /// Gets the aliased name of the imported item.
    fn aliased_name(&self) -> &'a str {
        match self {
            ImportedItem::Struct(s) => &s.local_name,
            ImportedItem::Enum(e) => &e.local_name,
            ImportedItem::Task(t) => &t.local_name,
            ImportedItem::Workflow(w) => &w.local_name,
        }
    }
}

/// An item in the current document's scope.
#[derive(Copy, Clone, Debug)]
pub enum MaybeImported<Local, Imported> {
    /// The item is locally defined.
    Local(Local),
    /// The item was imported from another document.
    Imported(Imported),
}

impl<L, I> MaybeImported<L, I> {
    /// Returns true if the item was imported.
    pub fn is_imported(&self) -> bool {
        matches!(self, Self::Imported(_))
    }

    /// Returns the imported item.
    ///
    /// # Panics
    ///
    /// This will panic if the item was locally defined.
    pub fn expect_imported(self) -> I {
        match self {
            Self::Imported(i) => i,
            Self::Local(_) => panic!("expected an imported item"),
        }
    }

    /// Returns the contained local item.
    ///
    /// # Panics
    ///
    /// This will panic if the item was imported.
    pub fn expect_local(self) -> L {
        match self {
            Self::Local(l) => l,
            Self::Imported(_) => panic!("expected a locally defined item"),
        }
    }
}

/// A reference to an item in the document's scope.
pub(in crate::document) type Item<'a> = MaybeImported<CachedItemRef<'a>, ImportedItem<'a>>;

/// A reference to a workflow in the document's scope.
pub type WorkflowRef<'a> = MaybeImported<&'a Workflow, &'a ImportedWorkflow>;
impl<'a> WorkflowRef<'a> {
    /// Gets the name of the workflow.
    pub fn name(&self) -> &'a str {
        match self {
            WorkflowRef::Local(w) => w.name(),
            WorkflowRef::Imported(i) => &i.local_name,
        }
    }

    /// Gets the span of the name.
    pub fn name_span(&self) -> Span {
        match self {
            WorkflowRef::Local(w) => w.name_span(),
            WorkflowRef::Imported(i) => i.span,
        }
    }

    /// The inputs of the workflow.
    pub fn inputs(&self) -> Arc<IndexMap<String, Input>> {
        match self {
            WorkflowRef::Local(w) => Arc::clone(&w.inputs),
            WorkflowRef::Imported(i) => Arc::clone(&i.inputs),
        }
    }

    /// The outputs of the workflow.
    pub fn outputs(&self) -> Arc<IndexMap<String, Output>> {
        match self {
            WorkflowRef::Local(w) => Arc::clone(&w.outputs),
            WorkflowRef::Imported(i) => Arc::clone(&i.outputs),
        }
    }

    /// Gets the source of the workflow, if it was imported.
    pub fn source(&self) -> Option<Arc<Url>> {
        match self {
            WorkflowRef::Local(_) => None,
            WorkflowRef::Imported(i) => Some(i.source()),
        }
    }
}

/// A reference to a task in the document's scope.
pub type TaskRef<'a> = MaybeImported<&'a Task, &'a ImportedTask>;
impl<'a> TaskRef<'a> {
    /// Gets the name of the task.
    pub fn name(&self) -> &'a str {
        match self {
            TaskRef::Local(t) => t.name(),
            TaskRef::Imported(i) => &i.local_name,
        }
    }

    /// Gets the span of the name.
    pub fn name_span(&self) -> Span {
        match self {
            TaskRef::Local(t) => t.name_span(),
            TaskRef::Imported(i) => i.span,
        }
    }

    /// The inputs of the task.
    pub fn inputs(&self) -> Arc<IndexMap<String, Input>> {
        match self {
            TaskRef::Local(t) => Arc::clone(&t.inputs),
            TaskRef::Imported(i) => Arc::clone(&i.inputs),
        }
    }

    /// The outputs of the task.
    pub fn outputs(&self) -> Arc<IndexMap<String, Output>> {
        match self {
            TaskRef::Local(t) => Arc::clone(&t.outputs),
            TaskRef::Imported(i) => Arc::clone(&i.outputs),
        }
    }

    /// Gets the source of the task, if it was imported.
    pub fn source(&self) -> Option<Arc<Url>> {
        match self {
            TaskRef::Local(_) => None,
            TaskRef::Imported(i) => Some(i.source()),
        }
    }
}

/// A reference to a struct in the document's scope.
pub type StructRef<'a> = MaybeImported<&'a Struct, &'a ImportedStruct>;
impl<'a> StructRef<'a> {
    /// Gets the name of the struct.
    pub fn name(&self) -> &'a str {
        match self {
            StructRef::Local(s) => s.name(),
            StructRef::Imported(i) => &i.local_name,
        }
    }

    /// Gets the span of the name.
    pub fn name_span(&self) -> Span {
        match self {
            StructRef::Local(s) => s.name_span(),
            StructRef::Imported(i) => i.span,
        }
    }

    /// Gets the type of the struct, if it was computed.
    pub fn ty(&self) -> Option<&'a Type> {
        match self {
            StructRef::Local(s) => s.ty(),
            StructRef::Imported(i) => i.ty(),
        }
    }

    /// Gets the source of the struct, if it was imported.
    pub fn source(&self) -> Option<Arc<Url>> {
        match self {
            StructRef::Local(_) => None,
            StructRef::Imported(i) => Some(i.source()),
        }
    }

    /// Reconstructs the AST definition from the stored green node.
    pub fn definition(&self) -> StructDefinition {
        match self {
            StructRef::Local(s) => s.definition(),
            StructRef::Imported(i) => i.definition(),
        }
    }

    /// Gets the offset of the struct in the source document's CST.
    pub fn offset(&self) -> usize {
        match self {
            StructRef::Local(s) => s.offset(),
            StructRef::Imported(i) => i.offset(),
        }
    }

    /// Gets the node of the struct.
    pub fn node(&self) -> &'a rowan::GreenNode {
        match self {
            StructRef::Local(s) => s.node(),
            StructRef::Imported(i) => i.node(),
        }
    }
}

/// A reference to an enum in the document's scope.
pub type EnumRef<'a> = MaybeImported<&'a Enum, &'a ImportedEnum>;
impl<'a> EnumRef<'a> {
    /// Gets the name of the enum.
    pub fn name(&self) -> &'a str {
        match self {
            EnumRef::Local(e) => e.name(),
            EnumRef::Imported(i) => &i.local_name,
        }
    }

    /// Gets the span of the name.
    pub fn name_span(&self) -> Span {
        match self {
            EnumRef::Local(e) => e.name_span(),
            EnumRef::Imported(i) => i.span,
        }
    }

    /// Gets the type of the enum, if it was computed.
    pub fn ty(&self) -> Option<&'a Type> {
        match self {
            EnumRef::Local(e) => e.ty(),
            EnumRef::Imported(i) => i.ty(),
        }
    }

    /// Gets the source of the enum if it was imported.
    pub fn source(&self) -> Option<Arc<Url>> {
        match self {
            EnumRef::Local(_) => None,
            EnumRef::Imported(i) => Some(i.source()),
        }
    }

    /// Reconstructs the AST definition from the stored green node.
    pub fn definition(&self) -> EnumDefinition {
        match self {
            EnumRef::Local(e) => e.definition(),
            EnumRef::Imported(i) => i.definition(),
        }
    }

    /// Gets the offset of the enum.
    pub fn offset(&self) -> usize {
        match self {
            EnumRef::Local(e) => e.offset(),
            EnumRef::Imported(i) => i.offset(),
        }
    }

    /// Gets the node of the enum.
    pub fn node(&self) -> &'a rowan::GreenNode {
        match self {
            EnumRef::Local(e) => e.node(),
            EnumRef::Imported(i) => i.node(),
        }
    }
}

impl<'a> Item<'a> {
    /// Get the name of the item, if it introduces one.
    fn name(&self) -> Option<&'a str> {
        match self {
            Item::Local(i) => i.name(),
            Item::Imported(i) => Some(i.aliased_name()),
        }
    }

    /// Get the [`SignatureHash`] of the item, if it was locally defined.
    pub fn signature_hash(&self) -> Option<SignatureHash> {
        match self {
            Item::Local(i) => Some(i.signature_hash()),
            Item::Imported(_) => None,
        }
    }
}

/// A hash of an item's signature.
///
/// Any changes to the signature of an item will invalidate it and all of its
/// dependents.
pub(in crate::document) type SignatureHash = [u8; 32];
/// A hash of an item's body.
///
/// Any change to the body of an item will only invalidate itself.
pub(in crate::document) type BodyHash = [u8; 32];

/// An analyzed item with an associated [`BodyHash`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WithBodyHash<T> {
    /// The hash of the item's body.
    pub body_hash: BodyHash,
    /// The analyzed item.
    pub item: T,
}

/// A cached, analyzed document item.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedItem<T> {
    /// The hash for this item's signature.
    signature_hash: SignatureHash,
    /// The offset of the item in the document's CST.
    offset: usize,
    /// The analyzed item.
    item: T,
    /// Diagnostics produced during the analysis of this item.
    diagnostics: Diagnostics,
}

impl<T> CachedItem<T> {
    /// Create a new cached item.
    pub(in crate::document) fn new(
        signature_hash: SignatureHash,
        offset: usize,
        item: T,
        diagnostics: Diagnostics,
    ) -> Self {
        Self {
            signature_hash,
            offset,
            item,
            diagnostics,
        }
    }

    /// Get the item that this cached item represents.
    pub fn item(&self) -> &T {
        &self.item
    }

    /// Get a mutable reference to the item that this cached item represents.
    pub fn item_mut(&mut self) -> &mut T {
        &mut self.item
    }

    /// Overwrite the diagnostics for this cached item.
    ///
    /// NOTE: This expects diagnostics with absolute offsets.
    pub fn set_diagnostics(&mut self, diagnostics: Diagnostics) {
        self.diagnostics = diagnostics;
        self.shift_diagnostic_offsets();
    }

    /// Adds a diagnostic to this item.
    ///
    /// NOTE: This expects diagnostics with absolute offsets.
    pub(in crate::document) fn add_diagnostic(&mut self, mut diagnostic: Diagnostic) {
        diagnostic.offset(-(self.offset as isize));
        self.diagnostics.add(diagnostic);
    }

    /// See [`Diagnostics::exceptable_add()`]
    ///
    /// NOTE: This expects diagnostics with absolute offsets.
    pub(in crate::document) fn exceptable_add<N: TreeNode + Exceptable>(
        &mut self,
        mut diagnostic: Diagnostic,
        element: &N,
        exceptable_nodes: &Option<&'static [SyntaxKind]>,
    ) {
        diagnostic.offset(-(self.offset as isize));
        self.diagnostics
            .exceptable_add(diagnostic, element, exceptable_nodes);
    }

    /// Reposition the item's diagnostics to be relative to the item's offset,
    /// rather than the item's absolute offset in the document.
    fn shift_diagnostic_offsets(&mut self) {
        for diagnostic in &mut self.diagnostics.diagnostics {
            diagnostic.offset(-(self.offset as isize))
        }
    }
}

impl CachedItem<Struct> {
    /// Get the item this `CachedItem` wraps.
    fn target(&self) -> &Struct {
        &self.item
    }
}

impl CachedItem<Enum> {
    /// Get the item this `CachedItem` wraps.
    fn target(&self) -> &Enum {
        &self.item
    }
}

impl<T> CachedItem<WithBodyHash<T>> {
    /// Get the item this `CachedItem` wraps.
    fn target(&self) -> &T {
        &self.item.item
    }
}

/// A mutable reference to an item in the cache.
#[derive(Debug)]
pub(in crate::document) enum CachedItemRefMut<'a> {
    /// An analyzed struct.
    Struct(&'a mut CachedItem<Struct>),
    /// An analyzed enum.
    Enum(&'a mut CachedItem<Enum>),
    /// An analyzed task.
    Task(&'a mut CachedItem<WithBodyHash<Task>>),
    /// An analyzed workflow.
    Workflow(&'a mut CachedItem<WithBodyHash<Workflow>>),
    /// An analyzed import.
    Import(&'a mut CachedItem<WithBodyHash<Import>>),
}

impl CachedItemRefMut<'_> {
    /// Gets the current CST offset of the item.
    fn offset(&self) -> usize {
        match self {
            Self::Struct(s) => s.offset,
            Self::Enum(e) => e.offset,
            Self::Task(t) => t.offset,
            Self::Workflow(w) => w.offset,
            Self::Import(i) => i.offset,
        }
    }

    /// Gets a mutable reference to the diagnostics of the item.
    fn diagnostics_mut(&mut self) -> &mut Vec<Diagnostic> {
        match self {
            Self::Struct(s) => &mut s.diagnostics.diagnostics,
            Self::Enum(e) => &mut e.diagnostics.diagnostics,
            Self::Task(t) => &mut t.diagnostics.diagnostics,
            Self::Workflow(w) => &mut w.diagnostics.diagnostics,
            Self::Import(i) => &mut i.diagnostics.diagnostics,
        }
    }

    /// Shift the item's diagnostics based on the newly applied edits and the
    /// new item offset.
    ///
    /// When we first store an item in the cache, we shift all of its diagnostic
    /// spans to be relative to its position in the document. However, edits
    /// can occur that shift the item around without invalidating it (e.g.,
    /// adding comments/whitespace).
    ///
    /// For example, in the following document:
    ///
    /// ```wdl
    /// version 1.3
    ///
    /// task foo {
    ///     input {
    ///         String unused_input
    ///     }
    ///
    ///     command <<<>>>
    /// }
    /// ```
    ///
    /// If we make edits like:
    ///
    /// ```wdl
    /// version 1.3
    ///
    /// # Here's a comment that shifts the entire task down
    /// task foo {
    ///     # Woah! Here's a bunch of comments and whitespace
    ///
    ///     # This should shift the diagnostics around a lot!
    ///     input {
    ///         String unused_input
    ///     }
    ///
    ///     command <<<>>>
    /// }
    /// ```
    ///
    /// `foo` doesn't get invalidated. Instead, we're able to recalculate the
    /// new positions of the diagnostics based on the newly applied edits.
    fn shift_existing_diagnostics(&mut self, edits: &[AppliedEdit], new_item_offset: usize) {
        /// Shift an absolutely position span based on the given `edits`.
        fn shift_absolute_span(span: Span, edits: &[AppliedEdit]) -> Span {
            let mut start = span.start();
            let mut end = span.end();
            for edit in edits {
                let edit_start = edit.range.start;
                let edit_end = edit.range.end;
                let replacement_end = edit_start + edit.replacement_length;
                let edit_diff = edit.replacement_length as isize - edit.range.len() as isize;

                start = if start < edit_start {
                    start
                } else if start <= edit_end {
                    replacement_end
                } else {
                    start.saturating_add_signed(edit_diff)
                };

                end = if end < edit_start {
                    end
                } else if end <= edit_end {
                    replacement_end
                } else {
                    end.saturating_add_signed(edit_diff)
                };
            }
            Span::new(start, end - start)
        }

        if edits.is_empty() {
            // Nothing to do, might be from a full source replacement
            return;
        }

        let original_item_offset = self.offset();
        for diagnostic in self.diagnostics_mut() {
            for label in diagnostic.labels_mut() {
                let start_absolute = original_item_offset + label.span().start();
                let end_absolute = original_item_offset + label.span().end();
                let new_span = shift_absolute_span(
                    Span::new(start_absolute, end_absolute - start_absolute),
                    edits,
                );

                // Shrink it back to be relative to the item's offset
                let new_relative_start = new_span.start().saturating_sub(new_item_offset);
                label.set_span(Span::new(new_relative_start, new_span.len()));
            }
        }

        // Shift the spans of the items themselves
        match self {
            Self::Struct(s) => {
                let Struct {
                    name: _,
                    name_span,
                    offset: _,
                    node: _,
                    ty: _,
                } = &mut s.item;

                *name_span = shift_absolute_span(*name_span, edits)
            }
            Self::Enum(e) => {
                let Enum {
                    name: _,
                    name_span,
                    offset: _,
                    node: _,
                    ty: _,
                } = &mut e.item;

                *name_span = shift_absolute_span(*name_span, edits)
            }
            Self::Task(t) => {
                let Task {
                    name: _,
                    name_span,
                    span,
                    scopes,
                    inputs: _,
                    outputs,
                } = &mut t.item.item;

                *name_span = shift_absolute_span(*name_span, edits);
                *span = shift_absolute_span(*span, edits);
                for scope in scopes {
                    scope.span = shift_absolute_span(scope.span, edits);
                    for name in scope.names.values_mut() {
                        name.span = shift_absolute_span(name.span, edits);
                    }
                }
                for output in Arc::make_mut(outputs).values_mut() {
                    output.name_span = shift_absolute_span(output.name_span, edits);
                }
            }
            Self::Workflow(wf) => {
                let Workflow {
                    name: _,
                    name_span,
                    span,
                    scopes,
                    inputs: _,
                    outputs,
                    allows_nested_inputs: _,
                    calls: _,
                } = &mut wf.item.item;

                *name_span = shift_absolute_span(*name_span, edits);
                *span = shift_absolute_span(*span, edits);
                for scope in scopes {
                    scope.span = shift_absolute_span(scope.span, edits);
                    for name in scope.names.values_mut() {
                        name.span = shift_absolute_span(name.span, edits);
                    }
                }
                for output in Arc::make_mut(outputs).values_mut() {
                    output.name_span = shift_absolute_span(output.name_span, edits);
                }
            }
            Self::Import(i) => match &mut i.item.item {
                Import::Namespace(n) => {
                    let Namespace {
                        name: _,
                        span,
                        source: _,
                        document: _,
                        used: _,
                        imported_structs,
                        imported_enums,
                    } = n;

                    *span = shift_absolute_span(*span, edits);
                    for s in imported_structs.values_mut() {
                        s.span = shift_absolute_span(s.span, edits);
                    }
                    for e in imported_enums.values_mut() {
                        e.span = shift_absolute_span(e.span, edits);
                    }
                }
                Import::Merging(m) => {
                    let MergingImport {
                        imported_tasks,
                        imported_workflows,
                        imported_structs,
                        imported_enums,
                    } = m;

                    for t in imported_tasks.values_mut() {
                        t.span = shift_absolute_span(t.span, edits);
                    }
                    for w in imported_workflows.values_mut() {
                        w.span = shift_absolute_span(w.span, edits);
                    }
                    for s in imported_structs.values_mut() {
                        s.span = shift_absolute_span(s.span, edits);
                    }
                    for e in imported_enums.values_mut() {
                        e.span = shift_absolute_span(e.span, edits);
                    }
                }
            },
        }
    }

    /// Change the item's CST offset.
    fn swap_offset(&mut self, offset: usize) {
        match self {
            Self::Struct(s) => s.offset = offset,
            Self::Enum(e) => e.offset = offset,
            Self::Task(t) => t.offset = offset,
            Self::Workflow(w) => w.offset = offset,
            Self::Import(i) => i.offset = offset,
        }
    }
}

/// A reference to an item in the cache.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(in crate::document) enum CachedItemRef<'a> {
    /// An analyzed struct.
    Struct(&'a CachedItem<Struct>),
    /// An analyzed enum.
    Enum(&'a CachedItem<Enum>),
    /// An analyzed task.
    Task(&'a CachedItem<WithBodyHash<Task>>),
    /// An analyzed workflow.
    Workflow(&'a CachedItem<WithBodyHash<Workflow>>),
    /// An analyzed import.
    Import(&'a CachedItem<WithBodyHash<Import>>),
}

impl<'a> CachedItemRef<'a> {
    /// Get the name of the item, if it has one.
    pub fn name(&self) -> Option<&'a str> {
        match self {
            Self::Struct(s) => Some(s.item.name()),
            Self::Enum(e) => Some(e.item.name()),
            Self::Task(t) => Some(t.item.item.name()),
            Self::Workflow(w) => Some(w.item.item.name()),
            Self::Import(i) => match &i.item.item {
                Import::Merging { .. } => None,
                Import::Namespace(ns) => Some(ns.name()),
            },
        }
    }

    /// Get the diagnostics produced for this item.
    pub fn diagnostics(&self) -> impl Iterator<Item = Diagnostic> + use<'a> {
        let (offset, diagnostics) = match self {
            Self::Struct(s) => (s.offset, s.diagnostics.iter()),
            Self::Enum(e) => (e.offset, e.diagnostics.iter()),
            Self::Task(t) => (t.offset, t.diagnostics.iter()),
            Self::Workflow(w) => (w.offset, w.diagnostics.iter()),
            Self::Import(i) => (i.offset, i.diagnostics.iter()),
        };

        // We need to shift the diagnostics back to their absolute positions
        // within the document. `CachedItemRef` stores diagnostics
        // relative to the start of the item.
        diagnostics.cloned().map(move |mut d| {
            d.offset(offset as isize);
            d
        })
    }

    /// Get the [`SignatureHash`] of the item.
    pub fn signature_hash(&self) -> SignatureHash {
        match self {
            Self::Struct(s) => s.signature_hash,
            Self::Enum(e) => e.signature_hash,
            Self::Task(t) => t.signature_hash,
            Self::Workflow(w) => w.signature_hash,
            Self::Import(i) => i.signature_hash,
        }
    }

    /// Get the [`BodyHash`] of the item, if it has one.
    pub fn body_hash(&self) -> Option<BodyHash> {
        match self {
            Self::Import(i) => Some(i.item.body_hash),
            Self::Task(t) => Some(t.item.body_hash),
            Self::Workflow(w) => Some(w.item.body_hash),
            _ => None,
        }
    }
}

/// Extra data retained during test analysis runs.
#[cfg(test)]
#[derive(Debug, Clone, Default)]
struct TestCache {
    /// The list of items whose signatures were invalidated in the last pass.
    invalidated_signatures: Vec<SignatureHash>,
    /// The list of items whose bodies were invalidated in the last pass.
    invalidated_bodies: Vec<SignatureHash>,
}

/// A cache of a document's analyzed items.
#[derive(Debug, Clone, Default)]
pub(crate) struct AnalysisCache {
    /// Map of struct hashes to their cached analysis results.
    pub structs: IndexMap<SignatureHash, CachedItem<Struct>>,
    /// Map of enum hashes to their cached analysis results.
    pub enums: IndexMap<SignatureHash, CachedItem<Enum>>,
    /// Map of task hashes to their cached analysis results.
    pub tasks: IndexMap<SignatureHash, CachedItem<WithBodyHash<Task>>>,
    /// The workflow in the document.
    pub workflow: Option<CachedItem<WithBodyHash<Workflow>>>,
    /// Map of import hashes to their cached analysis results.
    pub imports: IndexMap<SignatureHash, CachedItem<WithBodyHash<Import>>>,
    /// Analysis item dependency graph.
    dependencies: DiGraphMap<SignatureHash, ()>,
    /// Extra data used for tests.
    #[cfg(test)]
    tests: TestCache,
}

impl PartialEq for AnalysisCache {
    fn eq(&self, other: &Self) -> bool {
        self.structs == other.structs
            && self.enums == other.enums
            && self.tasks == other.tasks
            && self.workflow == other.workflow
            && self.imports == other.imports
            && self
                .dependencies
                .all_edges()
                .all(|(a, b, _)| other.dependencies.contains_edge(a, b))
    }
}

/// Generates the common methods for local and imported items.
macro_rules! item_getters {
    (
        $(
            (
                $item_ty:ident, $cache_field:ident, $all_by_name:ident, $local_fn:ident, $local_fn_by_name:ident, $import_fn:ident, $import_fn_by_name:ident
            ) => ($ty:ty, $ref_ty:ident, $imported_ty:ty)
        ),+ $(,)+
    ) => {
        $(
        paste::paste! {
            #[doc = "Gets the " $item_ty "s locally defined in the document."]
            ///
            /// Returns `(index, hash, item)` tuples, where:
            ///
            /// * `index` - The position of the item in the cache. See
            #[doc = "[`Self::" $item_ty "_by_index()`]."]
            pub(crate) fn $local_fn(&self) -> impl Iterator<Item = (usize, SignatureHash, &$ty)> {
                self.$cache_field
                    .iter()
                    .enumerate()
                    .map(|(idx, (hash, i))| (idx, *hash, i.target()))
            }

            #[doc = "Gets a locally defined " $item_ty " in the document by name."]
            ///
            #[doc = "See: [`Self::" $local_fn "`]"]
            pub(crate) fn $local_fn_by_name(&self, name: &str) -> Option<(usize, SignatureHash, &$ty)> {
                self.$local_fn().find(|(_idx, _hash, i)| i.name() == name)
            }

            #[doc = "Gets the " $item_ty "s in the document."]
            ///
            /// NOTE: This includes both locally defined and imported items.
            ///
            #[doc = "See: [`Self::" $local_fn "`], [`Self::" $import_fn "`]."]
            pub(crate) fn $cache_field(&self) -> impl Iterator<Item = $ref_ty<'_>> {
                self.$local_fn()
                    .map(|(_idx, _hash, t)| $ref_ty::Local(t))
                    .chain(self.$import_fn().map(|(_hash, t)| $ref_ty::Imported(t)))
            }

            #[doc = "Gets a " $item_ty " in the document by name."]
            ///
            #[doc = "See: [`Self::" $local_fn_by_name "`], [`Self::" $import_fn_by_name "`]."]
            pub(crate) fn $all_by_name(&self, name: &str) -> Option<(SignatureHash, $ref_ty<'_>)> {
                self.$local_fn_by_name(name)
                    .map(|(_idx, hash, t)| (hash, $ref_ty::Local(t)))
                    .or_else(|| {
                        self.$import_fn_by_name(name)
                            .map(|(hash, t)| (hash, $ref_ty::Imported(t)))
                    })
            }

            #[doc = "Gets an imported " $item_ty " in the document by local name."]
            ///
            /// NOTE: This only includes imports in the current document's scope. Namespaced imports
            ///       are available through [`Self::namespaces()`].
            pub(crate) fn $import_fn_by_name(
                &self,
                name: &str,
            ) -> Option<(SignatureHash, &$imported_ty)> {
                self.$import_fn()
                    .find(|(_hash, t)| t.local_name == name)
            }
        }
        )+
    }
}

// Public getters
impl AnalysisCache {
    item_getters!(
        (task, tasks, task_by_name, local_tasks, local_task_by_name, imported_tasks, imported_task_by_name) => (Task, TaskRef, ImportedTask),
        (struct, structs, struct_by_name, local_structs, local_struct_by_name, imported_structs, imported_struct_by_name) => (Struct, StructRef, ImportedStruct),
        (enum, enums, enum_by_name, local_enums, local_enum_by_name, imported_enums, imported_enum_by_name) => (Enum, EnumRef, ImportedEnum),
    );

    /// Returns the number of items in the cache.
    pub fn len(&self) -> usize {
        let Self {
            structs,
            enums,
            tasks,
            workflow,
            imports,
            dependencies: _,
            #[cfg(test)]
                tests: _,
        } = self;

        structs.len() + enums.len() + tasks.len() + workflow.is_some() as usize + imports.len()
    }

    /// Returns whether the cache is empty.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Gets all imported structs in the document.
    ///
    /// NOTE: This includes structs from all import forms.
    pub(crate) fn imported_structs(
        &self,
    ) -> impl Iterator<Item = (SignatureHash, &ImportedStruct)> {
        self.imports()
            .flat_map(|(_idx, hash, i)| i.structs().map(move |s| (hash, s)))
    }

    /// Gets all imported enums in the document.
    ///
    /// NOTE: This includes enums from all import forms.
    pub(crate) fn imported_enums(&self) -> impl Iterator<Item = (SignatureHash, &ImportedEnum)> {
        self.imports()
            .flat_map(|(_idx, hash, i)| i.enums().map(move |e| (hash, e)))
    }

    /// Gets all imported tasks in the document.
    ///
    /// NOTE: This only includes tasks in the current document's scope (e.g.,
    /// those from select/wildcard imports).
    pub(crate) fn imported_tasks(&self) -> impl Iterator<Item = (SignatureHash, &ImportedTask)> {
        self.imports()
            .filter_map(|(_idx, hash, i)| i.merging().map(|m| (hash, m)))
            .flat_map(|(hash, i)| i.imported_tasks.values().map(move |t| (hash, t)))
    }

    /// Gets the import statements in the document.
    pub(crate) fn imports(&self) -> impl Iterator<Item = (usize, SignatureHash, &Import)> {
        self.imports
            .iter()
            .enumerate()
            .map(|(idx, (hash, i))| (idx, *hash, &i.item.item))
    }

    /// Gets the namespaces in the document.
    pub(crate) fn namespaces(&self) -> impl Iterator<Item = (SignatureHash, &Namespace)> {
        self.imports()
            .filter_map(|(_idx, hash, i)| i.namespace().map(|ns| (hash, ns)))
    }

    /// Gets a successfully resolved namespace in the document by name.
    pub fn namespace_by_name(&self, name: &str) -> Option<(SignatureHash, &Namespace)> {
        self.namespaces().find(|(_, ns)| ns.name == name)
    }

    /// Gets the workflow in the document.
    ///
    /// Returns `None` if the document did not contain a workflow.
    pub(crate) fn workflow(&self) -> Option<&Workflow> {
        self.workflow.as_ref().map(|i| &i.item.item)
    }

    /// Gets an imported workflow in the document by local name.
    ///
    /// NOTE: This only includes workflows in the current document's scope
    /// (e.g., those from select/wildcard imports).
    pub(crate) fn imported_workflow_by_name(
        &self,
        name: &str,
    ) -> Option<(SignatureHash, &ImportedWorkflow)> {
        self.imports()
            .filter_map(|(_idx, hash, i)| i.merging().map(|m| (hash, m)))
            .find_map(|(hash, i)| i.imported_workflows.get(name).map(|w| (hash, w)))
    }

    /// Gets all imported workflows in the document.
    ///
    /// NOTE: This only includes workflows in the current document's scope
    /// (e.g., those from select/wildcard imports).
    pub(crate) fn imported_workflows(
        &self,
    ) -> impl Iterator<Item = (SignatureHash, &ImportedWorkflow)> {
        self.imports()
            .filter_map(|(_idx, hash, i)| i.merging().map(|m| (hash, m)))
            .flat_map(|(hash, i)| i.imported_workflows.values().map(move |w| (hash, w)))
    }

    /// Gets a task in the document by name.
    ///
    /// See: [`Self::imported_workflow_by_name()`].
    pub(crate) fn workflow_by_name(&self, name: &str) -> Option<(SignatureHash, WorkflowRef<'_>)> {
        self.workflow
            .as_ref()
            .filter(|wf| wf.item.item.name == name)
            .map(|item| (item.signature_hash, WorkflowRef::Local(&item.item.item)))
            .or_else(|| {
                self.imported_workflow_by_name(name)
                    .map(|(hash, wf)| (hash, WorkflowRef::Imported(wf)))
            })
    }
}

// Private getters (only ever used in `populate_document`)
impl AnalysisCache {
    /// Returns an iterator over dirty items in `current_ast` that are missing
    /// from the cache.
    ///
    /// The iterator is ordered for the `populate_document` passes:
    ///
    /// 1. `import`
    /// 2. `struct`, `enum`
    /// 3. `task`, `workflow`
    pub(in crate::document) fn dirty<'a>(
        &self,
        current_ast: &'a AstItems,
    ) -> impl Iterator<Item = (SignatureHash, Option<BodyHash>, &'a DocumentItem)> {
        current_ast.items.iter().filter_map(move |ast_item| {
            match self.get(&ast_item.signature_hash) {
                Some(cache_item) => {
                    let Some(expected_body_hash) = cache_item.body_hash() else {
                        return None; // signature comparison is enough, not dirty
                    };

                    let Some(new_body_hash) = ast_item.body_hash else {
                        // This is only ever the case for imports. The
                        // `BodyHash` of imports is
                        // calculated separately in `Self::intersect()`. If it's
                        // still in the cache at this
                        // point, it isn't dirty.
                        return None;
                    };

                    if expected_body_hash == new_body_hash {
                        return None; // body unchanged, not dirty
                    }

                    Some((ast_item.signature_hash, ast_item.body_hash, &ast_item.item))
                }
                // Either a new or changed item
                None => Some((ast_item.signature_hash, ast_item.body_hash, &ast_item.item)),
            }
        })
    }

    /// Gets all of the items in the cache.
    fn items(&self) -> impl Iterator<Item = CachedItemRef<'_>> {
        self.structs
            .values()
            .map(CachedItemRef::Struct)
            .chain(self.enums.values().map(CachedItemRef::Enum))
            .chain(self.tasks.values().map(CachedItemRef::Task))
            .chain(
                self.workflow
                    .as_ref()
                    .into_iter()
                    .map(CachedItemRef::Workflow),
            )
            .chain(self.imports.values().map(CachedItemRef::Import))
    }

    /// Gets a mutable reference all of the items in the cache.
    pub(in crate::document) fn items_mut(&mut self) -> impl Iterator<Item = CachedItemRefMut<'_>> {
        self.structs
            .values_mut()
            .map(CachedItemRefMut::Struct)
            .chain(self.enums.values_mut().map(CachedItemRefMut::Enum))
            .chain(self.tasks.values_mut().map(CachedItemRefMut::Task))
            .chain(
                self.workflow
                    .as_mut()
                    .into_iter()
                    .map(CachedItemRefMut::Workflow),
            )
            .chain(self.imports.values_mut().map(CachedItemRefMut::Import))
    }

    /// Gets all of the diagnostics in the cache.
    pub(in crate::document) fn diagnostics(&self) -> impl Iterator<Item = Diagnostic> + use<'_> {
        self.items().flat_map(|i| i.diagnostics())
    }

    /// Looks up a cached item by hash.
    pub(in crate::document) fn get(&self, hash: &SignatureHash) -> Option<CachedItemRef<'_>> {
        self.structs
            .get(hash)
            .map(CachedItemRef::Struct)
            .or_else(|| self.enums.get(hash).map(CachedItemRef::Enum))
            .or_else(|| self.tasks.get(hash).map(CachedItemRef::Task))
            .or_else(|| {
                if self.workflow.as_ref().map(|w| &w.signature_hash) == Some(hash) {
                    self.workflow.as_ref().map(CachedItemRef::Workflow)
                } else {
                    None
                }
            })
            .or_else(|| self.imports.get(hash).map(CachedItemRef::Import))
    }

    /// Looks up a cached item by hash.
    fn get_mut(&mut self, hash: &SignatureHash) -> Option<CachedItemRefMut<'_>> {
        self.structs
            .get_mut(hash)
            .map(CachedItemRefMut::Struct)
            .or_else(|| self.enums.get_mut(hash).map(CachedItemRefMut::Enum))
            .or_else(|| self.tasks.get_mut(hash).map(CachedItemRefMut::Task))
            .or_else(|| {
                if self.workflow.as_ref().map(|w| &w.signature_hash) == Some(hash) {
                    self.workflow.as_mut().map(CachedItemRefMut::Workflow)
                } else {
                    None
                }
            })
            .or_else(|| self.imports.get_mut(hash).map(CachedItemRefMut::Import))
    }

    /// Get an item in the document's scope by name.
    pub(in crate::document) fn item_by_name(&self, name: &str) -> Option<Item<'_>> {
        self.items()
            .map(Item::Local)
            .chain(
                self.imports()
                    .filter_map(|(_idx, _hash, i)| i.merging())
                    .flat_map(|i| i.items().map(Item::Imported)),
            )
            .find(|i| i.name() == Some(name))
    }

    /// Gets a struct in the document at the given cache index.
    pub(in crate::document) fn struct_by_index(&self, index: usize) -> Option<&Struct> {
        Some(&self.structs.get_index(index)?.1.item)
    }

    /// Gets an enum in the document at the given cache index.
    pub(in crate::document) fn enum_by_index(&self, index: usize) -> Option<&Enum> {
        Some(&self.enums.get_index(index)?.1.item)
    }

    /// Gets all of the signature hashes in the cache.
    ///
    /// NOTE: These are not guaranteed to be in stable order between
    /// invalidations.
    fn keys(&self) -> impl Iterator<Item = &SignatureHash> {
        self.structs
            .keys()
            .chain(self.enums.keys())
            .chain(self.tasks.keys())
            .chain(self.workflow.as_ref().map(|w| &w.signature_hash))
            .chain(self.imports.keys())
    }

    /// Hash all of the exported symbols in the cache.
    pub(in crate::document) fn exports_hash(&self) -> BodyHash {
        let mut hasher = Sha256::default();

        // Invalidation, on both the signature and body level, will shift around
        // the keys in the cache. We need to sort them here to keep the
        // hash stable.
        let mut keys: Vec<_> = self.keys().collect();
        keys.sort_unstable();

        for signature in keys {
            hasher.update(signature);
        }
        hasher.finalize().into()
    }
}

/// The method for invalidating an existing cache item.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::document) enum InvalidationStrategy {
    /// The item's signature needs to be invalidated.
    Signature,
    /// The item's body needs to be invalidated.
    Body,
}

// Mutation (only ever used in `populate_document`)
impl AnalysisCache {
    /// Inserts an import into the cache.
    pub(in crate::document) fn insert_import(
        &mut self,
        mut item: CachedItem<WithBodyHash<Import>>,
    ) {
        let hash = item.signature_hash;
        item.shift_diagnostic_offsets();

        self.imports.insert(item.signature_hash, item);
        self.dependencies.add_node(hash);
    }

    /// Inserts an enum into the cache.
    pub(in crate::document) fn insert_enum(&mut self, mut item: CachedItem<Enum>) {
        let hash = item.signature_hash;
        item.shift_diagnostic_offsets();

        self.enums.insert(item.signature_hash, item);
        self.dependencies.add_node(hash);
    }

    /// Inserts a struct into the cache.
    pub(in crate::document) fn insert_struct(&mut self, mut item: CachedItem<Struct>) {
        let hash = item.signature_hash;
        item.shift_diagnostic_offsets();

        self.structs.insert(item.signature_hash, item);
        self.dependencies.add_node(hash);
    }

    /// Inserts a task into the cache.
    pub(in crate::document) fn insert_task(&mut self, mut item: CachedItem<WithBodyHash<Task>>) {
        let hash = item.signature_hash;
        item.shift_diagnostic_offsets();

        self.tasks.insert(item.signature_hash, item);
        self.dependencies.add_node(hash);
    }

    /// Inserts a workflow into the cache.
    pub(in crate::document) fn set_workflow(&mut self, item: CachedItem<WithBodyHash<Workflow>>) {
        // NOTE: We don't shift the diagnostics here. Workflow addition and
        // population are different steps. Diagnostics are shifted
        // *after* `populate_workflow()`.
        let hash = item.signature_hash;
        self.workflow = Some(item);
        self.dependencies.add_node(hash);
    }

    /// Gets the namespaces in the document.
    pub(in crate::document) fn namespaces_mut(&mut self) -> impl Iterator<Item = &mut Namespace> {
        self.imports
            .values_mut()
            .flat_map(|i| i.item.item.namespace_mut())
    }

    /// Gets a mutable reference to the cached item for the workflow in the
    /// document.
    pub(in crate::document) fn workflow_item_mut(
        &mut self,
    ) -> Option<&mut CachedItem<WithBodyHash<Workflow>>> {
        self.workflow.as_mut()
    }

    /// Gets a mutable reference to a `struct` `CachedItem` at the given index.
    pub(in crate::document) fn struct_item_mut(
        &mut self,
        index: usize,
    ) -> Option<&mut CachedItem<Struct>> {
        Some(self.structs.get_index_mut(index)?.1)
    }

    /// Remove an item by its [`SignatureHash`].
    ///
    /// NOTE: This does not invalidate the item's dependents.
    fn remove_item(&mut self, hash: &SignatureHash) {
        self.structs
            .shift_remove(hash)
            .map(|_| ())
            .or_else(|| self.enums.shift_remove(hash).map(|_| ()))
            .or_else(|| self.tasks.shift_remove(hash).map(|_| ()))
            .or_else(|| self.imports.shift_remove(hash).map(|_| ()));

        if self.workflow.as_ref().map(|w| &w.signature_hash) == Some(hash) {
            self.workflow = None;
        }
    }

    /// Invalidates the given items and all of their dependents from the cache.
    pub(in crate::document) fn invalidate(
        &mut self,
        ast_items: &AstItems,
        edits: &[AppliedEdit],
        hashes: impl IntoIterator<Item = (InvalidationStrategy, SignatureHash)>,
    ) {
        let mut dirty_set = std::collections::HashSet::new();
        for (strategy, hash) in hashes {
            match strategy {
                // Invalidate the item and every dependent.
                InvalidationStrategy::Signature => {
                    let mut stack = vec![hash];
                    while let Some(node) = stack.pop() {
                        if dirty_set.insert(node) {
                            #[cfg(test)]
                            self.tests.invalidated_signatures.push(node);

                            for dependent in self
                                .dependencies
                                .neighbors_directed(node, petgraph::Direction::Incoming)
                            {
                                stack.push(dependent);
                            }
                        }
                    }
                }
                // Invalidate the item only.
                InvalidationStrategy::Body => {
                    #[cfg(test)]
                    self.tests.invalidated_bodies.push(hash);

                    self.remove_item(&hash);
                    let outgoing: Vec<_> = self
                        .dependencies
                        .neighbors_directed(hash, petgraph::Direction::Outgoing)
                        .collect();
                    for dependency in outgoing {
                        self.dependencies.remove_edge(hash, dependency);
                    }
                }
            }
        }

        for hash in dirty_set {
            self.remove_item(&hash);
            self.dependencies.remove_node(hash);
        }

        for ast_item in &ast_items.items {
            let hash = ast_item.signature_hash;
            let Some(mut item) = self.get_mut(&hash) else {
                continue;
            };

            // Existing diagnostics need to be shifted
            item.shift_existing_diagnostics(edits, ast_item.offset);
            item.swap_offset(ast_item.offset);
        }
    }

    /// Drop items and their dependents from the cache that are not present in
    /// `current_ast` or whose body hash has changed.
    pub(in crate::document) fn intersect(
        &self,
        current_ast: &AstItems,
        mut resolve_import_body_hash: impl FnMut(&ImportStatement) -> Option<BodyHash>,
    ) -> Vec<(InvalidationStrategy, SignatureHash)> {
        let mut to_remove = Vec::new();
        for cache_item in self.items() {
            let signature_hash = cache_item.signature_hash();
            if !current_ast.contains(&signature_hash) {
                to_remove.push((InvalidationStrategy::Signature, signature_hash));
                continue;
            }

            let new_body_hash = match cache_item {
                CachedItemRef::Import(_) => {
                    let import_stmt = current_ast
                        .imports()
                        .find(|(h, _)| **h == signature_hash)
                        .map(|(_, i)| i)
                        .expect("should exist because current_ast contains hash");
                    resolve_import_body_hash(import_stmt)
                }
                _ => current_ast.get_body_hash(&signature_hash),
            };

            if cache_item.body_hash() != new_body_hash {
                if matches!(cache_item, CachedItemRef::Import(_)) {
                    to_remove.push((InvalidationStrategy::Signature, signature_hash));
                } else {
                    to_remove.push((InvalidationStrategy::Body, signature_hash));
                }
            }
        }

        to_remove
    }

    /// Gets a mutable reference to an `enum` `CachedItem` at the given index.
    pub(in crate::document) fn enum_item_mut(
        &mut self,
        index: usize,
    ) -> Option<&mut CachedItem<Enum>> {
        Some(self.enums.get_index_mut(index)?.1)
    }

    /// Adds a dependency edge to the graph.
    pub(in crate::document) fn add_dependency(
        &mut self,
        dependent: SignatureHash,
        dependency: SignatureHash,
    ) {
        self.dependencies.add_edge(dependent, dependency, ());
    }
}

/// Represents an item in the AST.
struct AstItem {
    /// The signature hash of the item.
    signature_hash: SignatureHash,
    /// The body hash of the item, if applicable.
    body_hash: Option<BodyHash>,
    /// The CST offset of the item.
    offset: usize,
    /// The item itself.
    item: DocumentItem,
}

/// A collection of the items in the document's AST.
pub(in crate::document) struct AstItems {
    /// The items in the AST.
    items: Vec<AstItem>,
}

impl AstItems {
    /// Checks if the AST contains an item with the given [`SignatureHash`].
    pub fn contains(&self, signature_hash: &SignatureHash) -> bool {
        self.items
            .iter()
            .any(|item| &item.signature_hash == signature_hash)
    }

    /// Gets the body hash of an item by its [`SignatureHash`].
    pub fn get_body_hash(&self, signature_hash: &SignatureHash) -> Option<BodyHash> {
        self.items.iter().find_map(|item| {
            if &item.signature_hash == signature_hash {
                item.body_hash
            } else {
                None
            }
        })
    }

    /// Get the number of items in the AST.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Gets all of the import statements in the document.
    fn imports(&self) -> impl Iterator<Item = (&SignatureHash, &ImportStatement)> {
        self.items.iter().filter_map(|item| match &item.item {
            DocumentItem::Import(i) => Some((&item.signature_hash, i)),
            _ => None,
        })
    }

    /// Create a new [`AstItems`] from the document's AST.
    pub fn new(ast: &Ast) -> Self {
        #[derive(PartialEq, Eq)]
        struct DocumentItemOrd<'a>(&'a DocumentItem);

        impl DocumentItemOrd<'_> {
            fn ord(&self) -> u8 {
                match self.0 {
                    DocumentItem::Import(_) => 0,
                    DocumentItem::Struct(_) | DocumentItem::Enum(_) => 1,
                    DocumentItem::Task(_) | DocumentItem::Workflow(_) => 2,
                }
            }
        }

        impl PartialOrd for DocumentItemOrd<'_> {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for DocumentItemOrd<'_> {
            fn cmp(&self, other: &Self) -> Ordering {
                self.ord().cmp(&other.ord())
            }
        }

        let mut items = Vec::new();

        for item in ast.items() {
            let offset = usize::from(item.inner().text_range().start());
            let (signature_hash, body_hash) = match &item {
                DocumentItem::Import(i) => (HashableItem::hash(i), None),
                DocumentItem::Struct(s) => (HashableItem::hash(s), None),
                DocumentItem::Enum(e) => (HashableItem::hash(e), None),
                DocumentItem::Task(t) => {
                    let (signature_hash, body_hash) = t.hash_callable();
                    (signature_hash, Some(body_hash))
                }
                DocumentItem::Workflow(w) => {
                    let (signature_hash, body_hash) = w.hash_callable();
                    (signature_hash, Some(body_hash))
                }
            };

            items.push(AstItem {
                signature_hash,
                body_hash,
                offset,
                item,
            });
        }

        items.sort_by(|a, b| DocumentItemOrd(&a.item).cmp(&DocumentItemOrd(&b.item)));

        Self { items }
    }
}
