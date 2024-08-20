//! Conversion of a V1 AST to a document scope.
use std::collections::HashMap;
use std::sync::Arc;

use indexmap::IndexMap;
use petgraph::algo::has_path_connecting;
use petgraph::algo::toposort;
use petgraph::graph::NodeIndex;
use petgraph::prelude::DiGraphMap;
use url::Url;
use wdl_ast::v1;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::Span;
use wdl_ast::ToSpan;
use wdl_ast::Version;

use super::DocumentScope;
use super::NameContext;
use super::Namespace;
use super::Scope;
use super::ScopeContext;
use super::ScopedName;
use super::ScopedNameContext;
use super::Struct;
use super::TaskScope;
use super::WorkflowScope;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::StructType;
use crate::Type;

/// Creates a "name conflict" diagnostic
fn name_conflict(name: &str, conflicting: NameContext, first: NameContext) -> Diagnostic {
    Diagnostic::error(format!("conflicting {conflicting} name `{name}`"))
        .with_label(
            format!("this conflicts with a {first} of the same name"),
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
fn call_conflict(name: &Ident, first: NameContext, suggest_fix: bool) -> Diagnostic {
    let diagnostic = Diagnostic::error(format!(
        "conflicting call name `{name}`",
        name = name.as_str()
    ))
    .with_label(
        format!("this conflicts with a {first} of the same name"),
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
fn recursive_struct(name: &Ident, member: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "struct `{name}` has a recursive definition",
        name = name.as_str()
    ))
    .with_highlight(name.span())
    .with_label("this struct member participates in the recursion", member)
}

impl DocumentScope {
    /// Creates a new document scope for a V1 AST.
    pub(crate) fn from_ast_v1(
        graph: &DocumentGraph,
        index: NodeIndex,
        ast: &v1::Ast,
        version: &Version,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Self {
        let mut scope = DocumentScope::default();
        let mut structs = 0;
        for item in ast.items() {
            match item {
                v1::DocumentItem::Import(import) => {
                    scope.add_namespace_v1(graph, &import, index, version, diagnostics);
                }
                v1::DocumentItem::Struct(s) => {
                    scope.add_struct_v1(&s, structs, diagnostics);
                    structs += 1;
                }
                v1::DocumentItem::Task(task) => {
                    scope.add_task_scope_v1(&task, diagnostics);
                }
                v1::DocumentItem::Workflow(workflow) => {
                    scope.add_workflow_scope_v1(&workflow, diagnostics);
                }
            }
        }

        scope.calculate_types_v1(ast, diagnostics);
        scope
    }

    /// Adds a namespace to the document scope.
    fn add_namespace_v1(
        &mut self,
        graph: &DocumentGraph,
        import: &v1::ImportStatement,
        importer_index: NodeIndex,
        importer_version: &Version,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Start by resolving the import to its document scope
        let (uri, scope) =
            match Self::resolve_import_v1(graph, import, importer_index, importer_version) {
                Ok(scope) => scope,
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
                if let Some(prev) = self.namespaces.get(&ns) {
                    diagnostics.push(namespace_conflict(
                        &ns,
                        span,
                        prev.span,
                        import.explicit_namespace().is_none(),
                    ));
                    return;
                } else {
                    self.namespaces.insert(
                        ns.clone(),
                        Namespace {
                            span,
                            node: import.syntax().green().into(),
                            source: uri.clone(),
                            scope: scope.clone(),
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
                if !scope.structs.contains_key(from.as_str()) {
                    diagnostics.push(struct_not_in_scope(&from));
                    return None;
                }

                Some((from.as_str().to_string(), to))
            })
            .collect::<HashMap<_, _>>();

        // Insert the scope's struct definitions
        for (name, s) in &scope.structs {
            let namespace = self.namespaces.get(&ns).unwrap();
            let (span, aliased_name, aliased) = aliases
                .get(name)
                .map(|n| (n.span(), n.as_str(), true))
                .unwrap_or_else(|| (span, name, false));
            match self.structs.get(aliased_name) {
                Some(prev) => {
                    // Import conflicts with a struct defined in this document
                    if prev.namespace.is_none() {
                        diagnostics.push(struct_conflicts_with_import(
                            aliased_name,
                            prev.span,
                            span,
                        ));
                        continue;
                    }

                    if !prev.is_equal(s) {
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
                    self.structs.insert(
                        aliased_name.to_string(),
                        Struct {
                            span,
                            namespace: Some(ns.clone()),
                            node: s.node.clone(),
                            ty: s.ty.map(|ty| self.types.import(&namespace.scope.types, ty)),
                            index: None,
                        },
                    );
                }
            }
        }
    }

    /// Adds a struct to the document scope.
    fn add_struct_v1(
        &mut self,
        definition: &v1::StructDefinition,
        index: usize,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let name = definition.name();
        if let Some(prev) = self.structs.get(name.as_str()) {
            if prev.namespace.is_some() {
                diagnostics.push(struct_conflicts_with_import(
                    name.as_str(),
                    name.span(),
                    prev.span,
                ))
            } else {
                diagnostics.push(name_conflict(
                    name.as_str(),
                    NameContext::Struct(name.span()),
                    NameContext::Struct(prev.span),
                ));
            }
        } else {
            // Ensure there are no duplicate members
            let mut members = IndexMap::new();
            for decl in definition.members() {
                let name = decl.name();
                if let Some(prev_span) = members.get(name.as_str()) {
                    diagnostics.push(name_conflict(
                        name.as_str(),
                        NameContext::StructMember(name.span()),
                        NameContext::StructMember(*prev_span),
                    ));
                } else {
                    members.insert(name.as_str().to_string(), name.span());
                }
            }

            self.structs.insert(
                name.as_str().to_string(),
                Struct {
                    span: name.span(),
                    namespace: None,
                    node: definition.syntax().green().into(),
                    ty: None,
                    index: Some(index),
                },
            );
        }
    }

    /// Adds inputs to a names collection.
    fn add_inputs(
        names: &mut IndexMap<String, ScopedName>,
        section: &v1::InputSection,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for decl in section.declarations() {
            let name = decl.name();
            let context = ScopedNameContext::Input(name.span());
            if let Some(prev) = names.get(name.as_str()) {
                diagnostics.push(name_conflict(
                    name.as_str(),
                    context.into(),
                    prev.context().into(),
                ));
                continue;
            }

            names.insert(
                name.as_str().to_string(),
                ScopedName::new(context, decl.syntax().green().into(), false),
            );
        }
    }

    /// Adds outputs to a names collection.
    fn add_outputs_v1(
        names: &mut IndexMap<String, ScopedName>,
        section: &v1::OutputSection,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for decl in section.declarations() {
            let name = decl.name();
            let context = ScopedNameContext::Output(name.span());
            if let Some(prev) = names.get(name.as_str()) {
                diagnostics.push(name_conflict(
                    name.as_str(),
                    context.into(),
                    prev.context().into(),
                ));
                continue;
            }

            names.insert(
                name.as_str().to_string(),
                ScopedName::new(context, decl.syntax().green().into(), false),
            );
        }
    }

    /// Adds a task scope to the document's scope.
    fn add_task_scope_v1(&mut self, task: &v1::TaskDefinition, diagnostics: &mut Vec<Diagnostic>) {
        // Check for a conflict with another task or workflow
        let name = task.name();
        if let Some(s) = self.tasks.get(name.as_str()) {
            diagnostics.push(name_conflict(
                name.as_str(),
                NameContext::Task(name.span()),
                NameContext::Task(s.name_span),
            ));
            return;
        } else if let Some(s) = &self.workflow {
            if s.name == name.as_str() {
                diagnostics.push(name_conflict(
                    name.as_str(),
                    NameContext::Task(name.span()),
                    NameContext::Workflow(s.name_span),
                ));
                return;
            }
        }

        // Populate the scope's names
        let mut names: IndexMap<_, ScopedName> = IndexMap::new();
        let mut saw_input = false;
        let mut saw_output = false;
        for item in task.items() {
            match item {
                v1::TaskItem::Input(section) if !saw_input => {
                    saw_input = true;
                    Self::add_inputs(&mut names, &section, diagnostics);
                }
                v1::TaskItem::Output(section) if !saw_output => {
                    saw_output = true;
                    Self::add_outputs_v1(&mut names, &section, diagnostics);
                }
                v1::TaskItem::Declaration(decl) => {
                    let name = decl.name();
                    let context = ScopedNameContext::Decl(name.span());
                    if let Some(prev) = names.get(name.as_str()) {
                        diagnostics.push(name_conflict(
                            name.as_str(),
                            context.into(),
                            prev.context().into(),
                        ));
                        continue;
                    }

                    names.insert(
                        name.as_str().to_string(),
                        ScopedName::new(context, decl.syntax().green().into(), false),
                    );
                }
                v1::TaskItem::Input(_)
                | v1::TaskItem::Output(_)
                | v1::TaskItem::Command(_)
                | v1::TaskItem::Requirements(_)
                | v1::TaskItem::Hints(_)
                | v1::TaskItem::Runtime(_)
                | v1::TaskItem::Metadata(_)
                | v1::TaskItem::ParameterMetadata(_) => continue,
            }
        }

        let span = Self::scope_span(task.syntax());
        let (index, _) = self.tasks.insert_full(
            name.as_str().to_string(),
            TaskScope {
                name_span: name.span(),
                scope: Scope {
                    span,
                    node: task.syntax().green().into(),
                    names,
                    children: Default::default(),
                },
            },
        );

        self.scopes.push((span, ScopeContext::Task(index)));
    }

    /// Adds a workflow scope to the document scope.
    fn add_workflow_scope_v1(
        &mut self,
        workflow: &v1::WorkflowDefinition,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Check for conflicts with task names or an existing workspace
        let name = workflow.name();
        if let Some(s) = self.tasks.get(name.as_str()) {
            diagnostics.push(name_conflict(
                name.as_str(),
                NameContext::Workflow(name.span()),
                NameContext::Task(s.name_span),
            ));
            return;
        } else if let Some(s) = &self.workflow {
            diagnostics.push(duplicate_workflow(&name, s.name_span));
            return;
        }

        // First populate the "root" scope
        let mut scopes = vec![Scope {
            span: Self::scope_span(workflow.syntax()),
            node: workflow.syntax().green().into(),
            names: Default::default(),
            children: Default::default(),
        }];

        let mut saw_input = false;
        let mut saw_output = false;
        for item in workflow.items() {
            match item {
                v1::WorkflowItem::Input(section) if !saw_input => {
                    saw_input = true;
                    let scope = scopes.last_mut().unwrap();
                    Self::add_inputs(&mut scope.names, &section, diagnostics);
                }
                v1::WorkflowItem::Output(section) if !saw_output => {
                    saw_output = true;
                    let scope = scopes.last_mut().unwrap();
                    Self::add_outputs_v1(&mut scope.names, &section, diagnostics);
                }
                v1::WorkflowItem::Declaration(decl) => {
                    Self::add_workflow_statement_decls_v1(
                        &v1::WorkflowStatement::Declaration(decl),
                        &mut scopes,
                        diagnostics,
                    );
                }
                v1::WorkflowItem::Conditional(stmt) => {
                    Self::add_workflow_statement_decls_v1(
                        &v1::WorkflowStatement::Conditional(stmt),
                        &mut scopes,
                        diagnostics,
                    );
                }
                v1::WorkflowItem::Scatter(stmt) => {
                    Self::add_workflow_statement_decls_v1(
                        &v1::WorkflowStatement::Scatter(stmt),
                        &mut scopes,
                        diagnostics,
                    );
                }
                v1::WorkflowItem::Call(stmt) => {
                    Self::add_workflow_statement_decls_v1(
                        &v1::WorkflowStatement::Call(stmt),
                        &mut scopes,
                        diagnostics,
                    );
                }
                v1::WorkflowItem::Input(_)
                | v1::WorkflowItem::Output(_)
                | v1::WorkflowItem::Metadata(_)
                | v1::WorkflowItem::ParameterMetadata(_)
                | v1::WorkflowItem::Hints(_) => continue,
            }
        }

        let scope = scopes.pop().unwrap();
        let span = scope.span;
        self.workflow = Some(WorkflowScope {
            name_span: name.span(),
            name: name.as_str().to_string(),
            scope,
        });
        self.scopes.push((span, ScopeContext::Workflow));
    }

    /// Adds declarations from workflow statements.
    fn add_workflow_statement_decls_v1(
        stmt: &v1::WorkflowStatement,
        scopes: &mut Vec<Scope>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        /// Finds a name by walking up the scope stack
        fn find_name<'a>(name: &str, scopes: &'a [Scope]) -> Option<&'a ScopedName> {
            for scope in scopes.iter().rev() {
                if let Some(name) = scope.names.get(name) {
                    return Some(name);
                }
            }

            None
        }

        match stmt {
            v1::WorkflowStatement::Conditional(stmt) => {
                scopes.push(Scope {
                    span: Self::scope_span(stmt.syntax()),
                    node: stmt.syntax().green().into(),
                    names: Default::default(),
                    children: Default::default(),
                });

                for stmt in stmt.statements() {
                    Self::add_workflow_statement_decls_v1(&stmt, scopes, diagnostics);
                }

                let scope = scopes.pop().unwrap();
                let parent = scopes.last_mut().unwrap();
                for (name, descendant) in &scope.names {
                    parent.names.insert(
                        name.clone(),
                        ScopedName::new(descendant.context, descendant.node.clone(), true),
                    );
                }

                parent.children.push(scope);
            }
            v1::WorkflowStatement::Scatter(stmt) => {
                let variable = stmt.variable();
                let context = ScopedNameContext::ScatterVariable(variable.span());
                let mut names = IndexMap::new();
                if let Some(prev) = find_name(variable.as_str(), scopes) {
                    diagnostics.push(name_conflict(
                        variable.as_str(),
                        context.into(),
                        prev.context().into(),
                    ));
                } else {
                    names.insert(
                        variable.as_str().to_string(),
                        ScopedName::new(context, stmt.syntax().green().into(), false),
                    );
                }

                scopes.push(Scope {
                    span: Self::scope_span(stmt.syntax()),
                    node: stmt.syntax().green().into(),
                    names,
                    children: Default::default(),
                });

                for stmt in stmt.statements() {
                    Self::add_workflow_statement_decls_v1(&stmt, scopes, diagnostics);
                }

                let scope = scopes.pop().unwrap();
                let parent = scopes.last_mut().unwrap();
                for (name, descendant) in &scope.names {
                    // Don't add an implicit name to the parent for the scatter variable
                    if descendant.is_scatter_variable() {
                        continue;
                    }

                    parent.names.insert(
                        name.clone(),
                        ScopedName::new(descendant.context, descendant.node.clone(), true),
                    );
                }

                parent.children.push(scope);
            }
            v1::WorkflowStatement::Call(stmt) => {
                let name = stmt.alias().map(|a| a.name()).unwrap_or_else(|| {
                    stmt.target()
                        .names()
                        .last()
                        .expect("expected a last call target name")
                });
                if let Some(prev) = find_name(name.as_str(), scopes) {
                    diagnostics.push(call_conflict(
                        &name,
                        prev.context().into(),
                        stmt.alias().is_none(),
                    ));

                    // Define the name in this scope if it conflicted with a scatter variable
                    if !prev.is_scatter_variable() {
                        return;
                    }
                }

                scopes.last_mut().unwrap().names.insert(
                    name.as_str().to_string(),
                    ScopedName::new(
                        ScopedNameContext::Call(name.span()),
                        stmt.syntax().green().into(),
                        false,
                    ),
                );
            }
            v1::WorkflowStatement::Declaration(decl) => {
                let name = decl.name();
                let context = ScopedNameContext::Decl(name.span());
                if let Some(prev) = find_name(name.as_str(), scopes) {
                    diagnostics.push(name_conflict(
                        name.as_str(),
                        context.into(),
                        prev.context().into(),
                    ));

                    // Define the name in this scope if it conflicted with a scatter variable
                    if !prev.is_scatter_variable() {
                        return;
                    }
                }

                scopes.last_mut().unwrap().names.insert(
                    name.as_str().to_string(),
                    ScopedName::new(context, decl.syntax().green().into(), false),
                );
            }
        }
    }

    /// Resolves an import to its document scope.
    fn resolve_import_v1(
        graph: &DocumentGraph,
        stmt: &v1::ImportStatement,
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

    /// Calculates the types from a V1 AST.
    fn calculate_types_v1(&mut self, ast: &v1::Ast, diagnostics: &mut Vec<Diagnostic>) {
        // Start by populating struct types
        self.calculate_struct_types_v1(ast, diagnostics);
    }

    /// Calculates the struct types from a V1 AST.
    fn calculate_struct_types_v1(&mut self, ast: &v1::Ast, diagnostics: &mut Vec<Diagnostic>) {
        if self.structs.is_empty() {
            return;
        }

        let definitions = ast.structs().collect::<Vec<_>>();

        // Populate a type dependency graph; any edges that would form cycles are turned
        // into diagnostics.
        let mut graph = DiGraphMap::new();
        let mut space = Default::default();
        for (_, s) in &self.structs {
            // Skip imported structs
            let from = match s.index {
                Some(index) => index,
                None => continue,
            };

            graph.add_node(from);
            for member in definitions[from].members() {
                if let v1::Type::Ref(r) = member.ty() {
                    // Add an edge to the referenced struct is locally defined
                    if let Some(s) = self.structs.get(r.name().as_str()) {
                        let to = match s.index {
                            Some(index) => index,
                            None => continue,
                        };

                        // Check to see if the edge would form a cycle
                        if has_path_connecting(&graph, from, to, Some(&mut space)) {
                            diagnostics.push(recursive_struct(
                                &definitions[from].name(),
                                member.name().span(),
                            ));
                        } else {
                            graph.add_edge(to, from, ());
                        }
                    }
                }
            }
        }

        // At this point the graph is populated without any cycles; now
        // calculate the struct types in topological order
        for index in toposort(&graph, Some(&mut space)).expect("graph should not contain cycles") {
            let definition = &definitions[index];
            let structs = &self.structs;
            match StructType::from_ast_v1(&mut self.types, definition, &|n| {
                // Lookup the type name; if we couldn't calculate the type, return union to
                // indicate indeterminate
                structs.get(n).map(|s| s.ty.unwrap_or(Type::Union))
            }) {
                Ok(ty) => {
                    let name = definition.name();
                    let s = self
                        .structs
                        .get_mut(name.as_str())
                        .expect("struct should exist");

                    assert!(s.ty.is_none(), "type should not already be present");
                    self.structs
                        .get_mut(name.as_str())
                        .expect("struct should exist")
                        .ty = Some(self.types.add_struct(ty))
                }
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
        }
    }
}
