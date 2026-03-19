//! A lint rule for flagging Map[String, *] types where a Struct would be clearer.
//!
//! This rule encourages the use of WDL's type system for self-documenting interfaces
//! by flagging Map[String, *] types in declarations where a Struct would provide
//! clearer semantics and better validation.

use std::fmt::Debug;

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::PrimitiveTypeKind;
use wdl_ast::v1::Type;
use wdl_ast::v1::UnboundDecl;
use wdl_ast::version::V1;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the PreferStructOverMap rule.
const ID: &str = "PreferStructOverMap";

/// Creates a "prefer struct over map" diagnostic.
fn prefer_struct_over_map(span: Span, map_type: &str) -> Diagnostic {
    Diagnostic::warning(format!(
        "consider using a Struct instead of `{}`",
        map_type
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix("define a Struct type with explicit fields instead of using a Map")
}

/// Detects Map[String, *] types that should be replaced with Structs.
#[derive(Debug, Clone)]
pub struct PreferStructOverMapRule {
    /// The detected version of the current document.
    version: Option<SupportedVersion>,
    /// Whether pedantic mode is enabled.
    pedantic: bool,
}

impl Default for PreferStructOverMapRule {
    fn default() -> Self {
        Self {
            version: None,
            pedantic: false,
        }
    }
}

impl PreferStructOverMapRule {
    /// Create a new instance of `PreferStructOverMapRule`.
    pub fn new(pedantic: bool) -> Self {
        Self {
            version: None,
            pedantic,
        }
    }

    /// Check if a type is Map[String, *] and emit a diagnostic if so.
    /// This recursively checks nested types (e.g., Array[Map[String, File]]).
    fn check_type(
        &self,
        diagnostics: &mut Diagnostics,
        ty: &Type,
        exceptable_nodes: &Option<&'static [SyntaxKind]>,
    ) {
        // If not in pedantic mode, do nothing
        if !self.pedantic {
            return;
        }

        // Check if the type is a Map
        if let Type::Map(map_type) = ty {
            let (key_type, _) = map_type.types();

            // Only flag Map[String, *]
            if key_type.kind() == PrimitiveTypeKind::String {
                diagnostics.exceptable_add(
                    prefer_struct_over_map(map_type.inner().span(), &ty.to_string()),
                    SyntaxElement::from(ty.inner().clone()),
                    exceptable_nodes,
                );
            }
        }

        // Recursively check nested types
        match ty {
            Type::Array(array_type) => {
                // Get the inner type of the array
                if let Some(inner) = Type::cast(array_type.inner().clone()) {
                    self.check_type(diagnostics, &inner, exceptable_nodes);
                }
            }
            Type::Pair(pair_type) => {
                // Get both types of the pair
                let mut children = pair_type.inner().children().filter_map(Type::cast);
                if let Some(first) = children.next() {
                    self.check_type(diagnostics, &first, exceptable_nodes);
                }
                if let Some(second) = children.next() {
                    self.check_type(diagnostics, &second, exceptable_nodes);
                }
            }
            Type::Map(_) => {
                // Already checked above, no need to recurse into value type
                // as that would cause duplicate diagnostics
            }
            Type::Ref(_) | Type::Object(_) | Type::Primitive(_) => {
                // These types don't contain other types, nothing to check
            }
        }
    }
}

impl Rule for PreferStructOverMapRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that Map[String, *] types are not used where a Struct would be clearer."
    }

    fn explanation(&self) -> &'static str {
        "Map[String, *] types are often used to represent structured data, but they lack \
         self-documenting semantics and validation that Structs provide. Consider defining \
         a Struct with explicit fields instead. \n\n\
         Legitimate uses of Map[String, *] exist (e.g., arbitrary key-value metadata, \
         environment variables, user-defined annotations where keys are not known at \
         authoring time). Authors should suppress the diagnostic with `#@ except: \
         PreferStructOverMap` in such cases."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Clarity])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::StructDefinitionNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
            SyntaxKind::InputSectionNode,
            SyntaxKind::OutputSectionNode,
            SyntaxKind::BoundDeclNode,
            SyntaxKind::UnboundDeclNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for PreferStructOverMapRule {
    fn reset(&mut self) {
        let pedantic = self.pedantic;
        *self = Self::default();
        self.pedantic = pedantic;
    }

    fn document(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        _: &Document,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.version = Some(version);
    }

    fn bound_decl(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, decl: &BoundDecl) {
        if reason == VisitReason::Exit {
            return;
        }

        // Check version - only apply to WDL >= 1.1
        if let Some(SupportedVersion::V1(version)) = self.version {
            if version < V1::One {
                return;
            }
        } else {
            return;
        }

        let ty = decl.ty();
        self.check_type(diagnostics, &ty, &self.exceptable_nodes());
    }

    fn unbound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &UnboundDecl,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Check version - only apply to WDL >= 1.1
        if let Some(SupportedVersion::V1(version)) = self.version {
            if version < V1::One {
                return;
            }
        } else {
            return;
        }

        let ty = decl.ty();
        self.check_type(diagnostics, &ty, &self.exceptable_nodes());
    }
}
