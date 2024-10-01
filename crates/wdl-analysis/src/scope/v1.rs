//! Conversion of a V1 AST to a document scope.
use std::collections::HashMap;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

use indexmap::IndexMap;
use petgraph::algo::has_path_connecting;
use petgraph::algo::toposort;
use petgraph::graph::NodeIndex;
use petgraph::prelude::DiGraphMap;
use url::Url;
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
use wdl_ast::v1::Ast;
use wdl_ast::v1::CallStatement;
use wdl_ast::v1::CommandPart;
use wdl_ast::v1::ConditionalStatement;
use wdl_ast::v1::Decl;
use wdl_ast::v1::DocumentItem;
use wdl_ast::v1::Expr;
use wdl_ast::v1::ImportStatement;
use wdl_ast::v1::MetadataValue;
use wdl_ast::v1::ScatterStatement;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::v1::WorkflowHintsItemValue;
use wdl_ast::version::V1;

use super::DocumentScope;
use super::Namespace;
use super::Scope;
use super::ScopeIndex;
use super::Struct;
use super::TASK_VAR_NAME;
use super::Task;
use super::Workflow;
use super::braced_scope_span;
use super::heredoc_scope_span;
use crate::diagnostics::Context;
use crate::diagnostics::call_input_type_mismatch;
use crate::diagnostics::duplicate_workflow;
use crate::diagnostics::if_conditional_mismatch;
use crate::diagnostics::import_cycle;
use crate::diagnostics::import_failure;
use crate::diagnostics::import_missing_version;
use crate::diagnostics::imported_struct_conflict;
use crate::diagnostics::incompatible_import;
use crate::diagnostics::invalid_relative_import;
use crate::diagnostics::missing_call_input;
use crate::diagnostics::name_conflict;
use crate::diagnostics::namespace_conflict;
use crate::diagnostics::only_one_namespace;
use crate::diagnostics::recursive_struct;
use crate::diagnostics::recursive_workflow_call;
use crate::diagnostics::struct_conflicts_with_import;
use crate::diagnostics::struct_not_in_scope;
use crate::diagnostics::type_is_not_array;
use crate::diagnostics::type_mismatch;
use crate::diagnostics::unknown_io_name;
use crate::diagnostics::unknown_namespace;
use crate::diagnostics::unknown_task_or_workflow;
use crate::diagnostics::unknown_type;
use crate::eval::v1::TaskGraph;
use crate::eval::v1::TaskGraphNode;
use crate::eval::v1::WorkflowGraph;
use crate::eval::v1::WorkflowGraphNode;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::scope::ScopeRef;
use crate::types::ArrayType;
use crate::types::CallOutputType;
use crate::types::Coercible;
use crate::types::CompoundTypeDef;
use crate::types::Optional;
use crate::types::PrimitiveTypeKind;
use crate::types::Type;
use crate::types::Types;
use crate::types::v1::AstTypeConverter;
use crate::types::v1::ExprTypeEvaluator;

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

    assert!(
        document.version.is_some(),
        "expected a supported V1 version"
    );

    // First start by processing imports and struct definitions
    // This needs to be performed before processing tasks and workflows as
    // declarations might reference an imported or locally-defined struct
    for item in ast.items() {
        match item {
            DocumentItem::Import(import) => {
                add_namespace(&mut document, graph, &import, index, version, diagnostics);
            }
            DocumentItem::Struct(s) => {
                add_struct(&mut document, &s, diagnostics);
            }
            DocumentItem::Task(_) | DocumentItem::Workflow(_) => {
                continue;
            }
        }
    }

    // Populate the struct types now that all structs have been processed
    set_struct_types(&mut document, diagnostics);

    // Now process the tasks and workflows
    let mut definition = None;
    for item in ast.items() {
        match item {
            DocumentItem::Task(task) => {
                add_task(&mut document, &task, diagnostics);
            }
            DocumentItem::Workflow(workflow) => {
                // Note that this doesn't populate the workflow scope; we delay that until after
                // we've seen every task in the document
                if add_workflow(&mut document, &workflow, diagnostics) {
                    definition = Some(workflow.clone());
                }
            }
            DocumentItem::Import(_) | DocumentItem::Struct(_) => {
                continue;
            }
        }
    }

    if let Some(definition) = definition {
        populate_workflow_scope(&mut document, &definition, diagnostics);
    }

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
                document.namespaces.insert(ns.clone(), Namespace {
                    span,
                    source: uri.clone(),
                    scope: imported.clone(),
                });
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
                document.structs.insert(aliased_name.to_string(), Struct {
                    span,
                    offset: s.offset,
                    node: s.node.clone(),
                    namespace: Some(ns.clone()),
                    ty: s
                        .ty
                        .map(|ty| document.types.import(&namespace.scope.types, ty)),
                });
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

    document.structs.insert(name.as_str().to_string(), Struct {
        span: name.span(),
        namespace: None,
        offset: definition.span().start(),
        node: definition.syntax().green().into(),
        ty: None,
    });
}

/// Converts an AST type to an analysis type.
fn convert_ast_type(
    document: &mut DocumentScope,
    ty: &wdl_ast::v1::Type,
    diagnostics: &mut Vec<Diagnostic>,
) -> Type {
    let mut converter = AstTypeConverter::new(&mut document.types, |name, span| {
        lookup_type(&document.structs, name, span)
    });

    match converter.convert_type(ty) {
        Ok(ty) => ty,
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            Type::Union
        }
    }
}

/// Creates an input type map.
fn create_input_type_map(
    document: &mut DocumentScope,
    declarations: impl Iterator<Item = Decl>,
    diagnostics: &mut Vec<Diagnostic>,
) -> HashMap<String, (Type, bool)> {
    let mut map = HashMap::new();
    for decl in declarations {
        let name = decl.name();
        if map.contains_key(name.as_str()) {
            // Ignore the duplicate
            continue;
        }

        let ty = convert_ast_type(document, &decl.ty(), diagnostics);
        map.insert(
            name.as_str().to_string(),
            (ty, decl.expr().is_none() && !ty.is_optional()),
        );
    }

    map
}

/// Creates an output type map.
fn create_output_type_map(
    document: &mut DocumentScope,
    declarations: impl Iterator<Item = Decl>,
    diagnostics: &mut Vec<Diagnostic>,
) -> HashMap<String, Type> {
    let mut map = HashMap::new();
    for decl in declarations {
        let name = decl.name();
        if map.contains_key(name.as_str()) {
            // Ignore the duplicate
            continue;
        }

        let ty = convert_ast_type(document, &decl.ty(), diagnostics);
        map.insert(name.as_str().to_string(), ty);
    }

    map
}

/// Adds a task to the document's scope.
fn add_task(
    document: &mut DocumentScope,
    task: &TaskDefinition,
    diagnostics: &mut Vec<Diagnostic>,
) {
    /// Helper function for creating a scope for a task section.
    fn create_section_scope(
        document: &mut DocumentScope,
        task_name: &Ident,
        parent: ScopeIndex,
        span: Span,
    ) -> ScopeIndex {
        let scope = document.add_scope(Scope::new(Some(parent), span));

        // Command and output sections in 1.2 have access to the `task` variable
        if document.version >= Some(SupportedVersion::V1(V1::Two)) {
            document
                .scope_mut(scope)
                .insert(TASK_VAR_NAME, task_name.span(), Type::Task);
        }

        scope
    }

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

    // Populate type maps for the task's inputs and outputs
    let inputs = create_input_type_map(
        document,
        task.input().into_iter().flat_map(|s| s.declarations()),
        diagnostics,
    );
    let outputs = create_output_type_map(
        document,
        task.output()
            .into_iter()
            .flat_map(|s| s.declarations().map(Decl::Bound)),
        diagnostics,
    );

    // Process the task in evaluation order
    let graph = TaskGraph::new(document.version.unwrap(), task, diagnostics);
    let scope = document.add_scope(Scope::new(None, braced_scope_span(task)));
    let mut output_scope = None;
    let mut command_scope = None;

    for node in graph.toposort() {
        match node {
            TaskGraphNode::Input(decl) => {
                add_decl(
                    document,
                    scope,
                    &decl,
                    |_, n, _, _| inputs[n].0,
                    diagnostics,
                );
            }
            TaskGraphNode::Decl(decl) => {
                add_decl(
                    document,
                    scope,
                    &decl,
                    |doc, _, decl, diag| convert_ast_type(doc, &decl.ty(), diag),
                    diagnostics,
                );
            }
            TaskGraphNode::Output(decl) => {
                let scope = *output_scope.get_or_insert_with(|| {
                    create_section_scope(
                        document,
                        &name,
                        scope,
                        braced_scope_span(&task.output().expect("should have output section")),
                    )
                });
                add_decl(document, scope, &decl, |_, n, _, _| outputs[n], diagnostics);
            }
            TaskGraphNode::Command(section) => {
                let scope = *command_scope.get_or_insert_with(|| {
                    let span = if section.is_heredoc() {
                        heredoc_scope_span(&section)
                    } else {
                        braced_scope_span(&section)
                    };

                    create_section_scope(document, &name, scope, span)
                });

                // Perform type checking on the command section's placeholders
                let mut evaluator = ExprTypeEvaluator::new(
                    document.version.unwrap(),
                    &mut document.types,
                    diagnostics,
                    |name, span| lookup_type(&document.structs, name, span),
                );

                for part in section.parts() {
                    if let CommandPart::Placeholder(p) = part {
                        evaluator.check_placeholder(&ScopeRef::new(&document.scopes, scope), &p);
                    }
                }
            }
            TaskGraphNode::Runtime(section) => {
                // Perform type checking on the runtime section's expressions
                let mut evaluator = ExprTypeEvaluator::new(
                    document.version.unwrap(),
                    &mut document.types,
                    diagnostics,
                    |name, span| lookup_type(&document.structs, name, span),
                );

                let scope = ScopeRef::new(&document.scopes, scope);
                for item in section.items() {
                    evaluator.evaluate_runtime_item(&scope, &item.name(), &item.expr());
                }
            }
            TaskGraphNode::Requirements(section) => {
                // Perform type checking on the requirements section's expressions
                let mut evaluator = ExprTypeEvaluator::new(
                    document.version.unwrap(),
                    &mut document.types,
                    diagnostics,
                    |name, span| lookup_type(&document.structs, name, span),
                );

                let scope = ScopeRef::new(&document.scopes, scope);
                for item in section.items() {
                    evaluator.evaluate_requirements_item(&scope, &item.name(), &item.expr());
                }
            }
            TaskGraphNode::Hints(section) => {
                // Perform type checking on the hints section's expressions
                let mut evaluator = ExprTypeEvaluator::new(
                    document.version.unwrap(),
                    &mut document.types,
                    diagnostics,
                    |name, span| lookup_type(&document.structs, name, span),
                );

                // Create a special scope for evaluating the hints section which allows for the
                // `hints`, `input`, and `output` hidden types
                let scope = ScopeRef {
                    scopes: &document.scopes,
                    scope,
                    task_name: Some(name.as_str()),
                    inputs: Some(&inputs),
                    outputs: Some(&outputs),
                };

                for item in section.items() {
                    evaluator.evaluate_hints_item(&scope, &item.name(), &item.expr())
                }
            }
        }
    }

    document.tasks.insert(name.as_str().to_string(), Task {
        name_span: name.span(),
        scope,
        inputs,
        outputs,
    });
}

/// Adds a declaration to a scope.
fn add_decl(
    document: &mut DocumentScope,
    scope: ScopeIndex,
    decl: &Decl,
    ty: impl FnOnce(&mut DocumentScope, &str, &Decl, &mut Vec<Diagnostic>) -> Type,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (name, expr) = (decl.name(), decl.expr());
    if document.scope(scope).lookup(name.as_str()).is_some() {
        // The declaration is conflicting; don't add to the scope
        return;
    }

    let ty = ty(document, name.as_str(), decl, diagnostics);

    document
        .scope_mut(scope)
        .insert(name.as_str(), name.span(), ty);

    if let Some(expr) = expr {
        type_check_expr(document, scope, &expr, ty, name.span(), diagnostics);
    }
}

/// Adds a workflow to the document scope.
///
/// Returns `true` if the workflow was added to the document or `false` if not
/// (i.e. there was a conflict).
fn add_workflow(
    document: &mut DocumentScope,
    workflow: &WorkflowDefinition,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    // Check for conflicts with task names or an existing workspace
    let name = workflow.name();
    if let Some(s) = document.tasks.get(name.as_str()) {
        diagnostics.push(name_conflict(
            name.as_str(),
            Context::Workflow(name.span()),
            Context::Task(s.name_span),
        ));
        return false;
    } else if let Some(s) = &document.workflow {
        diagnostics.push(duplicate_workflow(&name, s.name_span));
        return false;
    }

    // Note: we delay actually populating the workflow scope until later on so that
    // we can populate all tasks in the document first.

    document.workflow = Some(Workflow {
        name_span: name.span(),
        name: name.as_str().to_string(),
        scope: document.add_scope(Scope::new(None, braced_scope_span(workflow))),
        inputs: Default::default(),
        outputs: Default::default(),
    });

    true
}

/// Determines if nested inputs are allowed for a workflow.
fn is_nested_inputs_allowed(document: &DocumentScope, definition: &WorkflowDefinition) -> bool {
    match document.version() {
        Some(SupportedVersion::V1(V1::Zero)) => return true,
        Some(SupportedVersion::V1(V1::One)) => {
            // Fall through to below
        }
        Some(SupportedVersion::V1(V1::Two)) => {
            // Check the hints section
            let allow = definition.hints().and_then(|s| {
                s.items().find_map(|i| {
                    if matches!(
                        i.name().as_str(),
                        "allow_nested_inputs" | "allowNestedInputs"
                    ) {
                        match i.value() {
                            WorkflowHintsItemValue::Boolean(v) => Some(v.value()),
                            _ => Some(false),
                        }
                    } else {
                        None
                    }
                })
            });

            if let Some(allow) = allow {
                return allow;
            }

            // Fall through to below
        }
        _ => return false,
    }

    // Check the metadata section
    definition
        .metadata()
        .and_then(|s| {
            s.items().find_map(|i| {
                if i.name().as_str() == "allowNestedInputs" {
                    match i.value() {
                        MetadataValue::Boolean(v) => Some(v.value()),
                        _ => Some(false),
                    }
                } else {
                    None
                }
            })
        })
        .unwrap_or(false)
}

/// Finishes processing a workflow by populating its scope.
fn populate_workflow_scope(
    document: &mut DocumentScope,
    definition: &WorkflowDefinition,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Populate type maps for the workflow's inputs and outputs
    let inputs = create_input_type_map(
        document,
        definition
            .input()
            .into_iter()
            .flat_map(|s| s.declarations()),
        diagnostics,
    );
    let outputs = create_output_type_map(
        document,
        definition
            .output()
            .into_iter()
            .flat_map(|s| s.declarations().map(Decl::Bound)),
        diagnostics,
    );

    let nested_inputs_allowed = is_nested_inputs_allowed(document, definition);

    // Keep a map of scopes from syntax node that introduced the scope to the scope
    // index
    let mut scopes = HashMap::new();
    let workflow_scope = document
        .workflow
        .as_ref()
        .map(|w| w.scope)
        .expect("should have a workflow");
    let mut output_scope = None;
    let graph = WorkflowGraph::new(definition, diagnostics);
    for node in graph.toposort() {
        match node {
            WorkflowGraphNode::Input(decl) => {
                add_decl(
                    document,
                    workflow_scope,
                    &decl,
                    |_, n, _, _| inputs[n].0,
                    diagnostics,
                );
            }
            WorkflowGraphNode::Decl(decl) => {
                let scope = scopes
                    .get(&decl.syntax().parent().expect("should have parent"))
                    .copied()
                    .unwrap_or(workflow_scope);
                add_decl(
                    document,
                    scope,
                    &decl,
                    |doc, _, decl, diag| convert_ast_type(doc, &decl.ty(), diag),
                    diagnostics,
                );
            }
            WorkflowGraphNode::Output(decl) => {
                let scope = *output_scope.get_or_insert_with(|| {
                    document.add_scope(Scope::new(
                        Some(workflow_scope),
                        braced_scope_span(
                            &definition.output().expect("should have output section"),
                        ),
                    ))
                });
                add_decl(document, scope, &decl, |_, n, _, _| outputs[n], diagnostics);
            }
            WorkflowGraphNode::Conditional(statement) => {
                let parent = scopes
                    .get(&statement.syntax().parent().expect("should have parent"))
                    .copied()
                    .unwrap_or(workflow_scope);
                add_conditional_statement(document, parent, &mut scopes, &statement, diagnostics);
            }
            WorkflowGraphNode::Scatter(statement) => {
                let parent = scopes
                    .get(&statement.syntax().parent().expect("should have parent"))
                    .copied()
                    .unwrap_or(workflow_scope);
                add_scatter_statement(document, parent, &mut scopes, &statement, diagnostics);
            }
            WorkflowGraphNode::Call(statement) => {
                let scope = scopes
                    .get(&statement.syntax().parent().expect("should have parent"))
                    .copied()
                    .unwrap_or(workflow_scope);
                add_call_statement(
                    document,
                    definition.name().as_str(),
                    scope,
                    &statement,
                    nested_inputs_allowed,
                    diagnostics,
                );
            }
            WorkflowGraphNode::ExitConditional(statement) => {
                let scope = scopes
                    .get(statement.syntax())
                    .copied()
                    .expect("should have scope");
                promote_scope(document, scope, None, promote_optional_type);
            }
            WorkflowGraphNode::ExitScatter(statement) => {
                let scope = scopes
                    .get(statement.syntax())
                    .copied()
                    .expect("should have scope");
                let variable = statement.variable();
                promote_scope(document, scope, Some(variable.as_str()), promote_array_type);
            }
        }
    }

    let workflow = document.workflow.as_mut().expect("expected a workflow");
    workflow.inputs = inputs;
    workflow.outputs = outputs;
}

/// Adds a conditional statement to the current scope.
fn add_conditional_statement(
    document: &mut DocumentScope,
    parent: ScopeIndex,
    scopes: &mut HashMap<SyntaxNode, ScopeIndex>,
    statement: &ConditionalStatement,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let scope = document.add_scope(Scope::new(Some(parent), braced_scope_span(statement)));
    scopes.insert(statement.syntax().clone(), scope);

    let mut evaluator = ExprTypeEvaluator::new(
        document.version.unwrap(),
        &mut document.types,
        diagnostics,
        |name, span| lookup_type(&document.structs, name, span),
    );

    // Evaluate the statement's expression; it is expected to be a boolean
    let expr = statement.expr();
    let ty = evaluator
        .evaluate_expr(&ScopeRef::new(&document.scopes, scope), &expr)
        .unwrap_or(Type::Union);

    if !ty.is_coercible_to(&document.types, &PrimitiveTypeKind::Boolean.into()) {
        diagnostics.push(if_conditional_mismatch(&document.types, ty, expr.span()));
    }
}

/// Adds a scatter statement to the current scope.
fn add_scatter_statement(
    document: &mut DocumentScope,
    parent: ScopeIndex,
    scopes: &mut HashMap<SyntaxNode, ScopeIndex>,
    statement: &ScatterStatement,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let scope = document.add_scope(Scope::new(Some(parent), braced_scope_span(statement)));
    scopes.insert(statement.syntax().clone(), scope);

    let mut evaluator = ExprTypeEvaluator::new(
        document.version.unwrap(),
        &mut document.types,
        diagnostics,
        |name, span| lookup_type(&document.structs, name, span),
    );

    // Evaluate the statement expression; it is expected to be an array
    let expr = statement.expr();
    let ty = evaluator
        .evaluate_expr(&ScopeRef::new(&document.scopes, scope), &expr)
        .unwrap_or(Type::Union);
    let element_ty = match ty {
        Type::Compound(compound_ty) => {
            match document.types.type_definition(compound_ty.definition()) {
                CompoundTypeDef::Array(ty) => ty.element_type(),
                _ => {
                    diagnostics.push(type_is_not_array(&document.types, ty, expr.span()));
                    Type::Union
                }
            }
        }
        Type::Union => Type::Union,
        _ => {
            diagnostics.push(type_is_not_array(&document.types, ty, expr.span()));
            Type::Union
        }
    };

    // Introduce the scatter variable into the scope
    let variable = statement.variable();
    document
        .scope_mut(scope)
        .insert(variable.as_str().to_string(), variable.span(), element_ty);
}

/// Adds a call statement to the current scope.
fn add_call_statement(
    document: &mut DocumentScope,
    workflow_name: &str,
    scope: ScopeIndex,
    statement: &CallStatement,
    nested_inputs_allowed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let target_name = statement
        .target()
        .names()
        .last()
        .expect("expected a last call target name");

    let name = statement
        .alias()
        .map(|a| a.name())
        .unwrap_or_else(|| target_name.clone());

    let ty = match resolve_call_target(document, workflow_name, statement, diagnostics) {
        Some(target) => {
            // Type check the call inputs
            let mut seen = HashSet::new();
            for input in statement.inputs() {
                let input_name = input.name();

                let expected_ty = target
                    .inputs
                    .get(input_name.as_str())
                    .copied()
                    .map(|(ty, _)| ty)
                    .unwrap_or_else(|| {
                        diagnostics.push(unknown_io_name(
                            name.as_str(),
                            &input_name,
                            target.workflow,
                            true,
                        ));
                        Type::Union
                    });

                match input.expr() {
                    Some(expr) => {
                        type_check_expr(
                            document,
                            scope,
                            &expr,
                            expected_ty,
                            input_name.span(),
                            diagnostics,
                        );
                    }
                    None => {
                        if let Some((_, actual_ty)) =
                            document.scope(scope).lookup(input_name.as_str())
                        {
                            if expected_ty != Type::Union
                                && !actual_ty.is_coercible_to(&document.types, &expected_ty)
                            {
                                diagnostics.push(call_input_type_mismatch(
                                    &document.types,
                                    &input_name,
                                    expected_ty,
                                    actual_ty,
                                ));
                            }
                        }
                    }
                }

                // Don't bother keeping track of seen inputs if nested inputs are allowed
                if !nested_inputs_allowed {
                    seen.insert(TokenStrHash::new(input_name));
                }
            }

            if !nested_inputs_allowed {
                for (input, (_, required)) in &target.inputs {
                    if *required && !seen.contains(input.as_str()) {
                        diagnostics.push(missing_call_input(target.workflow, &target_name, input));
                    }
                }
            }

            document.types.add_call_output(CallOutputType::new(
                name.as_str(),
                target.outputs,
                target.workflow,
            ))
        }
        None => Type::Union,
    };

    // Don't add if there's a conflict
    if document.scope(scope).lookup(name.as_str()).is_some() {
        return;
    }

    document
        .scope_mut(scope)
        .insert(name.as_str(), name.span(), ty);
}

/// Represents information about a call target.
struct CallTarget {
    /// Whether or not the target is a workflow.
    workflow: bool,
    /// The inputs of the call target.
    ///
    /// The value is the pair of the input type and whether or not the input is
    /// required.
    inputs: HashMap<String, (Type, bool)>,
    /// The outputs of the call target.
    ///
    /// The value is the output type.
    outputs: HashMap<String, Type>,
}

/// Resolves the target of a call statement.
///
/// Returns `None` if the call target could not be resolved.
fn resolve_call_target(
    document: &mut DocumentScope,
    workflow_name: &str,
    statement: &CallStatement,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<CallTarget> {
    let mut targets = statement.target().names().peekable();
    let mut namespace = None;
    let mut name = None;
    while let Some(target) = targets.next() {
        if targets.peek().is_none() {
            name = Some(target);
            break;
        }

        if namespace.is_some() {
            diagnostics.push(only_one_namespace(target.span()));
            return None;
        }

        match document.namespaces.get(target.as_str()) {
            Some(ns) => namespace = Some(ns),
            None => {
                diagnostics.push(unknown_namespace(&target));
                return None;
            }
        }
    }

    let target = namespace.map(|ns| ns.scope.as_ref()).unwrap_or(document);
    let name = name.expect("should have name");

    if namespace.is_none() && name.as_str() == workflow_name {
        diagnostics.push(recursive_workflow_call(&name));
        return None;
    }

    let (workflow, mut inputs, mut outputs) = if let Some(task) = target.tasks.get(name.as_str()) {
        (false, task.inputs.clone(), task.outputs.clone())
    } else {
        match &target.workflow {
            Some(workflow) if workflow.name == name.as_str() => {
                (true, workflow.inputs.clone(), workflow.outputs.clone())
            }
            _ => {
                diagnostics.push(unknown_task_or_workflow(namespace.map(|ns| ns.span), &name));
                return None;
            }
        }
    };

    // If the target is from an import, we need to import its type definitions into
    // the current document scope
    if let Some(types) = namespace.map(|ns| &ns.scope.types) {
        for (ty, _) in inputs.values_mut() {
            *ty = document.types.import(types, *ty);
        }

        for ty in outputs.values_mut() {
            *ty = document.types.import(types, *ty);
        }
    }

    Some(CallTarget {
        workflow,
        inputs,
        outputs,
    })
}

/// Promotes the names in the current to the parent scope.
fn promote_scope<F>(
    document: &mut DocumentScope,
    scope: ScopeIndex,
    skip: Option<&str>,
    transform: F,
) where
    F: Fn(&mut Types, Type) -> Type,
{
    // We need to split the scopes as we want to read from one part of the slice and
    // write to another; the left side will contain the parent at it's index and the
    // right side will contain the child scope at it's index minus the parent's
    let parent = document.scopes[scope.0]
        .parent
        .expect("should have a parent scope");
    assert!(scope.0 > parent.0);
    let (left, right) = document.scopes.split_at_mut(parent.0 + 1);
    let scope = &right[scope.0 - parent.0 - 1];
    let parent = &mut left[parent.0];
    for (name, (span, ty)) in scope.names.iter() {
        if Some(name.as_str()) == skip {
            continue;
        }

        parent
            .names
            .entry(name.clone())
            .or_insert_with(|| (*span, transform(&mut document.types, *ty)));
    }
}

/// Promotes a type to an array type for scatter statements.
fn promote_array_type(types: &mut Types, ty: Type) -> Type {
    match ty {
        Type::Compound(ty) => match types.type_definition(ty.definition()) {
            CompoundTypeDef::CallOutput(ty) => {
                let mut ty = ty.clone();
                for ty in ty.outputs_mut().values_mut() {
                    *ty = types.add_array(ArrayType::new(*ty))
                }

                types.add_call_output(ty)
            }
            _ => types.add_array(ArrayType::new(Type::Compound(ty))),
        },
        _ => types.add_array(ArrayType::new(ty)),
    }
}

/// Promotes a type to an optional type for conditional statements.
fn promote_optional_type(types: &mut Types, ty: Type) -> Type {
    match ty {
        Type::Compound(ty) => match types.type_definition(ty.definition()) {
            CompoundTypeDef::CallOutput(ty) => {
                let mut ty = ty.clone();
                for ty in ty.outputs_mut().values_mut() {
                    *ty = ty.optional();
                }

                types.add_call_output(ty)
            }
            _ => Type::Compound(ty.optional()),
        },
        _ => ty.optional(),
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

/// Sets the struct types in the document.
fn set_struct_types(document: &mut DocumentScope, diagnostics: &mut Vec<Diagnostic>) {
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

/// Performs a type check of an expression.
fn type_check_expr(
    document: &mut DocumentScope,
    scope: ScopeIndex,
    expr: &Expr,
    expected: Type,
    expected_span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut evaluator = ExprTypeEvaluator::new(
        document.version.unwrap(),
        &mut document.types,
        diagnostics,
        |name, span| lookup_type(&document.structs, name, span),
    );

    let actual = evaluator
        .evaluate_expr(&ScopeRef::new(&document.scopes, scope), expr)
        .unwrap_or(Type::Union);

    if expected != Type::Union && !actual.is_coercible_to(&document.types, &expected) {
        diagnostics.push(type_mismatch(
            &document.types,
            expected,
            expected_span,
            actual,
            expr.span(),
        ));
    }
}
