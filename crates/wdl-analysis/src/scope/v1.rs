//! Conversion of a V1 AST to a document scope.
use std::collections::HashMap;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

use indexmap::IndexMap;
use petgraph::algo::has_path_connecting;
use petgraph::algo::toposort;
use petgraph::graph::DiGraph;
use petgraph::graph::NodeIndex;
use petgraph::prelude::DiGraphMap;
use url::Url;
use wdl_ast::v1::Ast;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::CommandPart;
use wdl_ast::v1::CommandSection;
use wdl_ast::v1::Decl;
use wdl_ast::v1::DocumentItem;
use wdl_ast::v1::Expr;
use wdl_ast::v1::HintsSection;
use wdl_ast::v1::ImportStatement;
use wdl_ast::v1::NameRef;
use wdl_ast::v1::RequirementsSection;
use wdl_ast::v1::RuntimeSection;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::TaskItem;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::v1::WorkflowItem;
use wdl_ast::v1::WorkflowStatement;
use wdl_ast::version::V1;
use wdl_ast::AstNode;
use wdl_ast::AstNodeExt;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxNode;
use wdl_ast::ToSpan;
use wdl_ast::TokenStrHash;
use wdl_ast::Version;

use super::braced_scope_span;
use super::heredoc_scope_span;
use super::Context;
use super::DocumentScope;
use super::Name;
use super::NameContext;
use super::Namespace;
use super::Scope;
use super::ScopeIndex;
use super::ScopeRefMut;
use super::Struct;
use super::Task;
use super::Workflow;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::scope::ScopeRef;
use crate::scope::TaskOutputScope;
use crate::types::v1::type_mismatch;
use crate::types::v1::AstTypeConverter;
use crate::types::v1::ExprTypeEvaluator;
use crate::types::Coercible;
use crate::types::Type;

/// The `task` variable name available in task command sections and outputs in
/// WDL 1.2.
const TASK_VAR_NAME: &str = "task";

/// Creates a "name conflict" diagnostic
fn name_conflict(name: &str, conflicting: Context, first: Context) -> Diagnostic {
    Diagnostic::error(format!("conflicting {conflicting} name `{name}`"))
        .with_label(
            format!("this {conflicting} conflicts with a previously used name"),
            conflicting.span(),
        )
        .with_label(
            format!("the {first} with the conflicting name is here"),
            first.span(),
        )
}

/// Creates a "namespace conflict" diagnostic
fn namespace_conflict(name: &str, conflicting: Span, first: Span, suggest_fix: bool) -> Diagnostic {
    let diagnostic = Diagnostic::error(format!("conflicting import namespace `{name}`"))
        .with_label("this conflicts with another import namespace", conflicting)
        .with_label(
            "the conflicting import namespace was introduced here",
            first,
        );

    if suggest_fix {
        diagnostic.with_fix("add an `as` clause to the import to specify a namespace")
    } else {
        diagnostic
    }
}

/// Creates an "import cycle" diagnostic
fn import_cycle(span: Span) -> Diagnostic {
    Diagnostic::error("import introduces a dependency cycle")
        .with_label("this import has been skipped to break the cycle", span)
}

/// Creates an "import failure" diagnostic
fn import_failure(uri: &str, error: &anyhow::Error, span: Span) -> Diagnostic {
    Diagnostic::error(format!("failed to import `{uri}`: {error:?}")).with_highlight(span)
}

/// Creates an "incompatible import" diagnostic
fn incompatible_import(
    import_version: &str,
    import_span: Span,
    importer_version: &Version,
) -> Diagnostic {
    Diagnostic::error("imported document has incompatible version")
        .with_label(
            format!("the imported document is version `{import_version}`"),
            import_span,
        )
        .with_label(
            format!(
                "the importing document is version `{version}`",
                version = importer_version.as_str()
            ),
            importer_version.span(),
        )
}

/// Creates an "import missing version" diagnostic
fn import_missing_version(span: Span) -> Diagnostic {
    Diagnostic::error("imported document is missing a version statement").with_highlight(span)
}

/// Creates an "invalid relative import" diagnostic
fn invalid_relative_import(error: &url::ParseError, span: Span) -> Diagnostic {
    Diagnostic::error(format!("{error:?}")).with_highlight(span)
}

/// Creates a "struct not in scope" diagnostic
fn struct_not_in_scope(name: &Ident) -> Diagnostic {
    Diagnostic::error(format!(
        "a struct named `{name}` does not exist in the imported document",
        name = name.as_str()
    ))
    .with_label("this struct does not exist", name.span())
}

/// Creates an "imported struct conflict" diagnostic
fn imported_struct_conflict(
    name: &str,
    conflicting: Span,
    first: Span,
    suggest_fix: bool,
) -> Diagnostic {
    let diagnostic = Diagnostic::error(format!("conflicting struct name `{name}`"))
        .with_label(
            "this import introduces a conflicting definition",
            conflicting,
        )
        .with_label("the first definition was introduced by this import", first);

    if suggest_fix {
        diagnostic.with_fix("add an `alias` clause to the import to specify a different name")
    } else {
        diagnostic
    }
}

/// Creates a "struct conflicts with import" diagnostic
fn struct_conflicts_with_import(name: &str, conflicting: Span, import: Span) -> Diagnostic {
    Diagnostic::error(format!("conflicting struct name `{name}`"))
        .with_label("this name conflicts with an imported struct", conflicting)
        .with_label("the import that introduced the struct is here", import)
        .with_fix(
            "either rename the struct or use an `alias` clause on the import with a different name",
        )
}

/// Creates a "duplicate workflow" diagnostic
fn duplicate_workflow(name: &Ident, first: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "cannot define workflow `{name}` as only one workflow is allowed per source file",
        name = name.as_str(),
    ))
    .with_label("consider moving this workflow to a new file", name.span())
    .with_label("first workflow is defined here", first)
}

/// Creates a "call conflict" diagnostic
fn call_conflict(name: &Ident, first: Context, suggest_fix: bool) -> Diagnostic {
    let diagnostic = Diagnostic::error(format!(
        "conflicting call name `{name}`",
        name = name.as_str()
    ))
    .with_label(
        "this call name conflicts with a previously used name",
        name.span(),
    )
    .with_label(
        format!("the {first} with the conflicting name is here"),
        first.span(),
    );

    if suggest_fix {
        diagnostic.with_fix("add an `as` clause to the call to specify a different name")
    } else {
        diagnostic
    }
}

/// Creates a "recursive struct" diagnostic.
fn recursive_struct(name: &str, span: Span, member: Span) -> Diagnostic {
    Diagnostic::error(format!("struct `{name}` has a recursive definition",))
        .with_highlight(span)
        .with_label("this struct member participates in the recursion", member)
}

/// Creates an "unknown type" diagnostic.
fn unknown_type(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error(format!("unknown type name `{name}`")).with_highlight(span)
}

/// Creates an "unknown name" diagnostic.
fn unknown_name(name: &str, span: Span) -> Diagnostic {
    // Handle special case names here
    let message = match name {
        "task" => "the `task` variable may only be used within a task command section or task \
                   output section using WDL 1.2 or later"
            .to_string(),
        _ => format!("unknown name `{name}`"),
    };

    Diagnostic::error(message).with_highlight(span)
}

/// Creates a "self-referential" diagnostic.
fn self_referential(name: &str, span: Span, reference: Span) -> Diagnostic {
    Diagnostic::error(format!("declaration of `{name}` is self-referential"))
        .with_label("self-reference is here", reference)
        .with_highlight(span)
}

/// Creates a "reference cycle" diagnostic.
fn reference_cycle(from: &str, from_span: Span, to: &str, to_span: Span) -> Diagnostic {
    Diagnostic::error("a name reference cycle was detected")
        .with_label(
            format!("ensure this expression does not directly or indirectly refer to `{from}`"),
            to_span,
        )
        .with_label(format!("a reference back to `{to}` is here"), from_span)
}

/// Creates a new document scope for a V1 AST.
pub(crate) fn scope_from_ast(
    graph: &DocumentGraph,
    index: NodeIndex,
    ast: &Ast,
    version: &Version,
    diagnostics: &mut Vec<Diagnostic>,
) -> DocumentScope {
    let mut document = DocumentScope {
        version: SupportedVersion::from_str(version.as_str()).ok(),
        ..Default::default()
    };

    for item in ast.items() {
        match item {
            DocumentItem::Import(import) => {
                add_namespace(&mut document, graph, &import, index, version, diagnostics);
            }
            DocumentItem::Struct(s) => {
                add_struct(&mut document, &s, diagnostics);
            }
            DocumentItem::Task(task) => {
                add_task(&mut document, &task, diagnostics);
            }
            DocumentItem::Workflow(workflow) => {
                add_workflow(&mut document, &workflow, diagnostics);
            }
        }
    }

    // Populate the struct types now that all structs are accounted for
    add_struct_types(&mut document, diagnostics);

    // Finally, perform a type check
    type_check(&mut document, ast, diagnostics);

    document
}

/// Adds a namespace to the document scope.
fn add_namespace(
    document: &mut DocumentScope,
    graph: &DocumentGraph,
    import: &ImportStatement,
    importer_index: NodeIndex,
    importer_version: &Version,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Start by resolving the import to its document scope
    let (uri, imported) = match resolve_import(graph, import, importer_index, importer_version) {
        Ok(resolved) => resolved,
        Err(Some(diagnostic)) => {
            diagnostics.push(diagnostic);
            return;
        }
        Err(None) => return,
    };

    // Check for conflicting namespaces
    let span = import.uri().syntax().text_range().to_span();
    let ns = match import.namespace() {
        Some((ns, span)) => {
            if let Some(prev) = document.namespaces.get(&ns) {
                diagnostics.push(namespace_conflict(
                    &ns,
                    span,
                    prev.span,
                    import.explicit_namespace().is_none(),
                ));
                return;
            } else {
                document.namespaces.insert(
                    ns.clone(),
                    Namespace {
                        span,
                        source: uri.clone(),
                        scope: imported.clone(),
                    },
                );
                ns
            }
        }
        None => {
            // Invalid import namespaces are caught during validation, so there is already a
            // diagnostic for this issue; ignore the import here
            return;
        }
    };

    // Get the alias map for the namespace.
    let aliases = import
        .aliases()
        .filter_map(|a| {
            let (from, to) = a.names();
            if !imported.structs.contains_key(from.as_str()) {
                diagnostics.push(struct_not_in_scope(&from));
                return None;
            }

            Some((from.as_str().to_string(), to))
        })
        .collect::<HashMap<_, _>>();

    // Insert the import scope's struct definitions
    for (name, s) in &imported.structs {
        let namespace = document.namespaces.get(&ns).unwrap();
        let (span, aliased_name, aliased) = aliases
            .get(name)
            .map(|n| (n.span(), n.as_str(), true))
            .unwrap_or_else(|| (span, name, false));
        match document.structs.get(aliased_name) {
            Some(prev) => {
                // Import conflicts with a struct defined in this document
                if prev.namespace.is_none() {
                    diagnostics.push(struct_conflicts_with_import(aliased_name, prev.span, span));
                    continue;
                }

                if !are_structs_equal(prev, s) {
                    diagnostics.push(imported_struct_conflict(
                        aliased_name,
                        span,
                        prev.span,
                        !aliased,
                    ));
                    continue;
                }
            }
            None => {
                document.structs.insert(
                    aliased_name.to_string(),
                    Struct {
                        span,
                        offset: s.offset,
                        node: s.node.clone(),
                        namespace: Some(ns.clone()),
                        ty: s
                            .ty
                            .map(|ty| document.types.import(&namespace.scope.types, ty)),
                    },
                );
            }
        }
    }
}

/// Compares two structs for structural equality.
fn are_structs_equal(a: &Struct, b: &Struct) -> bool {
    let a = StructDefinition::cast(SyntaxNode::new_root(a.node.clone())).expect("node should cast");
    let b = StructDefinition::cast(SyntaxNode::new_root(b.node.clone())).expect("node should cast");
    for (a, b) in a.members().zip(b.members()) {
        if a.name().as_str() != b.name().as_str() {
            return false;
        }

        if a.ty() != b.ty() {
            return false;
        }
    }

    true
}

/// Adds a struct to the document scope.
fn add_struct(
    document: &mut DocumentScope,
    definition: &StructDefinition,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let name = definition.name();
    if let Some(prev) = document.structs.get(name.as_str()) {
        if prev.namespace.is_some() {
            diagnostics.push(struct_conflicts_with_import(
                name.as_str(),
                name.span(),
                prev.span,
            ))
        } else {
            diagnostics.push(name_conflict(
                name.as_str(),
                Context::Struct(name.span()),
                Context::Struct(prev.span),
            ));
        }
        return;
    }

    // Ensure there are no duplicate members
    let mut members = IndexMap::new();
    for decl in definition.members() {
        let name = decl.name();
        if let Some(prev_span) = members.get(name.as_str()) {
            diagnostics.push(name_conflict(
                name.as_str(),
                Context::StructMember(name.span()),
                Context::StructMember(*prev_span),
            ));
        } else {
            members.insert(name.as_str().to_string(), name.span());
        }
    }

    document.structs.insert(
        name.as_str().to_string(),
        Struct {
            span: name.span(),
            namespace: None,
            offset: definition.span().start(),
            node: definition.syntax().green().into(),
            ty: None,
        },
    );
}

/// Adds an input to a scope.
fn add_input(mut scope: ScopeRefMut<'_>, decl: Decl, diagnostics: &mut Vec<Diagnostic>) {
    let name = decl.name();
    if let Some(prev) = scope.lookup(name.as_str()) {
        diagnostics.push(name_conflict(
            name.as_str(),
            NameContext::Input(name.span()).into(),
            prev.context.into(),
        ));
        return;
    }

    scope.insert(
        name.as_str().to_string(),
        Name::new(NameContext::Input(name.span())),
    );
}

/// Adds an output to a scope.
fn add_output(mut scope: ScopeRefMut<'_>, decl: BoundDecl, diagnostics: &mut Vec<Diagnostic>) {
    let name = decl.name();
    if let Some(prev) = scope.lookup(name.as_str()) {
        diagnostics.push(name_conflict(
            name.as_str(),
            NameContext::Output(name.span()).into(),
            prev.context.into(),
        ));
        return;
    }

    scope.insert(
        name.as_str().to_string(),
        Name::new(NameContext::Output(name.span())),
    );
}

/// Adds a declaration to a scope.
fn add_decl(mut scope: ScopeRefMut<'_>, decl: BoundDecl, diagnostics: &mut Vec<Diagnostic>) {
    let name = decl.name();
    if let Some(prev) = scope.lookup(name.as_str()) {
        diagnostics.push(name_conflict(
            name.as_str(),
            NameContext::Decl(name.span()).into(),
            prev.context.into(),
        ));
        return;
    }

    scope.insert(
        name.as_str().to_string(),
        Name::new(NameContext::Decl(name.span())),
    );
}

/// Adds a task to the document's scope.
fn add_task(
    document: &mut DocumentScope,
    task: &TaskDefinition,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Check for a name conflict with another task or workflow
    let name = task.name();
    if let Some(s) = document.tasks.get(name.as_str()) {
        diagnostics.push(name_conflict(
            name.as_str(),
            Context::Task(name.span()),
            Context::Task(s.name_span),
        ));
        return;
    } else if let Some(s) = &document.workflow {
        if s.name == name.as_str() {
            diagnostics.push(name_conflict(
                name.as_str(),
                Context::Task(name.span()),
                Context::Workflow(s.name_span),
            ));
            return;
        }
    }

    // Populate the task's scope and evaluation graph
    let scope = document.add_scope(Scope::new(None, braced_scope_span(task)));
    let mut saw_input = false;
    let mut outputs = None;
    let mut command = None;
    for item in task.items() {
        match item {
            TaskItem::Input(section) if !saw_input => {
                saw_input = true;
                for decl in section.declarations() {
                    add_input(document.scope_mut(scope), decl, diagnostics)
                }
            }
            TaskItem::Output(section) if outputs.is_none() => {
                let child =
                    document.add_scope(Scope::new(Some(scope), braced_scope_span(&section)));
                document.scope_mut(scope).add_child(child);

                if document.version >= Some(SupportedVersion::V1(V1::Two)) {
                    document.scopes[child.0].names.insert(
                        TASK_VAR_NAME.to_string(),
                        Name {
                            context: NameContext::Task(name.span()),
                            ty: Some(Type::Task),
                        },
                    );
                }

                for decl in section.declarations() {
                    add_output(document.scope_mut(child), decl, diagnostics);
                }

                outputs = Some(child);
            }
            TaskItem::Declaration(decl) => {
                add_decl(document.scope_mut(scope), decl, diagnostics);
            }
            TaskItem::Command(section) if command.is_none() => {
                let span = if section.is_heredoc() {
                    heredoc_scope_span(&section)
                } else {
                    braced_scope_span(&section)
                };

                let child = document.add_scope(Scope::new(Some(scope), span));
                document.scope_mut(scope).add_child(child);

                if document.version >= Some(SupportedVersion::V1(V1::Two)) {
                    document.scopes[child.0].names.insert(
                        TASK_VAR_NAME.to_string(),
                        Name {
                            context: NameContext::Task(name.span()),
                            ty: Some(Type::Task),
                        },
                    );
                }

                command = Some(child);
            }
            TaskItem::Input(_)
            | TaskItem::Output(_)
            | TaskItem::Command(_)
            | TaskItem::Requirements(_)
            | TaskItem::Hints(_)
            | TaskItem::Runtime(_)
            | TaskItem::Metadata(_)
            | TaskItem::ParameterMetadata(_) => continue,
        }
    }

    document.tasks.insert(
        name.as_str().to_string(),
        Task {
            name_span: name.span(),
            scope,
            outputs,
            command,
        },
    );
}

/// Adds a workflow to the document scope.
fn add_workflow(
    document: &mut DocumentScope,
    workflow: &WorkflowDefinition,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Check for conflicts with task names or an existing workspace
    let name = workflow.name();
    if let Some(s) = document.tasks.get(name.as_str()) {
        diagnostics.push(name_conflict(
            name.as_str(),
            Context::Workflow(name.span()),
            Context::Task(s.name_span),
        ));
        return;
    } else if let Some(s) = &document.workflow {
        diagnostics.push(duplicate_workflow(&name, s.name_span));
        return;
    }

    let scope = document.add_scope(Scope::new(None, braced_scope_span(workflow)));
    let mut saw_input = false;
    let mut saw_output = false;
    for item in workflow.items() {
        match item {
            WorkflowItem::Input(section) if !saw_input => {
                saw_input = true;
                for decl in section.declarations() {
                    add_input(document.scope_mut(scope), decl, diagnostics)
                }
            }
            WorkflowItem::Output(section) if !saw_output => {
                saw_output = true;
                let outputs =
                    document.add_scope(Scope::new(Some(scope), braced_scope_span(&section)));
                document.scope_mut(scope).add_child(outputs);
                for decl in section.declarations() {
                    add_output(document.scope_mut(outputs), decl, diagnostics);
                }
            }
            WorkflowItem::Declaration(decl) => {
                add_workflow_statement_decls(
                    document,
                    &WorkflowStatement::Declaration(decl),
                    scope,
                    diagnostics,
                );
            }
            WorkflowItem::Conditional(stmt) => {
                add_workflow_statement_decls(
                    document,
                    &WorkflowStatement::Conditional(stmt),
                    scope,
                    diagnostics,
                );
            }
            WorkflowItem::Scatter(stmt) => {
                add_workflow_statement_decls(
                    document,
                    &WorkflowStatement::Scatter(stmt),
                    scope,
                    diagnostics,
                );
            }
            WorkflowItem::Call(stmt) => {
                add_workflow_statement_decls(
                    document,
                    &WorkflowStatement::Call(stmt),
                    scope,
                    diagnostics,
                );
            }
            WorkflowItem::Input(_)
            | WorkflowItem::Output(_)
            | WorkflowItem::Metadata(_)
            | WorkflowItem::ParameterMetadata(_)
            | WorkflowItem::Hints(_) => continue,
        }
    }

    document.workflow = Some(Workflow {
        name_span: name.span(),
        name: name.as_str().to_string(),
        scope,
    });
}

/// Adds declarations from workflow statements.
fn add_workflow_statement_decls(
    document: &mut DocumentScope,
    stmt: &WorkflowStatement,
    parent: ScopeIndex,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match stmt {
        WorkflowStatement::Conditional(stmt) => {
            let scope = document.add_scope(Scope::new(Some(parent), braced_scope_span(stmt)));
            document.scope_mut(parent).add_child(scope);

            for stmt in stmt.statements() {
                add_workflow_statement_decls(document, &stmt, scope, diagnostics);
            }

            // We need to split the scopes as we want to read from one part of the slice and
            // write to another; the left side will contain the parent at it's index and the
            // right side will contain the child scope at it's index minus the parent's
            assert!(scope.0 > parent.0);
            let (left, right) = document.scopes.split_at_mut(parent.0 + 1);
            let scope = &right[scope.0 - parent.0 - 1];
            let parent = &mut left[parent.0];
            for (name, local) in scope.names.iter() {
                parent.names.insert(name.clone(), Name::new(local.context));
            }
        }
        WorkflowStatement::Scatter(stmt) => {
            let scope = document.add_scope(Scope::new(Some(parent), braced_scope_span(stmt)));
            document.scope_mut(parent).add_child(scope);

            // Introduce the scatter variable into the scope
            let variable = stmt.variable();
            let context = NameContext::ScatterVariable(variable.span());
            if let Some(prev) = document.scope(scope).lookup(variable.as_str()) {
                diagnostics.push(name_conflict(
                    variable.as_str(),
                    context.into(),
                    prev.context().into(),
                ));
            }

            document
                .scope_mut(scope)
                .insert(variable.as_str().to_string(), Name::new(context));

            // Process the statements
            for stmt in stmt.statements() {
                add_workflow_statement_decls(document, &stmt, scope, diagnostics);
            }

            // We need to split the scopes as we want to read from one part of the slice and
            // write to another; the left side will contain the parent at it's index and the
            // right side will contain the child scope at it's index minus the parent's
            assert!(scope.0 > parent.0);
            let (left, right) = document.scopes.split_at_mut(parent.0 + 1);
            let scope = &right[scope.0 - parent.0 - 1];
            let parent = &mut left[parent.0];

            for (name, local) in scope.names.iter() {
                // Don't export the scatter variable into the parent scope
                if !matches!(local.context, NameContext::ScatterVariable(_)) {
                    parent.names.insert(name.clone(), Name::new(local.context));
                }
            }
        }
        WorkflowStatement::Call(stmt) => {
            let name = stmt.alias().map(|a| a.name()).unwrap_or_else(|| {
                stmt.target()
                    .names()
                    .last()
                    .expect("expected a last call target name")
            });

            if let Some(prev) = document.scope(parent).lookup(name.as_str()) {
                diagnostics.push(call_conflict(
                    &name,
                    prev.context().into(),
                    stmt.alias().is_none(),
                ));

                // Define the name in this scope if it conflicted with a scatter variable
                if !matches!(prev.context, NameContext::ScatterVariable(_)) {
                    return;
                }
            }

            document.scope_mut(parent).insert(
                name.as_str().to_string(),
                Name::new(NameContext::Call(name.span())),
            );
        }
        WorkflowStatement::Declaration(decl) => {
            let name = decl.name();
            let context = NameContext::Decl(name.span());
            if let Some(prev) = document.scope(parent).lookup(name.as_str()) {
                diagnostics.push(name_conflict(
                    name.as_str(),
                    context.into(),
                    prev.context().into(),
                ));

                // Define the name in this scope if it conflicted with a scatter variable
                if !matches!(prev.context, NameContext::ScatterVariable(_)) {
                    return;
                }
            }

            document
                .scope_mut(parent)
                .insert(name.as_str().to_string(), Name::new(context));
        }
    }
}

/// Resolves an import to its document scope.
fn resolve_import(
    graph: &DocumentGraph,
    stmt: &ImportStatement,
    importer_index: NodeIndex,
    importer_version: &Version,
) -> Result<(Arc<Url>, Arc<DocumentScope>), Option<Diagnostic>> {
    let uri = stmt.uri();
    let span = uri.syntax().text_range().to_span();
    let text = match uri.text() {
        Some(text) => text,
        None => {
            // The import URI isn't valid; this is caught at validation time, so we do not
            // emit any additional diagnostics for it here.
            return Err(None);
        }
    };

    let uri = match graph.get(importer_index).uri().join(text.as_str()) {
        Ok(uri) => uri,
        Err(e) => return Err(Some(invalid_relative_import(&e, span))),
    };

    let import_index = graph.get_index(&uri).expect("missing import node in graph");
    let import_node = graph.get(import_index);

    // Check for an import cycle to report
    if graph.contains_cycle(importer_index, import_index) {
        return Err(Some(import_cycle(span)));
    }

    // Check for a failure to load the import
    if let ParseState::Error(e) = import_node.parse_state() {
        return Err(Some(import_failure(text.as_str(), e, span)));
    }

    // Ensure the import has a matching WDL version
    let import_document = import_node.document().expect("import should have parsed");
    let import_scope = import_node
        .analysis()
        .map(|a| a.scope().clone())
        .expect("import should have been analyzed");

    // Check for compatible imports
    match import_document.version_statement() {
        Some(stmt) => {
            let our_version = stmt.version();
            if matches!((our_version.as_str().split('.').next(), importer_version.as_str().split('.').next()), (Some(our_major), Some(their_major)) if our_major != their_major)
            {
                return Err(Some(incompatible_import(
                    our_version.as_str(),
                    span,
                    importer_version,
                )));
            }
        }
        None => {
            return Err(Some(import_missing_version(span)));
        }
    }

    Ok((import_node.uri().clone(), import_scope))
}

/// Adds the struct types to the document.
fn add_struct_types(document: &mut DocumentScope, diagnostics: &mut Vec<Diagnostic>) {
    if document.structs.is_empty() {
        return;
    }

    // Populate a type dependency graph; any edges that would form cycles are turned
    // into diagnostics.
    let mut graph = DiGraphMap::new();
    let mut space = Default::default();
    for (from, s) in document.structs.values().enumerate() {
        // Only look at locally defined structs
        if s.namespace.is_some() {
            continue;
        }

        graph.add_node(from);
        let definition: StructDefinition =
            StructDefinition::cast(SyntaxNode::new_root(s.node.clone())).expect("node should cast");
        for member in definition.members() {
            if let wdl_ast::v1::Type::Ref(r) = member.ty() {
                // Add an edge to the referenced struct
                if let Some(to) = document.structs.get_index_of(r.name().as_str()) {
                    // Only add an edge to another local struct definition
                    if document.structs[to].namespace.is_some() {
                        continue;
                    }

                    // Check to see if the edge would form a cycle
                    if has_path_connecting(&graph, from, to, Some(&mut space)) {
                        let name = definition.name();
                        let name_span = name.span();
                        let member_span = member.name().span();
                        diagnostics.push(recursive_struct(
                            name.as_str(),
                            Span::new(name_span.start() + s.offset, name_span.len()),
                            Span::new(member_span.start() + s.offset, member_span.len()),
                        ));
                    } else {
                        graph.add_edge(to, from, ());
                    }
                }
            }
        }
    }

    // At this point the graph is guaranteed acyclic; now calculate the struct types
    // in topological order
    for index in toposort(&graph, Some(&mut space)).expect("graph should be acyclic") {
        let definition =
            StructDefinition::cast(SyntaxNode::new_root(document.structs[index].node.clone()))
                .expect("node should cast");

        let structs = &document.structs;
        let mut converter = AstTypeConverter::new(&mut document.types, |name, span| {
            if let Some(s) = structs.get(name) {
                Ok(s.ty().unwrap_or(Type::Union))
            } else {
                diagnostics.push(unknown_type(
                    name,
                    Span::new(span.start() + structs[index].offset, span.len()),
                ));
                Ok(Type::Union)
            }
        });

        let ty = converter
            .convert_struct_type(&definition)
            .expect("struct type conversion should not fail");

        let s = &mut document.structs[index];
        assert!(s.ty.is_none(), "type should not already be present");
        s.ty = Some(document.types.add_struct(ty));
    }
}

/// Performs type checking on a document.
fn type_check(document: &mut DocumentScope, ast: &Ast, diagnostics: &mut Vec<Diagnostic>) {
    let mut seen = HashSet::new();
    for item in ast.items() {
        match item {
            DocumentItem::Import(_) | DocumentItem::Struct(_) => continue,
            DocumentItem::Task(definition) => {
                if let Some(task) = document.tasks.get_index_of(definition.name().as_str()) {
                    // Only process the first task we encounter in the AST with this name
                    // Duplicates that come later will be excluded from type checking, but a
                    // diagnostic will have already been added for the duplicate
                    if seen.insert(task) {
                        type_check_task(document, &definition, task, diagnostics);
                    }
                }
            }
            DocumentItem::Workflow(_) => {
                // TODO: implement
            }
        }
    }
}

/// Performs a type check on a task.
fn type_check_task(
    document: &mut DocumentScope,
    definition: &TaskDefinition,
    task_index: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    /// Represents a node in the name reference graph.
    enum GraphNode {
        /// The node is an input/output/decl node.
        Decl {
            /// The scope to use to evaluate the expression.
            scope: ScopeIndex,
            /// The expected type of the expression.
            expected: Type,
            /// The optional expression to evaluate.
            expr: Option<Expr>,
            /// The span of the associated declaration name.
            span: Span,
        },
        /// The node is the task's command.
        Command {
            /// The scope to use to evaluate the command.
            scope: ScopeIndex,
            /// The command section.
            section: CommandSection,
        },
        /// The node is a runtime section.
        Runtime {
            /// The scope to use for evaluating the runtime section.
            scope: ScopeIndex,
            /// The runtime section.
            section: RuntimeSection,
        },
        /// The node is a requirements section.
        Requirements {
            /// The scope to use for evaluating the requirements section.
            scope: ScopeIndex,
            /// The requirements section.
            section: RequirementsSection,
        },
        /// The node is a hints section.
        Hints {
            /// The scope to use for evaluating the hints section.
            scope: ScopeIndex,
            /// The hints section.
            section: HintsSection,
        },
    }

    /// Looks up a struct type.
    fn lookup_type(
        structs: &IndexMap<String, Struct>,
        name: &str,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        structs
            .get(name)
            .map(|s| s.ty().expect("struct should have type"))
            .ok_or_else(|| unknown_type(name, span))
    }

    /// Adds a decl node to the name reference graph.
    ///
    /// This also populates the declaration type for the name in the scope.
    fn add_decl_node(
        document: &mut DocumentScope,
        graph: &mut DiGraph<GraphNode, ()>,
        names: &mut IndexMap<TokenStrHash<Ident>, NodeIndex>,
        scope: ScopeIndex,
        decl: Decl,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let name = decl.name();
        if let Some(local) = document.scopes[scope.0].names.get_mut(name.as_str()) {
            if local.ty.is_some() {
                return;
            }

            // Convert the AST type
            let mut converter = AstTypeConverter::new(&mut document.types, |name, span| {
                lookup_type(&document.structs, name, span)
            });
            let ty = match converter.convert_type(&decl.ty()) {
                Ok(ty) => ty,
                Err(diagnostic) => {
                    diagnostics.push(diagnostic);
                    Type::Union
                }
            };
            local.ty = Some(ty);

            // Add a node to the graph for this declaration
            let span = name.span();
            names.insert(
                TokenStrHash::new(name),
                graph.add_node(GraphNode::Decl {
                    scope,
                    expected: ty,
                    expr: decl.expr(),
                    span,
                }),
            );
        }
    }

    /// Adds edges from task sections to declarations.
    fn add_section_edges(
        document: &DocumentScope,
        graph: &mut DiGraph<GraphNode, ()>,
        names: &IndexMap<TokenStrHash<Ident>, NodeIndex>,
        diagnostics: &mut Vec<Diagnostic>,
        from: NodeIndex,
        descendants: impl Iterator<Item = SyntaxNode>,
        scope: ScopeIndex,
    ) {
        // Add edges for any descendant name references
        for r in descendants.filter_map(NameRef::cast) {
            let name = r.name();

            // Look up the name; we don't check for cycles here as decls can't
            // reference a section.
            if document.scope(scope).lookup(name.as_str()).is_some() {
                if let Some(to) = names.get(name.as_str()) {
                    graph.update_edge(*to, from, ());
                }
            } else {
                diagnostics.push(unknown_name(name.as_str(), name.span()));
            }
        }
    }

    /// Adds name reference edges to the graph.
    fn add_reference_edges(
        document: &DocumentScope,
        graph: &mut DiGraph<GraphNode, ()>,
        names: &IndexMap<TokenStrHash<Ident>, NodeIndex>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut space = Default::default();

        // Populate edges for any nodes that reference other nodes by name
        for from in graph.node_indices() {
            match &graph[from] {
                GraphNode::Decl {
                    scope,
                    expected: _,
                    expr,
                    span,
                } => {
                    let scope = *scope;
                    let span = *span;
                    let expr = expr.clone();

                    if let Some(expr) = expr {
                        for r in expr.syntax().descendants().filter_map(NameRef::cast) {
                            let name = r.name();

                            // Look up the name, checking for cycles
                            if document.scope(scope).lookup(name.as_str()).is_some() {
                                if let Some(to) = names.get(name.as_str()) {
                                    // Check to see if the node is self-referential
                                    if *to == from {
                                        diagnostics.push(self_referential(
                                            name.as_str(),
                                            span,
                                            name.span(),
                                        ));
                                        continue;
                                    }

                                    // Check to see if the edge would form a cycle
                                    if has_path_connecting(&*graph, from, *to, Some(&mut space)) {
                                        diagnostics.push(reference_cycle(
                                            names
                                                .get_index(from.index())
                                                .unwrap()
                                                .0
                                                .as_ref()
                                                .as_str(),
                                            r.span(),
                                            name.as_str(),
                                            match &graph[*to] {
                                                GraphNode::Decl { expr, .. } => expr
                                                    .as_ref()
                                                    .map(|e| e.span())
                                                    .expect("should have expr to form a cycle"),
                                                _ => panic!("expected decl node"),
                                            },
                                        ));
                                        continue;
                                    }

                                    graph.update_edge(*to, from, ());
                                }
                            } else {
                                diagnostics.push(unknown_name(name.as_str(), name.span()));
                            }
                        }
                    }
                }
                GraphNode::Command { scope, section } => {
                    // Add name references from the command section to any decls in scope
                    let scope = *scope;
                    let section = section.clone();
                    for part in section.parts() {
                        if let CommandPart::Placeholder(p) = part {
                            add_section_edges(
                                document,
                                graph,
                                names,
                                diagnostics,
                                from,
                                p.syntax().descendants(),
                                scope,
                            );
                        }
                    }
                }
                GraphNode::Runtime { scope, section } => {
                    // Add name references from the runtime section to any decls in scope
                    let scope = *scope;
                    let section = section.clone();
                    for item in section.items() {
                        add_section_edges(
                            document,
                            graph,
                            names,
                            diagnostics,
                            from,
                            item.expr().syntax().descendants(),
                            scope,
                        );
                    }
                }
                GraphNode::Requirements { scope, section } => {
                    // Add name references from the requirements section to any decls in scope
                    let scope = *scope;
                    let section = section.clone();
                    for item in section.items() {
                        add_section_edges(
                            document,
                            graph,
                            names,
                            diagnostics,
                            from,
                            item.expr().syntax().descendants(),
                            scope,
                        );
                    }
                }
                GraphNode::Hints { scope, section } => {
                    // Add name references from the hints section to any decls in scope
                    let scope = *scope;
                    let section = section.clone();
                    for item in section.items() {
                        add_section_edges(
                            document,
                            graph,
                            names,
                            diagnostics,
                            from,
                            item.expr().syntax().descendants(),
                            scope,
                        );
                    }
                }
            }
        }
    }

    // Populate the declaration types and build a name reference graph
    let mut saw_inputs = false;
    let mut saw_outputs = false;
    let mut saw_runtime = false;
    let mut saw_requirements = false;
    let mut saw_hints = false;
    let mut command = None;
    let mut graph = DiGraph::new();
    let mut names = IndexMap::new();
    for item in definition.items() {
        match item {
            TaskItem::Input(section) if !saw_inputs => {
                saw_inputs = true;
                for decl in section.declarations() {
                    add_decl_node(
                        document,
                        &mut graph,
                        &mut names,
                        document.tasks[task_index].scope,
                        decl,
                        diagnostics,
                    );
                }
            }
            TaskItem::Output(section) if !saw_outputs => {
                saw_outputs = true;
                let scope = document.tasks[task_index]
                    .outputs
                    .expect("should have output scope");
                for decl in section.declarations() {
                    add_decl_node(
                        document,
                        &mut graph,
                        &mut names,
                        scope,
                        Decl::Bound(decl),
                        diagnostics,
                    );
                }
            }
            TaskItem::Declaration(decl) => {
                add_decl_node(
                    document,
                    &mut graph,
                    &mut names,
                    document.tasks[task_index].scope,
                    Decl::Bound(decl),
                    diagnostics,
                );
            }
            TaskItem::Command(section) if command.is_none() => {
                command = Some(
                    graph.add_node(GraphNode::Command {
                        scope: document.tasks[task_index]
                            .command
                            .expect("should have command scope"),
                        section,
                    }),
                );
            }
            TaskItem::Runtime(section) if !saw_runtime => {
                saw_runtime = true;
                graph.add_node(GraphNode::Runtime {
                    scope: document.tasks[task_index].scope,
                    section,
                });
            }
            TaskItem::Requirements(section) if !saw_requirements => {
                saw_requirements = true;
                graph.add_node(GraphNode::Requirements {
                    scope: document.tasks[task_index].scope,
                    section,
                });
            }
            TaskItem::Hints(section) if !saw_hints => {
                saw_hints = true;
                graph.add_node(GraphNode::Hints {
                    scope: document.tasks[task_index].scope,
                    section,
                });
            }
            _ => continue,
        }
    }

    add_reference_edges(document, &mut graph, &names, diagnostics);

    // Type check the nodes
    for index in graph.node_indices() {
        match &graph[index] {
            GraphNode::Decl {
                scope,
                expected,
                expr,
                span,
            } => {
                if let Some(expr) = expr {
                    let scopes = &document.scopes;
                    let mut evaluator = ExprTypeEvaluator::new(
                        document.version.expect("document should be a 1.x version"),
                        &mut document.types,
                        diagnostics,
                        |name, span| lookup_type(&document.structs, name, span),
                    );
                    let actual = evaluator
                        .evaluate_expr(&ScopeRef::new(scopes, *scope), expr)
                        .unwrap_or(Type::Union);

                    if *expected != Type::Union
                        && !actual.is_coercible_to(&document.types, expected)
                    {
                        diagnostics.push(type_mismatch(
                            &document.types,
                            *expected,
                            *span,
                            actual,
                            expr.span(),
                        ));
                    }
                }
            }
            GraphNode::Command { scope, section } => {
                let mut evaluator = ExprTypeEvaluator::new(
                    document.version.expect("document should be a 1.x version"),
                    &mut document.types,
                    diagnostics,
                    |name, span| lookup_type(&document.structs, name, span),
                );

                // Check any placeholder expression
                for part in section.parts() {
                    if let CommandPart::Placeholder(p) = part {
                        evaluator.check_placeholder(&ScopeRef::new(&document.scopes, *scope), &p);
                    }
                }
            }
            GraphNode::Runtime { scope, section } => {
                let mut evaluator = ExprTypeEvaluator::new(
                    document.version.expect("document should be a 1.x version"),
                    &mut document.types,
                    diagnostics,
                    |name, span| lookup_type(&document.structs, name, span),
                );

                let scope = ScopeRef::new(&document.scopes, *scope);
                for item in section.items() {
                    evaluator.evaluate_runtime_item(&scope, &item.name(), &item.expr());
                }
            }
            GraphNode::Requirements { scope, section } => {
                let mut evaluator = ExprTypeEvaluator::new(
                    document.version.expect("document should be a 1.x version"),
                    &mut document.types,
                    diagnostics,
                    |name, span| lookup_type(&document.structs, name, span),
                );

                let scope = ScopeRef::new(&document.scopes, *scope);
                for item in section.items() {
                    evaluator.evaluate_requirements_item(&scope, &item.name(), &item.expr());
                }
            }
            GraphNode::Hints { scope, section } => {
                let mut evaluator = ExprTypeEvaluator::new(
                    document.version.expect("document should be a 1.x version"),
                    &mut document.types,
                    diagnostics,
                    |name, span| lookup_type(&document.structs, name, span),
                );

                // Create a special scope for evaluating the hints section which allows for the
                // `hints`, `input`, and `output` hidden types
                let scope = ScopeRef {
                    scopes: &document.scopes,
                    scope: *scope,
                    inputs: Some(*scope),
                    outputs: Some(
                        document.tasks[task_index]
                            .outputs
                            .map(TaskOutputScope::Present)
                            .unwrap_or(TaskOutputScope::NotPresent),
                    ),
                    hints: true,
                };

                for item in section.items() {
                    evaluator.evaluate_hints_item(&scope, &item.name(), &item.expr())
                }
            }
        }
    }
}
