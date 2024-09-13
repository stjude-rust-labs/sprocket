//! Validation of unique keys in an AST.

use std::collections::HashSet;
use std::fmt;

use crate::v1::Expr;
use crate::v1::HintsSection;
use crate::v1::LiteralExpr;
use crate::v1::MetadataObject;
use crate::v1::MetadataSection;
use crate::v1::ParameterMetadataSection;
use crate::v1::RequirementsSection;
use crate::v1::RuntimeSection;
use crate::AstToken;
use crate::Diagnostic;
use crate::Diagnostics;
use crate::Document;
use crate::Ident;
use crate::Span;
use crate::SupportedVersion;
use crate::TokenStrHash;
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
        }
    }
}

/// Creates a "duplicate key" diagnostic
fn duplicate_key(context: Context, name: &Ident, first: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "duplicate key `{name}` in {context}",
        name = name.as_str(),
    ))
    .with_label("this key is a duplicate", name.span())
    .with_label("first key with this name is here", first)
}

/// Creates a "conflicting key" diagnostic
fn conflicting_key(context: Context, name: &Ident, first: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "conflicting key `{name}` in {context}",
        name = name.as_str(),
    ))
    .with_label("this key conflicts with an alias", name.span())
    .with_label("the conflicting alias is here", first)
}

/// Checks the given set of keys for duplicates
fn check_duplicate_keys(
    keys: &mut HashSet<TokenStrHash<Ident>>,
    aliases: &[(&str, &str)],
    names: impl Iterator<Item = Ident>,
    context: Context,
    diagnostics: &mut Diagnostics,
) {
    keys.clear();
    for name in names {
        if let Some(first) = keys.get(name.as_str()) {
            diagnostics.add(duplicate_key(context, &name, first.as_ref().span()));
            continue;
        }

        for (first, second) in aliases {
            let alias = if *first == name.as_str() {
                second
            } else if *second == name.as_str() {
                first
            } else {
                continue;
            };

            if let Some(first) = keys.get(*alias) {
                diagnostics.add(conflicting_key(context, &name, first.as_ref().span()));
                break;
            }
        }

        keys.insert(TokenStrHash::new(name));
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
pub struct UniqueKeysVisitor(HashSet<TokenStrHash<Ident>>);

impl Visitor for UniqueKeysVisitor {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
    }

    fn requirements_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &RequirementsSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        check_duplicate_keys(
            &mut self.0,
            &[
                ("container", "docker"),
                ("max_retries", "maxRetries"),
                ("return_codes", "returnCodes"),
            ],
            section.items().map(|i| i.name()),
            Context::RequirementsSection,
            state,
        );
    }

    fn hints_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &HintsSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        check_duplicate_keys(
            &mut self.0,
            &[
                ("max_cpu", "maxCpu"),
                ("max_memory", "maxMemory"),
                ("localization_optional", "localizationOptional"),
            ],
            section.items().map(|i| i.name()),
            Context::HintsSection,
            state,
        );
    }

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
            &[("container", "docker")],
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
            &[],
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
            &[],
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
        // use a different set to check the keys
        let mut keys = HashSet::new();
        check_duplicate_keys(
            &mut keys,
            &[],
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
                    &[],
                    o.items().map(|i| i.name_value().0),
                    Context::LiteralObject,
                    state,
                );
            }
            Expr::Literal(LiteralExpr::Struct(s)) => {
                check_duplicate_keys(
                    &mut self.0,
                    &[],
                    s.items().map(|i| i.name_value().0),
                    Context::LiteralStruct,
                    state,
                );
            }
            _ => {}
        }
    }
}
