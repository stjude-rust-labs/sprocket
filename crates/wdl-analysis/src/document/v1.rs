//! Conversion of a V1 AST to an analyzed document.
use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;
use std::hash::RandomState;
use std::sync::Arc;

use indexmap::IndexMap;
use indexmap::map::Entry as IndexMapEntry;
use itertools::EitherOrBoth;
use itertools::Itertools as _;
use petgraph::Direction;
use petgraph::algo::has_path_connecting;
use petgraph::algo::toposort;
use petgraph::graph::NodeIndex;
use petgraph::prelude::DiGraphMap;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxNode;
use wdl_ast::TreeNode;
use wdl_ast::TreeToken;
use wdl_ast::v1::Ast;
use wdl_ast::v1::CallStatement;
use wdl_ast::v1::CommandPart;
use wdl_ast::v1::ConditionalStatement;
use wdl_ast::v1::ConditionalStatementClauseKind;
use wdl_ast::v1::Decl;
use wdl_ast::v1::DocumentItem;
use wdl_ast::v1::EnumDefinition;
use wdl_ast::v1::Expr;
use wdl_ast::v1::ImportForm;
use wdl_ast::v1::ImportSource;
use wdl_ast::v1::ImportStatement;
use wdl_ast::v1::LiteralExpr;
use wdl_ast::v1::ScatterStatement;
use wdl_ast::v1::StringPart;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::TypeRef;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::version::V1;
use wdl_grammar::SyntaxKind;

use super::Document;
use super::DocumentData;
use super::Enum;
use super::ImportedEnum;
use super::ImportedStruct;
use super::ImportedTask;
use super::ImportedWorkflow;
use super::Input;
use super::Namespace;
use super::Output;
use super::Scope;
use super::ScopeIndex;
use super::ScopeRef;
use super::ScopeRefMut;
use super::ScopeUnion;
use super::Struct;
use super::TASK_VAR_NAME;
use super::Task;
use super::Workflow;
use crate::Diagnostics;
use crate::Exceptable;
use crate::MisleadingDeclarationOrderRule;
use crate::UnusedCallRule;
use crate::UnusedDeclarationRule;
use crate::UnusedImportRule;
use crate::UnusedInputRule;
use crate::config::Config;
use crate::config::DiagnosticsConfig;
use crate::diagnostics::Context;
use crate::diagnostics::Io;
use crate::diagnostics::NameContext;
use crate::diagnostics::call_input_type_mismatch;
use crate::diagnostics::duplicate_workflow;
use crate::diagnostics::else_if_not_supported;
use crate::diagnostics::else_not_supported;
use crate::diagnostics::enum_conflicts_with_import;
use crate::diagnostics::enum_not_supported;
use crate::diagnostics::if_conditional_mismatch;
use crate::diagnostics::import_cycle;
use crate::diagnostics::import_failure;
use crate::diagnostics::import_missing_version;
use crate::diagnostics::imported_enum_conflict;
use crate::diagnostics::imported_struct_conflict;
use crate::diagnostics::incompatible_import;
use crate::diagnostics::invalid_relative_import;
use crate::diagnostics::misleading_declaration_order;
use crate::diagnostics::missing_call_input;
use crate::diagnostics::name_conflict;
use crate::diagnostics::namespace_conflict;
use crate::diagnostics::non_empty_array_assignment;
use crate::diagnostics::non_literal_enum_value;
use crate::diagnostics::only_one_namespace;
use crate::diagnostics::recursive_enum;
use crate::diagnostics::recursive_struct;
use crate::diagnostics::recursive_workflow_call;
use crate::diagnostics::selected_import_conflict;
use crate::diagnostics::selected_member_not_found;
use crate::diagnostics::struct_conflicts_with_import;
use crate::diagnostics::type_is_not_array;
use crate::diagnostics::type_mismatch;
use crate::diagnostics::type_not_in_document;
use crate::diagnostics::unknown_call_io;
use crate::diagnostics::unknown_name;
use crate::diagnostics::unknown_namespace;
use crate::diagnostics::unknown_task_or_workflow;
use crate::diagnostics::unknown_type;
use crate::diagnostics::unused_call;
use crate::diagnostics::unused_declaration;
use crate::diagnostics::unused_import;
use crate::diagnostics::unused_input;
use crate::diagnostics::wildcard_import_conflict;
use crate::diagnostics::workflow_conflict;
use crate::document::Name;
use crate::document::cache::*;
use crate::eval::v1::TaskGraphBuilder;
use crate::eval::v1::TaskGraphNode;
use crate::eval::v1::WorkflowGraphBuilder;
use crate::eval::v1::WorkflowGraphNode;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::types::ArrayType;
use crate::types::CallKind;
use crate::types::CallType;
use crate::types::Coercible;
use crate::types::CompoundType;
use crate::types::EnumType;
use crate::types::HiddenType;
use crate::types::MapType;
use crate::types::Optional;
use crate::types::PairType;
use crate::types::PrimitiveType;
use crate::types::Type;
use crate::types::TypeNameRef;
use crate::types::TypeNameResolver;
use crate::types::v1::AstTypeConverter;
use crate::types::v1::ExprTypeEvaluator;

/// Adds a scope to a list of scopes.
///
/// Returns the index of the newly added scope.
fn add_scope(scopes: &mut Vec<Scope>, scope: Scope) -> ScopeIndex {
    let index = ScopeIndex(scopes.len());
    scopes.push(scope);
    index
}

/// Sorts a list of scopes by the start of the scope.
///
/// This handles remapping any parent indexes in each scope.
fn sort_scopes(scopes: &mut Vec<Scope>) {
    // To sort the scopes, we need to start by mapping the old indexes to scope span
    // start
    let mut remapped = scopes
        .iter()
        .enumerate()
        .map(|(i, s)| (i, s.span.start()))
        .collect::<HashMap<_, _>>();

    // Now sort the scopes by the start
    scopes.sort_by_key(|s| s.span.start());

    // Update the remapping so that it now maps old index -> new index
    for v in remapped.values_mut() {
        *v = scopes
            .binary_search_by_key(v, |s| s.span.start())
            .expect("should have exact match");
    }

    // Finally, update the parent indexes in the sorted scopes
    for scope in scopes {
        if let Some(parent) = &mut scope.parent {
            *parent = ScopeIndex(remapped[&parent.0]);
        }
    }
}

/// Creates a new document for a V1 AST with optional caching.
pub(crate) fn populate_document(
    document: &mut DocumentData,
    config: &Config,
    graph: &mut DocumentGraph,
    index: NodeIndex,
    ast: &Ast,
    edits: &[crate::analyzer::AppliedEdit],
) {
    assert!(
        matches!(
            document.version.expect("document should have a version"),
            SupportedVersion::V1(_)
        ),
        "expected a supported V1 version"
    );

    let ast_items = AstItems::new(ast);
    document.cache.intersect(&ast_items, edits, |import| {
        resolve_import(graph, import, index)
            .ok()
            .and_then(|(_, cache)| cache.map(|c| c.exports_hash()))
    });

    let dirty_items: Vec<_> = document.cache.dirty(&ast_items).collect();
    if dirty_items.is_empty() {
        tracing::trace!(
            document = document.uri.as_str(),
            "already analyzed, nothing to do",
        );
        document
            .analysis_diagnostics
            .extend(document.cache.diagnostics());
        return;
    }

    tracing::trace!(
        document = document.uri.as_str(),
        "cache dirty, re-analyzing {} items",
        dirty_items.len()
    );

    // Pre-populate all of the lint exceptions
    document.analysis_diagnostics = Diagnostics::default();
    document.analysis_diagnostics.add_exceptions(
        std::iter::successors(ast.inner().first_token(), |t| t.next_token())
            .filter_map(|t| Comment::cast(t)?.directive()?.into_except())
            .flatten(),
    );

    let mut import_nodes_by_namespace = HashMap::new();

    // First pass: imports, struct definitions, enum definitions
    for (signature_hash, _body_hash, item) in &dirty_items {
        match item {
            DocumentItem::Import(import) => {
                add_import(
                    *signature_hash,
                    document,
                    graph,
                    import,
                    index,
                    &mut import_nodes_by_namespace,
                );
            }
            DocumentItem::Struct(s) => {
                add_struct(document, *signature_hash, s);
            }
            DocumentItem::Enum(e) => {
                add_enum(document, *signature_hash, e);
            }
            DocumentItem::Task(_) | DocumentItem::Workflow(_) => continue,
        }
    }

    // Populate the types if any struct or enum is missing populated types
    if document
        .cache
        .local_structs()
        .any(|(_idx, _hash, s)| s.ty().is_none())
        || document
            .cache
            .local_enums()
            .any(|(_idx, _hash, e)| e.ty().is_none())
    {
        populate_types(document);
    }

    // Second pass: tasks and workflows
    let mut workflow_to_populate = None;
    for (signature_hash, body_hash, item) in &dirty_items {
        match item {
            DocumentItem::Task(task) => {
                add_task(
                    config,
                    document,
                    *signature_hash,
                    body_hash.expect("tasks should have a body hash"),
                    task,
                );
            }
            DocumentItem::Workflow(ast_wf) => {
                let body_hash = body_hash.expect("workflows should have a body hash");
                if add_workflow(document, *signature_hash, body_hash, ast_wf) {
                    workflow_to_populate = Some((*signature_hash, body_hash, ast_wf));
                }
            }
            DocumentItem::Import(_) | DocumentItem::Struct(_) | DocumentItem::Enum(_) => continue,
        }
    }

    if let Some((signature_hash, _body_hash, ast_wf)) = workflow_to_populate {
        populate_workflow(config, document, ast_wf, signature_hash);
        if let Some(item) = document.cache.workflow_item_mut() {
            item.diagnostics
                .append(&mut document.analysis_diagnostics.diagnostics);
            item.shift_diagnostic_offsets();
        }
    }

    if let Some(severity) = document.config.diagnostics_config().unused_import {
        for item in document.cache.items_mut() {
            let CachedItemRefMut::Import(import) = item else {
                continue;
            };

            let Some(ns) = import.item.item.namespace() else {
                continue;
            };

            if ns.used {
                continue;
            }

            let Some(node) = import_nodes_by_namespace.get(ns.name()) else {
                continue;
            };

            document.analysis_diagnostics.exceptable_add(
                unused_import(ns.name(), ns.span).with_severity(severity),
                node,
                &UnusedImportRule::EXCEPTABLE_NODES,
            );

            import
                .diagnostics
                .append(&mut document.analysis_diagnostics.diagnostics);
        }
    }

    // Fold all item diagnostics together
    document
        .analysis_diagnostics
        .extend(document.cache.diagnostics());
}

/// Add an import to the document.
fn add_import(
    ast_hash: SignatureHash,
    document: &mut DocumentData,
    graph: &DocumentGraph,
    import: &ImportStatement,
    importer_index: NodeIndex,
    import_nodes_by_namespace: &mut HashMap<String, SyntaxNode>,
) {
    // Start by resolving the import to its document
    let (imported_doc, cache) = match resolve_import(graph, import, importer_index) {
        Ok((doc, cache)) => (
            doc,
            cache.expect("imported document should be analyzed already"),
        ),
        Err(Some(diagnostic)) => {
            match import.form() {
                ImportForm::Selected => {
                    record_failed_selected_imports(document, import);
                }
                ImportForm::Wildcard => {
                    document.failed_wildcard_import = true;
                }
                ImportForm::Namespace => {}
            }

            document.analysis_diagnostics.add(diagnostic);
            if let Some((ns, _)) = import.namespace() {
                document.failed_imports.insert(ns, import.source().span());
            }
            return;
        }
        Err(None) => return,
    };

    // The `BodyHash` of an import statement is the hash of the source document's
    // exported symbols.
    let import_body_hash = cache.exports_hash();
    match import.form() {
        ImportForm::Namespace => {
            let added = add_namespace(ast_hash, import_body_hash, document, imported_doc, import);
            if added && let Some((ns, _span)) = import.namespace() {
                import_nodes_by_namespace.insert(ns.to_string(), import.inner().clone());
            }
        }
        ImportForm::Wildcard => {
            add_wildcard_import(ast_hash, import_body_hash, document, imported_doc, import);
        }
        ImportForm::Selected => {
            add_selected_import(ast_hash, import_body_hash, document, imported_doc, import);
        }
    }
}

/// Adds a namespace to the document.
///
/// Returns `true` if the namespace was added, otherwise a diagnostic was
/// emitted.
fn add_namespace(
    signature_hash: SignatureHash,
    body_hash: BodyHash,
    document: &mut DocumentData,
    imported_doc: Document,
    import: &ImportStatement,
) -> bool {
    let (ns, span) = match import.namespace() {
        Some((ns, span)) => {
            let existing = document
                .cache
                .namespace_by_name(&ns)
                .map(|(_, prev)| prev.span)
                .or_else(|| document.failed_imports.get(&ns).copied());
            match existing {
                Some(prev_span) => {
                    document.analysis_diagnostics.add(namespace_conflict(
                        &ns,
                        span,
                        prev_span,
                        import.explicit_namespace().is_none(),
                    ));
                    return false;
                }
                None => (ns, span),
            }
        }
        None => {
            // Invalid import namespaces are caught during validation, so there is already a
            // diagnostic for this issue; ignore the import here
            return false;
        }
    };

    let mut cache_item = Import::Namespace(Namespace {
        name: ns,
        span,
        source: imported_doc.uri(),
        document: imported_doc.clone(),
        used: false,
        imported_structs: Default::default(),
        imported_enums: Default::default(),
    });

    // Get the alias map for the namespace (for structs)
    let mut failed = false;
    let aliases = import
        .aliases()
        .filter_map(|a| {
            let (from, to) = a.names();

            let is_struct = imported_doc
                .data
                .cache
                .struct_by_name(from.text())
                .is_some();
            let is_enum = imported_doc.data.cache.enum_by_name(from.text()).is_some();

            // Check to see if the type name exists in the document
            if !is_struct && !is_enum {
                document
                    .analysis_diagnostics
                    .add(type_not_in_document(&from));
                failed = true;
                return None;
            }

            if is_enum {
                // We'll process enums below
                return None;
            }

            Some((from.text().to_string(), to))
        })
        .collect::<HashMap<_, _>>();

    // Insert the imported document's struct definitions
    for s in imported_doc.data.cache.structs() {
        let (span, aliased_name, aliased) = aliases
            .get(s.name())
            .map(|n| (n.span(), n.text(), true))
            .unwrap_or_else(|| (span, s.name(), false));
        match document.cache.struct_by_name(aliased_name) {
            Some((_hash, prev)) => {
                let a = prev.definition();
                let b = s.definition();
                if !are_structs_equal(&a, &b) {
                    match prev {
                        // Import conflicts with a struct defined in this document
                        StructRef::Local(prev) => {
                            document
                                .analysis_diagnostics
                                .add(struct_conflicts_with_import(
                                    aliased_name,
                                    prev.name_span,
                                    span,
                                ));
                        }
                        StructRef::Imported(prev) => {
                            document.analysis_diagnostics.add(imported_struct_conflict(
                                aliased_name,
                                span,
                                prev.span,
                                !aliased,
                            ));
                        }
                    }

                    failed = true;
                    continue;
                }
            }
            None => {
                cache_item.add_struct(ImportedStruct {
                    span,
                    local_name: aliased_name.to_string(),
                    node: s.node().clone(),
                    document: imported_doc.clone(),
                    ty: s.ty().cloned(),
                    offset: s.offset(),
                });
            }
        }
    }

    // Get the alias map for the namespace (for enums)
    let aliases = import
        .aliases()
        .filter_map(|a| {
            let (from, to) = a.names();

            #[allow(clippy::question_mark)] // For clarity
            if imported_doc.data.cache.enum_by_name(from.text()).is_none() {
                // Ignore structs or unknown type names (diagnostic added above)
                return None;
            }

            Some((from.text().to_string(), to))
        })
        .collect::<HashMap<_, _>>();

    // Insert the imported document's enum definitions
    for e in imported_doc.data.cache.enums() {
        let (span, aliased_name, aliased) = aliases
            .get(e.name())
            .map(|n| (n.span(), n.text(), true))
            .unwrap_or_else(|| (span, e.name(), false));
        match document.cache.enum_by_name(aliased_name) {
            Some((_hash, prev)) => {
                let a = prev.definition();
                let b = e.definition();
                if !are_enums_equal(&a, &b) {
                    match prev {
                        // Import conflicts with an enum defined in this document
                        EnumRef::Local(prev) => {
                            document
                                .analysis_diagnostics
                                .add(enum_conflicts_with_import(
                                    aliased_name,
                                    prev.name_span,
                                    span,
                                ));
                        }
                        EnumRef::Imported(prev) => {
                            document.analysis_diagnostics.add(imported_enum_conflict(
                                aliased_name,
                                span,
                                prev.span,
                                !aliased,
                            ));
                        }
                    }
                    failed = true;
                    continue;
                }
            }
            None => {
                cache_item.add_enum(ImportedEnum {
                    local_name: aliased_name.to_string(),
                    span,
                    node: e.node().clone(),
                    document: imported_doc.clone(),
                    ty: e.ty().cloned(),
                    offset: e.offset(),
                });
            }
        }
    }

    let offset = usize::from(import.inner().text_range().start());
    document.cache.insert_import(CachedItem {
        signature_hash,
        offset,
        item: WithBodyHash {
            body_hash,
            item: cache_item,
        },
        diagnostics: std::mem::take(&mut document.analysis_diagnostics.diagnostics),
    });

    !failed
}

/// Compares two structs for structural equality.
fn are_structs_equal(a: &StructDefinition, b: &StructDefinition) -> bool {
    for result in a.members().zip_longest(b.members()) {
        // If the length of `a` and `b` is not equal, the structs are not equal.
        let EitherOrBoth::Both(a_member, b_member) = result else {
            return false;
        };

        if a_member.name().text() != b_member.name().text() {
            return false;
        }

        if a_member.ty() != b_member.ty() {
            return false;
        }
    }

    true
}

/// Compares two enums for equality.
fn are_enums_equal(a: &EnumDefinition, b: &EnumDefinition) -> bool {
    // Compare type parameters
    match (a.type_parameter(), b.type_parameter()) {
        (Some(a_ty), Some(b_ty)) => {
            if a_ty.ty() != b_ty.ty() {
                return false;
            }
        }
        (None, None) => {}
        _ => return false,
    }

    for result in a.choices().zip_longest(b.choices()) {
        // If the length of `a` and `b` is not equal, the enums are not equal.
        let EitherOrBoth::Both(var_a, var_b) = result else {
            return false;
        };

        if var_a.name().text() != var_b.name().text() {
            return false;
        }

        match (var_a.value(), var_b.value()) {
            (Some(val_a), Some(val_b)) => {
                if val_a.inner().text() != val_b.inner().text() {
                    return false;
                }
            }
            (None, None) => {}
            _ => return false,
        }
    }

    true
}

/// Imports all items from the resolved document into the importing document's
/// scope.
fn add_wildcard_import(
    signature_hash: SignatureHash,
    body_hash: BodyHash,
    document: &mut DocumentData,
    imported_doc: Document,
    import_statement: &ImportStatement,
) {
    let span = import_statement.source().span();

    let mut import = MergingImport::default();

    import_structs(
        &mut import.imported_structs,
        document,
        imported_doc
            .data
            .cache
            .local_structs()
            .map(|(_idx, _hash, s)| ImportedStruct {
                local_name: s.name().to_string(),
                node: s.node.clone(),
                span,
                document: imported_doc.clone(),
                ty: s.ty().cloned(),
                offset: s.offset,
            }),
        span,
        wildcard_import_conflict,
    );

    import_enums(
        &mut import.imported_enums,
        document,
        imported_doc
            .data
            .cache
            .local_enums()
            .map(|(_idx, _hash, e)| ImportedEnum {
                local_name: e.name().to_string(),
                node: e.node.clone(),
                span,
                document: imported_doc.clone(),
                ty: e.ty().cloned(),
                offset: e.offset,
            }),
        span,
        wildcard_import_conflict,
    );

    import_tasks(
        &mut import.imported_tasks,
        document,
        imported_doc
            .data
            .cache
            .local_tasks()
            .map(|(_idx, _hash, task)| ImportedTask {
                local_name: task.name().to_string(),
                name: task.name().to_string(),
                span,
                document: imported_doc.clone(),
                inputs: task.inputs.clone(),
                outputs: task.outputs.clone(),
            })
            .chain(
                imported_doc
                    .data
                    .cache
                    .imported_tasks()
                    .map(|(_, task)| ImportedTask {
                        local_name: task.name.clone(),
                        name: task.name.clone(),
                        span,
                        document: task.document.clone(),
                        inputs: task.inputs.clone(),
                        outputs: task.outputs.clone(),
                    }),
            ),
        span,
        wildcard_import_conflict,
    );

    for (name, source_doc, inputs, outputs) in imported_doc
        .data
        .cache
        .workflow()
        .map(|w| (w.name.as_str(), imported_doc.clone(), &w.inputs, &w.outputs))
        .into_iter()
        .chain(
            imported_doc
                .data
                .cache
                .imported_workflows()
                .map(|(_, w)| (w.name.as_str(), w.document.clone(), &w.inputs, &w.outputs)),
        )
    {
        insert_imported_workflow(
            &mut import.imported_workflows,
            document,
            ImportedWorkflow {
                local_name: name.to_string(),
                name: name.to_string(),
                span,
                document: source_doc,
                inputs: inputs.clone(),
                outputs: outputs.clone(),
            },
            span,
            wildcard_import_conflict,
        );
    }

    let offset = usize::from(import_statement.inner().text_range().start());
    document.cache.insert_import(CachedItem {
        signature_hash,
        offset,
        item: WithBodyHash {
            body_hash,
            item: Import::Merging(import),
        },
        diagnostics: std::mem::take(&mut document.analysis_diagnostics.diagnostics),
    });
}

/// Records selected import names whose source import failed to resolve.
fn record_failed_selected_imports(document: &mut DocumentData, import: &ImportStatement) {
    let Some(members) = import.members() else {
        return;
    };

    document
        .failed_selected_imports
        .extend(members.members().map(|member| {
            member
                .alias()
                .map(|alias| alias.text().to_string())
                .unwrap_or_else(|| member.name().text().to_string())
        }));
}

/// Imports only the listed members from the resolved document.
fn add_selected_import(
    signature_hash: SignatureHash,
    body_hash: BodyHash,
    document: &mut DocumentData,
    imported_doc: Document,
    import_statement: &ImportStatement,
) {
    let Some(members) = import_statement.members() else {
        return;
    };

    let mut import = MergingImport::default();

    for member in members.members() {
        let member_name = member.name();
        let local_name = member
            .alias()
            .map(|a| a.text().to_string())
            .unwrap_or_else(|| member_name.text().to_string());
        let member_span = member
            .alias()
            .map(|a| a.span())
            .unwrap_or(member_name.span());

        let Some(item) = imported_doc.data.cache.item_by_name(member_name.text()) else {
            document.failed_selected_imports.insert(local_name);
            document.analysis_diagnostics.add(selected_member_not_found(
                member_name.text(),
                member_name.span(),
            ));

            continue;
        };

        match item {
            Item::Local(CachedItemRef::Struct(s)) => {
                let entry = ImportedStruct {
                    local_name,
                    node: s.item.node.clone(),
                    span: member_span,
                    document: imported_doc.clone(),
                    ty: s.item.ty().cloned(),
                    offset: s.item.offset,
                };

                import_structs(
                    &mut import.imported_structs,
                    document,
                    [entry],
                    member_span,
                    selected_import_conflict,
                )
            }
            Item::Imported(ImportedItem::Struct(s)) => {
                let entry = ImportedStruct {
                    local_name,
                    node: s.node.clone(),
                    span: member_span,
                    document: s.document.clone(),
                    ty: s.ty().cloned(),
                    offset: s.offset,
                };

                import_structs(
                    &mut import.imported_structs,
                    document,
                    [entry],
                    member_span,
                    selected_import_conflict,
                )
            }
            Item::Local(CachedItemRef::Enum(e)) => {
                let entry = ImportedEnum {
                    local_name,
                    node: e.item.node.clone(),
                    span: member_span,
                    document: imported_doc.clone(),
                    ty: e.item.ty().cloned(),
                    offset: e.item.offset,
                };

                import_enums(
                    &mut import.imported_enums,
                    document,
                    [entry],
                    member_span,
                    selected_import_conflict,
                )
            }
            Item::Imported(ImportedItem::Enum(e)) => {
                let entry = ImportedEnum {
                    local_name,
                    node: e.node.clone(),
                    span: member_span,
                    document: e.document.clone(),
                    ty: e.ty().cloned(),
                    offset: e.offset,
                };

                import_enums(
                    &mut import.imported_enums,
                    document,
                    [entry],
                    member_span,
                    selected_import_conflict,
                )
            }
            Item::Local(CachedItemRef::Task(t)) => {
                let entry = ImportedTask {
                    local_name,
                    name: t.item.item.name.clone(),
                    span: member_span,
                    document: imported_doc.clone(),
                    inputs: t.item.item.inputs.clone(),
                    outputs: t.item.item.outputs.clone(),
                };

                import_tasks(
                    &mut import.imported_tasks,
                    document,
                    [entry],
                    member_span,
                    selected_import_conflict,
                );
            }
            Item::Imported(ImportedItem::Task(t)) => {
                let entry = ImportedTask {
                    local_name,
                    name: t.name.clone(),
                    span: member_span,
                    document: t.document.clone(),
                    inputs: t.inputs.clone(),
                    outputs: t.outputs.clone(),
                };

                import_tasks(
                    &mut import.imported_tasks,
                    document,
                    [entry],
                    member_span,
                    selected_import_conflict,
                );
            }
            Item::Local(CachedItemRef::Workflow(wf)) => {
                let entry = ImportedWorkflow {
                    local_name,
                    name: wf.item.item.name.clone(),
                    span: member_span,
                    document: imported_doc.clone(),
                    inputs: wf.item.item.inputs.clone(),
                    outputs: wf.item.item.outputs.clone(),
                };

                insert_imported_workflow(
                    &mut import.imported_workflows,
                    document,
                    entry,
                    member_span,
                    selected_import_conflict,
                );
            }
            Item::Imported(ImportedItem::Workflow(wf)) => {
                let entry = ImportedWorkflow {
                    local_name,
                    name: wf.name.clone(),
                    span: member_span,
                    document: wf.document.clone(),
                    inputs: wf.inputs.clone(),
                    outputs: wf.outputs.clone(),
                };

                insert_imported_workflow(
                    &mut import.imported_workflows,
                    document,
                    entry,
                    member_span,
                    selected_import_conflict,
                );
            }
            // N/A
            Item::Local(CachedItemRef::Import(_)) => {}
        }
    }

    let offset = import_statement.span().start();
    document.cache.insert_import(CachedItem {
        signature_hash,
        offset,
        item: WithBodyHash {
            body_hash,
            item: Import::Merging(import),
        },
        diagnostics: std::mem::take(&mut document.analysis_diagnostics.diagnostics),
    });
}

/// Inserts a re-exported struct into `document` under `local_name`.
///
/// When a struct by that name already exists, the `conflict`
/// diagnostic is emitted (highlighting `conflict_span` and the previous
/// definition) and the entry is not inserted.
fn import_structs(
    map: &mut IndexMap<String, ImportedStruct>,
    document: &mut DocumentData,
    entries: impl IntoIterator<Item = ImportedStruct>,
    conflict_span: Span,
    conflict: impl Fn(&str, Span, Span) -> Diagnostic,
) {
    for entry in entries {
        if let Some((_hash, local_struct)) = document.cache.struct_by_name(&entry.local_name) {
            let a = local_struct.definition();
            let b = entry.definition();
            if !are_structs_equal(&a, &b) {
                document.analysis_diagnostics.add(conflict(
                    &entry.local_name,
                    conflict_span,
                    local_struct.name_span(),
                ));
                continue;
            }
        } else {
            map.insert(entry.local_name.clone(), entry);
        }
    }
}

/// Inserts a re-exported task into `document` under `local_name`.
///
/// When an enum by that name already exists, the `conflict`
/// diagnostic is emitted (highlighting `conflict_span` and the previous
/// definition) and the entry is not inserted.
fn import_enums(
    map: &mut IndexMap<String, ImportedEnum>,
    document: &mut DocumentData,
    entries: impl IntoIterator<Item = ImportedEnum>,
    conflict_span: Span,
    conflict: impl Fn(&str, Span, Span) -> Diagnostic,
) {
    for entry in entries {
        if let Some((_hash, local_enum)) = document.cache.enum_by_name(&entry.local_name) {
            let a = local_enum.definition();
            let b = entry.definition();
            if !are_enums_equal(&a, &b) {
                document.analysis_diagnostics.add(conflict(
                    &entry.local_name,
                    conflict_span,
                    local_enum.name_span(),
                ));
                continue;
            }
        } else {
            map.insert(entry.local_name.clone(), entry);
        }
    }
}

/// Inserts a re-exported task into `document` under `local_name`.
///
/// When a callable by that name already exists, the `conflict`
/// diagnostic is emitted (highlighting `conflict_span` and the previous
/// definition) and the entry is not inserted.
fn import_tasks(
    map: &mut IndexMap<String, ImportedTask>,
    document: &mut DocumentData,
    entries: impl IntoIterator<Item = ImportedTask>,
    conflict_span: Span,
    conflict: impl Fn(&str, Span, Span) -> Diagnostic,
) {
    for entry in entries {
        // A name brought in twice that denotes the same underlying
        // declaration in the same resolved source document is not a
        // conflict; the two imports refer to that single declaration. This
        // keeps diamond-shaped import graphs usable without renames.
        if let Some((_hash, existing)) = document.cache.imported_task_by_name(&entry.local_name)
            && existing.source() == entry.source()
            && existing.name == entry.name
        {
            continue;
        }

        if let Some(prev_span) = callable_conflict_span(document, &entry.local_name) {
            document.analysis_diagnostics.add(conflict(
                &entry.local_name,
                conflict_span,
                prev_span,
            ));
            continue;
        }

        map.insert(entry.local_name.to_string(), entry);
    }
}

/// Inserts a re-exported workflow into `document` under `local_name`.
///
/// Checks are performed in order.
///
/// 1. Same underlying declaration re-imported (diamond pattern) is silently
///    deduplicated; no diagnostic is emitted.
/// 2. A distinct workflow already occupies local scope; only one workflow may
///    be in scope at a time, so the import is rejected and `workflow_conflict`
///    is emitted.
/// 3. The local name collides with a task or other callable; the `conflict`
///    callback is called (preserving the import-form-specific diagnostic).
fn insert_imported_workflow(
    map: &mut IndexMap<String, ImportedWorkflow>,
    document: &mut DocumentData,
    entry: ImportedWorkflow,
    conflict_span: Span,
    conflict: impl Fn(&str, Span, Span) -> Diagnostic,
) {
    // The same underlying declaration re-imported under the same name is
    // not a conflict; deduplicate silently.
    if let Some((_hash, existing)) = document.cache.imported_workflow_by_name(&entry.local_name)
        && existing.source() == entry.source()
        && existing.name == entry.name
    {
        return;
    }

    // Only one distinct workflow may occupy local scope at a time.
    if let Some((_hash, existing)) =
        document
            .cache
            .imported_workflows()
            .find(|(_hash, existing)| {
                existing.source() != entry.source() || existing.name != entry.name
            })
    {
        document.analysis_diagnostics.add(workflow_conflict(
            &entry.name,
            conflict_span,
            existing.name(),
            existing.span,
        ));
        return;
    }

    // The local name collides with a task or other callable.
    if let Some(prev_span) = callable_conflict_span(document, &entry.local_name) {
        document
            .analysis_diagnostics
            .add(conflict(&entry.local_name, conflict_span, prev_span));
        return;
    }

    map.insert(entry.local_name.to_string(), entry);
}

/// Returns the span of a callable that already owns `name`.
fn callable_conflict_span(document: &DocumentData, name: &str) -> Option<Span> {
    document
        .cache
        .task_by_name(name)
        .map(|(_hash, task)| task.name_span())
        .or_else(|| {
            document
                .cache
                .workflow_by_name(name)
                .map(|(_hash, workflow)| workflow.name_span())
        })
}

/// Adds a struct to the document.
fn add_struct(document: &mut DocumentData, hash: SignatureHash, definition: &StructDefinition) {
    let name = definition.name();
    tracing::trace!(
        document = document.uri.as_str(),
        "adding struct `{}`",
        name.text()
    );

    // Check for a conflict with imported struct first otherwise for any name
    if let Some((_hash, prev)) = document.cache.imported_struct_by_name(name.text()) {
        let prev_def = prev.definition();

        if !are_structs_equal(definition, &prev_def) {
            document
                .analysis_diagnostics
                .add(struct_conflicts_with_import(
                    name.text(),
                    name.span(),
                    prev.span,
                ));
            return;
        }
    } else if let Some(ctx) = document.context(name.text()) {
        document.analysis_diagnostics.add(name_conflict(
            name.text(),
            Context::Struct(name.span()),
            ctx,
        ));
        return;
    }

    // Ensure there are no duplicate members
    let mut members = IndexMap::new();
    for decl in definition.members() {
        let name = decl.name();
        match members.get(name.text()) {
            Some(prev_span) => {
                document.analysis_diagnostics.add(name_conflict(
                    name.text(),
                    Context::StructMember(name.span()),
                    Context::StructMember(*prev_span),
                ));
            }
            _ => {
                members.insert(name.text().to_string(), name.span());
            }
        }
    }

    document.cache.insert_struct(CachedItem {
        signature_hash: hash,
        offset: definition.span().start(),
        item: Struct {
            name_span: name.span(),
            name: name.text().to_string(),
            offset: definition.span().start(),
            node: definition.inner().green().into(),
            ty: None,
        },
        diagnostics: std::mem::take(&mut document.analysis_diagnostics.diagnostics),
    });
}

/// Adds an enum definition to the document.
fn add_enum(document: &mut DocumentData, hash: SignatureHash, definition: &EnumDefinition) {
    let name = definition.name();
    tracing::trace!(
        document = document.uri.as_str(),
        "adding enum `{}`",
        name.text()
    );

    // Check if enums are supported in this version
    let version = document.version.expect("should have version");
    if version < SupportedVersion::V1(V1::Three) {
        document
            .analysis_diagnostics
            .add(enum_not_supported(version, definition.name().span()));
        return;
    }

    // Check for a conflict with imported enum first otherwise for any name
    if let Some((_hash, prev)) = document.cache.imported_enum_by_name(name.text()) {
        let prev_def = prev.definition();
        if !are_enums_equal(definition, &prev_def) {
            document
                .analysis_diagnostics
                .add(enum_conflicts_with_import(
                    name.text(),
                    name.span(),
                    prev.span,
                ))
        }
    } else if let Some(ctx) = document.context(name.text()) {
        document.analysis_diagnostics.add(name_conflict(
            name.text(),
            Context::Enum(name.span()),
            ctx,
        ));
        return;
    }

    // Ensure there are no duplicate choices
    let mut choices = IndexMap::new();
    for choice in definition.choices() {
        let name = choice.name();
        match choices.get(name.text()) {
            Some(prev_span) => {
                document.analysis_diagnostics.add(name_conflict(
                    name.text(),
                    Context::EnumChoice(name.span()),
                    Context::EnumChoice(*prev_span),
                ));
            }
            _ => {
                choices.insert(name.text().to_string(), name.span());
            }
        }
    }

    let offset = definition.span().start();
    document.cache.insert_enum(CachedItem {
        signature_hash: hash,
        offset,
        item: Enum {
            name_span: name.span(),
            name: name.text().to_string(),
            offset: definition.span().start(),
            node: definition.inner().green().into(),
            ty: None,
        },
        diagnostics: std::mem::take(&mut document.analysis_diagnostics.diagnostics),
    });
}

/// Converts an AST type to an analysis type.
fn convert_ast_type(
    document: &mut DocumentData,
    ty: &wdl_ast::v1::Type,
    dependent: Option<SignatureHash>,
) -> Type {
    /// Used to resolve a type name from a document.
    struct Resolver<'a> {
        document: &'a mut DocumentData,
        dependent: Option<SignatureHash>,
    }

    impl TypeNameResolver for Resolver<'_> {
        fn resolve(&mut self, name: &str, span: Span) -> Result<Type, Diagnostic> {
            if let Some((ty, dependency, uri)) =
                self.document.cache.struct_by_name(name).map(|(hash, s)| {
                    (
                        s.ty().cloned().unwrap_or(Type::Union),
                        hash,
                        s.source().clone(),
                    )
                })
            {
                if let Some(dependent) = self.dependent {
                    self.document.cache.add_dependency(dependent, dependency);
                }

                if let Some(uri) = uri
                    && let Some(resolved) = self
                        .document
                        .cache
                        .namespaces_mut()
                        .find(|n| n.source() == uri)
                {
                    resolved.used = true;
                }
                return Ok(ty);
            }

            if let Some((ty, dependency, uri)) =
                self.document.cache.enum_by_name(name).map(|(hash, e)| {
                    (
                        e.ty().cloned().unwrap_or(Type::Union),
                        hash,
                        e.source().clone(),
                    )
                })
            {
                if let Some(dependent) = self.dependent {
                    self.document.cache.add_dependency(dependent, dependency);
                }

                if let Some(uri) = uri
                    && let Some(resolved) = self
                        .document
                        .cache
                        .namespaces_mut()
                        .find(|n| n.source() == uri)
                {
                    resolved.used = true;
                }
                return Ok(ty);
            }

            Err(unknown_type(name, span))
        }
    }
    let mut converter = crate::types::v1::AstTypeConverter::new(Resolver {
        document,
        dependent,
    });
    match converter.convert_type(ty) {
        Ok(ty) => ty,
        Err(diagnostic) => {
            document.analysis_diagnostics.add(diagnostic);
            Type::Union
        }
    }
}

/// Creates an input type map.
fn create_input_type_map(
    document: &mut DocumentData,
    declarations: impl Iterator<Item = Decl>,
    dependent: Option<SignatureHash>,
) -> Arc<IndexMap<String, Input>> {
    let mut map = IndexMap::new();
    for decl in declarations {
        let name = decl.name();
        if map.contains_key(name.text()) {
            // Ignore the duplicate
            continue;
        }

        let ty = convert_ast_type(document, &decl.ty(), dependent);
        let optional = ty.is_optional();
        map.insert(
            name.text().to_string(),
            Input {
                ty,
                required: decl.expr().is_none() && !optional,
            },
        );
    }

    map.into()
}

/// Creates an output type map.
fn create_output_type_map(
    document: &mut DocumentData,
    declarations: impl Iterator<Item = Decl>,
    dependent: Option<SignatureHash>,
) -> Arc<IndexMap<String, Output>> {
    let mut map = IndexMap::new();
    for decl in declarations {
        let name = decl.name();
        if map.contains_key(name.text()) {
            // Ignore the duplicate
            continue;
        }

        let ty = convert_ast_type(document, &decl.ty(), dependent);
        map.insert(name.text().to_string(), Output::new(ty, name.span()));
    }

    map.into()
}

/// Adds a task to the document.
fn add_task(
    config: &Config,
    document: &mut DocumentData,
    signature_hash: SignatureHash,
    body_hash: BodyHash,
    definition: &TaskDefinition,
) {
    /// Helper function for creating a scope for a task section.
    fn create_section_scope(
        version: Option<SupportedVersion>,
        scopes: &mut Vec<Scope>,
        task_name: &Ident,
        span: Span,
        task_type: HiddenType,
    ) -> ScopeIndex {
        let index = add_scope(scopes, Scope::new(Some(ScopeIndex(0)), span));

        match task_type {
            HiddenType::TaskPreEvaluation => {
                // Pre-evaluation task type is available in v1.3+.
                if version >= Some(SupportedVersion::V1(V1::Three)) {
                    scopes[index.0].insert(
                        TASK_VAR_NAME,
                        task_name.span(),
                        Type::Hidden(task_type),
                    );
                }
            }
            HiddenType::TaskPostEvaluation => {
                // Post-evaluation task type is available in v1.2+.
                if version >= Some(SupportedVersion::V1(V1::Two)) {
                    scopes[index.0].insert(
                        TASK_VAR_NAME,
                        task_name.span(),
                        Type::Hidden(task_type),
                    );
                }
            }
            _ => panic!("task type should be either `TaskPreEvaluation` or `TaskPostEvaluation`"),
        }

        index
    }

    let name = definition.name();
    tracing::trace!(
        document = document.uri.as_str(),
        "adding task `{}`",
        name.text()
    );

    if let Some(ctx) = document.context(name.text()) {
        document.analysis_diagnostics.add(name_conflict(
            name.text(),
            Context::Task(name.span()),
            ctx,
        ));
        return;
    }
    if let Some(prev_span) = callable_conflict_span(document, name.text()) {
        document.analysis_diagnostics.add(selected_import_conflict(
            name.text(),
            prev_span,
            name.span(),
        ));
        document
            .failed_selected_imports
            .insert(name.text().to_string());
        return;
    }

    // Populate type maps for the tasks's inputs and outputs
    let inputs = match definition.input() {
        Some(section) => {
            create_input_type_map(document, section.declarations(), Some(signature_hash))
        }
        None => Default::default(),
    };
    let outputs = match definition.output() {
        Some(section) => create_output_type_map(
            document,
            section.declarations().map(Decl::Bound),
            Some(signature_hash),
        ),
        None => Default::default(),
    };

    // Process the task in evaluation order
    let graph = TaskGraphBuilder::default().build(
        document.version.unwrap(),
        definition,
        &mut document.analysis_diagnostics,
        |name| {
            document.cache.struct_by_name(name).is_some()
                || document.cache.enum_by_name(name).is_some()
        },
    );

    let mut task = Task {
        name_span: name.span(),
        name: name.text().to_string(),
        span: definition.span(),
        scopes: vec![Scope::new(
            None,
            definition
                .braced_scope_span(false)
                .expect("should have brace scope span"),
        )],
        inputs,
        outputs,
    };

    let command_section_span = graph.node_weights().find_map(|node| {
        if let TaskGraphNode::Command(section) = node {
            Some(section.span())
        } else {
            None
        }
    });

    let mut output_scope = None;
    let mut command_scope = None;
    let mut requirements_scope = None;
    let mut hints_scope = None;
    let mut runtime_scope = None;

    for index in toposort(&graph, None).expect("graph should be acyclic") {
        match graph[index].clone() {
            TaskGraphNode::Input(decl) => {
                if !add_decl(
                    config,
                    document,
                    ScopeRefMut::new(&mut task.scopes, ScopeIndex(0)),
                    &decl,
                    |_, n, _| task.inputs[n].ty.clone(),
                ) {
                    continue;
                }

                // Check for unused input
                if let Some(severity) = config.diagnostics_config().unused_input
                    && decl.env().is_none()
                {
                    // For any input that isn't an environment variable, check to see if there's
                    // a single implicit dependency edge; if so, it might be unused
                    let mut edges = graph.edges_directed(index, Direction::Outgoing);

                    if edges.all(|e| *e.weight()) {
                        let name = decl.name();

                        document.analysis_diagnostics.exceptable_add(
                            unused_input(name.text(), name.span()).with_severity(severity),
                            decl.inner(),
                            &UnusedInputRule::EXCEPTABLE_NODES,
                        );
                    }
                }
            }
            TaskGraphNode::Decl(decl) => {
                let name = decl.name();

                if let Some(command_section_span) = command_section_span
                    && decl.inner().span().start() > command_section_span.end()
                    && let Some(severity) = config.diagnostics_config().misleading_declaration_order
                {
                    document.analysis_diagnostics.exceptable_add(
                        misleading_declaration_order(name.text(), name.span())
                            .with_severity(severity),
                        decl.inner(),
                        &MisleadingDeclarationOrderRule::EXCEPTABLE_NODES,
                    );
                }

                if !add_decl(
                    config,
                    document,
                    ScopeRefMut::new(&mut task.scopes, ScopeIndex(0)),
                    &decl,
                    |doc, _, decl| convert_ast_type(doc, &decl.ty(), Some(signature_hash)),
                ) {
                    continue;
                }

                // Check for unused declaration
                let Some(severity) = config.diagnostics_config().unused_declaration else {
                    continue;
                };

                let name = decl.name();

                // Don't warn for environment variables as they are always implicitly used
                if decl.env().is_none()
                    && graph
                        .edges_directed(index, Direction::Outgoing)
                        .next()
                        .is_none()
                {
                    document.analysis_diagnostics.exceptable_add(
                        unused_declaration(name.text(), name.span()).with_severity(severity),
                        decl.inner(),
                        &UnusedDeclarationRule::EXCEPTABLE_NODES,
                    );
                }
            }
            TaskGraphNode::Output(decl) => {
                let scope_index = *output_scope.get_or_insert_with(|| {
                    create_section_scope(
                        document.version,
                        &mut task.scopes,
                        &name,
                        definition
                            .output()
                            .expect("should have output section")
                            .braced_scope_span(false)
                            .expect("should have braced scope span"),
                        HiddenType::TaskPostEvaluation,
                    )
                });
                add_decl(
                    config,
                    document,
                    ScopeRefMut::new(&mut task.scopes, scope_index),
                    &decl,
                    |_, n, _| task.outputs[n].ty.clone(),
                );
            }
            TaskGraphNode::Command(section) => {
                let scope_index = *command_scope.get_or_insert_with(|| {
                    let span = if section.is_heredoc() {
                        section.heredoc_scope_span(false)
                    } else {
                        section.braced_scope_span(false)
                    };

                    create_section_scope(
                        document.version,
                        &mut task.scopes,
                        &name,
                        span.expect("should have scope span"),
                        HiddenType::TaskPostEvaluation,
                    )
                });

                let mut context = EvaluationContext::new(
                    document,
                    ScopeRef::new(&task.scopes, scope_index),
                    config.clone(),
                );
                let mut evaluator = ExprTypeEvaluator::new(&mut context);
                for part in section.parts() {
                    if let CommandPart::Placeholder(p) = part {
                        evaluator.check_placeholder(&p);
                    }
                }
            }
            TaskGraphNode::Runtime(section) => {
                let scope_index = *runtime_scope.get_or_insert_with(|| {
                    create_section_scope(
                        document.version,
                        &mut task.scopes,
                        &name,
                        section
                            .braced_scope_span(false)
                            .expect("should have braced scope span"),
                        HiddenType::TaskPreEvaluation,
                    )
                });

                // Perform type checking on the runtime section's expressions
                let mut context = EvaluationContext::new(
                    document,
                    ScopeRef::new(&task.scopes, scope_index),
                    config.clone(),
                );
                let mut evaluator = ExprTypeEvaluator::new(&mut context);
                for item in section.items() {
                    evaluator.evaluate_runtime_item(&item.name(), &item.expr());
                }
            }
            TaskGraphNode::Requirements(section) => {
                let scope_index = *requirements_scope.get_or_insert_with(|| {
                    create_section_scope(
                        document.version,
                        &mut task.scopes,
                        &name,
                        section
                            .braced_scope_span(false)
                            .expect("should have braced scope span"),
                        HiddenType::TaskPreEvaluation,
                    )
                });

                // Perform type checking on the requirements section's expressions
                let mut context = EvaluationContext::new(
                    document,
                    ScopeRef::new(&task.scopes, scope_index),
                    config.clone(),
                );
                let mut evaluator = ExprTypeEvaluator::new(&mut context);
                for item in section.items() {
                    evaluator.evaluate_requirements_item(&item.name(), &item.expr());
                }
            }
            TaskGraphNode::Hints(section) => {
                let scope_index = *hints_scope.get_or_insert_with(|| {
                    create_section_scope(
                        document.version,
                        &mut task.scopes,
                        &name,
                        section
                            .braced_scope_span(false)
                            .expect("should have braced scope span"),
                        HiddenType::TaskPreEvaluation,
                    )
                });

                // Perform type checking on the hints section's expressions
                let mut context = EvaluationContext::new_for_task(
                    document,
                    ScopeRef::new(&task.scopes, scope_index),
                    config.clone(),
                    &task,
                );
                let mut evaluator = ExprTypeEvaluator::new(&mut context);
                for item in section.items() {
                    evaluator.evaluate_hints_item(&item.name(), &item.expr())
                }
            }
        }
    }

    // Sort the scopes
    sort_scopes(&mut task.scopes);

    let offset = definition.span().start();
    document.cache.insert_task(CachedItem {
        signature_hash,
        offset,
        item: WithBodyHash {
            body_hash,
            item: task,
        },
        diagnostics: std::mem::take(&mut document.analysis_diagnostics.diagnostics),
    });
}

/// Adds a declaration to a scope.
fn add_decl(
    config: &Config,
    document: &mut DocumentData,
    mut scope: ScopeRefMut<'_>,
    decl: &Decl,
    ty: impl FnOnce(&mut DocumentData, &str, &Decl) -> Type,
) -> bool {
    let (name, expr) = (decl.name(), decl.expr());
    if scope.lookup(name.text()).is_some() {
        // The declaration is conflicting; don't add to the scope
        return false;
    }

    let ty = ty(document, name.text(), decl);
    scope.insert(name.text(), name.span(), ty.clone());

    if let Some(expr) = expr {
        type_check_expr(
            config,
            document,
            scope.as_scope_ref(),
            &expr,
            &ty,
            name.span(),
        );
    }

    true
}

/// Adds a workflow to the document.
///
/// Returns `true` if the workflow was added to the document or `false` if not
/// (i.e. there was a conflict).
fn add_workflow(
    document: &mut DocumentData,
    signature_hash: SignatureHash,
    body_hash: BodyHash,
    workflow: &WorkflowDefinition,
) -> bool {
    // Check for duplicate workflow first
    let name = workflow.name();
    tracing::trace!(
        document = document.uri.as_str(),
        "adding workflow `{}`",
        name.text()
    );

    if let Some(prev) = document.cache.workflow() {
        document
            .analysis_diagnostics
            .add(duplicate_workflow(&name, prev.name_span));
        return false;
    }

    // An imported workflow already occupies local scope; reject this definition.
    if let Some((_hash, imported)) = document.cache.imported_workflows().next() {
        document.analysis_diagnostics.add(workflow_conflict(
            name.text(),
            name.span(),
            &imported.name,
            imported.span,
        ));
        return false;
    }

    // Check for a name conflict
    if let Some(ctx) = document.context(name.text()) {
        document.analysis_diagnostics.add(name_conflict(
            name.text(),
            Context::Workflow(name.span()),
            ctx,
        ));
        return false;
    }
    if let Some(prev_span) = callable_conflict_span(document, name.text()) {
        document.analysis_diagnostics.add(selected_import_conflict(
            name.text(),
            prev_span,
            name.span(),
        ));
        document
            .failed_selected_imports
            .insert(name.text().to_string());
        return false;
    }

    let workflow_item = Workflow {
        name_span: name.span(),
        name: name.text().to_string(),
        span: workflow.span(),
        scopes: Default::default(),
        inputs: Default::default(),
        outputs: Default::default(),
        calls: Default::default(),
        allows_nested_inputs: document
            .version
            .map(|v| workflow.allows_nested_inputs(v))
            .unwrap_or(false),
    };

    let offset = workflow.span().start();
    document.cache.set_workflow(CachedItem {
        signature_hash,
        offset,
        item: WithBodyHash {
            body_hash,
            item: workflow_item,
        },
        diagnostics: std::mem::take(&mut document.analysis_diagnostics.diagnostics),
    });

    true
}

/// Finishes populating a workflow.
fn populate_workflow(
    config: &Config,
    document: &mut DocumentData,
    workflow_def: &WorkflowDefinition,
    signature_hash: SignatureHash,
) {
    let workflow_name = workflow_def.name().text().to_string();
    let allows_nested_inputs = document
        .version
        .map(|v| workflow_def.allows_nested_inputs(v))
        .unwrap_or(false);

    // Populate type maps for the workflow's inputs and outputs
    let inputs = match workflow_def.input() {
        Some(section) => {
            create_input_type_map(document, section.declarations(), Some(signature_hash))
        }
        None => Default::default(),
    };
    let outputs = match workflow_def.output() {
        Some(section) => create_output_type_map(
            document,
            section.declarations().map(Decl::Bound),
            Some(signature_hash),
        ),
        None => Default::default(),
    };

    // Keep a map of scopes from syntax node that introduced the scope to the scope
    // index
    let mut scope_indexes: HashMap<SyntaxNode, ScopeIndex> = HashMap::new();
    let mut scopes = vec![Scope::new(
        None,
        workflow_def
            .braced_scope_span(false)
            .expect("should have braced scope span"),
    )];
    let mut output_scope = None;
    let mut calls = HashMap::new();

    // For static analysis, we don't need to provide inputs to the workflow graph
    // builder
    let graph = WorkflowGraphBuilder::default().build(
        workflow_def,
        &mut document.analysis_diagnostics,
        |_| false,
        |name| {
            document.cache.struct_by_name(name).is_some()
                || document.cache.enum_by_name(name).is_some()
        },
    );

    for index in toposort(&graph, None).expect("graph should be acyclic") {
        match graph[index].clone() {
            WorkflowGraphNode::Input(decl) => {
                if !add_decl(
                    config,
                    document,
                    ScopeRefMut::new(&mut scopes, ScopeIndex(0)),
                    &decl,
                    |_, n, _| inputs[n].ty.clone(),
                ) {
                    continue;
                }

                // Check for unused input
                if let Some(severity) = config.diagnostics_config().unused_input
                    && graph
                        .edges_directed(index, Direction::Outgoing)
                        .next()
                        .is_none()
                {
                    let name = decl.name();

                    document.analysis_diagnostics.exceptable_add(
                        unused_input(name.text(), name.span()).with_severity(severity),
                        decl.inner(),
                        &UnusedInputRule::EXCEPTABLE_NODES,
                    );
                }
            }
            WorkflowGraphNode::Decl(decl) => {
                let scope_index = scope_indexes
                    .get(&decl.inner().parent().expect("should have parent"))
                    .copied()
                    .unwrap_or(ScopeIndex(0));

                if !add_decl(
                    config,
                    document,
                    ScopeRefMut::new(&mut scopes, scope_index),
                    &decl,
                    |doc, _, decl| convert_ast_type(doc, &decl.ty(), Some(signature_hash)),
                ) {
                    continue;
                }

                // Check for unused declaration
                if let Some(severity) = config.diagnostics_config().unused_declaration {
                    let name = decl.name();
                    if graph
                        .edges_directed(index, Direction::Outgoing)
                        .next()
                        .is_none()
                    {
                        document.analysis_diagnostics.exceptable_add(
                            unused_declaration(name.text(), name.span()).with_severity(severity),
                            decl.inner(),
                            &UnusedDeclarationRule::EXCEPTABLE_NODES,
                        );
                    }
                }
            }
            WorkflowGraphNode::Output(decl) => {
                let scope_index = *output_scope.get_or_insert_with(|| {
                    add_scope(
                        &mut scopes,
                        Scope::new(
                            Some(ScopeIndex(0)),
                            workflow_def
                                .output()
                                .expect("should have output section")
                                .braced_scope_span(false)
                                .expect("should have braced scope span"),
                        ),
                    )
                });
                add_decl(
                    config,
                    document,
                    ScopeRefMut::new(&mut scopes, scope_index),
                    &decl,
                    |_, n, _| outputs[n].ty.clone(),
                );
            }
            WorkflowGraphNode::Conditional(statement, _) => {
                let parent = scope_indexes
                    .get(&statement.inner().parent().expect("should have parent"))
                    .copied()
                    .unwrap_or(ScopeIndex(0));
                add_conditional_statement(
                    config,
                    document,
                    &mut scopes,
                    parent,
                    &mut scope_indexes,
                    &statement,
                );
            }
            WorkflowGraphNode::ConditionalClause(..) => {
                // Conditional clause nodes are intermediate nodes used for subgraph splitting
                // during evaluation. They don't need to be processed here as the
                // conditional node already handles all clauses.
                continue;
            }
            WorkflowGraphNode::Scatter(statement, _) => {
                let parent = scope_indexes
                    .get(&statement.inner().parent().expect("should have parent"))
                    .copied()
                    .unwrap_or(ScopeIndex(0));
                add_scatter_statement(
                    config,
                    document,
                    &mut scopes,
                    parent,
                    &mut scope_indexes,
                    &statement,
                );
            }
            WorkflowGraphNode::Call(statement) => {
                let scope_index = scope_indexes
                    .get(&statement.inner().parent().expect("should have parent"))
                    .copied()
                    .unwrap_or(ScopeIndex(0));
                add_call_statement(
                    config,
                    document,
                    ScopeRefMut::new(&mut scopes, scope_index),
                    &statement,
                    &workflow_name,
                    &mut calls,
                    allows_nested_inputs,
                    graph
                        .edges_directed(index, Direction::Outgoing)
                        .next()
                        .is_some(),
                    signature_hash,
                );
            }
            WorkflowGraphNode::ExitConditional(statement) => {
                let mut scope_union = ScopeUnion::new();

                for clause in statement.clauses() {
                    let scope_index = scope_indexes
                        .get(clause.inner())
                        .copied()
                        .expect("should have scope");

                    scope_union.insert(
                        ScopeRef::new(&scopes, scope_index),
                        matches!(clause.kind(), ConditionalStatementClauseKind::Else),
                    );
                }

                let parent_scope = {
                    let index = scope_indexes
                        .get(
                            statement
                                .clauses()
                                .next()
                                .expect("conditional statement does not have a clause")
                                .inner(),
                        )
                        .copied()
                        .expect("should have scope");
                    scopes[index.0].parent.expect("should have parent")
                };

                match scope_union.resolve() {
                    Ok(results) => {
                        for (name, info) in results {
                            match scopes[parent_scope.0].names.entry(name.clone()) {
                                IndexMapEntry::Vacant(entry) => {
                                    entry.insert(info);
                                }
                                IndexMapEntry::Occupied(entry) => {
                                    document.analysis_diagnostics.add(name_conflict(
                                        &name,
                                        Context::Name(NameContext::Decl(info.span)),
                                        Context::Name(NameContext::Decl(entry.get().span)),
                                    ));
                                }
                            }
                        }
                    }
                    Err(diagnostics) => document.analysis_diagnostics.extend(diagnostics),
                }
            }
            WorkflowGraphNode::ExitScatter(statement) => {
                let scope_index = scope_indexes
                    .get(statement.inner())
                    .copied()
                    .expect("should have scope");
                let variable = statement.variable();

                // We need to split the scopes as we want to read from one part of the slice and
                // write to another; the left side will contain the parent at its index and the
                // right side will contain the child scope at its index minus the parent's
                let parent = scopes[scope_index.0]
                    .parent
                    .expect("should have a parent scope");
                assert!(scope_index.0 > parent.0);
                let (left, right) = scopes.split_at_mut(parent.0 + 1);
                let scope = &right[scope_index.0 - parent.0 - 1];
                let parent = &mut left[parent.0];
                for (name, Name { span, ty }) in scope.names.iter() {
                    if name.as_str() == variable.text() {
                        continue;
                    }

                    parent.names.entry(name.clone()).or_insert_with(|| Name {
                        span: *span,
                        ty: ty.promote_scatter(),
                    });
                }
            }
        }
    }

    // Sort the scopes
    sort_scopes(&mut scopes);

    // Finally, populate the workflow
    let workflow = document
        .cache
        .workflow_mut()
        .expect("workflow should exist in cache");
    workflow.scopes = scopes;
    workflow.inputs = inputs;
    workflow.outputs = outputs;
    workflow.calls = calls;
}

/// Adds a conditional statement to the current scope.
fn add_conditional_statement(
    config: &Config,
    document: &mut DocumentData,
    scopes: &mut Vec<Scope>,
    parent: ScopeIndex,
    scope_indexes: &mut HashMap<SyntaxNode, ScopeIndex>,
    statement: &ConditionalStatement,
) {
    let version = document.version.expect("should have version");
    if version < SupportedVersion::V1(V1::Three) {
        for clause in statement.clauses() {
            match clause.kind() {
                ConditionalStatementClauseKind::ElseIf => {
                    let else_span = clause
                        .else_keyword()
                        .expect("should have `else` keyword")
                        .span();
                    let if_span = clause
                        .if_keyword()
                        .expect("should have `if` keyword")
                        .span();
                    let span = Span::new(else_span.start(), if_span.end() - else_span.start());
                    document
                        .analysis_diagnostics
                        .add(else_if_not_supported(version, span));
                }
                ConditionalStatementClauseKind::Else => {
                    let span = clause
                        .else_keyword()
                        .expect("should have `else` keyword")
                        .span();
                    document
                        .analysis_diagnostics
                        .add(else_not_supported(version, span));
                }
                ConditionalStatementClauseKind::If => {}
            }
        }
    }

    for clause in statement.clauses() {
        let scope_index = add_scope(
            scopes,
            Scope::new(
                Some(parent),
                clause
                    .braced_scope_span(false)
                    .expect("should have braced scope span"),
            ),
        );
        scope_indexes.insert(clause.inner().clone(), scope_index);

        let Some(expr) = clause.expr() else {
            continue;
        };
        let mut context =
            EvaluationContext::new(document, ScopeRef::new(scopes, scope_index), config.clone());
        let mut evaluator = ExprTypeEvaluator::new(&mut context);
        let ty = evaluator.evaluate_expr(&expr).unwrap_or(Type::Union);

        if !ty.is_coercible_to(&PrimitiveType::Boolean.into()) {
            document
                .analysis_diagnostics
                .add(if_conditional_mismatch(&ty, expr.span()));
        }
    }
}

/// Adds a scatter statement to the current scope.
fn add_scatter_statement(
    config: &Config,
    document: &mut DocumentData,
    scopes: &mut Vec<Scope>,
    parent: ScopeIndex,
    scopes_indexes: &mut HashMap<SyntaxNode, ScopeIndex>,
    statement: &ScatterStatement,
) {
    let scope_index = add_scope(
        scopes,
        Scope::new(
            Some(parent),
            statement
                .braced_scope_span(false)
                .expect("should have braced scope span"),
        ),
    );
    scopes_indexes.insert(statement.inner().clone(), scope_index);

    // Evaluate the statement expression; it is expected to be an array
    let expr = statement.expr();
    let mut context =
        EvaluationContext::new(document, ScopeRef::new(scopes, scope_index), config.clone());
    let mut evaluator = ExprTypeEvaluator::new(&mut context);
    let ty = evaluator.evaluate_expr(&expr).unwrap_or(Type::Union);
    let element_ty = match ty {
        Type::Union => Type::Union,
        Type::Compound(CompoundType::Array(ty), _) => ty.element_type().clone(),
        _ => {
            document
                .analysis_diagnostics
                .add(type_is_not_array(&ty, expr.span()));
            Type::Union
        }
    };

    // Introduce the scatter variable into the scope
    let variable = statement.variable();
    scopes[scope_index.0].insert(variable.text().to_string(), variable.span(), element_ty);
}

/// Adds a call statement to the current scope.
#[allow(clippy::too_many_arguments)]
fn add_call_statement(
    config: &Config,
    document: &mut DocumentData,
    mut scope: ScopeRefMut<'_>,
    statement: &CallStatement,
    workflow_name: &str,
    calls: &mut HashMap<String, CallType>,
    allows_nested_inputs: bool,
    is_used: bool,
    dependent: SignatureHash,
) {
    // Determine the target name
    let target_name = statement
        .target()
        .names()
        .last()
        .expect("expected a last call target name");

    // Determine the name of the call itself
    let name = statement
        .alias()
        .map(|a| a.name())
        .unwrap_or_else(|| target_name.clone());

    let ty = match resolve_call_type(document, workflow_name, statement, dependent) {
        Some(call_ty) => {
            // Type check the call inputs
            let mut seen = HashSet::new();
            for input in statement.inputs() {
                let input_name = input.name();

                let (expected_input_ty, required) = call_ty
                    .inputs()
                    .get(input_name.text())
                    .map(|i| (i.ty.clone(), i.required))
                    .unwrap_or_else(|| {
                        document.analysis_diagnostics.add(unknown_call_io(
                            &call_ty,
                            &input_name,
                            Io::Input,
                        ));
                        (Type::Union, true)
                    });

                // We accept optional types for the input even if the input's
                // type is non-optional; if the runtime value is `None` for a
                // non-optional input, the default expression will be evaluated
                // instead.
                let expected_input_ty = if !required {
                    expected_input_ty.optional()
                } else {
                    expected_input_ty
                };

                match input.expr() {
                    Some(expr) => {
                        type_check_expr(
                            config,
                            document,
                            scope.as_scope_ref(),
                            &expr,
                            &expected_input_ty,
                            input_name.span(),
                        );
                    }
                    None => match scope.lookup(input_name.text()) {
                        Some(name) => {
                            if !matches!(expected_input_ty, Type::Union)
                                && !name.ty.is_coercible_to(&expected_input_ty)
                            {
                                document.analysis_diagnostics.add(call_input_type_mismatch(
                                    &input_name,
                                    &expected_input_ty,
                                    &name.ty,
                                ));
                            }
                        }
                        None => {
                            document
                                .analysis_diagnostics
                                .add(unknown_name(input_name.text(), input_name.span()));
                        }
                    },
                }

                seen.insert(input_name.hashable());
            }

            for (name, input) in call_ty.inputs() {
                if input.required && !seen.contains(name.as_str()) {
                    document.analysis_diagnostics.add(missing_call_input(
                        call_ty.kind(),
                        &target_name,
                        name,
                        allows_nested_inputs,
                    ));
                }
            }

            // Add the call to the workflow
            if !calls.contains_key(name.text()) {
                calls.insert(name.text().to_string(), call_ty.clone());
            }

            call_ty.into()
        }
        _ => Type::Union,
    };

    // Don't modify the scope if there's a conflict
    if scope.lookup(name.text()).is_none() {
        // Check for unused call
        if let Some(severity) = config.diagnostics_config().unused_call
            && !is_used
            && let Some(ty) = ty.as_call()
            && !ty.outputs().is_empty()
        {
            document.analysis_diagnostics.exceptable_add(
                unused_call(name.text(), name.span()).with_severity(severity),
                statement.inner(),
                &UnusedCallRule::EXCEPTABLE_NODES,
            );
        }

        scope.insert(name.text(), name.span(), ty);
    }
}

/// Resolves the type of a call statement.
///
/// Returns `None` if the type could not be resolved.
fn resolve_call_type(
    document: &mut DocumentData,
    workflow_name: &str,
    statement: &CallStatement,
    dependent: SignatureHash,
) -> Option<CallType> {
    let target = statement.target();
    let mut targets = target.names().peekable();
    let mut namespace = None;
    let mut name = None;
    let mut local_dep = None;
    while let Some(target) = targets.next() {
        if targets.peek().is_none() {
            name = Some(target);
            break;
        }

        if namespace.is_some() {
            document
                .analysis_diagnostics
                .add(only_one_namespace(target.span()));
            return None;
        }

        if document.failed_imports.contains_key(target.text()) {
            return None;
        }

        let ns = document
            .cache
            .namespaces_mut()
            .find(|ns| ns.name() == target.text());
        match ns {
            Some(ns) => {
                ns.used = true;
                let (hash, ns) = document.cache.namespace_by_name(target.text()).unwrap();
                namespace = Some(ns);
                local_dep = Some(hash);
            }
            None => {
                document
                    .analysis_diagnostics
                    .add(unknown_namespace(&target));
                return None;
            }
        }
    }

    let target = namespace
        .map(|ns| ns.document().data.as_ref())
        .unwrap_or(document);
    let name = name.expect("should have name");
    let has_namespace = namespace.is_some();
    let namespace_span = namespace.map(|ns| ns.span());
    if !has_namespace && name.text() == workflow_name {
        document
            .analysis_diagnostics
            .add(recursive_workflow_call(name.text(), name.span()));
        return None;
    }

    // A locally failed selected import short-circuits before any lookup,
    // but only for an unqualified call against the local document.
    if !has_namespace && document.failed_selected_imports.contains(name.text()) {
        return None;
    }
    if !has_namespace && document.failed_wildcard_import {
        return None;
    }

    let (kind, inputs, outputs, local_dep) = match target.cache.task_by_name(name.text()) {
        Some((_hash, task)) => {
            let local_dep = if !has_namespace {
                match document.cache.item_by_name(name.text()) {
                    Some(Item::Local(item)) => Some(item.signature_hash()),
                    _ => None,
                }
            } else {
                local_dep
            };
            (CallKind::Task, task.inputs(), task.outputs(), local_dep)
        }
        _ => match target.cache.workflow() {
            Some(workflow) if workflow.name == name.text() => {
                let local_dep = if !has_namespace {
                    match document.cache.item_by_name(name.text()) {
                        Some(Item::Local(item)) => Some(item.signature_hash()),
                        _ => None,
                    }
                } else {
                    local_dep
                };
                (
                    CallKind::Workflow,
                    workflow.inputs.clone(),
                    workflow.outputs.clone(),
                    local_dep,
                )
            }
            _ if !has_namespace => {
                if document.failed_selected_imports.contains(name.text()) {
                    return None;
                } else if let Some((hash, imported)) =
                    document.cache.imported_task_by_name(name.text())
                {
                    (
                        CallKind::Task,
                        imported.inputs.clone(),
                        imported.outputs.clone(),
                        Some(hash),
                    )
                } else if let Some((hash, imported)) =
                    document.cache.imported_workflow_by_name(name.text())
                {
                    (
                        CallKind::Workflow,
                        imported.inputs.clone(),
                        imported.outputs.clone(),
                        Some(hash),
                    )
                } else {
                    document.analysis_diagnostics.add(unknown_task_or_workflow(
                        None,
                        name.text(),
                        name.span(),
                    ));
                    return None;
                }
            }
            _ => {
                document.analysis_diagnostics.add(unknown_task_or_workflow(
                    namespace_span,
                    name.text(),
                    name.span(),
                ));
                return None;
            }
        },
    };

    if let Some(dependency) = local_dep {
        document.cache.add_dependency(dependent, dependency);
    }

    let specified = Arc::new(
        statement
            .inputs()
            .map(|i| i.name().text().to_string())
            .collect(),
    );

    if has_namespace {
        Some(CallType::namespaced(
            kind,
            statement.target().names().next().unwrap().text(),
            name.text(),
            specified,
            inputs,
            outputs,
        ))
    } else {
        Some(CallType::new(kind, name.text(), specified, inputs, outputs))
    }
}

/// Resolves an import to its document.
///
/// On success, returns the resolved URI of the imported document along with
/// the [`Document`] itself.
///
/// On failure, returns an [`Option<Diagnostic>`]: `Some(diagnostic)` carries a
/// diagnostic the caller should push (e.g. an unresolvable symbolic import, an
/// import cycle, a load or analysis failure, or an incompatible WDL version),
/// while `None` means the import is malformed in a way that is already
/// diagnosed elsewhere and should be silently ignored here.
fn resolve_import<'a>(
    graph: &'a DocumentGraph,
    stmt: &ImportStatement,
    importer_index: NodeIndex,
) -> Result<(Document, Option<&'a AnalysisCache>), Option<Diagnostic>> {
    let importer_node = graph.get(importer_index);
    let (span, imported_index, source_label) = match stmt.source() {
        ImportSource::Uri(uri) => {
            let span = uri.span();
            let text = match uri.text() {
                Some(text) => text,
                // The import URI isn't valid; this is caught at validation time, so we do not
                // emit any additional diagnostics for it here.
                None => return Err(None),
            };
            let label = text.text().to_string();
            let resolved = match importer_node.uri().join(text.text()) {
                Ok(uri) => uri,
                Err(e) => return Err(Some(invalid_relative_import(&e, span))),
            };
            let index = graph
                .get_index(&resolved)
                .expect("missing import node in graph");
            (span, index, label)
        }
        ImportSource::ModulePath(module_path) => {
            let span = module_path.span();
            // A symbolic import in a pre-1.4 document is already rejected
            // during validation (see `validation::version`), so when the
            // feature is not enabled here the import is silently skipped.
            if !importer_node
                .parse_state()
                .symbolic_imports_enabled(graph.config())
            {
                return Err(None);
            }

            let path_text = module_path.text();
            match graph.get_resolved_symbolic_import(importer_index, &path_text) {
                Some(uri) => {
                    let index = graph
                        .get_index(uri)
                        .expect("resolved symbolic import missing from graph");
                    (span, index, path_text)
                }
                None => {
                    let message = if let Some(error) =
                        graph.get_failed_symbolic_import(importer_index, &path_text)
                    {
                        format!("failed to resolve symbolic import `{path_text}`: {error}")
                    } else {
                        format!("`{path_text}` is not a declared dependency")
                    };
                    return Err(Some(Diagnostic::error(message).with_highlight(span)));
                }
            }
        }
    };

    let imported_node = graph.get(imported_index);

    // Check for an import cycle to report
    if graph.contains_cycle(importer_index, imported_index) {
        return Err(Some(import_cycle(span)));
    }

    // Check for a failure to load the import
    if let ParseState::Error(e) = imported_node.parse_state() {
        return Err(Some(import_failure(&source_label, e, span)));
    }

    // Check for analysis error
    if let Some(e) = imported_node.analysis_error() {
        return Err(Some(import_failure(&source_label, e, span)));
    }

    // Ensure the import has a matching WDL version
    let imported_document = imported_node
        .document()
        .cloned()
        .expect("import should have been analyzed");

    let Some(imported_version) = imported_document.version() else {
        match imported_document.root().version_statement() {
            // The import's version statement is flat-out missing
            None => return Err(Some(import_missing_version(span))),
            // The import has a version statement, but it's not a supported version and no fallback
            // is configured
            Some(imported_version_stmt) => {
                return Err(Some(incompatible_import(
                    imported_version_stmt.version().text(),
                    span,
                    &importer_node
                        .root()
                        .and_then(|root| root.version_statement())
                        .expect("importer should have a version statement")
                        .version(),
                )));
            }
        }
    };
    let ParseState::Parsed {
        wdl_version: Some(importer_version),
        ..
    } = importer_node.parse_state()
    else {
        panic!("importer should have a parsed version");
    };
    // The imported document must share the importer's major version and
    // have a minor version no greater than the importer's. A dependency
    // authored against a newer minor version therefore fails at import.
    if !imported_version.has_same_major_version(*importer_version)
        || imported_version > *importer_version
    {
        return Err(Some(incompatible_import(
            &imported_version.to_string(),
            span,
            &importer_node
                .root()
                .and_then(|root| root.version_statement())
                .expect("importer should have a version statement")
                .version(),
        )));
    }

    Ok((imported_document, imported_node.cache()))
}

/// Populates struct and enum type information in the document.
fn populate_types(document: &mut DocumentData) {
    /// Used to resolve a type name from a document.
    struct Resolver<'a> {
        /// The document to resolve the type name from.
        document: &'a mut DocumentData,
        /// The offset to use to adjust the start of diagnostics.
        offset: usize,
        /// Dependent item hash
        dependent: Option<SignatureHash>,
    }

    impl TypeNameResolver for Resolver<'_> {
        fn resolve(&mut self, name: &str, span: Span) -> Result<Type, Diagnostic> {
            if let Some((ty, dependency, uri)) =
                self.document.cache.struct_by_name(name).map(|(hash, s)| {
                    (
                        s.ty().cloned().unwrap_or(Type::Union),
                        hash,
                        s.source().clone(),
                    )
                })
            {
                if let Some(dependent) = self.dependent {
                    self.document.cache.add_dependency(dependent, dependency);
                }

                if let Some(uri) = uri
                    && let Some(resolved) = self
                        .document
                        .cache
                        .namespaces_mut()
                        .find(|n| n.source() == uri)
                {
                    resolved.used = true;
                }
                return Ok(ty);
            }

            if let Some((ty, dependency, uri)) =
                self.document.cache.enum_by_name(name).map(|(hash, e)| {
                    (
                        e.ty().cloned().unwrap_or(Type::Union),
                        hash,
                        e.source().clone(),
                    )
                })
            {
                if let Some(dependent) = self.dependent {
                    self.document.cache.add_dependency(dependent, dependency);
                }

                if let Some(uri) = uri
                    && let Some(resolved) = self
                        .document
                        .cache
                        .namespaces_mut()
                        .find(|n| n.source() == uri)
                {
                    resolved.used = true;
                }
                return Ok(ty);
            }

            self.document.analysis_diagnostics.add(unknown_type(
                name,
                Span::new(span.start() + self.offset, span.len()),
            ));
            Ok(Type::Union)
        }
    }

    if document.cache.local_structs().count() == 0 && document.cache.local_enums().count() == 0 {
        return; // No types to populate
    }

    /// Recursively finds all nested type dependencies to build dependency
    /// graphs
    fn find_type_refs(ty: &wdl_ast::v1::Type, deps: &mut Vec<TypeRef>) {
        match ty {
            wdl_ast::v1::Type::Ref(r) => deps.push(r.clone()),
            wdl_ast::v1::Type::Array(a) => {
                find_type_refs(&a.element_type(), deps);
            }
            wdl_ast::v1::Type::Map(m) => {
                let (_, v) = m.types();
                find_type_refs(&v, deps);
            }
            wdl_ast::v1::Type::Pair(p) => {
                let (left, right) = p.types();
                find_type_refs(&left, deps);
                find_type_refs(&right, deps);
            }
            wdl_ast::v1::Type::Object(_) | wdl_ast::v1::Type::Primitive(_) => {}
        }
    }

    // Populate a type dependency graph; any edges that would form cycles are turned
    // into diagnostics.
    let mut graph: DiGraphMap<_, _, RandomState> = DiGraphMap::new();
    let mut space = Default::default();

    // Map struct dependencies
    for (from, _hash, s) in document.cache.local_structs() {
        let from_idx = TypeIndex::Struct(from);
        graph.add_node(from_idx);
        let definition = s.definition();
        for member in definition.members() {
            let mut deps = Vec::new();
            find_type_refs(&member.ty(), &mut deps);

            for dep in deps {
                let Some(to_idx) = resolve_dep(&document.cache, dep.name().text()) else {
                    continue;
                };

                if has_path_connecting(&graph, from_idx, to_idx, Some(&mut space)) {
                    let def_name = definition.name();
                    let def_span = def_name.span();
                    let member_span = member.name().span();
                    document.analysis_diagnostics.add(recursive_struct(
                        def_name.text(),
                        Span::new(def_span.start() + s.offset, def_span.len()),
                        Span::new(member_span.start() + s.offset, member_span.len()),
                    ));
                } else {
                    graph.add_edge(to_idx, from_idx, ());
                }
            }
        }
    }

    // Map enum dependencies
    for (from, _hash, e) in document.cache.local_enums() {
        let from_idx = TypeIndex::Enum(from);
        graph.add_node(from_idx);
        let definition = e.definition();
        if let Some(type_param) = definition.type_parameter() {
            let mut deps = Vec::new();
            find_type_refs(&type_param.ty(), &mut deps);

            for dep in deps {
                let Some(to_idx) = resolve_dep(&document.cache, dep.name().text()) else {
                    continue;
                };

                if has_path_connecting(&graph, from_idx, to_idx, Some(&mut space)) {
                    let def_name = definition.name();
                    let def_span = def_name.span();
                    document.analysis_diagnostics.add(recursive_enum(
                        def_name.text(),
                        Span::new(def_span.start() + e.offset, def_span.len()),
                        match to_idx {
                            TypeIndex::Struct(index) => {
                                document.cache.struct_by_index(index).unwrap().name()
                            }
                            TypeIndex::Enum(index) => {
                                document.cache.enum_by_index(index).unwrap().name()
                            }
                        },
                    ));
                } else {
                    graph.add_edge(to_idx, from_idx, ());
                }
            }
        }
    }

    // At this point the graph is guaranteed acyclic; now calculate the struct and
    // enum types in topological order
    for index in toposort(&graph, Some(&mut space)).expect("graph should be acyclic") {
        match index {
            TypeIndex::Struct(index) => {
                let definition = document.cache.struct_by_index(index).unwrap().definition();

                let offset = document.cache.struct_by_index(index).unwrap().offset();
                let struct_name = document
                    .cache
                    .struct_by_index(index)
                    .unwrap()
                    .name()
                    .to_string();
                let dependent = match document.cache.item_by_name(&struct_name) {
                    Some(Item::Local(item)) => Some(item.signature_hash()),
                    _ => None,
                };
                let mut converter = AstTypeConverter::new(Resolver {
                    document,
                    offset,
                    dependent,
                });
                match converter.convert_struct_type(&definition) {
                    Ok(ty) => {
                        let s = document.cache.struct_by_index_mut(index).unwrap();
                        assert!(s.ty().is_none(), "type should not already be present");
                        s.ty = Some(ty.into());
                    }
                    Err(mut diagnostic) => {
                        for label in diagnostic.labels_mut() {
                            let span = label.span();
                            label.set_span(Span::new(span.start() + offset, span.len()));
                        }
                        document.analysis_diagnostics.add(diagnostic);
                    }
                }
            }
            TypeIndex::Enum(index) => {
                let e = document.cache.enum_by_index(index).unwrap().clone();
                let definition = e.definition();
                let mut choices = Vec::new();
                let mut choice_spans = Vec::new();

                for choice in definition.choices() {
                    let choice_name = choice.name().text().to_string();
                    let choice_type = if let Some(value_expr) = choice.value() {
                        match parse_literal_value(&document.cache, &value_expr) {
                            Some(ty) => ty,
                            None => {
                                let span = value_expr.span();
                                let adjusted_span = Span::new(span.start() + e.offset, span.len());
                                document
                                    .analysis_diagnostics
                                    .add(non_literal_enum_value(adjusted_span));
                                Type::Union
                            }
                        }
                    } else {
                        PrimitiveType::String.into()
                    };

                    choices.push((choice_name, choice_type));
                    choice_spans.push(Span::new(
                        choice.span().start() + e.offset(),
                        choice.span().len(),
                    ));
                }

                let result = if let Some(type_param) = definition.type_parameter().map(|t| t.ty()) {
                    let enum_name = document
                        .cache
                        .enum_by_index(index)
                        .unwrap()
                        .name()
                        .to_string();
                    let dependent = match document.cache.item_by_name(&enum_name) {
                        Some(Item::Local(item)) => Some(item.signature_hash()),
                        _ => None,
                    };
                    let type_param = convert_ast_type(document, &type_param, dependent);
                    let e = document.cache.enum_by_index(index).unwrap();
                    EnumType::new(
                        e.name().to_string(),
                        e.name_span(),
                        type_param,
                        choices,
                        &choice_spans,
                    )
                } else {
                    EnumType::infer(e.name.clone(), choices, &choice_spans)
                };

                match result {
                    Ok(enum_ty) => {
                        let e = document.cache.enum_by_index_mut(index).unwrap();
                        e.ty = Some(enum_ty.into());
                    }
                    Err(diagnostic) => {
                        document.analysis_diagnostics.add(diagnostic);
                    }
                }
            }
        }
    }
}

/// An index to a type in a [`Document`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum TypeIndex {
    /// Index into `DocumentData::structs`.
    Struct(usize),
    /// Index into `DocumentData::enums`.
    Enum(usize),
}

/// Attempt to find a locally defined type in the `document` by name.
fn resolve_dep(cache: &crate::document::cache::AnalysisCache, name: &str) -> Option<TypeIndex> {
    if let Some((to_idx, _hash, _to)) = cache.local_struct_by_name(name) {
        Some(TypeIndex::Struct(to_idx))
    } else if let Some((to_idx, _hash, _to)) = cache.local_enum_by_name(name) {
        Some(TypeIndex::Enum(to_idx))
    } else {
        None
    }
}

/// Infers the type of a literal expression.
///
/// Returns `None` if the expression is not a literal or contains interpolation.
/// For struct literals, returns `None` since struct type information is not
/// available.
pub fn infer_type_from_literal(expr: &Expr) -> Option<Type> {
    match expr {
        Expr::Literal(lit) => match lit {
            LiteralExpr::Boolean(_) => Some(PrimitiveType::Boolean.into()),
            LiteralExpr::Integer(_) => Some(PrimitiveType::Integer.into()),
            LiteralExpr::Float(_) => Some(PrimitiveType::Float.into()),
            LiteralExpr::String(s) => {
                for part in s.parts() {
                    if matches!(part, StringPart::Placeholder(_)) {
                        return None;
                    }
                }
                Some(PrimitiveType::String.into())
            }
            LiteralExpr::Array(arr) => {
                let element_type = arr
                    .elements()
                    .filter_map(|e| infer_type_from_literal(&e))
                    .next()
                    .unwrap_or(Type::Union);
                Some(ArrayType::new(element_type).into())
            }
            LiteralExpr::Pair(pair) => {
                let (left, right) = pair.exprs();
                Some(
                    PairType::new(
                        infer_type_from_literal(&left)?,
                        infer_type_from_literal(&right)?,
                    )
                    .into(),
                )
            }
            LiteralExpr::Map(map) => {
                let mut items = map.items();
                let first = items.next();
                let (key_type, value_type) = match first {
                    Some(item) => {
                        let (k, v) = item.key_value();
                        (infer_type_from_literal(&k)?, infer_type_from_literal(&v)?)
                    }
                    None => (Type::Union, Type::Union),
                };
                Some(MapType::new(key_type, value_type).into())
            }
            LiteralExpr::Object(obj) => {
                for item in obj.items() {
                    let (_, val_expr) = item.name_value();
                    infer_type_from_literal(&val_expr)?;
                }
                Some(Type::Object)
            }
            LiteralExpr::None(_) => Some(Type::None),
            LiteralExpr::Struct(_)
            | LiteralExpr::Hints(_)
            | LiteralExpr::Input(_)
            | LiteralExpr::Output(_) => None,
        },
        _ => None,
    }
}

/// Validates that an expression is a literal and converts it to a type.
///
/// Returns `None` if the expression is not a valid literal for enum choice
/// values.
fn parse_literal_value(cache: &AnalysisCache, expr: &Expr) -> Option<Type> {
    // Handle struct literals specially since they need struct definitions
    if let Expr::Literal(LiteralExpr::Struct(s)) = expr {
        for item in s.items() {
            let (_, val_expr) = item.name_value();
            parse_literal_value(cache, &val_expr)?;
        }

        return cache
            .struct_by_name(s.name().text())
            .and_then(|(_hash, st)| st.ty().cloned());
    }

    infer_type_from_literal(expr)
}

/// Represents context to an expression type evaluator.
#[derive(Debug)]
struct EvaluationContext<'a> {
    /// The document data being evaluated.
    document: &'a mut DocumentData,
    /// The current evaluation scope.
    scope: ScopeRef<'a>,
    /// The configuration to use for expression evaluation.
    config: Config,
    /// The context of the task being evaluated.
    ///
    /// This is only `Some` when evaluating a task's `hints` section.`
    task: Option<&'a Task>,
}

impl<'a> EvaluationContext<'a> {
    /// Constructs a new expression type evaluation context.
    pub fn new(document: &'a mut DocumentData, scope: ScopeRef<'a>, config: Config) -> Self {
        Self {
            document,
            scope,
            config,
            task: None,
        }
    }

    /// Constructs a new expression type evaluation context with the given task.
    ///
    /// This is used to evaluated the type of expressions inside of a task's
    /// `hints` section.
    pub fn new_for_task(
        document: &'a mut DocumentData,
        scope: ScopeRef<'a>,
        config: Config,
        task: &'a Task,
    ) -> Self {
        Self {
            document,
            scope,
            config,
            task: Some(task),
        }
    }
}

impl crate::types::v1::EvaluationContext for EvaluationContext<'_> {
    fn version(&self) -> SupportedVersion {
        self.document
            .version
            .expect("document should have a version")
    }

    fn resolve_name(&mut self, name: &str, _: Span) -> Option<Type> {
        // Check if there are any variables with this name and return if so.
        if let Some(var) = self.scope.lookup(name).map(|n| n.ty().clone()) {
            return Some(var);
        }

        // If the name is a reference to a struct, return it as a [`Type::TypeNameRef`].
        if let Some(s) = self
            .document
            .cache
            .struct_by_name(name)
            .and_then(|(_hash, s)| s.ty().cloned())
        {
            return Some(
                TypeNameRef::new(
                    name,
                    s.as_struct()
                        .expect("type should be a struct")
                        .clone()
                        .into(),
                )
                .into(),
            );
        }

        // If the name is a reference to an enum, return it as a [`Type::TypeNameRef`].
        if let Some(e) = self
            .document
            .cache
            .enum_by_name(name)
            .and_then(|(_hash, e)| e.ty().cloned())
        {
            return Some(
                TypeNameRef::new(
                    name,
                    e.as_enum().expect("type should be an enum").clone().into(),
                )
                .into(),
            );
        }

        None
    }

    fn resolve_type_name(&mut self, name: &str, span: Span) -> Result<Type, Diagnostic> {
        if let Some((ty, uri)) = self
            .document
            .cache
            .struct_by_name(name)
            .map(|(_hash, s)| (s.ty().expect("struct should have type").clone(), s.source()))
        {
            if let Some(uri) = uri
                && let Some(resolved) = self
                    .document
                    .cache
                    .namespaces_mut()
                    .find(|n| n.source() == uri)
            {
                resolved.used = true;
            }

            return Ok(ty);
        }

        if let Some((ty, uri)) = self
            .document
            .cache
            .enum_by_name(name)
            .map(|(_hash, e)| (e.ty().expect("enum should have type").clone(), e.source()))
        {
            if let Some(uri) = uri
                && let Some(resolved) = self
                    .document
                    .cache
                    .namespaces_mut()
                    .find(|n| n.source() == uri)
            {
                resolved.used = true;
            }

            return Ok(ty);
        }

        Err(unknown_type(name, span))
    }

    fn task(&self) -> Option<&Task> {
        self.task
    }

    fn diagnostics_config(&self) -> DiagnosticsConfig {
        *self.config.diagnostics_config()
    }

    fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.document.analysis_diagnostics.add(diagnostic);
    }

    fn exceptable_add_diagnostic<N: TreeNode + Exceptable>(
        &mut self,
        diagnostic: Diagnostic,
        element: &N,
        exceptable_nodes: &Option<&'static [SyntaxKind]>,
    ) {
        self.document
            .analysis_diagnostics
            .exceptable_add(diagnostic, element, exceptable_nodes);
    }
}

/// Performs a type check of an expression.
fn type_check_expr(
    config: &Config,
    document: &mut DocumentData,
    scope: ScopeRef<'_>,
    expr: &Expr,
    expected: &Type,
    expected_span: Span,
) {
    let mut context = EvaluationContext::new(document, scope, config.clone());
    let mut evaluator = ExprTypeEvaluator::new(&mut context);
    let actual = evaluator.evaluate_expr(expr).unwrap_or(Type::Union);

    if !matches!(expected, Type::Union) && !actual.is_coercible_to(expected) {
        document.analysis_diagnostics.add(type_mismatch(
            expected,
            expected_span,
            &actual,
            expr.span(),
        ));
    }
    // Check to see if we're assigning an empty array literal to a non-empty type; we can statically
    // flag these as errors; otherwise, non-empty array constraints are checked at runtime
    else if let Type::Compound(CompoundType::Array(ty), _) = expected
        && ty.is_non_empty()
        && expr.is_empty_array_literal()
    {
        document
            .analysis_diagnostics
            .add(non_empty_array_assignment(expected_span, expr.span()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_scope(names: Vec<(impl Into<String>, Type)>) -> Scope {
        let mut scope = Scope::new(None, Span::new(0, 0));
        for (name, ty) in names.into_iter() {
            scope.insert(name, Span::new(0, 0), ty);
        }
        scope
    }

    fn example_scopes() -> Vec<Scope> {
        // if (...) {
        //   String a
        //   String b
        //   String always_available
        // } else if (...) {
        //   # If this clause executes, both `a` and `b` will be `None`.
        //   String? b = None
        //   String c = "bar"
        //   String always_available = "bar"
        // } else {
        //   String a = "baz"
        //   String b = "baz"
        //   String c = "baz"
        //   String always_available = "baz"
        // }
        //
        // Both `a` and `b` can be `None` or unevaluated, so they both promote as a
        // `String?`. `c` is missing from the first scope, so it must also be
        // marked as `String?`. `always_available` is always available, so it
        // will be promoted as a `String`.
        vec![
            example_scope(vec![
                ("a", Type::Primitive(PrimitiveType::String, false)),
                ("b", Type::Primitive(PrimitiveType::String, false)),
                (
                    "always_available",
                    Type::Primitive(PrimitiveType::String, false),
                ),
            ]),
            example_scope(vec![
                ("b", Type::Primitive(PrimitiveType::String, true)),
                ("c", Type::Primitive(PrimitiveType::String, false)),
                (
                    "always_available",
                    Type::Primitive(PrimitiveType::String, false),
                ),
            ]),
            example_scope(vec![
                ("a", Type::Primitive(PrimitiveType::String, false)),
                ("b", Type::Primitive(PrimitiveType::String, false)),
                ("c", Type::Primitive(PrimitiveType::String, false)),
                (
                    "always_available",
                    Type::Primitive(PrimitiveType::String, false),
                ),
            ]),
        ]
    }

    #[test_log::test]
    fn smoke() {
        let scopes = example_scopes();

        // Test with else clause (exhaustive)
        let mut scope_union = ScopeUnion::new();
        scope_union.insert(ScopeRef::new(&scopes, ScopeIndex(0)), false);
        scope_union.insert(ScopeRef::new(&scopes, ScopeIndex(1)), false);
        scope_union.insert(ScopeRef::new(&scopes, ScopeIndex(2)), true);

        let results = scope_union.resolve().expect("should resolve");

        // `a` is missing from clause 1, so it's optional
        assert_eq!(
            results["a"].ty,
            Type::Primitive(PrimitiveType::String, true)
        );

        // `b` is optional in clause 1, so it's optional
        assert_eq!(
            results["b"].ty,
            Type::Primitive(PrimitiveType::String, true)
        );

        // `c` is missing from clause 0, so it's optional
        assert_eq!(
            results["c"].ty,
            Type::Primitive(PrimitiveType::String, true)
        );

        // `always_available` is in all clauses with the same type, so it's non-optional
        assert_eq!(
            results["always_available"].ty,
            Type::Primitive(PrimitiveType::String, false)
        );
    }

    #[test_log::test]
    fn type_conflicts() {
        // Test scopes with type conflicts
        // if (...) {
        //   Int bad = 1
        // } else {
        //   String bad = "baz"
        // }
        //
        // `bad` will return an error, as there is no common type between a `String`
        // and an `Int`.
        let bad_scopes = vec![
            example_scope(vec![(
                "bad",
                Type::Primitive(PrimitiveType::Integer, false),
            )]),
            example_scope(vec![("bad", Type::Primitive(PrimitiveType::String, false))]),
        ];

        let mut scope_union = ScopeUnion::new();
        scope_union.insert(ScopeRef::new(&bad_scopes, ScopeIndex(0)), false);
        scope_union.insert(ScopeRef::new(&bad_scopes, ScopeIndex(1)), true);
        let err = scope_union.resolve().expect_err("should error on bad");
        assert_eq!(err.len(), 1);
        assert!(err[0].message().contains("type mismatch"));
    }
}
