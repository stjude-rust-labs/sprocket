//! Validation of unique keys in a V1 AST.

use std::collections::HashMap;
use std::fmt;

use crate::v1::Expr;
use crate::v1::LiteralExpr;
use crate::v1::MetadataObject;
use crate::v1::MetadataSection;
use crate::v1::ParameterMetadataSection;
use crate::v1::RuntimeSection;
use crate::AstToken;
use crate::Diagnostic;
use crate::Diagnostics;
use crate::Ident;
use crate::Span;
use crate::VisitReason;
use crate::Visitor;

/// Represents context about a unique key validation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
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
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimeSection => write!(f, "runtime section"),
            Self::MetadataSection => write!(f, "metadata section"),
            Self::ParameterMetadataSection => write!(f, "parameter metadata section"),
            Self::MetadataObject => write!(f, "metadata object"),
            Self::LiteralObject => write!(f, "literal object"),
            Self::LiteralStruct => write!(f, "literal struct"),
        }
    }
}

/// Creates a "duplicate key" diagnostic
fn duplicate_key(context: Context, name: Ident, first: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "duplicate key `{name}` in {context}",
        name = name.as_str(),
    ))
    .with_label("this key is a duplicate", name.span())
    .with_label("first key with this name is here", first)
}

/// Checks the given set of keys for duplicates
fn check_duplicate_keys(
    keys: &mut HashMap<String, Span>,
    names: impl Iterator<Item = Ident>,
    context: Context,
    diagnostics: &mut Diagnostics,
) {
    keys.clear();
    for name in names {
        if let Some(first) = keys.get(name.as_str()) {
            diagnostics.add(duplicate_key(context, name, *first));
            continue;
        }

        keys.insert(name.as_str().to_string(), name.span());
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
pub struct UniqueKeysVisitor(HashMap<String, Span>);

impl Visitor for UniqueKeysVisitor {
    type State = Diagnostics;

    fn runtime_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &RuntimeSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        check_duplicate_keys(
            &mut self.0,
            section.items().map(|i| i.name()),
            Context::RuntimeSection,
            state,
        );
    }

    fn metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &MetadataSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        check_duplicate_keys(
            &mut self.0,
            section.items().map(|i| i.name()),
            Context::MetadataSection,
            state,
        );
    }

    fn parameter_metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &ParameterMetadataSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        check_duplicate_keys(
            &mut self.0,
            section.items().map(|i| i.name()),
            Context::ParameterMetadataSection,
            state,
        );
    }

    fn metadata_object(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        object: &MetadataObject,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // As metadata objects are nested inside of metadata sections and objects,
        // use a different map to check the keys
        let mut keys = HashMap::new();
        check_duplicate_keys(
            &mut keys,
            object.items().map(|i| i.name()),
            Context::MetadataObject,
            state,
        );
    }

    fn expr(&mut self, state: &mut Self::State, reason: VisitReason, expr: &Expr) {
        if reason == VisitReason::Exit {
            return;
        }

        match expr {
            Expr::Literal(LiteralExpr::Object(o)) => {
                check_duplicate_keys(
                    &mut self.0,
                    o.items().map(|i| i.name_value().0),
                    Context::LiteralObject,
                    state,
                );
            }
            Expr::Literal(LiteralExpr::Struct(s)) => {
                check_duplicate_keys(
                    &mut self.0,
                    s.items().map(|i| i.name_value().0),
                    Context::LiteralStruct,
                    state,
                );
            }
            _ => {}
        }
    }
}
