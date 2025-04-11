//! A lint rule that disallows declaration names with type information.

use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::Decl;
use wdl_ast::v1::PrimitiveTypeKind;
use wdl_ast::v1::Type;
use wdl_ast::v1::UnboundDecl;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the declaration name rule.
const ID: &str = "DeclarationName";

/// A rule that identifies declaration names that include their type names.
#[derive(Debug, Default)]
pub struct DeclarationNameRule;

/// Create a diagnostic for a declaration identifier that contains its type
/// name.
fn decl_identifier_with_type(span: Span, decl_name: &str, type_name: &str) -> Diagnostic {
    Diagnostic::note(format!(
        "declaration identifier '{decl_name}' contains type name '{type_name}'",
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix("rename the identifier to not include the type name")
}

impl Rule for DeclarationNameRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures declaration names do not redundantly include their type name."
    }

    fn explanation(&self) -> &'static str {
        "Declaration names should not include their type. This makes the code more verbose and \
         often redundant. For example, use 'counter' instead of 'counter_int' or 'is_active' \
         instead of 'is_active_bool'. Exceptions are made for String, File, and user-defined \
         struct types, which are not flagged by this rule."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style, Tag::Clarity])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::InputSectionNode,
            SyntaxKind::OutputSectionNode,
            SyntaxKind::BoundDeclNode,
            SyntaxKind::UnboundDeclNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &["InputName", "OutputName"]
    }
}

impl Visitor for DeclarationNameRule {
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

    fn bound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &BoundDecl) {
        if reason == VisitReason::Enter {
            check_decl_name(state, &Decl::Bound(decl.clone()), &self.exceptable_nodes());
        }
    }

    fn unbound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &UnboundDecl) {
        if reason == VisitReason::Enter {
            check_decl_name(
                state,
                &Decl::Unbound(decl.clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}

/// Check declaration name for type suffixes.
fn check_decl_name(
    state: &mut Diagnostics,
    decl: &Decl,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    let (type_name, alt_type_name) = match decl.ty() {
        Type::Ref(_) => return, // Skip type reference types (user-defined structs)
        Type::Primitive(ty) => {
            match ty.kind() {
                // Skip File and String types as they cause too many false positives
                PrimitiveTypeKind::File | PrimitiveTypeKind::String => return,
                PrimitiveTypeKind::Boolean => ("Boolean", Some("Bool")),
                PrimitiveTypeKind::Integer => ("Int", Some("Integer")),
                PrimitiveTypeKind::Float => ("Float", None),
                PrimitiveTypeKind::Directory => ("Directory", Some("Dir")),
            }
        }
        Type::Array(_) => ("Array", None),
        Type::Map(_) => ("Map", None),
        Type::Pair(_) => ("Pair", None),
        Type::Object(_) => ("Object", None),
    };

    let ident = decl.name();
    let name = ident.text();
    let name_lower = name.to_lowercase();

    for type_name in [type_name].into_iter().chain(alt_type_name) {
        let type_lower = type_name.to_lowercase();

        // Special handling for short type names (3 characters or less).
        // These require word-based checks to avoid false positives.
        if type_lower.len() <= 3 {
            let words = convert_case::split(&name, &convert_case::Boundary::defaults());
            if words.into_iter().any(|w| w == type_lower) {
                let diagnostic = decl_identifier_with_type(ident.span(), name, type_name);
                state.exceptable_add(
                    diagnostic,
                    rowan::NodeOrToken::Node(decl.inner().to_owned()),
                    exceptable_nodes,
                );
                return;
            }
        } else if name_lower.contains(&type_lower) {
            let diagnostic = decl_identifier_with_type(ident.span(), name, type_name);
            state.exceptable_add(
                diagnostic,
                rowan::NodeOrToken::Node(decl.inner().to_owned()),
                exceptable_nodes,
            );
            return;
        }
    }
}
