//! Conversion of a V1 AST to an analyzed document.
use std::collections::HashMap;
use std::collections::HashSet;
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
use url::Url;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxNode;
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
use wdl_ast::v1::ImportStatement;
use wdl_ast::v1::LiteralExpr;
use wdl_ast::v1::ScatterStatement;
use wdl_ast::v1::StringPart;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::TypeRef;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::version::V1;

use super::Document;
use super::DocumentData;
use super::Enum;
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
use crate::Exceptable;
use crate::UNUSED_CALL_RULE_ID;
use crate::UNUSED_DECL_RULE_ID;
use crate::UNUSED_IMPORT_RULE_ID;
use crate::UNUSED_INPUT_RULE_ID;
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
use crate::diagnostics::missing_call_input;
use crate::diagnostics::name_conflict;
use crate::diagnostics::namespace_conflict;
use crate::diagnostics::non_empty_array_assignment;
use crate::diagnostics::non_literal_enum_value;
use crate::diagnostics::only_one_namespace;
use crate::diagnostics::recursive_struct;
use crate::diagnostics::recursive_workflow_call;
use crate::diagnostics::struct_conflicts_with_import;
use crate::diagnostics::struct_not_in_document;
use crate::diagnostics::type_is_not_array;
use crate::diagnostics::type_mismatch;
use crate::diagnostics::unknown_call_io;
use crate::diagnostics::unknown_name;
use crate::diagnostics::unknown_namespace;
use crate::diagnostics::unknown_task_or_workflow;
use crate::diagnostics::unknown_type;
use crate::diagnostics::unused_call;
use crate::diagnostics::unused_declaration;
use crate::diagnostics::unused_input;
use crate::document::Name;
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
    document: &mut DocumentData,
    config: &Config,
    graph: &DocumentGraph,
    index: NodeIndex,
    ast: &Ast,
) {
    assert!(
        matches!(
            document.version.expect("document should have a version"),
            SupportedVersion::V1(_)
        ),
        "expected a supported V1 version"
    );

    // First start by processing imports, struct definitions, and enum definitions
    // This needs to be performed before processing tasks and workflows as
    // declarations might reference an imported or locally-defined struct or enum
    for item in ast.items() {
        match item {
            DocumentItem::Import(import) => {
                add_namespace(document, graph, &import, index);
            }
            DocumentItem::Struct(s) => {
                add_struct(document, &s);
            }
            DocumentItem::Enum(e) => {
                add_enum(document, &e);
            }
            DocumentItem::Task(_) | DocumentItem::Workflow(_) => {
                continue;
            }
        }
    }

    // Populate the struct types now that all structs have been processed
    set_struct_types(document);

    // Populate the enum types now that all enums have been processed
    set_enum_types(document);

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
            DocumentItem::Import(_) | DocumentItem::Struct(_) | DocumentItem::Enum(_) => {
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
    document: &mut DocumentData,
    graph: &DocumentGraph,
    import: &ImportStatement,
    importer_index: NodeIndex,
) {
    // Start by resolving the import to its document
    let (uri, imported) = match resolve_import(graph, import, importer_index) {
        Ok(resolved) => resolved,
        Err(Some(diagnostic)) => {
            document.analysis_diagnostics.push(diagnostic);
            return;
        }
        Err(None) => return,
    };

    // Check for conflicting namespaces
    let span = import.uri().span();
    let ns = match import.namespace() {
        Some((ns, span)) => match document.namespaces.get(&ns) {
            Some(prev) => {
                document.analysis_diagnostics.push(namespace_conflict(
                    &ns,
                    span,
                    prev.span,
                    import.explicit_namespace().is_none(),
                ));
                return;
            }
            None => {
                document.namespaces.insert(
                    ns.clone(),
                    Namespace {
                        span,
                        source: uri.clone(),
                        document: imported.clone(),
                        used: false,
                        excepted: import.inner().is_rule_excepted(UNUSED_IMPORT_RULE_ID),
                    },
                );
                ns
            }
        },
        None => {
            // Invalid import namespaces are caught during validation, so there is already a
            // diagnostic for this issue; ignore the import here
            return;
        }
    };

    // Get the alias map for the namespace (for structs)
    let aliases = import
        .aliases()
        .filter_map(|a| {
            let (from, to) = a.names();
            if !imported.data.structs.contains_key(from.text()) {
                document
                    .analysis_diagnostics
                    .push(struct_not_in_document(&from));
                return None;
            }

            Some((from.text().to_string(), to))
        })
        .collect::<HashMap<_, _>>();

    // Insert the imported document's struct definitions
    for (name, s) in &imported.data.structs {
        let (span, aliased_name, aliased) = aliases
            .get(name)
            .map(|n| (n.span(), n.text(), true))
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
                        document
                            .analysis_diagnostics
                            .push(struct_conflicts_with_import(
                                aliased_name,
                                prev.name_span,
                                span,
                            ));
                    } else {
                        document.analysis_diagnostics.push(imported_struct_conflict(
                            aliased_name,
                            span,
                            prev.name_span,
                            !aliased,
                        ));
                    }
                    continue;
                }
            }
            None => {
                document.structs.insert(
                    aliased_name.to_string(),
                    Struct {
                        name_span: span,
                        name: aliased_name.to_string(),
                        offset: s.offset,
                        node: s.node.clone(),
                        namespace: Some(ns.clone()),
                        ty: s.ty.clone(),
                    },
                );
            }
        }
    }

    // Get the alias map for the namespace (for enums)
    let aliases = import
        .aliases()
        .filter_map(|a| {
            let (from, to) = a.names();
            if !imported.data.enums.contains_key(from.text()) {
                return None;
            }

            Some((from.text().to_string(), to))
        })
        .collect::<HashMap<_, _>>();

    // Insert the imported document's enum definitions
    for (name, e) in &imported.data.enums {
        let (span, aliased_name, aliased) = aliases
            .get(name)
            .map(|n| (n.span(), n.text(), true))
            .unwrap_or_else(|| (span, name, false));
        match document.enums.get(aliased_name) {
            Some(prev) => {
                let a = prev.definition();
                let b = e.definition();
                if !are_enums_equal(&a, &b) {
                    // Import conflicts with an enum defined in this document
                    if prev.namespace.is_none() {
                        document
                            .analysis_diagnostics
                            .push(enum_conflicts_with_import(
                                aliased_name,
                                prev.name_span,
                                span,
                            ));
                    } else {
                        document.analysis_diagnostics.push(imported_enum_conflict(
                            aliased_name,
                            span,
                            prev.name_span,
                            !aliased,
                        ));
                    }
                    continue;
                }
            }
            None => {
                document.enums.insert(
                    aliased_name.to_string(),
                    Enum {
                        name_span: span,
                        name: aliased_name.to_string(),
                        offset: e.offset,
                        node: e.node.clone(),
                        namespace: Some(ns.clone()),
                        ty: e.ty.clone(),
                    },
                );
            }
        }
    }
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

    for result in a.variants().zip_longest(b.variants()) {
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

/// Adds a struct to the document.
fn add_struct(document: &mut DocumentData, definition: &StructDefinition) {
    let name = definition.name();

    // Check for a conflict with imported struct first otherwise for any name
    if let Some(prev) = document.structs.get(name.text())
        && prev.namespace.is_some()
    {
        let prev_def = StructDefinition::cast(SyntaxNode::new_root(prev.node.clone()))
            .expect("node should cast");

        if !are_structs_equal(definition, &prev_def) {
            document
                .analysis_diagnostics
                .push(struct_conflicts_with_import(
                    name.text(),
                    name.span(),
                    prev.name_span,
                ));
            return;
        }
    } else if let Some(ctx) = document.context(name.text()) {
        document.analysis_diagnostics.push(name_conflict(
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
                document.analysis_diagnostics.push(name_conflict(
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

    document.structs.insert(
        name.text().to_string(),
        Struct {
            name_span: name.span(),
            name: name.text().to_string(),
            namespace: None,
            offset: definition.span().start(),
            node: definition.inner().green().into(),
            ty: None,
        },
    );
}

/// Adds an enum definition to the document.
fn add_enum(document: &mut DocumentData, definition: &EnumDefinition) {
    let name = definition.name();

    // Check if enums are supported in this version
    let version = document.version.expect("should have version");
    if version < SupportedVersion::V1(V1::Three) {
        document
            .analysis_diagnostics
            .push(enum_not_supported(version, definition.name().span()));
        return;
    }

    // Check for a conflict with imported enum first otherwise for any name
    if let Some(prev) = document.enums.get(name.text())
        && prev.namespace.is_some()
    {
        let prev_def = prev.definition();
        if !are_enums_equal(definition, &prev_def) {
            document
                .analysis_diagnostics
                .push(enum_conflicts_with_import(
                    name.text(),
                    name.span(),
                    prev.name_span,
                ))
        }
    } else if let Some(ctx) = document.context(name.text()) {
        document.analysis_diagnostics.push(name_conflict(
            name.text(),
            Context::Enum(name.span()),
            ctx,
        ));
        return;
    }

    // Ensure there are no duplicate variants
    let mut variants = IndexMap::new();
    for variant in definition.variants() {
        let name = variant.name();
        match variants.get(name.text()) {
            Some(prev_span) => {
                document.analysis_diagnostics.push(name_conflict(
                    name.text(),
                    Context::EnumVariant(name.span()),
                    Context::EnumVariant(*prev_span),
                ));
            }
            _ => {
                variants.insert(name.text().to_string(), name.span());
            }
        }
    }

    document.enums.insert(
        name.text().to_string(),
        Enum {
            name_span: name.span(),
            name: name.text().to_string(),
            namespace: None,
            offset: definition.span().start(),
            node: definition.inner().green().into(),
            ty: None,
        },
    );
}

/// Converts an AST type to an analysis type.
fn convert_ast_type(document: &mut DocumentData, ty: &wdl_ast::v1::Type) -> Type {
    /// Used to resolve a type name from a document.
    struct Resolver<'a>(&'a mut DocumentData);

    impl TypeNameResolver for Resolver<'_> {
        fn resolve(&mut self, name: &str, span: Span) -> Result<Type, Diagnostic> {
            if let Some(s) = self.0.structs.get(name) {
                if let Some(ns) = &s.namespace {
                    self.0.namespaces[ns].used = true;
                }
                return Ok(s.ty().expect("struct should have type").clone());
            }

            if let Some(e) = self.0.enums.get(name)
                // Ensure the inner type has been successfully calculated
                && e.ty().is_some()
            {
                if let Some(ns) = &e.namespace {
                    self.0.namespaces[ns].used = true;
                }

                // SAFETY: we just checked to make sure the type was
                // successfully calculated above.
                return Ok(e.ty().unwrap().clone());
            }

            Err(unknown_type(name, span))
        }
    }

    let mut converter = AstTypeConverter::new(Resolver(document));
    match converter.convert_type(ty) {
        Ok(ty) => ty,
        Err(diagnostic) => {
            document.analysis_diagnostics.push(diagnostic);
            Type::Union
        }
    }
}

/// Creates an input type map.
fn create_input_type_map(
    document: &mut DocumentData,
    declarations: impl Iterator<Item = Decl>,
) -> Arc<IndexMap<String, Input>> {
    let mut map = IndexMap::new();
    for decl in declarations {
        let name = decl.name();
        if map.contains_key(name.text()) {
            // Ignore the duplicate
            continue;
        }

        let ty = convert_ast_type(document, &decl.ty());
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
) -> Arc<IndexMap<String, Output>> {
    let mut map = IndexMap::new();
    for decl in declarations {
        let name = decl.name();
        if map.contains_key(name.text()) {
            // Ignore the duplicate
            continue;
        }

        let ty = convert_ast_type(document, &decl.ty());
        map.insert(name.text().to_string(), Output::new(ty, name.span()));
    }

    map.into()
}

/// Adds a task to the document.
fn add_task(config: &Config, document: &mut DocumentData, definition: &TaskDefinition) {
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

    // Check for a name conflict
    let name = definition.name();
    if let Some(ctx) = document.context(name.text()) {
        document.analysis_diagnostics.push(name_conflict(
            name.text(),
            Context::Task(name.span()),
            ctx,
        ));
        return;
    }

    // Populate type maps for the tasks's inputs and outputs
    let inputs = match definition.input() {
        Some(section) => create_input_type_map(document, section.declarations()),
        None => Default::default(),
    };
    let outputs = match definition.output() {
        Some(section) => create_output_type_map(document, section.declarations().map(Decl::Bound)),
        None => Default::default(),
    };

    // Process the task in evaluation order
    let graph = TaskGraphBuilder::default().build(
        document.version.unwrap(),
        definition,
        &mut document.analysis_diagnostics,
        |name| document.structs.contains_key(name) || document.enums.contains_key(name),
    );

    let mut task = Task {
        name_span: name.span(),
        name: name.text().to_string(),
        scopes: vec![Scope::new(
            None,
            definition
                .braced_scope_span()
                .expect("should have brace scope span"),
        )],
        inputs,
        outputs,
    };

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

                    if let (Some(true), None) = (edges.next().map(|e| e.weight()), edges.next()) {
                        let name = decl.name();

                        // Determine if the input is really used based on its name and type
                        if is_input_used(name.text(), &task.inputs[name.text()].ty) {
                            continue;
                        }

                        if !decl.inner().is_rule_excepted(UNUSED_INPUT_RULE_ID) {
                            document.analysis_diagnostics.push(
                                unused_input(name.text(), name.span()).with_severity(severity),
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
                if let Some(severity) = config.diagnostics_config().unused_declaration {
                    let name = decl.name();
                    // Don't warn for environment variables as they are always implicitly used
                    if decl.env().is_none()
                        && graph
                            .edges_directed(index, Direction::Outgoing)
                            .next()
                            .is_none()
                        && !decl.inner().is_rule_excepted(UNUSED_DECL_RULE_ID)
                    {
                        document.analysis_diagnostics.push(
                            unused_declaration(name.text(), name.span()).with_severity(severity),
                        );
                    }
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
                            .braced_scope_span()
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
                        section.heredoc_scope_span()
                    } else {
                        section.braced_scope_span()
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
                            .braced_scope_span()
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
                            .braced_scope_span()
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
                            .braced_scope_span()
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
    document.tasks.insert(name.text().to_string(), task);
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
fn add_workflow(document: &mut DocumentData, workflow: &WorkflowDefinition) -> bool {
    // Check for duplicate workflow first
    let name = workflow.name();
    if let Some(prev) = &document.workflow {
        document
            .analysis_diagnostics
            .push(duplicate_workflow(&name, prev.name_span));
        return false;
    }

    // Check for a name conflict
    if let Some(ctx) = document.context(name.text()) {
        document.analysis_diagnostics.push(name_conflict(
            name.text(),
            Context::Workflow(name.span()),
            ctx,
        ));
        return false;
    }

    // Note: we delay populating the workflow until later on so that we can populate
    // all tasks in the document first; it is done this way so we can resolve local
    // task call targets.

    document.workflow = Some(Workflow {
        name_span: name.span(),
        name: name.text().to_string(),
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
fn populate_workflow(config: &Config, document: &mut DocumentData, workflow: &WorkflowDefinition) {
    // Populate type maps for the workflow's inputs and outputs
    let inputs = match workflow.input() {
        Some(section) => create_input_type_map(document, section.declarations()),
        None => Default::default(),
    };
    let outputs = match workflow.output() {
        Some(section) => create_output_type_map(document, section.declarations().map(Decl::Bound)),
        None => Default::default(),
    };

    // Keep a map of scopes from syntax node that introduced the scope to the scope
    // index
    let mut scope_indexes: HashMap<SyntaxNode, ScopeIndex> = HashMap::new();
    let mut scopes = vec![Scope::new(
        None,
        workflow
            .braced_scope_span()
            .expect("should have braced scope span"),
    )];
    let mut output_scope = None;

    // For static analysis, we don't need to provide inputs to the workflow graph
    // builder
    let graph = WorkflowGraphBuilder::default().build(
        workflow,
        &mut document.analysis_diagnostics,
        |_| false,
        |name| document.structs.contains_key(name) || document.enums.contains_key(name),
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
                if let Some(severity) = config.diagnostics_config().unused_input {
                    let name = decl.name();
                    if graph
                        .edges_directed(index, Direction::Outgoing)
                        .next()
                        .is_none()
                    {
                        // Determine if the input is really used based on its name and type
                        if is_input_used(name.text(), &inputs[name.text()].ty) {
                            continue;
                        }

                        if !decl.inner().is_rule_excepted(UNUSED_INPUT_RULE_ID) {
                            document.analysis_diagnostics.push(
                                unused_input(name.text(), name.span()).with_severity(severity),
                            );
                        }
                    }
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
                    |doc, _, decl| convert_ast_type(doc, &decl.ty()),
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
                        && !decl.inner().is_rule_excepted(UNUSED_DECL_RULE_ID)
                    {
                        document.analysis_diagnostics.push(
                            unused_declaration(name.text(), name.span()).with_severity(severity),
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
                            workflow
                                .output()
                                .expect("should have output section")
                                .braced_scope_span()
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
                    workflow.name().text(),
                    ScopeRefMut::new(&mut scopes, scope_index),
                    &statement,
                    document
                        .workflow
                        .as_ref()
                        .expect("should have workflow")
                        .allows_nested_inputs,
                    graph
                        .edges_directed(index, Direction::Outgoing)
                        .next()
                        .is_some(),
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
                                    document.analysis_diagnostics.push(name_conflict(
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
    let workflow = document.workflow.as_mut().expect("expected a workflow");
    workflow.scopes = scopes;
    workflow.inputs = inputs;
    workflow.outputs = outputs;
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
                        .push(else_if_not_supported(version, span));
                }
                ConditionalStatementClauseKind::Else => {
                    let span = clause
                        .else_keyword()
                        .expect("should have `else` keyword")
                        .span();
                    document
                        .analysis_diagnostics
                        .push(else_not_supported(version, span));
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
                    .braced_scope_span()
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
                .push(if_conditional_mismatch(&ty, expr.span()));
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
                .braced_scope_span()
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
                .push(type_is_not_array(&ty, expr.span()));
            Type::Union
        }
    };

    // Introduce the scatter variable into the scope
    let variable = statement.variable();
    scopes[scope_index.0].insert(variable.text().to_string(), variable.span(), element_ty);
}

/// Adds a call statement to the current scope.
fn add_call_statement(
    config: &Config,
    document: &mut DocumentData,
    workflow_name: &str,
    mut scope: ScopeRefMut<'_>,
    statement: &CallStatement,
    nested_inputs_allowed: bool,
    is_used: bool,
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

    let ty = match resolve_call_type(document, workflow_name, statement) {
        Some(ty) => {
            // Type check the call inputs
            let mut seen = HashSet::new();
            for input in statement.inputs() {
                let input_name = input.name();

                let (expected_ty, required) = ty
                    .inputs()
                    .get(input_name.text())
                    .map(|i| (i.ty.clone(), i.required))
                    .unwrap_or_else(|| {
                        document.analysis_diagnostics.push(unknown_call_io(
                            &ty,
                            &input_name,
                            Io::Input,
                        ));
                        (Type::Union, true)
                    });

                match input.expr() {
                    Some(expr) => {
                        // For WDL 1.2, we accept optional types for the input even if the input's
                        // type is non-optional; if the runtime value is `None` for a non-optional
                        // input, the default expression will be evaluated instead.
                        let expected_ty = if !required
                            && document
                                .version
                                .map(|v| v >= SupportedVersion::V1(V1::Two))
                                .unwrap_or(false)
                        {
                            expected_ty.optional()
                        } else {
                            expected_ty
                        };

                        type_check_expr(
                            config,
                            document,
                            scope.as_scope_ref(),
                            &expr,
                            &expected_ty,
                            input_name.span(),
                        );
                    }
                    None => match scope.lookup(input_name.text()) {
                        Some(name) => {
                            if !matches!(expected_ty, Type::Union)
                                && !name.ty.is_coercible_to(&expected_ty)
                            {
                                document.analysis_diagnostics.push(call_input_type_mismatch(
                                    &input_name,
                                    &expected_ty,
                                    &name.ty,
                                ));
                            }
                        }
                        None => {
                            document
                                .analysis_diagnostics
                                .push(unknown_name(input_name.text(), input_name.span()));
                        }
                    },
                }

                seen.insert(input_name.hashable());
            }

            for (name, input) in ty.inputs() {
                if input.required && !seen.contains(name.as_str()) {
                    document.analysis_diagnostics.push(missing_call_input(
                        ty.kind(),
                        &target_name,
                        name,
                        nested_inputs_allowed,
                    ));
                }
            }

            // Add the call to the workflow
            let calls = &mut document
                .workflow
                .as_mut()
                .expect("should have workflow")
                .calls;
            if !calls.contains_key(name.text()) {
                calls.insert(name.text().to_string(), ty.clone());
            }

            ty.into()
        }
        _ => Type::Union,
    };

    // Don't modify the scope if there's a conflict
    if scope.lookup(name.text()).is_none() {
        // Check for unused call
        if let Some(severity) = config.diagnostics_config().unused_call
            && !is_used
            && !statement.inner().is_rule_excepted(UNUSED_CALL_RULE_ID)
            && let Some(ty) = ty.as_call()
            && !ty.outputs().is_empty()
        {
            document
                .analysis_diagnostics
                .push(unused_call(name.text(), name.span()).with_severity(severity));
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
) -> Option<CallType> {
    let target = statement.target();
    let mut targets = target.names().peekable();
    let mut namespace = None;
    let mut name = None;
    while let Some(target) = targets.next() {
        if targets.peek().is_none() {
            name = Some(target);
            break;
        }

        if namespace.is_some() {
            document
                .analysis_diagnostics
                .push(only_one_namespace(target.span()));
            return None;
        }

        match document.namespaces.get_mut(target.text()) {
            Some(ns) => {
                ns.used = true;
                namespace = Some(&document.namespaces[target.text()])
            }
            None => {
                document
                    .analysis_diagnostics
                    .push(unknown_namespace(&target));
                return None;
            }
        }
    }

    let target = namespace
        .map(|ns| ns.document.data.as_ref())
        .unwrap_or(document);
    let name = name.expect("should have name");
    if namespace.is_none() && name.text() == workflow_name {
        document
            .analysis_diagnostics
            .push(recursive_workflow_call(name.text(), name.span()));
        return None;
    }

    let (kind, inputs, outputs) = match target.tasks.get(name.text()) {
        Some(task) => (CallKind::Task, task.inputs.clone(), task.outputs.clone()),
        _ => match &target.workflow {
            Some(workflow) if workflow.name == name.text() => (
                CallKind::Workflow,
                workflow.inputs.clone(),
                workflow.outputs.clone(),
            ),
            _ => {
                document.analysis_diagnostics.push(unknown_task_or_workflow(
                    namespace.map(|ns| ns.span),
                    name.text(),
                    name.span(),
                ));
                return None;
            }
        },
    };

    let specified = Arc::new(
        statement
            .inputs()
            .map(|i| i.name().text().to_string())
            .collect(),
    );

    if namespace.is_some() {
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
fn resolve_import(
    graph: &DocumentGraph,
    stmt: &ImportStatement,
    importer_index: NodeIndex,
) -> Result<(Arc<Url>, Document), Option<Diagnostic>> {
    let uri = stmt.uri();
    let span = uri.span();
    let text = match uri.text() {
        Some(text) => text,
        None => {
            // The import URI isn't valid; this is caught at validation time, so we do not
            // emit any additional diagnostics for it here.
            return Err(None);
        }
    };

    let importer_node = graph.get(importer_index);
    let uri = match importer_node.uri().join(text.text()) {
        Ok(uri) => uri,
        Err(e) => return Err(Some(invalid_relative_import(&e, span))),
    };

    let imported_index = graph.get_index(&uri).expect("missing import node in graph");
    let imported_node = graph.get(imported_index);

    // Check for an import cycle to report
    if graph.contains_cycle(importer_index, imported_index) {
        return Err(Some(import_cycle(span)));
    }

    // Check for a failure to load the import
    if let ParseState::Error(e) = imported_node.parse_state() {
        return Err(Some(import_failure(text.text(), e, span)));
    }

    // Check for analysis error
    if let Some(e) = imported_node.analysis_error() {
        return Err(Some(import_failure(text.text(), e, span)));
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
    if !imported_version.has_same_major_version(*importer_version) {
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

    Ok((imported_node.uri().clone(), imported_document))
}

/// Sets the struct types in the document.
fn set_struct_types(document: &mut DocumentData) {
    /// Used to resolve a type name from a document.
    struct Resolver<'a> {
        /// The document to resolve the type name from.
        document: &'a mut DocumentData,
        /// The offset to use to adjust the start of diagnostics.
        offset: usize,
    }

    impl TypeNameResolver for Resolver<'_> {
        fn resolve(&mut self, name: &str, span: Span) -> Result<Type, Diagnostic> {
            match self.document.structs.get(name) {
                Some(s) => {
                    // Mark the struct's namespace as used
                    if let Some(ns) = &s.namespace {
                        self.document.namespaces[ns].used = true;
                    }

                    Ok(s.ty().cloned().unwrap_or(Type::Union))
                }
                _ => {
                    self.document.analysis_diagnostics.push(unknown_type(
                        name,
                        Span::new(span.start() + self.offset, span.len()),
                    ));
                    Ok(Type::Union)
                }
            }
        }
    }

    if document.structs.is_empty() {
        return;
    }

    /// Recursively finds all nested struct type dependencies to build
    /// dependency graphs
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
    for (from, s) in document.structs.values().enumerate() {
        // Only look at locally defined structs
        if s.namespace.is_some() {
            continue;
        }

        graph.add_node(from);
        let definition: StructDefinition =
            StructDefinition::cast(SyntaxNode::new_root(s.node.clone())).expect("node should cast");
        for member in definition.members() {
            let mut deps = Vec::new();
            find_type_refs(&member.ty(), &mut deps);

            for dep in deps {
                // Add an edge to the referenced struct
                if let Some(to) = document.structs.get_index_of(dep.name().text()) {
                    // Only add an edge to another local struct definition
                    if document.structs[to].namespace.is_some() {
                        continue;
                    }

                    // Check to see if the edge would form a cycle
                    if has_path_connecting(&graph, from, to, Some(&mut space)) {
                        let name = definition.name();
                        let name_span = name.span();
                        let member_span = member.name().span();
                        document.analysis_diagnostics.push(recursive_struct(
                            name.text(),
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

/// Populates enum types for all locally defined enums in the document.
fn set_enum_types(document: &mut DocumentData) {
    if document.enums.is_empty() {
        return;
    }

    // Calculate the underlying type for every enum in the document
    for index in 0..document.enums.len() {
        let e = &document.enums[index];

        // Only look at locally defined enums
        if e.namespace.is_some() {
            continue;
        }

        let definition = e.definition();
        let mut variants = Vec::new();
        let mut variant_spans = Vec::new();

        // Populate the variants and their spans
        for variant in definition.variants() {
            let variant_name = variant.name().text().to_string();
            let variant_type = if let Some(value_expr) = variant.value() {
                // Validate that the value is a literal expression
                match parse_literal_value(&document.structs, &value_expr) {
                    Some(ty) => ty,
                    None => {
                        let span = value_expr.span();
                        let adjusted_span = Span::new(span.start() + e.offset, span.len());
                        document
                            .analysis_diagnostics
                            .push(non_literal_enum_value(adjusted_span));
                        Type::Union
                    }
                }
            } else {
                // Default to `String` type if no value is specified
                PrimitiveType::String.into()
            };

            variants.push((variant_name, variant_type));
            variant_spans.push(Span::new(
                variant.span().start() + e.offset(),
                variant.span().len(),
            ));
        }

        // Check for explicit type parameter or infer if one was not specified
        let result = if let Some(type_param) = definition.type_parameter().map(|t| t.ty()) {
            let type_param = convert_ast_type(document, &type_param);
            let e = &document.enums[index];
            EnumType::new(
                e.name.clone(),
                e.name_span,
                type_param,
                variants,
                &variant_spans,
            )
        } else {
            EnumType::infer(document.enums[index].name.clone(), variants, &variant_spans)
        };

        match result {
            Ok(enum_ty) => {
                document.enums[index].ty = Some(enum_ty.into());
            }
            Err(diagnostic) => {
                document.analysis_diagnostics.push(diagnostic);
            }
        }
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
                Some(Type::Compound(
                    CompoundType::Array(ArrayType::new(element_type)),
                    false,
                ))
            }
            LiteralExpr::Pair(pair) => {
                let (left, right) = pair.exprs();
                Some(Type::Compound(
                    CompoundType::Pair(PairType::new(
                        infer_type_from_literal(&left)?,
                        infer_type_from_literal(&right)?,
                    )),
                    false,
                ))
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
                Some(Type::Compound(
                    CompoundType::Map(MapType::new(key_type, value_type)),
                    false,
                ))
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
/// Returns `None` if the expression is not a valid literal for enum variant
/// values.
fn parse_literal_value(structs: &indexmap::IndexMap<String, Struct>, expr: &Expr) -> Option<Type> {
    // Handle struct literals specially since they need struct definitions
    if let Expr::Literal(LiteralExpr::Struct(s)) = expr {
        for item in s.items() {
            let (_, val_expr) = item.name_value();
            parse_literal_value(structs, &val_expr)?;
        }

        return structs.get(s.name().text()).and_then(|st| st.ty.clone());
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

    fn resolve_name(&self, name: &str, _: Span) -> Option<Type> {
        // Check if there are any variables with this name and return if so.
        if let Some(var) = self.scope.lookup(name).map(|n| n.ty().clone()) {
            return Some(var);
        }

        // If the name is a reference to a struct, return it as a [`Type::TypeNameRef`].
        if let Some(s) = self.document.structs.get(name).and_then(|s| s.ty()) {
            return Some(
                s.type_name_ref()
                    .expect("type name ref to be created from struct"),
            );
        }

        // If the name is a reference to an enum, return it as a [`Type::TypeNameRef`].
        if let Some(e) = self.document.enums.get(name).and_then(|e| e.ty()) {
            return Some(
                e.type_name_ref()
                    .expect("type name ref to be created from enum"),
            );
        }

        None
    }

    fn resolve_type_name(&mut self, name: &str, span: Span) -> Result<Type, Diagnostic> {
        if let Some(s) = self.document.structs.get(name) {
            if let Some(ns) = &s.namespace {
                self.document.namespaces[ns].used = true;
            }

            return Ok(s.ty().expect("struct should have type").clone());
        }

        if let Some(e) = self.document.enums.get(name) {
            if let Some(ns) = &e.namespace {
                self.document.namespaces[ns].used = true;
            }

            return Ok(e.ty().expect("enum should have type").clone());
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
        self.document.analysis_diagnostics.push(diagnostic);
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
        document.analysis_diagnostics.push(type_mismatch(
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
            .push(non_empty_array_assignment(expected_span, expr.span()));
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

    #[test]
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

    #[test]
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
