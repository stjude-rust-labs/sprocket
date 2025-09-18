//! Validation of unique keys in an AST.

use std::collections::HashSet;
use std::fmt;

use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::Span;
use wdl_ast::TokenText;
use wdl_ast::v1::CallStatement;
use wdl_ast::v1::Expr;
use wdl_ast::v1::LiteralExpr;
use wdl_ast::v1::MetadataObject;
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::ParameterMetadataSection;
use wdl_ast::v1::RequirementsSection;
use wdl_ast::v1::RuntimeSection;
use wdl_ast::v1::TASK_HINT_LOCALIZATION_OPTIONAL;
use wdl_ast::v1::TASK_HINT_LOCALIZATION_OPTIONAL_ALIAS;
use wdl_ast::v1::TASK_HINT_MAX_CPU;
use wdl_ast::v1::TASK_HINT_MAX_CPU_ALIAS;
use wdl_ast::v1::TASK_HINT_MAX_MEMORY;
use wdl_ast::v1::TASK_HINT_MAX_MEMORY_ALIAS;
use wdl_ast::v1::TASK_REQUIREMENT_CONTAINER;
use wdl_ast::v1::TASK_REQUIREMENT_CONTAINER_ALIAS;
use wdl_ast::v1::TASK_REQUIREMENT_MAX_RETRIES;
use wdl_ast::v1::TASK_REQUIREMENT_MAX_RETRIES_ALIAS;
use wdl_ast::v1::TASK_REQUIREMENT_RETURN_CODES;
use wdl_ast::v1::TASK_REQUIREMENT_RETURN_CODES_ALIAS;
use wdl_ast::v1::TaskHintsSection;
use wdl_ast::v1::WORKFLOW_HINT_ALLOW_NESTED_INPUTS;
use wdl_ast::v1::WORKFLOW_HINT_ALLOW_NESTED_INPUTS_ALIAS;
use wdl_ast::v1::WorkflowHintsSection;

use crate::Diagnostics;
use crate::VisitReason;
use crate::Visitor;

/// Represents context about a unique key validation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    /// The error is in the requirements section.
    RequirementsSection,
    /// The error is in the hints section.
    HintsSection,
    /// The error is in a runtime section.
    RuntimeSection,
    /// The error is in a metadata section.
    MetadataSection,
    /// The error is in a parameter metadata section.
    ParameterMetadataSection,
    /// The error is in a metadata object.
    MetadataObject,
    /// The error is in a literal object.
    LiteralObject,
    /// The error is in a literal struct.
    LiteralStruct,
    /// The error is in a call statement.
    CallStatement,
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RequirementsSection => write!(f, "requirements section"),
            Self::HintsSection => write!(f, "hints section"),
            Self::RuntimeSection => write!(f, "runtime section"),
            Self::MetadataSection => write!(f, "metadata section"),
            Self::ParameterMetadataSection => write!(f, "parameter metadata section"),
            Self::MetadataObject => write!(f, "metadata object"),
            Self::LiteralObject => write!(f, "literal object"),
            Self::LiteralStruct => write!(f, "literal struct"),
            Self::CallStatement => write!(f, "call statement"),
        }
    }
}

/// Creates a "duplicate key" diagnostic
fn duplicate_key(context: Context, name: &Ident, first: Span) -> Diagnostic {
    let kind = if context == Context::CallStatement {
        "call input"
    } else {
        "key"
    };

    Diagnostic::error(format!(
        "duplicate {kind} `{name}` in {context}",
        name = name.text(),
    ))
    .with_label(format!("this {kind} is a duplicate"), name.span())
    .with_label(format!("first {kind} with this name is here"), first)
}

/// Creates a "conflicting key" diagnostic
fn conflicting_key(context: Context, name: &Ident, first: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "conflicting key `{name}` in {context}",
        name = name.text(),
    ))
    .with_label("this key conflicts with an alias", name.span())
    .with_label("the conflicting alias is here", first)
}

/// Checks the given set of keys for duplicates
fn check_duplicate_keys(
    keys: &mut HashSet<TokenText>,
    aliases: &[(&str, &str)],
    names: impl Iterator<Item = Ident>,
    context: Context,
    diagnostics: &mut Diagnostics,
) {
    keys.clear();
    for name in names {
        if let Some(first) = keys.get(name.text()) {
            diagnostics.add(duplicate_key(context, &name, first.span()));
            continue;
        }

        for (first, second) in aliases {
            let alias = if *first == name.text() {
                second
            } else if *second == name.text() {
                first
            } else {
                continue;
            };

            if let Some(first) = keys.get(*alias) {
                diagnostics.add(conflicting_key(context, &name, first.span()));
                break;
            }
        }

        keys.insert(name.hashable());
    }
}

/// A visitor for ensuring unique keys within an AST.
///
/// Ensures that there are no duplicate keys in:
///
/// * a runtime section in tasks
/// * a metadata section in tasks and workflows
/// * a parameter metadata section in tasks and workflows
/// * metadata objects in metadata and parameter metadata sections
/// * object literals
/// * struct literals
#[derive(Default, Debug)]
pub struct UniqueKeysVisitor(HashSet<TokenText>);

impl Visitor for UniqueKeysVisitor {
    fn reset(&mut self) {
        self.0.clear();
    }

    fn requirements_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &RequirementsSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        check_duplicate_keys(
            &mut self.0,
            &[
                (TASK_REQUIREMENT_CONTAINER, TASK_REQUIREMENT_CONTAINER_ALIAS),
                (
                    TASK_REQUIREMENT_MAX_RETRIES,
                    TASK_REQUIREMENT_MAX_RETRIES_ALIAS,
                ),
                (
                    TASK_REQUIREMENT_RETURN_CODES,
                    TASK_REQUIREMENT_RETURN_CODES_ALIAS,
                ),
            ],
            section.items().map(|i| i.name()),
            Context::RequirementsSection,
            diagnostics,
        );
    }

    fn task_hints_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &TaskHintsSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        check_duplicate_keys(
            &mut self.0,
            &[
                (TASK_HINT_MAX_CPU, TASK_HINT_MAX_CPU_ALIAS),
                (TASK_HINT_MAX_MEMORY, TASK_HINT_MAX_MEMORY_ALIAS),
                (
                    TASK_HINT_LOCALIZATION_OPTIONAL,
                    TASK_HINT_LOCALIZATION_OPTIONAL_ALIAS,
                ),
            ],
            section.items().map(|i| i.name()),
            Context::HintsSection,
            diagnostics,
        );
    }

    fn workflow_hints_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &WorkflowHintsSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        check_duplicate_keys(
            &mut self.0,
            &[(
                WORKFLOW_HINT_ALLOW_NESTED_INPUTS,
                WORKFLOW_HINT_ALLOW_NESTED_INPUTS_ALIAS,
            )],
            section.items().map(|i| i.name()),
            Context::HintsSection,
            diagnostics,
        );
    }

    fn runtime_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &RuntimeSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        check_duplicate_keys(
            &mut self.0,
            &[(TASK_REQUIREMENT_CONTAINER, TASK_REQUIREMENT_CONTAINER_ALIAS)],
            section.items().map(|i| i.name()),
            Context::RuntimeSection,
            diagnostics,
        );
    }

    fn metadata_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &MetadataSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        check_duplicate_keys(
            &mut self.0,
            &[],
            section.items().map(|i| i.name()),
            Context::MetadataSection,
            diagnostics,
        );
    }

    fn parameter_metadata_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &ParameterMetadataSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        check_duplicate_keys(
            &mut self.0,
            &[],
            section.items().map(|i| i.name()),
            Context::ParameterMetadataSection,
            diagnostics,
        );
    }

    fn metadata_object(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        object: &MetadataObject,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // As metadata objects are nested inside of metadata sections and objects,
        // use a different set to check the keys
        let mut keys = HashSet::new();
        check_duplicate_keys(
            &mut keys,
            &[],
            object.items().map(|i| i.name()),
            Context::MetadataObject,
            diagnostics,
        );
    }

    fn expr(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, expr: &Expr) {
        if reason == VisitReason::Exit {
            return;
        }

        match expr {
            Expr::Literal(LiteralExpr::Object(o)) => {
                check_duplicate_keys(
                    &mut self.0,
                    &[],
                    o.items().map(|i| i.name_value().0),
                    Context::LiteralObject,
                    diagnostics,
                );
            }
            Expr::Literal(LiteralExpr::Struct(s)) => {
                check_duplicate_keys(
                    &mut self.0,
                    &[],
                    s.items().map(|i| i.name_value().0),
                    Context::LiteralStruct,
                    diagnostics,
                );
            }
            _ => {}
        }
    }

    fn call_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &CallStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        check_duplicate_keys(
            &mut self.0,
            &[],
            stmt.inputs().map(|i| i.name()),
            Context::CallStatement,
            diagnostics,
        );
    }
}
