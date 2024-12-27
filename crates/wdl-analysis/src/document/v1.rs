//! Conversion of a V1 AST to an analyzed document.
use std::collections::HashMap;
use std::collections::HashSet;
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
use crate::types::CompoundType;
use crate::types::Optional;
use crate::types::PrimitiveType;
use crate::types::PromotionKind;
use crate::types::Type;
use crate::types::TypeNameResolver;
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
fn is_input_used(name: &str, ty: &Type) -> bool {
    /// The suffixes that cause the input to be "used"
    const SUFFIXES: &[&str] = &[
        "index", "indexes", "indices", "idx", "tbi", "bai", "crai", "csi", "fai", "dict",
    ];

    // Determine if the input is `File` or `Array[File]`
    match ty {
        Type::Primitive(PrimitiveType::File, _) => {}
        Type::Compound(CompoundType::Array(ty), _) => match ty.element_type() {
            Type::Primitive(PrimitiveType::File, _) => {}
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
pub(crate) fn populate_document(
    document: &mut Document,
    config: DiagnosticsConfig,
    graph: &DocumentGraph,
    index: NodeIndex,
    ast: &Ast,
    version: &wdl_ast::Version,
) {
    assert!(
        matches!(
            document.version.expect("document should have a version"),
            SupportedVersion::V1(_)
        ),
        "expected a supported V1 version"
    );

    // First start by processing imports and struct definitions
    // This needs to be performed before processing tasks and workflows as
    // declarations might reference an imported or locally-defined struct
    for item in ast.items() {
        match item {
            DocumentItem::Import(import) => {
                add_namespace(document, graph, &import, index, version);
            }
            DocumentItem::Struct(s) => {
                add_struct(document, &s);
            }
            DocumentItem::Task(_) | DocumentItem::Workflow(_) => {
                continue;
            }
        }
    }

    // Populate the struct types now that all structs have been processed
    set_struct_types(document);

    // Now process the tasks and workflows
    let mut workflow = None;
    for item in ast.items() {
        match item {
            DocumentItem::Task(task) => {
                add_task(config, document, &task);
            }
            DocumentItem::Workflow(w) => {
                // Note that this doesn't populate the workflow; we delay that until after
                // we've seen every task in the document so that we can resolve call targets
                if add_workflow(document, &w) {
                    workflow = Some(w.clone());
                }
            }
            DocumentItem::Import(_) | DocumentItem::Struct(_) => {
                continue;
            }
        }
    }

    if let Some(workflow) = workflow {
        populate_workflow(config, document, &workflow);
    }
}

/// Adds a namespace to the document.
fn add_namespace(
    document: &mut Document,
    graph: &DocumentGraph,
    import: &ImportStatement,
    importer_index: NodeIndex,
    importer_version: &Version,
) {
    // Start by resolving the import to its document
    let (uri, imported) = match resolve_import(graph, import, importer_index, importer_version) {
        Ok(resolved) => resolved,
        Err(Some(diagnostic)) => {
            document.diagnostics.push(diagnostic);
            return;
        }
        Err(None) => return,
    };

    // Check for conflicting namespaces
    let span = import.uri().syntax().text_range().to_span();
    let ns = match import.namespace() {
        Some((ns, span)) => {
            if let Some(prev) = document.namespaces.get(&ns) {
                document.diagnostics.push(namespace_conflict(
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
                document.diagnostics.push(struct_not_in_document(&from));
                return None;
            }

            Some((from.as_str().to_string(), to))
        })
        .collect::<HashMap<_, _>>();

    // Insert the imported document's struct definitions
    for (name, s) in &imported.structs {
        let (span, aliased_name, aliased) = aliases
            .get(name)
            .map(|n| (n.span(), n.as_str(), true))
            .unwrap_or_else(|| (span, name, false));
        match document.structs.get(aliased_name) {
            Some(prev) => {
                let a = StructDefinition::cast(SyntaxNode::new_root(prev.node.clone()))
                    .expect("node should cast");
                let b = StructDefinition::cast(SyntaxNode::new_root(s.node.clone()))
                    .expect("node should cast");
                if !are_structs_equal(&a, &b) {
                    // Import conflicts with a struct defined in this document
                    if prev.namespace.is_none() {
                        document.diagnostics.push(struct_conflicts_with_import(
                            aliased_name,
                            prev.span,
                            span,
                        ));
                    } else {
                        document.diagnostics.push(imported_struct_conflict(
                            aliased_name,
                            span,
                            prev.span,
                            !aliased,
                        ));
                    }
                    continue;
                }
            }
            None => {
                document.structs.insert(aliased_name.to_string(), Struct {
                    span,
                    offset: s.offset,
                    node: s.node.clone(),
                    namespace: Some(ns.clone()),
                    ty: s.ty.clone(),
                });
            }
        }
    }
}

/// Compares two structs for structural equality.
fn are_structs_equal(a: &StructDefinition, b: &StructDefinition) -> bool {
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
fn add_struct(document: &mut Document, definition: &StructDefinition) {
    let name = definition.name();
    if let Some(prev) = document.structs.get(name.as_str()) {
        if prev.namespace.is_some() {
            let prev_def = StructDefinition::cast(SyntaxNode::new_root(prev.node.clone()))
                .expect("node should cast");
            if !are_structs_equal(definition, &prev_def) {
                document.diagnostics.push(struct_conflicts_with_import(
                    name.as_str(),
                    name.span(),
                    prev.span,
                ))
            }
        } else {
            document.diagnostics.push(name_conflict(
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
            document.diagnostics.push(name_conflict(
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
fn convert_ast_type(document: &mut Document, ty: &wdl_ast::v1::Type) -> Type {
    /// Used to resolve a type name from a document.
    struct Resolver<'a>(&'a mut Document);

    impl TypeNameResolver for Resolver<'_> {
        fn resolve(&mut self, name: &Ident) -> Result<Type, Diagnostic> {
            self.0
                .structs
                .get(name.as_str())
                .map(|s| {
                    // Mark the struct's namespace as used
                    if let Some(ns) = &s.namespace {
                        self.0.namespaces[ns].used = true;
                    }

                    s.ty().expect("struct should have type").clone()
                })
                .ok_or_else(|| unknown_type(name.as_str(), name.span()))
        }
    }

    let mut converter = AstTypeConverter::new(Resolver(document));
    match converter.convert_type(ty) {
        Ok(ty) => ty,
        Err(diagnostic) => {
            document.diagnostics.push(diagnostic);
            Type::Union
        }
    }
}

/// Creates an input type map.
fn create_input_type_map(
    document: &mut Document,
    declarations: impl Iterator<Item = Decl>,
) -> Arc<IndexMap<String, Input>> {
    let mut map = IndexMap::new();
    for decl in declarations {
        let name = decl.name();
        if map.contains_key(name.as_str()) {
            // Ignore the duplicate
            continue;
        }

        let ty = convert_ast_type(document, &decl.ty());
        let optional = ty.is_optional();
        map.insert(name.as_str().to_string(), Input {
            ty,
            required: decl.expr().is_none() && !optional,
        });
    }

    map.into()
}

/// Creates an output type map.
fn create_output_type_map(
    document: &mut Document,
    declarations: impl Iterator<Item = Decl>,
) -> Arc<IndexMap<String, Output>> {
    let mut map = IndexMap::new();
    for decl in declarations {
        let name = decl.name();
        if map.contains_key(name.as_str()) {
            // Ignore the duplicate
            continue;
        }

        let ty = convert_ast_type(document, &decl.ty());
        map.insert(name.as_str().to_string(), Output { ty });
    }

    map.into()
}

/// Adds a task to the document.
fn add_task(config: DiagnosticsConfig, document: &mut Document, definition: &TaskDefinition) {
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
    let name = definition.name();
    if let Some(s) = document.tasks.get(name.as_str()) {
        document.diagnostics.push(name_conflict(
            name.as_str(),
            Context::Task(name.span()),
            Context::Task(s.name_span),
        ));
        return;
    } else if let Some(s) = &document.workflow {
        if s.name == name.as_str() {
            document.diagnostics.push(name_conflict(
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
        definition
            .input()
            .into_iter()
            .flat_map(|s| s.declarations()),
    );
    let outputs = create_output_type_map(
        document,
        definition
            .output()
            .into_iter()
            .flat_map(|s| s.declarations().map(Decl::Bound)),
    );

    // Process the task in evaluation order
    let graph = TaskGraphBuilder::default().build(
        document.version.unwrap(),
        definition,
        &mut document.diagnostics,
    );

    let mut task = Task {
        name_span: name.span(),
        name: name.as_str().to_string(),
        scopes: vec![Scope::new(None, braced_scope_span(definition))],
        inputs,
        outputs,
    };

    let mut output_scope = None;
    let mut command_scope = None;

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
                if let Some(severity) = config.unused_input {
                    let name = decl.name();
                    if graph
                        .edges_directed(index, Direction::Outgoing)
                        .next()
                        .is_none()
                    {
                        // Determine if the input is really used based on its name and type
                        if is_input_used(name.as_str(), &task.inputs[name.as_str()].ty) {
                            continue;
                        }

                        if !decl.syntax().is_rule_excepted(UNUSED_INPUT_RULE_ID) {
                            document.diagnostics.push(
                                unused_input(name.as_str(), name.span()).with_severity(severity),
                            );
                        }
                    }
                }
            }
            TaskGraphNode::Decl(decl) => {
                if !add_decl(
                    config,
                    document,
                    ScopeRefMut::new(&mut task.scopes, ScopeIndex(0)),
                    &decl,
                    |doc, _, decl| convert_ast_type(doc, &decl.ty()),
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
                        document.diagnostics.push(
                            unused_declaration(name.as_str(), name.span()).with_severity(severity),
                        );
                    }
                }
            }
            TaskGraphNode::Output(decl) => {
                let scope_index = *output_scope.get_or_insert_with(|| {
                    create_section_scope(
                        document.version(),
                        &mut task.scopes,
                        &name,
                        braced_scope_span(
                            &definition.output().expect("should have output section"),
                        ),
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
                        heredoc_scope_span(&section)
                    } else {
                        braced_scope_span(&section)
                    };

                    create_section_scope(document.version(), &mut task.scopes, &name, span)
                });

                let mut context = EvaluationContext::new(
                    document,
                    ScopeRef::new(&task.scopes, scope_index),
                    config,
                );
                let mut evaluator = ExprTypeEvaluator::new(&mut context);
                for part in section.parts() {
                    if let CommandPart::Placeholder(p) = part {
                        evaluator.check_placeholder(&p);
                    }
                }
            }
            TaskGraphNode::Runtime(section) => {
                // Perform type checking on the runtime section's expressions
                let mut context = EvaluationContext::new(
                    document,
                    ScopeRef::new(&task.scopes, ScopeIndex(0)),
                    config,
                );
                let mut evaluator = ExprTypeEvaluator::new(&mut context);
                for item in section.items() {
                    evaluator.evaluate_runtime_item(&item.name(), &item.expr());
                }
            }
            TaskGraphNode::Requirements(section) => {
                // Perform type checking on the requirements section's expressions
                let mut context = EvaluationContext::new(
                    document,
                    ScopeRef::new(&task.scopes, ScopeIndex(0)),
                    config,
                );
                let mut evaluator = ExprTypeEvaluator::new(&mut context);
                for item in section.items() {
                    evaluator.evaluate_requirements_item(&item.name(), &item.expr());
                }
            }
            TaskGraphNode::Hints(section) => {
                // Perform type checking on the hints section's expressions
                let mut context = EvaluationContext::new_for_task(
                    document,
                    ScopeRef::new(&task.scopes, ScopeIndex(0)),
                    config,
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
    document.tasks.insert(name.as_str().to_string(), task);
}

/// Adds a declaration to a scope.
fn add_decl(
    config: DiagnosticsConfig,
    document: &mut Document,
    mut scope: ScopeRefMut<'_>,
    decl: &Decl,
    ty: impl FnOnce(&mut Document, &str, &Decl) -> Type,
) -> bool {
    let (name, expr) = (decl.name(), decl.expr());
    if scope.lookup(name.as_str()).is_some() {
        // The declaration is conflicting; don't add to the scope
        return false;
    }

    let ty = ty(document, name.as_str(), decl);

    scope.insert(name.as_str(), name.span(), ty.clone());

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
fn add_workflow(document: &mut Document, workflow: &WorkflowDefinition) -> bool {
    // Check for conflicts with task names or an existing workspace
    let name = workflow.name();
    if let Some(s) = document.tasks.get(name.as_str()) {
        document.diagnostics.push(name_conflict(
            name.as_str(),
            Context::Workflow(name.span()),
            Context::Task(s.name_span),
        ));
        return false;
    } else if let Some(s) = &document.workflow {
        document
            .diagnostics
            .push(duplicate_workflow(&name, s.name_span));
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
) {
    // Populate type maps for the workflow's inputs and outputs
    let inputs = create_input_type_map(
        document,
        workflow.input().into_iter().flat_map(|s| s.declarations()),
    );
    let outputs = create_output_type_map(
        document,
        workflow
            .output()
            .into_iter()
            .flat_map(|s| s.declarations().map(Decl::Bound)),
    );

    // Keep a map of scopes from syntax node that introduced the scope to the scope
    // index
    let mut scope_indexes: HashMap<SyntaxNode, ScopeIndex> = HashMap::new();
    let mut scopes = vec![Scope::new(None, braced_scope_span(workflow))];
    let mut output_scope = None;
    let graph = WorkflowGraphBuilder::default().build(workflow, &mut document.diagnostics);

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
                if let Some(severity) = config.unused_input {
                    let name = decl.name();
                    if graph
                        .edges_directed(index, Direction::Outgoing)
                        .next()
                        .is_none()
                    {
                        // Determine if the input is really used based on its name and type
                        if is_input_used(name.as_str(), &inputs[name.as_str()].ty) {
                            continue;
                        }

                        if !decl.syntax().is_rule_excepted(UNUSED_INPUT_RULE_ID) {
                            document.diagnostics.push(
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
                    config,
                    document,
                    ScopeRefMut::new(&mut scopes, scope_index),
                    &decl,
                    |doc, _, decl| convert_ast_type(doc, &decl.ty()),
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
                        document.diagnostics.push(
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
                    config,
                    document,
                    ScopeRefMut::new(&mut scopes, scope_index),
                    &decl,
                    |_, n, _| outputs[n].ty.clone(),
                );
            }
            WorkflowGraphNode::Conditional(statement) => {
                let parent = scope_indexes
                    .get(&statement.syntax().parent().expect("should have parent"))
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
            WorkflowGraphNode::Scatter(statement) => {
                let parent = scope_indexes
                    .get(&statement.syntax().parent().expect("should have parent"))
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
                    .get(&statement.syntax().parent().expect("should have parent"))
                    .copied()
                    .unwrap_or(ScopeIndex(0));
                add_call_statement(
                    config,
                    document,
                    workflow.name().as_str(),
                    ScopeRefMut::new(&mut scopes, scope_index),
                    &statement,
                    document
                        .workflow
                        .as_ref()
                        .expect("should have workflow")
                        .allows_nested_inputs,
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

                        document
                            .diagnostics
                            .push(unused_call(name.as_str(), name.span()).with_severity(severity));
                    }
                }
            }
            WorkflowGraphNode::ExitConditional(statement) => {
                let scope_index = scope_indexes
                    .get(statement.syntax())
                    .copied()
                    .expect("should have scope");
                promote_scope(&mut scopes, scope_index, None, PromotionKind::Conditional);
            }
            WorkflowGraphNode::ExitScatter(statement) => {
                let scope_index = scope_indexes
                    .get(statement.syntax())
                    .copied()
                    .expect("should have scope");
                let variable = statement.variable();
                promote_scope(
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
    config: DiagnosticsConfig,
    document: &mut Document,
    scopes: &mut Vec<Scope>,
    parent: ScopeIndex,
    scope_indexes: &mut HashMap<SyntaxNode, ScopeIndex>,
    statement: &ConditionalStatement,
) {
    let scope_index = add_scope(
        scopes,
        Scope::new(Some(parent), braced_scope_span(statement)),
    );
    scope_indexes.insert(statement.syntax().clone(), scope_index);

    // Evaluate the statement's expression; it is expected to be a boolean
    let expr = statement.expr();
    let mut context = EvaluationContext::new(document, ScopeRef::new(scopes, scope_index), config);
    let mut evaluator = ExprTypeEvaluator::new(&mut context);
    let ty = evaluator.evaluate_expr(&expr).unwrap_or(Type::Union);

    if !ty.is_coercible_to(&PrimitiveType::Boolean.into()) {
        document
            .diagnostics
            .push(if_conditional_mismatch(&ty, expr.span()));
    }
}

/// Adds a scatter statement to the current scope.
fn add_scatter_statement(
    config: DiagnosticsConfig,
    document: &mut Document,
    scopes: &mut Vec<Scope>,
    parent: ScopeIndex,
    scopes_indexes: &mut HashMap<SyntaxNode, ScopeIndex>,
    statement: &ScatterStatement,
) {
    let scope_index = add_scope(
        scopes,
        Scope::new(Some(parent), braced_scope_span(statement)),
    );
    scopes_indexes.insert(statement.syntax().clone(), scope_index);

    // Evaluate the statement expression; it is expected to be an array
    let expr = statement.expr();
    let mut context = EvaluationContext::new(document, ScopeRef::new(scopes, scope_index), config);
    let mut evaluator = ExprTypeEvaluator::new(&mut context);
    let ty = evaluator.evaluate_expr(&expr).unwrap_or(Type::Union);
    let element_ty = match ty {
        Type::Union => Type::Union,
        Type::Compound(CompoundType::Array(ty), _) => ty.element_type().clone(),
        _ => {
            document
                .diagnostics
                .push(type_is_not_array(&ty, expr.span()));
            Type::Union
        }
    };

    // Introduce the scatter variable into the scope
    let variable = statement.variable();
    scopes[scope_index.0].insert(variable.as_str().to_string(), variable.span(), element_ty);
}

/// Adds a call statement to the current scope.
fn add_call_statement(
    config: DiagnosticsConfig,
    document: &mut Document,
    workflow_name: &str,
    mut scope: ScopeRefMut<'_>,
    statement: &CallStatement,
    nested_inputs_allowed: bool,
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

    let ty = if let Some(ty) = resolve_call_type(document, workflow_name, statement) {
        // Type check the call inputs
        let mut seen = HashSet::new();
        for input in statement.inputs() {
            let input_name = input.name();

            let expected_ty = ty
                .inputs()
                .get(input_name.as_str())
                .map(|i| i.ty.clone())
                .unwrap_or_else(|| {
                    document
                        .diagnostics
                        .push(unknown_call_io(&ty, &input_name, Io::Input));
                    Type::Union
                });

            match input.expr() {
                Some(expr) => {
                    type_check_expr(
                        config,
                        document,
                        scope.as_scope_ref(),
                        &expr,
                        &expected_ty,
                        input_name.span(),
                    );
                }
                None => {
                    if let Some(name) = scope.lookup(input_name.as_str()) {
                        if !matches!(expected_ty, Type::Union)
                            && !name.ty.is_coercible_to(&expected_ty)
                        {
                            document.diagnostics.push(call_input_type_mismatch(
                                &input_name,
                                &expected_ty,
                                &name.ty,
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
                    document
                        .diagnostics
                        .push(missing_call_input(ty.kind(), &target_name, name));
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

        ty.into()
    } else {
        Type::Union
    };

    // Don't modify the scope if there's a conflict
    if scope.lookup(name.as_str()).is_none() {
        scope.insert(name.as_str(), name.span(), ty);
    }
}

/// Resolves the type of a call statement.
///
/// Returns `None` if the type could not be resolved.
fn resolve_call_type(
    document: &mut Document,
    workflow_name: &str,
    statement: &CallStatement,
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
            document.diagnostics.push(only_one_namespace(target.span()));
            return None;
        }

        match document.namespaces.get_mut(target.as_str()) {
            Some(ns) => {
                ns.used = true;
                namespace = Some(&document.namespaces[target.as_str()])
            }
            None => {
                document.diagnostics.push(unknown_namespace(&target));
                return None;
            }
        }
    }

    let target = namespace.map(|ns| ns.document.as_ref()).unwrap_or(document);
    let name = name.expect("should have name");
    if namespace.is_none() && name.as_str() == workflow_name {
        document.diagnostics.push(recursive_workflow_call(&name));
        return None;
    }

    let (kind, inputs, outputs) = if let Some(task) = target.tasks.get(name.as_str()) {
        (CallKind::Task, task.inputs.clone(), task.outputs.clone())
    } else {
        match &target.workflow {
            Some(workflow) if workflow.name == name.as_str() => (
                CallKind::Workflow,
                workflow.inputs.clone(),
                workflow.outputs.clone(),
            ),
            _ => {
                document
                    .diagnostics
                    .push(unknown_task_or_workflow(namespace.map(|ns| ns.span), &name));
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

    if namespace.is_some() {
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
fn promote_scope(scopes: &mut [Scope], index: ScopeIndex, skip: Option<&str>, kind: PromotionKind) {
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
            ty: ty.promote(kind),
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
        .cloned()
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
fn set_struct_types(document: &mut Document) {
    /// Used to resolve a type name from a document.
    struct Resolver<'a> {
        /// The document to resolve the type name from.
        document: &'a mut Document,
        /// The offset to use to adjust the start of diagnostics.
        offset: usize,
    }

    impl TypeNameResolver for Resolver<'_> {
        fn resolve(&mut self, name: &Ident) -> Result<Type, Diagnostic> {
            if let Some(s) = self.document.structs.get(name.as_str()) {
                // Mark the struct's namespace as used
                if let Some(ns) = &s.namespace {
                    self.document.namespaces[ns].used = true;
                }

                Ok(s.ty().cloned().unwrap_or(Type::Union))
            } else {
                let span = name.span();
                self.document.diagnostics.push(unknown_type(
                    name.as_str(),
                    Span::new(span.start() + self.offset, span.len()),
                ));
                Ok(Type::Union)
            }
        }
    }

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
                        document.diagnostics.push(recursive_struct(
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

        let offset = document.structs[index].offset;
        let mut converter = AstTypeConverter::new(Resolver { document, offset });
        let ty = converter
            .convert_struct_type(&definition)
            .expect("struct type conversion should not fail");

        let s = &mut document.structs[index];
        assert!(s.ty.is_none(), "type should not already be present");
        s.ty = Some(ty.into());
    }
}

/// Represents context to an expression type evaluator.
#[derive(Debug)]
struct EvaluationContext<'a> {
    /// The document being evaluated.
    document: &'a mut Document,
    /// The current evaluation scope.
    scope: ScopeRef<'a>,
    /// The diagnostics configuration to use for expression evaluation.
    config: DiagnosticsConfig,
    /// The context of the task being evaluated.
    ///
    /// This is only `Some` when evaluating a task's `hints` section.`
    task: Option<&'a Task>,
}

impl<'a> EvaluationContext<'a> {
    /// Constructs a new expression type evaluation context.
    pub fn new(document: &'a mut Document, scope: ScopeRef<'a>, config: DiagnosticsConfig) -> Self {
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
        document: &'a mut Document,
        scope: ScopeRef<'a>,
        config: DiagnosticsConfig,
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

    fn resolve_name(&self, name: &Ident) -> Option<Type> {
        self.scope.lookup(name.as_str()).map(|n| n.ty().clone())
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

                s.ty().expect("struct should have type").clone()
            })
            .ok_or_else(|| unknown_type(name.as_str(), name.span()))
    }

    fn task(&self) -> Option<&Task> {
        self.task
    }

    fn diagnostics_config(&self) -> DiagnosticsConfig {
        self.config
    }

    fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.document.diagnostics.push(diagnostic);
    }
}

/// Performs a type check of an expression.
fn type_check_expr(
    config: DiagnosticsConfig,
    document: &mut Document,
    scope: ScopeRef<'_>,
    expr: &Expr,
    expected: &Type,
    expected_span: Span,
) {
    let mut context = EvaluationContext::new(document, scope, config);
    let mut evaluator = ExprTypeEvaluator::new(&mut context);
    let actual = evaluator.evaluate_expr(expr).unwrap_or(Type::Union);

    if !matches!(expected, Type::Union) && !actual.is_coercible_to(expected) {
        document
            .diagnostics
            .push(type_mismatch(expected, expected_span, &actual, expr.span()));
    }
    // Check to see if we're assigning an empty array literal to a non-empty type; we can statically
    // flag these as errors; otherwise, non-empty array constraints are checked at runtime
    else if let Type::Compound(CompoundType::Array(ty), _) = expected {
        if ty.is_non_empty() && expr.is_empty_array_literal() {
            document
                .diagnostics
                .push(non_empty_array_assignment(expected_span, expr.span()));
        }
    }
}
