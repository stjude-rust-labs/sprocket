//! A lint rule that ensures structs are defined with pascal case names.

use convert_case::Boundary;
use convert_case::Case;
use convert_case::Converter;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;
use wdl_ast::v1::StructDefinition;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the pascal case rule.
const ID: &str = "PascalCase";

/// Creates a "use pascal case" diagnostic.
fn use_pascal_case(name: &str, properly_cased_name: &str, span: Span) -> Diagnostic {
    Diagnostic::warning(format!("struct name `{name}` is not PascalCase"))
        .with_rule(ID)
        .with_label("this name must be PascalCase", span)
        .with_fix(format!("replace `{name}` with `{properly_cased_name}`"))
}

/// Detects structs defined without a pascal case name.
#[derive(Default, Debug, Clone, Copy)]
pub struct PascalCaseRule;

impl Rule for PascalCaseRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that structs are defined with PascalCase names."
    }

    fn explanation(&self) -> &'static str {
        "Struct names should be in PascalCase. Maintaining a consistent naming convention makes \
         the code easier to read and understand."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Naming, Tag::Style, Tag::Clarity])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::StructDefinitionNode,
        ])
    }
}

/// Checks if the given name is pascal case, and if not adds a warning to the
/// diagnostics.
fn check_name(
    name: &str,
    span: Span,
    diagnostics: &mut Diagnostics,
    element: SyntaxElement,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    let converter = Converter::new()
        .remove_boundaries(&[Boundary::DIGIT_LOWER, Boundary::LOWER_DIGIT])
        .to_case(Case::Pascal);
    let properly_cased_name = converter.convert(name);
    if name != properly_cased_name {
        diagnostics.exceptable_add(
            use_pascal_case(name, &properly_cased_name, span),
            element,
            exceptable_nodes,
        );
    }
}

impl Visitor for PascalCaseRule {
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

    fn struct_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        def: &StructDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let name = def.name();
        check_name(
            name.text(),
            name.span(),
            state,
            SyntaxElement::from(def.inner().clone()),
            &self.exceptable_nodes(),
        );
    }
}
