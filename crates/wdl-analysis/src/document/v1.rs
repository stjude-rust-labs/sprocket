//! Conversion of a V1 AST to an analyzed document.
use std::collections::HashMap;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

use indexmap::IndexMap;
use petgraph::Direction;
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
use wdl_ast::SyntaxNodeExt;
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
use wdl_ast::v1::ScatterStatement;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::version::V1;

use super::Document;
use super::Input;
use super::Namespace;
use super::Output;
use super::Scope;
use super::ScopeIndex;
use super::ScopeRefMut;
use super::Struct;
use super::TASK_VAR_NAME;
use super::Task;
use super::Workflow;
use super::braced_scope_span;
use super::heredoc_scope_span;
use crate::DiagnosticsConfig;
use crate::UNUSED_CALL_RULE_ID;
use crate::UNUSED_DECL_RULE_ID;
use crate::UNUSED_IMPORT_RULE_ID;
use crate::UNUSED_INPUT_RULE_ID;
use crate::diagnostics::Context;
use crate::diagnostics::Io;
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
use crate::diagnostics::non_empty_array_assignment;
use crate::diagnostics::only_one_namespace;
use crate::diagnostics::recursive_struct;
use crate::diagnostics::recursive_workflow_call;
use crate::diagnostics::struct_conflicts_with_import;
use crate::diagnostics::struct_not_in_document;
use crate::diagnostics::type_is_not_array;
use crate::diagnostics::type_mismatch;
use crate::diagnostics::unknown_call_io;
use crate::diagnostics::unknown_namespace;
use crate::diagnostics::unknown_task_or_workflow;
use crate::diagnostics::unknown_type;
use crate::diagnostics::unused_call;
use crate::diagnostics::unused_declaration;
use crate::diagnostics::unused_input;
use crate::document::Name;
use crate::document::ScopeRef;
use crate::eval::v1::TaskGraphBuilder;
use crate::eval::v1::TaskGraphNode;
use crate::eval::v1::WorkflowGraphBuilder;
use crate::eval::v1::WorkflowGraphNode;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::types::CallKind;
use crate::types::CallType;
use crate::types::Coercible;
use crate::types::CompoundTypeDef;
use crate::types::Optional;
use crate::types::PrimitiveTypeKind;
use crate::types::PromotionKind;
use crate::types::Type;
use crate::types::Types;
use crate::types::v1::AstTypeConverter;
use crate::types::v1::ExprTypeEvaluator;

/// Determines if an input is used based off a name heuristic.
///
/// To localize related files, WDL tasks typically use additional `File` or
/// `Array[File]` inputs which aren't referenced in the task itself.
///
/// As such, we don't want to generate an "unused input" warning for these
/// inputs.
///
/// It is expected that the name of the input is suffixed with a particular
/// string (this list is based on the heuristic applied by `miniwdl`):
///
/// * index
/// * indexes
/// * indices
/// * idx
/// * tbi
/// * bai
/// * crai
/// * csi
/// * fai
/// * dict
fn is_input_used(document: &Document, name: &str, ty: Type) -> bool {
    /// The suffixes that cause the input to be "used"
    const SUFFIXES: &[&str] = &[
        "index", "indexes", "indices", "idx", "tbi", "bai", "crai", "csi", "fai", "dict",
    ];

    // Determine if the input is `File` or `Array[File]`
    match ty {
        Type::Primitive(ty) if ty.kind() == PrimitiveTypeKind::File => {}
        Type::Compound(ty) => match document.types.type_definition(ty.definition()) {
            CompoundTypeDef::Array(ty) => match ty.element_type() {
                Type::Primitive(ty) if ty.kind() == PrimitiveTypeKind::File => {}
                _ => return false,
            },
            _ => return false,
        },
        _ => return false,
    }

    let name = name.to_lowercase();
    for suffix in SUFFIXES {
        if name.ends_with(suffix) {
            return true;
        }
    }

    false
}

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

/// Creates a new document for a V1 AST.
pub(crate) fn create_document(
    config: DiagnosticsConfig,
    graph: &DocumentGraph,
    index: NodeIndex,
    ast: &Ast,
    version: &Version,
    diagnostics: &mut Vec<Diagnostic>,
) -> Document {
    let mut document = Document {
        root: Some(ast.syntax().green().into_owned()),
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
    let mut workflow = None;
    for item in ast.items() {
        match item {
            DocumentItem::Task(task) => {
                add_task(config, &mut document, &task, diagnostics);
            }
            DocumentItem::Workflow(w) => {
                // Note that this doesn't populate the workflow; we delay that until after
                // we've seen every task in the document so that we can resolve call targets
                if add_workflow(&mut document, &w, diagnostics) {
                    workflow = Some(w.clone());
                }
            }
            DocumentItem::Import(_) | DocumentItem::Struct(_) => {
                continue;
            }
        }
    }

    if let Some(workflow) = workflow {
        populate_workflow(config, &mut document, &workflow, diagnostics);
    }

    document
}

/// Adds a namespace to the document.
fn add_namespace(
    document: &mut Document,
    graph: &DocumentGraph,
    import: &ImportStatement,
    importer_index: NodeIndex,
    importer_version: &Version,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Start by resolving the import to its document
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
                    document: imported.clone(),
                    used: false,
                    excepted: import.syntax().is_rule_excepted(UNUSED_IMPORT_RULE_ID),
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
                diagnostics.push(struct_not_in_document(&from));
                return None;
            }

            Some((from.as_str().to_string(), to))
        })
        .collect::<HashMap<_, _>>();

    // Insert the imported document's struct definitions
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
                        .map(|ty| document.types.import(&namespace.document.types, ty)),
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

/// Adds a struct to the document.
fn add_struct(
    document: &mut Document,
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
    document: &mut Document,
    ty: &wdl_ast::v1::Type,
    diagnostics: &mut Vec<Diagnostic>,
) -> Type {
    let mut converter = AstTypeConverter::new(&mut document.types, |name, span| {
        document
            .structs
            .get(name)
            .map(|s| {
                // Mark the struct's namespace as used
                if let Some(ns) = &s.namespace {
                    document.namespaces[ns].used = true;
                }

                s.ty().expect("struct should have type")
            })
            .ok_or_else(|| unknown_type(name, span))
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
    document: &mut Document,
    declarations: impl Iterator<Item = Decl>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Arc<HashMap<String, Input>> {
    let mut map = HashMap::new();
    for decl in declarations {
        let name = decl.name();
        if map.contains_key(name.as_str()) {
            // Ignore the duplicate
            continue;
        }

        let ty = convert_ast_type(document, &decl.ty(), diagnostics);
        map.insert(name.as_str().to_string(), Input {
            ty,
            required: decl.expr().is_none() && !ty.is_optional(),
        });
    }

    map.into()
}

/// Creates an output type map.
fn create_output_type_map(
    document: &mut Document,
    declarations: impl Iterator<Item = Decl>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Arc<HashMap<String, Output>> {
    let mut map = HashMap::new();
    for decl in declarations {
        let name = decl.name();
        if map.contains_key(name.as_str()) {
            // Ignore the duplicate
            continue;
        }

        let ty = convert_ast_type(document, &decl.ty(), diagnostics);
        map.insert(name.as_str().to_string(), Output { ty });
    }

    map.into()
}

/// Adds a task to the document.
fn add_task(
    config: DiagnosticsConfig,
    document: &mut Document,
    task: &TaskDefinition,
    diagnostics: &mut Vec<Diagnostic>,
) {
    /// Helper function for creating a scope for a task section.
    fn create_section_scope(
        version: Option<SupportedVersion>,
        scopes: &mut Vec<Scope>,
        task_name: &Ident,
        span: Span,
    ) -> ScopeIndex {
        let index = add_scope(scopes, Scope::new(Some(ScopeIndex(0)), span));

        // Command and output sections in 1.2 have access to the `task` variable
        if version >= Some(SupportedVersion::V1(V1::Two)) {
            scopes[index.0].insert(TASK_VAR_NAME, task_name.span(), Type::Task);
        }

        index
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
    let graph = TaskGraphBuilder::default().build(document.version.unwrap(), task, diagnostics);
    let mut scopes = vec![Scope::new(None, braced_scope_span(task))];
    let mut output_scope = None;
    let mut command_scope = None;

    for index in toposort(&graph, None).expect("graph should be acyclic") {
        match graph[index].clone() {
            TaskGraphNode::Input(decl) => {
                if !add_decl(
                    document,
                    ScopeRefMut::new(&mut scopes, ScopeIndex(0)),
                    &decl,
                    |_, n, _, _| inputs[n].ty,
                    diagnostics,
                ) {
                    continue;
                }

                // Check for unused input
                if let Some(severity) = config.unused_input {
                    let name = decl.name();
                    if graph
                        .edges_directed(index, Direction::Outgoing)
                        .next()
                        .is_none()
                    {
                        // Determine if the input is really used based on its name and type
                        if is_input_used(document, name.as_str(), inputs[name.as_str()].ty) {
                            continue;
                        }

                        if !decl.syntax().is_rule_excepted(UNUSED_INPUT_RULE_ID) {
                            diagnostics.push(
                                unused_input(name.as_str(), name.span()).with_severity(severity),
                            );
                        }
                    }
                }
            }
            TaskGraphNode::Decl(decl) => {
                if !add_decl(
                    document,
                    ScopeRefMut::new(&mut scopes, ScopeIndex(0)),
                    &decl,
                    |doc, _, decl, diag| convert_ast_type(doc, &decl.ty(), diag),
                    diagnostics,
                ) {
                    continue;
                }

                // Check for unused declaration
                if let Some(severity) = config.unused_declaration {
                    let name = decl.name();
                    if graph
                        .edges_directed(index, Direction::Outgoing)
                        .next()
                        .is_none()
                        && !decl.syntax().is_rule_excepted(UNUSED_DECL_RULE_ID)
                    {
                        diagnostics.push(
                            unused_declaration(name.as_str(), name.span()).with_severity(severity),
                        );
                    }
                }
            }
            TaskGraphNode::Output(decl) => {
                let scope_index = *output_scope.get_or_insert_with(|| {
                    create_section_scope(
                        document.version(),
                        &mut scopes,
                        &name,
                        braced_scope_span(&task.output().expect("should have output section")),
                    )
                });
                add_decl(
                    document,
                    ScopeRefMut::new(&mut scopes, scope_index),
                    &decl,
                    |_, n, _, _| outputs[n].ty,
                    diagnostics,
                );
            }
            TaskGraphNode::Command(section) => {
                let scope_index = *command_scope.get_or_insert_with(|| {
                    let span = if section.is_heredoc() {
                        heredoc_scope_span(&section)
                    } else {
                        braced_scope_span(&section)
                    };

                    create_section_scope(document.version(), &mut scopes, &name, span)
                });

                let mut context =
                    EvaluationContext::new(document, ScopeRef::new(&scopes, scope_index));
                let mut evaluator = ExprTypeEvaluator::new(&mut context, diagnostics);
                for part in section.parts() {
                    if let CommandPart::Placeholder(p) = part {
                        evaluator.check_placeholder(&p);
                    }
                }
            }
            TaskGraphNode::Runtime(section) => {
                // Perform type checking on the runtime section's expressions
                let mut context =
                    EvaluationContext::new(document, ScopeRef::new(&scopes, ScopeIndex(0)));
                let mut evaluator = ExprTypeEvaluator::new(&mut context, diagnostics);
                for item in section.items() {
                    evaluator.evaluate_runtime_item(&item.name(), &item.expr());
                }
            }
            TaskGraphNode::Requirements(section) => {
                // Perform type checking on the requirements section's expressions
                let mut context =
                    EvaluationContext::new(document, ScopeRef::new(&scopes, ScopeIndex(0)));
                let mut evaluator = ExprTypeEvaluator::new(&mut context, diagnostics);
                for item in section.items() {
                    evaluator.evaluate_requirements_item(&item.name(), &item.expr());
                }
            }
            TaskGraphNode::Hints(section) => {
                // Perform type checking on the hints section's expressions
                let mut context = EvaluationContext::new_for_task(
                    document,
                    ScopeRef::new(&scopes, ScopeIndex(0)),
                    TaskEvaluationContext::new(name.as_str(), &inputs, &outputs),
                );
                let mut evaluator = ExprTypeEvaluator::new(&mut context, diagnostics);
                for item in section.items() {
                    evaluator.evaluate_hints_item(&item.name(), &item.expr())
                }
            }
        }
    }

    // Sort the scopes
    sort_scopes(&mut scopes);

    document.tasks.insert(name.as_str().to_string(), Task {
        name_span: name.span(),
        name: name.as_str().to_string(),
        scopes,
        inputs,
        outputs,
    });
}

/// Adds a declaration to a scope.
fn add_decl(
    document: &mut Document,
    mut scope: ScopeRefMut<'_>,
    decl: &Decl,
    ty: impl FnOnce(&mut Document, &str, &Decl, &mut Vec<Diagnostic>) -> Type,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let (name, expr) = (decl.name(), decl.expr());
    if scope.lookup(name.as_str()).is_some() {
        // The declaration is conflicting; don't add to the scope
        return false;
    }

    let ty = ty(document, name.as_str(), decl, diagnostics);

    scope.insert(name.as_str(), name.span(), ty);

    if let Some(expr) = expr {
        type_check_expr(
            document,
            scope.into_scope_ref(),
            &expr,
            ty,
            name.span(),
            diagnostics,
        );
    }

    true
}

/// Adds a workflow to the document.
///
/// Returns `true` if the workflow was added to the document or `false` if not
/// (i.e. there was a conflict).
fn add_workflow(
    document: &mut Document,
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

    // Note: we delay populating the workflow until later on so that we can populate
    // all tasks in the document first; it is done this way so we can resolve local
    // task call targets.

    document.workflow = Some(Workflow {
        name_span: name.span(),
        name: name.as_str().to_string(),
        scopes: Default::default(),
        inputs: Default::default(),
        outputs: Default::default(),
        calls: Default::default(),
        allows_nested_inputs: document
            .version
            .map(|v| workflow.allows_nested_inputs(v))
            .unwrap_or(false),
    });

    true
}

/// Finishes populating a workflow.
fn populate_workflow(
    config: DiagnosticsConfig,
    document: &mut Document,
    workflow: &WorkflowDefinition,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Populate type maps for the workflow's inputs and outputs
    let inputs = create_input_type_map(
        document,
        workflow.input().into_iter().flat_map(|s| s.declarations()),
        diagnostics,
    );
    let outputs = create_output_type_map(
        document,
        workflow
            .output()
            .into_iter()
            .flat_map(|s| s.declarations().map(Decl::Bound)),
        diagnostics,
    );

    // Keep a map of scopes from syntax node that introduced the scope to the scope
    // index
    let mut scope_indexes: HashMap<SyntaxNode, ScopeIndex> = HashMap::new();
    let mut scopes = vec![Scope::new(None, braced_scope_span(workflow))];
    let mut output_scope = None;
    let graph = WorkflowGraphBuilder::default().build(workflow, diagnostics);

    for index in toposort(&graph, None).expect("graph should be acyclic") {
        match graph[index].clone() {
            WorkflowGraphNode::Input(decl) => {
                if !add_decl(
                    document,
                    ScopeRefMut::new(&mut scopes, ScopeIndex(0)),
                    &decl,
                    |_, n, _, _| inputs[n].ty,
                    diagnostics,
                ) {
                    continue;
                }

                // Check for unused input
                if let Some(severity) = config.unused_input {
                    let name = decl.name();
                    if graph
                        .edges_directed(index, Direction::Outgoing)
                        .next()
                        .is_none()
                    {
                        // Determine if the input is really used based on its name and type
                        if is_input_used(document, name.as_str(), inputs[name.as_str()].ty) {
                            continue;
                        }

                        if !decl.syntax().is_rule_excepted(UNUSED_INPUT_RULE_ID) {
                            diagnostics.push(
                                unused_input(name.as_str(), name.span()).with_severity(severity),
                            );
                        }
                    }
                }
            }
            WorkflowGraphNode::Decl(decl) => {
                let scope_index = scope_indexes
                    .get(&decl.syntax().parent().expect("should have parent"))
                    .copied()
                    .unwrap_or(ScopeIndex(0));

                if !add_decl(
                    document,
                    ScopeRefMut::new(&mut scopes, scope_index),
                    &decl,
                    |doc, _, decl, diag| convert_ast_type(doc, &decl.ty(), diag),
                    diagnostics,
                ) {
                    continue;
                }

                // Check for unused declaration
                if let Some(severity) = config.unused_declaration {
                    let name = decl.name();
                    if graph
                        .edges_directed(index, Direction::Outgoing)
                        .next()
                        .is_none()
                        && !decl.syntax().is_rule_excepted(UNUSED_DECL_RULE_ID)
                    {
                        diagnostics.push(
                            unused_declaration(name.as_str(), name.span()).with_severity(severity),
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
                            braced_scope_span(
                                &workflow.output().expect("should have output section"),
                            ),
                        ),
                    )
                });
                add_decl(
                    document,
                    ScopeRefMut::new(&mut scopes, scope_index),
                    &decl,
                    |_, n, _, _| outputs[n].ty,
                    diagnostics,
                );
            }
            WorkflowGraphNode::Conditional(statement) => {
                let parent = scope_indexes
                    .get(&statement.syntax().parent().expect("should have parent"))
                    .copied()
                    .unwrap_or(ScopeIndex(0));
                add_conditional_statement(
                    document,
                    &mut scopes,
                    parent,
                    &mut scope_indexes,
                    &statement,
                    diagnostics,
                );
            }
            WorkflowGraphNode::Scatter(statement) => {
                let parent = scope_indexes
                    .get(&statement.syntax().parent().expect("should have parent"))
                    .copied()
                    .unwrap_or(ScopeIndex(0));
                add_scatter_statement(
                    document,
                    &mut scopes,
                    parent,
                    &mut scope_indexes,
                    &statement,
                    diagnostics,
                );
            }
            WorkflowGraphNode::Call(statement) => {
                let scope_index = scope_indexes
                    .get(&statement.syntax().parent().expect("should have parent"))
                    .copied()
                    .unwrap_or(ScopeIndex(0));
                add_call_statement(
                    document,
                    workflow.name().as_str(),
                    &mut scopes,
                    scope_index,
                    &statement,
                    document
                        .workflow
                        .as_ref()
                        .expect("should have workflow")
                        .allows_nested_inputs,
                    diagnostics,
                );

                // Check for unused call
                if let Some(severity) = config.unused_call {
                    if graph
                        .edges_directed(index, Direction::Outgoing)
                        .next()
                        .is_none()
                        && !statement.syntax().is_rule_excepted(UNUSED_CALL_RULE_ID)
                    {
                        let target_name = statement
                            .target()
                            .names()
                            .last()
                            .expect("expected a last call target name");

                        let name = statement
                            .alias()
                            .map(|a| a.name())
                            .unwrap_or_else(|| target_name);

                        diagnostics
                            .push(unused_call(name.as_str(), name.span()).with_severity(severity));
                    }
                }
            }
            WorkflowGraphNode::ExitConditional(statement) => {
                let scope_index = scope_indexes
                    .get(statement.syntax())
                    .copied()
                    .expect("should have scope");
                promote_scope(
                    &mut document.types,
                    &mut scopes,
                    scope_index,
                    None,
                    PromotionKind::Conditional,
                );
            }
            WorkflowGraphNode::ExitScatter(statement) => {
                let scope_index = scope_indexes
                    .get(statement.syntax())
                    .copied()
                    .expect("should have scope");
                let variable = statement.variable();
                promote_scope(
                    &mut document.types,
                    &mut scopes,
                    scope_index,
                    Some(variable.as_str()),
                    PromotionKind::Scatter,
                );
            }
        }
    }

    // Sort the scopes
    sort_scopes(&mut scopes);

    // Finally, populate the workflow
    let workflow = document.workflow.as_mut().expect("expected a workflow");
    workflow.scopes = scopes;
    workflow.inputs = inputs;
    workflow.outputs = outputs;
}

/// Adds a conditional statement to the current scope.
fn add_conditional_statement(
    document: &mut Document,
    scopes: &mut Vec<Scope>,
    parent: ScopeIndex,
    scope_indexes: &mut HashMap<SyntaxNode, ScopeIndex>,
    statement: &ConditionalStatement,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let scope_index = add_scope(
        scopes,
        Scope::new(Some(parent), braced_scope_span(statement)),
    );
    scope_indexes.insert(statement.syntax().clone(), scope_index);

    // Evaluate the statement's expression; it is expected to be a boolean
    let expr = statement.expr();
    let mut context = EvaluationContext::new(document, ScopeRef::new(scopes, scope_index));
    let mut evaluator = ExprTypeEvaluator::new(&mut context, diagnostics);
    let ty = evaluator.evaluate_expr(&expr).unwrap_or(Type::Union);

    if !ty.is_coercible_to(&document.types, &PrimitiveTypeKind::Boolean.into()) {
        diagnostics.push(if_conditional_mismatch(&document.types, ty, expr.span()));
    }
}

/// Adds a scatter statement to the current scope.
fn add_scatter_statement(
    document: &mut Document,
    scopes: &mut Vec<Scope>,
    parent: ScopeIndex,
    scopes_indexes: &mut HashMap<SyntaxNode, ScopeIndex>,
    statement: &ScatterStatement,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let scope_index = add_scope(
        scopes,
        Scope::new(Some(parent), braced_scope_span(statement)),
    );
    scopes_indexes.insert(statement.syntax().clone(), scope_index);

    // Evaluate the statement expression; it is expected to be an array
    let expr = statement.expr();
    let mut context = EvaluationContext::new(document, ScopeRef::new(scopes, scope_index));
    let mut evaluator = ExprTypeEvaluator::new(&mut context, diagnostics);
    let ty = evaluator.evaluate_expr(&expr).unwrap_or(Type::Union);
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
    scopes[scope_index.0].insert(variable.as_str().to_string(), variable.span(), element_ty);
}

/// Adds a call statement to the current scope.
fn add_call_statement(
    document: &mut Document,
    workflow_name: &str,
    scopes: &mut [Scope],
    index: ScopeIndex,
    statement: &CallStatement,
    nested_inputs_allowed: bool,
    diagnostics: &mut Vec<Diagnostic>,
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

    let ty = if let Some(ty) = resolve_call_type(document, workflow_name, statement, diagnostics) {
        // Type check the call inputs
        let mut seen = HashSet::new();
        for input in statement.inputs() {
            let input_name = input.name();

            let expected_ty = ty
                .inputs()
                .get(input_name.as_str())
                .copied()
                .map(|i| i.ty)
                .unwrap_or_else(|| {
                    diagnostics.push(unknown_call_io(&ty, &input_name, Io::Input));
                    Type::Union
                });

            match input.expr() {
                Some(expr) => {
                    type_check_expr(
                        document,
                        ScopeRef::new(scopes, index),
                        &expr,
                        expected_ty,
                        input_name.span(),
                        diagnostics,
                    );
                }
                None => {
                    if let Some(name) = ScopeRef::new(scopes, index).lookup(input_name.as_str()) {
                        if !matches!(expected_ty, Type::Union)
                            && !name.ty.is_coercible_to(&document.types, &expected_ty)
                        {
                            diagnostics.push(call_input_type_mismatch(
                                &document.types,
                                &input_name,
                                expected_ty,
                                name.ty,
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
            for (name, input) in ty.inputs() {
                if input.required && !seen.contains(name.as_str()) {
                    diagnostics.push(missing_call_input(ty.kind(), &target_name, name));
                }
            }
        }

        // Add the call to the workflow
        let calls = &mut document
            .workflow
            .as_mut()
            .expect("should have workflow")
            .calls;
        if !calls.contains_key(name.as_str()) {
            calls.insert(name.as_str().to_string(), ty.clone());
        }

        document.types.add_call(ty)
    } else {
        Type::Union
    };

    // Don't modify the scope if there's a conflict
    if ScopeRef::new(scopes, index).lookup(name.as_str()).is_none() {
        scopes[index.0].insert(name.as_str(), name.span(), ty);
    }
}

/// Resolves the type of a call statement.
///
/// Returns `None` if the type could not be resolved.
fn resolve_call_type(
    document: &mut Document,
    workflow_name: &str,
    statement: &CallStatement,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<CallType> {
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

        match document.namespaces.get_mut(target.as_str()) {
            Some(ns) => {
                ns.used = true;
                namespace = Some(&document.namespaces[target.as_str()])
            }
            None => {
                diagnostics.push(unknown_namespace(&target));
                return None;
            }
        }
    }

    let target = namespace.map(|ns| ns.document.as_ref()).unwrap_or(document);
    let name = name.expect("should have name");
    if namespace.is_none() && name.as_str() == workflow_name {
        diagnostics.push(recursive_workflow_call(&name));
        return None;
    }

    let (kind, mut inputs, mut outputs) = if let Some(task) = target.tasks.get(name.as_str()) {
        (CallKind::Task, task.inputs.clone(), task.outputs.clone())
    } else {
        match &target.workflow {
            Some(workflow) if workflow.name == name.as_str() => (
                CallKind::Workflow,
                workflow.inputs.clone(),
                workflow.outputs.clone(),
            ),
            _ => {
                diagnostics.push(unknown_task_or_workflow(namespace.map(|ns| ns.span), &name));
                return None;
            }
        }
    };

    let specified = Arc::new(
        statement
            .inputs()
            .map(|i| i.name().as_str().to_string())
            .collect(),
    );

    // If the target is from an import, we need to import its type definitions into
    // the current document's types collection
    if let Some(types) = namespace.map(|ns| &ns.document.types) {
        for input in Arc::make_mut(&mut inputs).values_mut() {
            input.ty = document.types.import(types, input.ty);
        }

        for output in Arc::make_mut(&mut outputs).values_mut() {
            output.ty = document.types.import(types, output.ty);
        }

        Some(CallType::namespaced(
            kind,
            statement.target().names().next().unwrap().as_str(),
            name.as_str(),
            specified,
            inputs,
            outputs,
        ))
    } else {
        Some(CallType::new(
            kind,
            name.as_str(),
            specified,
            inputs,
            outputs,
        ))
    }
}

/// Promotes the names in the current to the parent scope.
fn promote_scope(
    types: &mut Types,
    scopes: &mut [Scope],
    index: ScopeIndex,
    skip: Option<&str>,
    kind: PromotionKind,
) {
    // We need to split the scopes as we want to read from one part of the slice and
    // write to another; the left side will contain the parent at it's index and the
    // right side will contain the child scope at it's index minus the parent's
    let parent = scopes[index.0].parent.expect("should have a parent scope");
    assert!(index.0 > parent.0);
    let (left, right) = scopes.split_at_mut(parent.0 + 1);
    let scope = &right[index.0 - parent.0 - 1];
    let parent = &mut left[parent.0];
    for (name, Name { span, ty }) in scope.names.iter() {
        if Some(name.as_str()) == skip {
            continue;
        }

        parent.names.entry(name.clone()).or_insert_with(|| Name {
            span: *span,
            ty: ty.promote(types, kind),
        });
    }
}

/// Resolves an import to its document.
fn resolve_import(
    graph: &DocumentGraph,
    stmt: &ImportStatement,
    importer_index: NodeIndex,
    importer_version: &Version,
) -> Result<(Arc<Url>, Arc<Document>), Option<Diagnostic>> {
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
        .map(|a| a.document().clone())
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
fn set_struct_types(document: &mut Document, diagnostics: &mut Vec<Diagnostic>) {
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

        let mut converter = AstTypeConverter::new(&mut document.types, |name, span| {
            if let Some(s) = document.structs.get(name) {
                // Mark the struct's namespace as used
                if let Some(ns) = &s.namespace {
                    document.namespaces[ns].used = true;
                }

                Ok(s.ty().unwrap_or(Type::Union))
            } else {
                diagnostics.push(unknown_type(
                    name,
                    Span::new(span.start() + document.structs[index].offset, span.len()),
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

/// Represents context of a task being evaluated for an expression type
/// evaluator.
#[derive(Clone, Copy, Debug)]
struct TaskEvaluationContext<'a> {
    /// The name of the task.
    name: &'a str,
    /// The inputs of the task.
    inputs: &'a HashMap<String, Input>,
    /// The outputs of the task.
    outputs: &'a HashMap<String, Output>,
}

impl<'a> TaskEvaluationContext<'a> {
    /// Constructs a new task evaluation context given the task name, inputs,
    /// and outputs.
    fn new(
        name: &'a str,
        inputs: &'a HashMap<String, Input>,
        outputs: &'a HashMap<String, Output>,
    ) -> Self {
        Self {
            name,
            inputs,
            outputs,
        }
    }
}

/// Represents context to an expression type evaluator.
#[derive(Debug)]
struct EvaluationContext<'a> {
    /// The document being evaluated.
    document: &'a mut Document,
    /// The current evaluation scope.
    scope: ScopeRef<'a>,
    /// The context of the task being evaluated.
    ///
    /// This is only `Some` when evaluating a task's `hints` section.`
    task: Option<TaskEvaluationContext<'a>>,
}

impl<'a> EvaluationContext<'a> {
    /// Constructs a new expression type evaluation context.
    pub fn new(document: &'a mut Document, scope: ScopeRef<'a>) -> Self {
        Self {
            document,
            scope,
            task: None,
        }
    }

    /// Constructs a new expression type evaluation context with the given task
    /// context.
    ///
    /// This is used to evaluated the type of expressions inside of a task's
    /// `hints` section.
    pub fn new_for_task(
        document: &'a mut Document,
        scope: ScopeRef<'a>,
        task: TaskEvaluationContext<'a>,
    ) -> Self {
        Self {
            document,
            scope,
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

    fn types(&self) -> &Types {
        &self.document.types
    }

    fn types_mut(&mut self) -> &mut Types {
        &mut self.document.types
    }

    fn resolve_name(&self, name: &Ident) -> Option<Type> {
        self.scope.lookup(name.as_str()).map(|n| n.ty())
    }

    fn resolve_type_name(&mut self, name: &Ident) -> Result<Type, Diagnostic> {
        self.document
            .structs
            .get(name.as_str())
            .map(|s| {
                // Mark the struct's namespace as used
                if let Some(ns) = &s.namespace {
                    self.document.namespaces[ns].used = true;
                }

                s.ty().expect("struct should have type")
            })
            .ok_or_else(|| unknown_type(name.as_str(), name.span()))
    }

    fn input(&self, name: &str) -> Option<Input> {
        self.task.and_then(|task| task.inputs.get(name).copied())
    }

    fn output(&self, name: &str) -> Option<Output> {
        self.task.and_then(|task| task.outputs.get(name).copied())
    }

    fn task_name(&self) -> Option<&str> {
        self.task.map(|task| task.name)
    }

    fn supports_hints_type(&self) -> bool {
        self.task.is_some()
    }

    fn supports_input_type(&self) -> bool {
        self.task.is_some()
    }

    fn supports_output_type(&self) -> bool {
        self.task.is_some()
    }
}

/// Performs a type check of an expression.
fn type_check_expr(
    document: &mut Document,
    scope: ScopeRef<'_>,
    expr: &Expr,
    expected: Type,
    expected_span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut context = EvaluationContext::new(document, scope);
    let mut evaluator = ExprTypeEvaluator::new(&mut context, diagnostics);
    let actual = evaluator.evaluate_expr(expr).unwrap_or(Type::Union);

    if !matches!(expected, Type::Union) && !actual.is_coercible_to(&document.types, &expected) {
        diagnostics.push(type_mismatch(
            &document.types,
            expected,
            expected_span,
            actual,
            expr.span(),
        ));
    }
    // Check to see if we're assigning an empty array literal to a non-empty type; we can statically
    // flag these as errors; otherwise, non-empty array constraints are checked at runtime
    else if let Type::Compound(e) = expected {
        if let CompoundTypeDef::Array(e) = document.types.type_definition(e.definition()) {
            if e.is_non_empty() && expr.is_empty_array_literal() {
                diagnostics.push(non_empty_array_assignment(expected_span, expr.span()));
            }
        }
    }
}
