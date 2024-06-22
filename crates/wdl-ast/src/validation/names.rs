//! Validation of unique names in a V1 AST.

use std::collections::HashMap;
use std::fmt;

use rowan::ast::AstNode;
use wdl_grammar::Diagnostic;
use wdl_grammar::Span;
use wdl_grammar::ToSpan;

use crate::v1::BoundDecl;
use crate::v1::CallStatement;
use crate::v1::ImportStatement;
use crate::v1::ScatterStatement;
use crate::v1::StructDefinition;
use crate::v1::TaskDefinition;
use crate::v1::UnboundDecl;
use crate::v1::WorkflowDefinition;
use crate::AstToken;
use crate::Diagnostics;
use crate::Ident;
use crate::VisitReason;
use crate::Visitor;

/// Represents the context of a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NameContext {
    /// The name is a workflow name.
    Workflow(Span),
    /// The name is a task name.
    Task(Span),
    /// The name is a struct name.
    Struct(Span),
    /// The name is a struct member name.
    StructMember(Span),
    /// The name is a declaration name.
    Declaration(Span),
    /// The name is from a call statement.
    Call(Span),
    /// The name is a scatter variable.
    ScatterVariable(Span),
}

impl NameContext {
    /// Gets the span of the name.
    fn span(&self) -> Span {
        match self {
            Self::Workflow(s) => *s,
            Self::Task(s) => *s,
            Self::Struct(s) => *s,
            Self::StructMember(s) => *s,
            Self::Declaration(s) => *s,
            Self::Call(s) => *s,
            Self::ScatterVariable(s) => *s,
        }
    }
}

impl fmt::Display for NameContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Workflow(_) => write!(f, "workflow"),
            Self::Task(_) => write!(f, "task"),
            Self::Struct(_) => write!(f, "struct"),
            Self::StructMember(_) => write!(f, "struct member"),
            Self::Declaration(_) => write!(f, "declaration"),
            Self::Call(_) => write!(f, "call"),
            Self::ScatterVariable(_) => write!(f, "scatter variable"),
        }
    }
}

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

/// Creates a "call conflict" diagnostic
fn call_conflict(name: Ident, first: NameContext, suggest_fix: bool) -> Diagnostic {
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

/// Creates an "invalid import namespace" diagnostic
fn invalid_import_namespace(span: Span) -> Diagnostic {
    Diagnostic::error("import namespace is not a valid WDL identifier")
        .with_label(
            "a namespace name cannot be derived from this import path",
            span,
        )
        .with_fix("add an `as` clause to the import to specify a namespace")
}

/// A visitor of unique names within an AST.
///
/// Ensures that the following names are unique:
///
/// * Workflow names.
/// * Task names.
/// * Struct names from struct declarations and import aliases.
/// * Struct member names.
/// * Declarations and scatter variable names.
#[derive(Debug, Default)]
pub struct UniqueNamesVisitor {
    /// A map of namespace names to the span that introduced the name.
    namespaces: HashMap<String, Span>,
    /// A map of task and workflow names to the span of the first name.
    tasks_and_workflows: HashMap<String, NameContext>,
    /// A map of struct names to the span of the first name.
    structs: HashMap<String, Span>,
    /// A map of decl names to the context of what introduced the name.
    ///
    /// This map is cleared upon entry to a workflow, task, or struct.
    decls: HashMap<String, NameContext>,
    /// Whether or not we're inside a struct definition.
    inside_struct: bool,
}

impl Visitor for UniqueNamesVisitor {
    type State = Diagnostics;

    fn import_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &ImportStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Check for unique namespace name
        match stmt.namespace() {
            Some((ns, span)) => {
                if let Some(first) = self.namespaces.get(&ns) {
                    state.add(namespace_conflict(
                        &ns,
                        span,
                        *first,
                        stmt.explicit_namespace().is_none(),
                    ));
                } else {
                    self.namespaces.insert(ns, span);
                }
            }
            None => {
                state.add(invalid_import_namespace(
                    stmt.uri().syntax().text_range().to_span(),
                ));
            }
        }

        // Check for unique struct aliases
        for alias in stmt.aliases() {
            let (_, name) = alias.names();
            if let Some(first) = self.structs.get(name.as_str()) {
                state.add(name_conflict(
                    name.as_str(),
                    NameContext::Struct(name.span()),
                    NameContext::Struct(*first),
                ));
            } else {
                self.structs.insert(name.as_str().to_string(), name.span());
            }
        }
    }

    fn workflow_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        workflow: &WorkflowDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.decls.clear();

        let name = workflow.name();
        let context = NameContext::Workflow(name.span());
        if let Some(first) = self.tasks_and_workflows.get(name.as_str()) {
            state.add(name_conflict(name.as_str(), context, *first));
        } else {
            self.tasks_and_workflows
                .insert(name.as_str().to_string(), context);
        }
    }

    fn task_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.decls.clear();

        let name = task.name();
        let context = NameContext::Task(name.span());
        if let Some(first) = self.tasks_and_workflows.get(name.as_str()) {
            state.add(name_conflict(name.as_str(), context, *first));
        } else {
            self.tasks_and_workflows
                .insert(name.as_str().to_string(), context);
        }
    }

    fn struct_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        def: &StructDefinition,
    ) {
        if reason == VisitReason::Exit {
            self.inside_struct = false;
            return;
        }

        self.inside_struct = true;
        self.decls.clear();

        let name = def.name();
        if let Some(first) = self.structs.get(name.as_str()) {
            state.add(name_conflict(
                name.as_str(),
                NameContext::Struct(name.span()),
                NameContext::Struct(*first),
            ));
        } else {
            self.structs.insert(name.as_str().to_string(), name.span());
        }
    }

    fn bound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &BoundDecl) {
        if reason == VisitReason::Exit {
            return;
        }

        let name = decl.name();
        let context = NameContext::Declaration(name.span());
        if let Some(first) = self.decls.get_mut(name.as_str()) {
            state.add(name_conflict(name.as_str(), context, *first));

            // If the name came from a scatter variable, "promote" this declaration as the
            // source of any additional conflicts.
            if let NameContext::ScatterVariable(_) = first {
                *first = context;
            }
        } else {
            self.decls.insert(name.as_str().to_string(), context);
        }
    }

    fn unbound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &UnboundDecl) {
        if reason == VisitReason::Exit {
            return;
        }

        let name = decl.name();
        let context = if self.inside_struct {
            NameContext::StructMember(name.span())
        } else {
            NameContext::Declaration(name.span())
        };

        if let Some(first) = self.decls.get_mut(name.as_str()) {
            state.add(name_conflict(name.as_str(), context, *first));

            // If the name came from a scatter variable, "promote" this declaration as the
            // source of any additional conflicts.
            if let NameContext::ScatterVariable(_) = first {
                *first = context;
            }
        } else {
            self.decls.insert(name.as_str().to_string(), context);
        }
    }

    fn scatter_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &ScatterStatement,
    ) {
        let name = stmt.variable();
        if reason == VisitReason::Exit {
            // Check to see if this scatter statement introduced the name
            // If so, remove it from the set
            if let NameContext::ScatterVariable(span) = &self.decls[name.as_str()] {
                if name.span() == *span {
                    self.decls.remove(name.as_str());
                }
            }

            return;
        }

        let context = NameContext::ScatterVariable(name.span());
        if let Some(first) = self.decls.get(name.as_str()) {
            state.add(name_conflict(name.as_str(), context, *first));
        } else {
            self.decls.insert(name.as_str().to_string(), context);
        }
    }

    fn call_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &CallStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Call statements introduce a declaration from the result
        let (name, aliased) = stmt
            .alias()
            .map(|a| (a.name(), true))
            .unwrap_or_else(|| (stmt.target().name().1, false));
        let context = NameContext::Call(name.span());
        if let Some(first) = self.decls.get_mut(name.as_str()) {
            state.add(call_conflict(name, *first, !aliased));

            // If the name came from a scatter variable, "promote" this declaration as the
            // source of any additional conflicts.
            if let NameContext::ScatterVariable(_) = first {
                *first = context;
            }
        } else {
            self.decls.insert(name.as_str().to_string(), context);
        }
    }
}
