//! Implementation for AST visitation.
//!
//! An AST visitor is called when a WDL document is being visited (see
//! [Document::visit]); callbacks correspond to specific nodes and tokens in the
//! AST based on [SyntaxKind]. As `SyntaxKind` is the union of nodes and tokens
//! from _every_ version of WDL, the `Visitor` trait is also the union of
//! visitation callbacks.
//!
//! The [Visitor] trait is not WDL version-specific, meaning that the trait's
//! methods currently receive V1 representation of AST nodes.
//!
//! In the future, a major version change to the WDL specification will
//! introduce V2 representations for AST nodes that are either brand new or have
//! changed since V1.
//!
//! When this occurs, the `Visitor` trait will be extended to support the new
//! syntax; however, syntax that has not changed since V1 will continue to use
//! the V1 AST types.
//!
//! That means it is possible to receive callbacks for V1 nodes and tokens when
//! visiting a V2 document; the hope is that enables some visitors to be
//! "shared" across different WDL versions.

use rowan::WalkEvent;
use tracing::trace;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::VersionStatement;
use wdl_ast::Whitespace;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::CallStatement;
use wdl_ast::v1::CommandSection;
use wdl_ast::v1::CommandText;
use wdl_ast::v1::ConditionalStatement;
use wdl_ast::v1::Expr;
use wdl_ast::v1::ImportStatement;
use wdl_ast::v1::InputSection;
use wdl_ast::v1::MetadataArray;
use wdl_ast::v1::MetadataObject;
use wdl_ast::v1::MetadataObjectItem;
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::ParameterMetadataSection;
use wdl_ast::v1::Placeholder;
use wdl_ast::v1::RequirementsSection;
use wdl_ast::v1::RuntimeItem;
use wdl_ast::v1::RuntimeSection;
use wdl_ast::v1::ScatterStatement;
use wdl_ast::v1::StringText;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::TaskHintsSection;
use wdl_ast::v1::UnboundDecl;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::v1::WorkflowHintsSection;

use crate::Config;
use crate::Diagnostics;
use crate::document::Document as AnalysisDocument;

/// Represents the reason an AST node has been visited.
///
/// Each node is visited exactly once, but the visitor will receive a call for
/// entering the node and a call for exiting the node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VisitReason {
    /// The visit has entered the node.
    Enter,
    /// The visit has exited the node.
    Exit,
}

/// A trait used to implement an AST visitor.
///
/// Each encountered node will receive a corresponding method call
/// that receives both a [VisitReason::Enter] call and a
/// matching [VisitReason::Exit] call.
#[allow(unused_variables)]
pub trait Visitor {
    /// Registers configuration with a visitor.
    fn register(&mut self, config: &Config) {}

    /// Resets the visitor to its initial state.
    ///
    /// A visitor must implement this with resetting any internal state so that
    /// a visitor may be reused between documents.
    fn reset(&mut self);

    /// Visits the root document node.
    fn document(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        doc: &AnalysisDocument,
        version: SupportedVersion,
    ) {
    }

    /// Visits a whitespace token.
    fn whitespace(&mut self, diagnostics: &mut Diagnostics, whitespace: &Whitespace) {}

    /// Visit a comment token.
    fn comment(&mut self, diagnostics: &mut Diagnostics, comment: &Comment) {}

    /// Visits a top-level version statement node.
    fn version_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &VersionStatement,
    ) {
    }

    /// Visits a top-level import statement node.
    fn import_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &ImportStatement,
    ) {
    }

    /// Visits a struct definition node.
    fn struct_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        def: &StructDefinition,
    ) {
    }

    /// Visits a task definition node.
    fn task_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
    }

    /// Visits a workflow definition node.
    fn workflow_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        workflow: &WorkflowDefinition,
    ) {
    }

    /// Visits an input section node.
    fn input_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &InputSection,
    ) {
    }

    /// Visits an output section node.
    fn output_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &OutputSection,
    ) {
    }

    /// Visits a command section node.
    fn command_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &CommandSection,
    ) {
    }

    /// Visits a command text token in a command section node.
    fn command_text(&mut self, diagnostics: &mut Diagnostics, text: &CommandText) {}

    /// Visits a requirements section node.
    fn requirements_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &RequirementsSection,
    ) {
    }

    /// Visits a task hints section node.
    fn task_hints_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &TaskHintsSection,
    ) {
    }

    /// Visits a workflow hints section node.
    fn workflow_hints_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &WorkflowHintsSection,
    ) {
    }

    /// Visits a runtime section node.
    fn runtime_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &RuntimeSection,
    ) {
    }

    /// Visits a runtime item node.
    fn runtime_item(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        item: &RuntimeItem,
    ) {
    }

    /// Visits a metadata section node.
    fn metadata_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &MetadataSection,
    ) {
    }

    /// Visits a parameter metadata section node.
    fn parameter_metadata_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &ParameterMetadataSection,
    ) {
    }

    /// Visits a metadata object in a metadata or parameter metadata section.
    fn metadata_object(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        object: &MetadataObject,
    ) {
    }

    /// Visits a metadata object item in a metadata object.
    fn metadata_object_item(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        item: &MetadataObjectItem,
    ) {
    }

    /// Visits a metadata array node in a metadata or parameter metadata
    /// section.
    fn metadata_array(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        item: &MetadataArray,
    ) {
    }

    /// Visits an unbound declaration node.
    fn unbound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &UnboundDecl,
    ) {
    }

    /// Visits a bound declaration node.
    fn bound_decl(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, decl: &BoundDecl) {
    }

    /// Visits an expression node.
    fn expr(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, expr: &Expr) {}

    /// Visits a string text token in a literal string node.
    fn string_text(&mut self, diagnostics: &mut Diagnostics, text: &StringText) {}

    /// Visits a placeholder node.
    fn placeholder(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        placeholder: &Placeholder,
    ) {
    }

    /// Visits a conditional statement node in a workflow.
    fn conditional_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &ConditionalStatement,
    ) {
    }

    /// Visits a scatter statement node in a workflow.
    fn scatter_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &ScatterStatement,
    ) {
    }

    /// Visits a call statement node in a workflow.
    fn call_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &CallStatement,
    ) {
    }
}

/// Used to visit each descendant node of the given root in a preorder
/// traversal.
pub(crate) fn visit<V: Visitor>(
    document: &AnalysisDocument,
    diagnostics: &mut Diagnostics,
    visitor: &mut V,
) {
    trace!(
        uri = %document.uri(),
        "beginning document traversal",
    );
    for event in document.root().inner().preorder_with_tokens() {
        let (reason, element) = match event {
            WalkEvent::Enter(node) => (VisitReason::Enter, node),
            WalkEvent::Leave(node) => (VisitReason::Exit, node),
        };
        trace!(uri = %document.uri(), ?reason, element = ?element.kind());
        match element.kind() {
            SyntaxKind::RootNode => visitor.document(
                diagnostics,
                reason,
                document,
                document
                    .version()
                    .expect("visited document must have a version"),
            ),
            SyntaxKind::VersionStatementNode => visitor.version_statement(
                diagnostics,
                reason,
                &VersionStatement::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::ImportStatementNode => visitor.import_statement(
                diagnostics,
                reason,
                &ImportStatement::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::ImportAliasNode => {
                // Skip these nodes as they're part of an import statement
            }
            SyntaxKind::StructDefinitionNode => visitor.struct_definition(
                diagnostics,
                reason,
                &StructDefinition::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::TaskDefinitionNode => visitor.task_definition(
                diagnostics,
                reason,
                &TaskDefinition::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::WorkflowDefinitionNode => visitor.workflow_definition(
                diagnostics,
                reason,
                &WorkflowDefinition::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::UnboundDeclNode => visitor.unbound_decl(
                diagnostics,
                reason,
                &UnboundDecl::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::BoundDeclNode => visitor.bound_decl(
                diagnostics,
                reason,
                &BoundDecl::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::PrimitiveTypeNode
            | SyntaxKind::MapTypeNode
            | SyntaxKind::ArrayTypeNode
            | SyntaxKind::PairTypeNode
            | SyntaxKind::ObjectTypeNode
            | SyntaxKind::TypeRefNode => {
                // Skip these nodes as they're part of declarations
            }
            SyntaxKind::InputSectionNode => visitor.input_section(
                diagnostics,
                reason,
                &InputSection::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::OutputSectionNode => visitor.output_section(
                diagnostics,
                reason,
                &OutputSection::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::CommandSectionNode => visitor.command_section(
                diagnostics,
                reason,
                &CommandSection::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::RequirementsSectionNode => visitor.requirements_section(
                diagnostics,
                reason,
                &RequirementsSection::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::TaskHintsSectionNode => visitor.task_hints_section(
                diagnostics,
                reason,
                &TaskHintsSection::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::WorkflowHintsSectionNode => visitor.workflow_hints_section(
                diagnostics,
                reason,
                &WorkflowHintsSection::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::TaskHintsItemNode | SyntaxKind::WorkflowHintsItemNode => {
                // Skip this node as it's part of a hints section
            }
            SyntaxKind::RequirementsItemNode => {
                // Skip this node as it's part of a requirements section
            }
            SyntaxKind::RuntimeSectionNode => visitor.runtime_section(
                diagnostics,
                reason,
                &RuntimeSection::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::RuntimeItemNode => visitor.runtime_item(
                diagnostics,
                reason,
                &RuntimeItem::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::MetadataSectionNode => visitor.metadata_section(
                diagnostics,
                reason,
                &MetadataSection::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::ParameterMetadataSectionNode => visitor.parameter_metadata_section(
                diagnostics,
                reason,
                &ParameterMetadataSection::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::MetadataObjectNode => visitor.metadata_object(
                diagnostics,
                reason,
                &MetadataObject::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::MetadataObjectItemNode => visitor.metadata_object_item(
                diagnostics,
                reason,
                &MetadataObjectItem::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::MetadataArrayNode => visitor.metadata_array(
                diagnostics,
                reason,
                &MetadataArray::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::LiteralNullNode => {
                // Skip these nodes as they're part of a metadata section
            }
            k if Expr::<SyntaxNode>::can_cast(k) => {
                visitor.expr(
                    diagnostics,
                    reason,
                    &Expr::cast(element.into_node().expect(
                        "any element that is able to be turned into an expr should be a node",
                    ))
                    .expect("expr should be built"),
                )
            }
            SyntaxKind::LiteralMapItemNode
            | SyntaxKind::LiteralObjectItemNode
            | SyntaxKind::LiteralStructItemNode
            | SyntaxKind::LiteralHintsItemNode
            | SyntaxKind::LiteralInputItemNode
            | SyntaxKind::LiteralOutputItemNode => {
                // Skip these nodes as they're part of literal expressions
            }
            k @ (SyntaxKind::LiteralIntegerNode
            | SyntaxKind::LiteralFloatNode
            | SyntaxKind::LiteralBooleanNode
            | SyntaxKind::LiteralNoneNode
            | SyntaxKind::LiteralStringNode
            | SyntaxKind::LiteralPairNode
            | SyntaxKind::LiteralArrayNode
            | SyntaxKind::LiteralMapNode
            | SyntaxKind::LiteralObjectNode
            | SyntaxKind::LiteralStructNode
            | SyntaxKind::LiteralHintsNode
            | SyntaxKind::LiteralInputNode
            | SyntaxKind::LiteralOutputNode
            | SyntaxKind::ParenthesizedExprNode
            | SyntaxKind::NameRefExprNode
            | SyntaxKind::IfExprNode
            | SyntaxKind::LogicalNotExprNode
            | SyntaxKind::NegationExprNode
            | SyntaxKind::LogicalOrExprNode
            | SyntaxKind::LogicalAndExprNode
            | SyntaxKind::EqualityExprNode
            | SyntaxKind::InequalityExprNode
            | SyntaxKind::LessExprNode
            | SyntaxKind::LessEqualExprNode
            | SyntaxKind::GreaterExprNode
            | SyntaxKind::GreaterEqualExprNode
            | SyntaxKind::AdditionExprNode
            | SyntaxKind::SubtractionExprNode
            | SyntaxKind::MultiplicationExprNode
            | SyntaxKind::DivisionExprNode
            | SyntaxKind::ModuloExprNode
            | SyntaxKind::CallExprNode
            | SyntaxKind::IndexExprNode
            | SyntaxKind::AccessExprNode) => {
                unreachable!("`{k:?}` should be handled by `Expr::can_cast`")
            }
            SyntaxKind::PlaceholderNode => visitor.placeholder(
                diagnostics,
                reason,
                &Placeholder::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::PlaceholderSepOptionNode
            | SyntaxKind::PlaceholderDefaultOptionNode
            | SyntaxKind::PlaceholderTrueFalseOptionNode => {
                // Skip these nodes as they're part of a placeholder
            }
            SyntaxKind::ConditionalStatementNode => visitor.conditional_statement(
                diagnostics,
                reason,
                &ConditionalStatement::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::ScatterStatementNode => visitor.scatter_statement(
                diagnostics,
                reason,
                &ScatterStatement::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::CallStatementNode => visitor.call_statement(
                diagnostics,
                reason,
                &CallStatement::cast(element.into_node().unwrap()).expect("should cast"),
            ),
            SyntaxKind::CallTargetNode
            | SyntaxKind::CallAliasNode
            | SyntaxKind::CallAfterNode
            | SyntaxKind::CallInputItemNode => {
                // Skip these nodes as they're part of a call statement
            }
            SyntaxKind::Abandoned | SyntaxKind::MAX => {
                unreachable!("node should not exist in the tree")
            }
            SyntaxKind::Whitespace if reason == VisitReason::Enter => visitor.whitespace(
                diagnostics,
                &Whitespace::cast(element.into_token().unwrap()).expect("should cast"),
            ),
            SyntaxKind::Comment if reason == VisitReason::Enter => visitor.comment(
                diagnostics,
                &Comment::cast(element.into_token().unwrap()).expect("should cast"),
            ),
            SyntaxKind::LiteralStringText if reason == VisitReason::Enter => visitor.string_text(
                diagnostics,
                &StringText::cast(element.into_token().unwrap()).expect("should cast"),
            ),
            SyntaxKind::LiteralCommandText if reason == VisitReason::Enter => visitor.command_text(
                diagnostics,
                &CommandText::cast(element.into_token().unwrap()).expect("should cast"),
            ),
            _ => {
                // Skip remaining tokens
            }
        }
    }
}
