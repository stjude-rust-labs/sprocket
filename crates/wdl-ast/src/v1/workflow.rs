//! V1 AST representation for workflows.

use wdl_grammar::SupportedVersion;
use wdl_grammar::version::V1;

use super::BoundDecl;
use super::Expr;
use super::InputSection;
use super::LiteralBoolean;
use super::LiteralFloat;
use super::LiteralInteger;
use super::LiteralString;
use super::MetadataSection;
use super::MetadataValue;
use super::OutputSection;
use super::ParameterMetadataSection;
use crate::AstChildren;
use crate::AstNode;
use crate::AstToken;
use crate::Ident;
use crate::SyntaxElement;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::WorkflowDescriptionLanguage;
use crate::support::child;
use crate::support::children;
use crate::token;

/// Represents a workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowDefinition(pub(crate) SyntaxNode);

impl WorkflowDefinition {
    /// Gets the name of the workflow.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("workflow should have a name")
    }

    /// Gets the items of the workflow.
    pub fn items(&self) -> impl Iterator<Item = WorkflowItem> + use<> {
        WorkflowItem::children(&self.0)
    }

    /// Gets the input section of the workflow.
    pub fn input(&self) -> Option<InputSection> {
        child(&self.0)
    }

    /// Gets the output section of the workflow.
    pub fn output(&self) -> Option<OutputSection> {
        child(&self.0)
    }

    /// Gets the statements of the workflow.
    pub fn statements(&self) -> impl Iterator<Item = WorkflowStatement> + use<> {
        WorkflowStatement::children(&self.0)
    }

    /// Gets the metadata section of the workflow.
    pub fn metadata(&self) -> Option<MetadataSection> {
        child(&self.0)
    }

    /// Gets the parameter section of the workflow.
    pub fn parameter_metadata(&self) -> Option<ParameterMetadataSection> {
        child(&self.0)
    }

    /// Gets the hints section of the workflow.
    pub fn hints(&self) -> Option<WorkflowHintsSection> {
        child(&self.0)
    }

    /// Gets the private declarations of the workflow.
    pub fn declarations(&self) -> AstChildren<BoundDecl> {
        children(&self.0)
    }

    /// Determines if the workflow definition allows nested inputs.
    pub fn allows_nested_inputs(&self, version: SupportedVersion) -> bool {
        match version {
            SupportedVersion::V1(V1::Zero) => return true,
            SupportedVersion::V1(V1::One) => {
                // Fall through to below
            }
            SupportedVersion::V1(V1::Two) => {
                // Check the hints section
                let allow = self.hints().and_then(|s| {
                    s.items().find_map(|i| {
                        if matches!(
                            i.name().as_str(),
                            "allow_nested_inputs" | "allowNestedInputs"
                        ) {
                            match i.value() {
                                WorkflowHintsItemValue::Boolean(v) => Some(v.value()),
                                _ => Some(false),
                            }
                        } else {
                            None
                        }
                    })
                });

                if let Some(allow) = allow {
                    return allow;
                }

                // Fall through to below
            }
            _ => return false,
        }

        // Check the metadata section
        self.metadata()
            .and_then(|s| {
                s.items().find_map(|i| {
                    if i.name().as_str() == "allowNestedInputs" {
                        match i.value() {
                            MetadataValue::Boolean(v) => Some(v.value()),
                            _ => Some(false),
                        }
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(false)
    }
}

impl AstNode for WorkflowDefinition {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::WorkflowDefinitionNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::WorkflowDefinitionNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents an item in a workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkflowItem {
    /// The item is an input section.
    Input(InputSection),
    /// The item is an output section.
    Output(OutputSection),
    /// The item is a conditional statement.
    Conditional(ConditionalStatement),
    /// The item is a scatter statement.
    Scatter(ScatterStatement),
    /// The item is a call statement.
    Call(CallStatement),
    /// The item is a metadata section.
    Metadata(MetadataSection),
    /// The item is a parameter meta section.
    ParameterMetadata(ParameterMetadataSection),
    /// The item is a workflow hints section.
    Hints(WorkflowHintsSection),
    /// The item is a private bound declaration.
    Declaration(BoundDecl),
}

impl WorkflowItem {
    /// Returns whether or not a [`SyntaxKind`] is able to be cast to any of the
    /// underlying members within the [`WorkflowItem`].
    pub fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(
            kind,
            SyntaxKind::InputSectionNode
                | SyntaxKind::OutputSectionNode
                | SyntaxKind::ConditionalStatementNode
                | SyntaxKind::ScatterStatementNode
                | SyntaxKind::CallStatementNode
                | SyntaxKind::MetadataSectionNode
                | SyntaxKind::ParameterMetadataSectionNode
                | SyntaxKind::WorkflowHintsSectionNode
                | SyntaxKind::BoundDeclNode
        )
    }

    /// Attempts to cast the [`SyntaxNode`] to any of the underlying members
    /// within the [`WorkflowItem`].
    pub fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::InputSectionNode => Some(Self::Input(
                InputSection::cast(syntax).expect("input section to cast"),
            )),
            SyntaxKind::OutputSectionNode => Some(Self::Output(
                OutputSection::cast(syntax).expect("output section to cast"),
            )),
            SyntaxKind::ConditionalStatementNode => Some(Self::Conditional(
                ConditionalStatement::cast(syntax).expect("conditional statement to cast"),
            )),
            SyntaxKind::ScatterStatementNode => Some(Self::Scatter(
                ScatterStatement::cast(syntax).expect("scatter statement to cast"),
            )),
            SyntaxKind::CallStatementNode => Some(Self::Call(
                CallStatement::cast(syntax).expect("call statement to cast"),
            )),
            SyntaxKind::MetadataSectionNode => Some(Self::Metadata(
                MetadataSection::cast(syntax).expect("metadata section to cast"),
            )),
            SyntaxKind::ParameterMetadataSectionNode => Some(Self::ParameterMetadata(
                ParameterMetadataSection::cast(syntax).expect("parameter metadata section to cast"),
            )),
            SyntaxKind::WorkflowHintsSectionNode => Some(Self::Hints(
                WorkflowHintsSection::cast(syntax).expect("workflow hints section to cast"),
            )),
            SyntaxKind::BoundDeclNode => Some(Self::Declaration(
                BoundDecl::cast(syntax).expect("bound decl to cast"),
            )),
            _ => None,
        }
    }

    /// Gets a reference to the underlying [`SyntaxNode`].
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Input(element) => element.syntax(),
            Self::Output(element) => element.syntax(),
            Self::Conditional(element) => element.syntax(),
            Self::Scatter(element) => element.syntax(),
            Self::Call(element) => element.syntax(),
            Self::Metadata(element) => element.syntax(),
            Self::ParameterMetadata(element) => element.syntax(),
            Self::Hints(element) => element.syntax(),
            Self::Declaration(element) => element.syntax(),
        }
    }

    /// Attempts to get a reference to the inner [`InputSection`].
    ///
    /// * If `self` is a [`WorkflowItem::Input`], then a reference to the inner
    ///   [`InputSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_input_section(&self) -> Option<&InputSection> {
        match self {
            Self::Input(input_section) => Some(input_section),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`InputSection`].
    ///
    /// * If `self` is a [`WorkflowItem::Input`], then the inner
    ///   [`InputSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_input_section(self) -> Option<InputSection> {
        match self {
            Self::Input(input_section) => Some(input_section),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`OutputSection`].
    ///
    /// * If `self` is a [`WorkflowItem::Output`], then a reference to the inner
    ///   [`OutputSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_output_section(&self) -> Option<&OutputSection> {
        match self {
            Self::Output(output_section) => Some(output_section),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`OutputSection`].
    ///
    /// * If `self` is a [`WorkflowItem::Output`], then the inner
    ///   [`OutputSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_output_section(self) -> Option<OutputSection> {
        match self {
            Self::Output(output_section) => Some(output_section),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`ConditionalStatement`].
    ///
    /// * If `self` is a [`WorkflowItem::Conditional`], then a reference to the
    ///   inner [`ConditionalStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_conditional(&self) -> Option<&ConditionalStatement> {
        match self {
            Self::Conditional(conditional) => Some(conditional),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`ConditionalStatement`].
    ///
    /// * If `self` is a [`WorkflowItem::Conditional`], then the inner
    ///   [`ConditionalStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_conditional(self) -> Option<ConditionalStatement> {
        match self {
            Self::Conditional(conditional) => Some(conditional),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`ScatterStatement`].
    ///
    /// * If `self` is a [`WorkflowItem::Scatter`], then a reference to the
    ///   inner [`ScatterStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_scatter(&self) -> Option<&ScatterStatement> {
        match self {
            Self::Scatter(scatter) => Some(scatter),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`ScatterStatement`].
    ///
    /// * If `self` is a [`WorkflowItem::Scatter`], then the inner
    ///   [`ScatterStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_scatter(self) -> Option<ScatterStatement> {
        match self {
            Self::Scatter(scatter) => Some(scatter),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`CallStatement`].
    ///
    /// * If `self` is a [`WorkflowItem::Call`], then a reference to the inner
    ///   [`CallStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_call(&self) -> Option<&CallStatement> {
        match self {
            Self::Call(call) => Some(call),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`CallStatement`].
    ///
    /// * If `self` is a [`WorkflowItem::Call`], then the inner
    ///   [`CallStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_call(self) -> Option<CallStatement> {
        match self {
            Self::Call(call) => Some(call),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`MetadataSection`].
    ///
    /// * If `self` is a [`WorkflowItem::Metadata`], then a reference to the
    ///   inner [`MetadataSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_metadata_section(&self) -> Option<&MetadataSection> {
        match self {
            Self::Metadata(metadata_section) => Some(metadata_section),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`MetadataSection`].
    ///
    /// * If `self` is a [`WorkflowItem::Metadata`], then the inner
    ///   [`MetadataSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_metadata_section(self) -> Option<MetadataSection> {
        match self {
            Self::Metadata(metadata_section) => Some(metadata_section),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`ParameterMetadataSection`].
    ///
    /// * If `self` is a [`WorkflowItem::ParameterMetadata`], then a reference
    ///   to the inner [`ParameterMetadataSection`] is returned wrapped in
    ///   [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_parameter_metadata_section(&self) -> Option<&ParameterMetadataSection> {
        match self {
            Self::ParameterMetadata(parameter_metadata_section) => Some(parameter_metadata_section),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`ParameterMetadataSection`].
    ///
    /// * If `self` is a [`WorkflowItem::ParameterMetadata`], then the inner
    ///   [`ParameterMetadataSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_parameter_metadata_section(self) -> Option<ParameterMetadataSection> {
        match self {
            Self::ParameterMetadata(parameter_metadata_section) => Some(parameter_metadata_section),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`WorkflowHintsSection`].
    ///
    /// * If `self` is a [`WorkflowItem::Hints`], then a reference to the inner
    ///   [`WorkflowHintsSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_hints_section(&self) -> Option<&WorkflowHintsSection> {
        match self {
            Self::Hints(hints_section) => Some(hints_section),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`WorkflowHintsSection`].
    ///
    /// * If `self` is a [`WorkflowItem::Hints`], then the inner
    ///   [`WorkflowHintsSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_hints_section(self) -> Option<WorkflowHintsSection> {
        match self {
            Self::Hints(hints_section) => Some(hints_section),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`BoundDecl`].
    ///
    /// * If `self` is a [`WorkflowItem::Declaration`], then a reference to the
    ///   inner [`BoundDecl`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_declaration(&self) -> Option<&BoundDecl> {
        match self {
            Self::Declaration(declaration) => Some(declaration),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`BoundDecl`].
    ///
    /// * If `self` is a [`WorkflowItem::Declaration`], then the inner
    ///   [`BoundDecl`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_declaration(self) -> Option<BoundDecl> {
        match self {
            Self::Declaration(declaration) => Some(declaration),
            _ => None,
        }
    }

    /// Finds the first child that can be cast to an [`WorkflowItem`].
    ///
    /// This is meant to emulate the functionality of
    /// [`rowan::ast::support::child`] without requiring [`WorkflowItem`] to
    /// implement the `AstNode` trait.
    pub fn child(syntax: &SyntaxNode) -> Option<Self> {
        syntax.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to an [`WorkflowItem`].
    ///
    /// This is meant to emulate the functionality of
    /// [`rowan::ast::support::children`] without requiring [`WorkflowItem`] to
    /// implement the `AstNode` trait.
    pub fn children(syntax: &SyntaxNode) -> impl Iterator<Item = WorkflowItem> + use<> {
        syntax.children().filter_map(Self::cast)
    }
}

/// Represents a statement in a workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkflowStatement {
    /// The statement is a conditional statement.
    Conditional(ConditionalStatement),
    /// The statement is a scatter statement.
    Scatter(ScatterStatement),
    /// The statement is a call statement.
    Call(CallStatement),
    /// The statement is a private bound declaration.
    Declaration(BoundDecl),
}

impl WorkflowStatement {
    /// Returns whether or not a [`SyntaxKind`] is able to be cast to any of the
    /// underlying members within the [`WorkflowStatement`].
    pub fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(
            kind,
            SyntaxKind::ConditionalStatementNode
                | SyntaxKind::ScatterStatementNode
                | SyntaxKind::CallStatementNode
                | SyntaxKind::BoundDeclNode
        )
    }

    /// Attempts to cast the [`SyntaxNode`] to any of the underlying members
    /// within the [`WorkflowStatement`].
    pub fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::ConditionalStatementNode => Some(Self::Conditional(
                ConditionalStatement::cast(syntax).expect("conditional statement to cast"),
            )),
            SyntaxKind::ScatterStatementNode => Some(Self::Scatter(
                ScatterStatement::cast(syntax).expect("scatter statement to cast"),
            )),
            SyntaxKind::CallStatementNode => Some(Self::Call(
                CallStatement::cast(syntax).expect("call statement to cast"),
            )),
            SyntaxKind::BoundDeclNode => Some(Self::Declaration(
                BoundDecl::cast(syntax).expect("bound decl to cast"),
            )),
            _ => None,
        }
    }

    /// Gets a reference to the underlying [`SyntaxNode`].
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Conditional(element) => element.syntax(),
            Self::Scatter(element) => element.syntax(),
            Self::Call(element) => element.syntax(),
            Self::Declaration(element) => element.syntax(),
        }
    }

    /// Attempts to get a reference to the inner [`ConditionalStatement`].
    ///
    /// * If `self` is a [`WorkflowStatement::Conditional`], then a reference to
    ///   the inner [`ConditionalStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_conditional(&self) -> Option<&ConditionalStatement> {
        match self {
            Self::Conditional(conditional) => Some(conditional),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`ConditionalStatement`].
    ///
    /// * If `self` is a [`WorkflowStatement::Conditional`], then the inner
    ///   [`ConditionalStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_conditional(self) -> Option<ConditionalStatement> {
        match self {
            Self::Conditional(conditional) => Some(conditional),
            _ => None,
        }
    }

    /// Unwraps the statement into a conditional statement.
    ///
    /// # Panics
    ///
    /// Panics if the statement is not a conditional statement.
    pub fn unwrap_conditional(self) -> ConditionalStatement {
        match self {
            Self::Conditional(stmt) => stmt,
            _ => panic!("not a conditional statement"),
        }
    }

    /// Attempts to get a reference to the inner [`ScatterStatement`].
    ///
    /// * If `self` is a [`WorkflowStatement::Scatter`], then a reference to the
    ///   inner [`ScatterStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_scatter(&self) -> Option<&ScatterStatement> {
        match self {
            Self::Scatter(scatter) => Some(scatter),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`ScatterStatement`].
    ///
    /// * If `self` is a [`WorkflowStatement::Scatter`], then the inner
    ///   [`ScatterStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_scatter(self) -> Option<ScatterStatement> {
        match self {
            Self::Scatter(scatter) => Some(scatter),
            _ => None,
        }
    }

    /// Unwraps the statement into a scatter statement.
    ///
    /// # Panics
    ///
    /// Panics if the statement is not a scatter statement.
    pub fn unwrap_scatter(self) -> ScatterStatement {
        match self {
            Self::Scatter(stmt) => stmt,
            _ => panic!("not a scatter statement"),
        }
    }

    /// Attempts to get a reference to the inner [`CallStatement`].
    ///
    /// * If `self` is a [`WorkflowStatement::Call`], then a reference to the
    ///   inner [`CallStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_call(&self) -> Option<&CallStatement> {
        match self {
            Self::Call(call) => Some(call),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`CallStatement`].
    ///
    /// * If `self` is a [`WorkflowStatement::Call`], then the inner
    ///   [`CallStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_call(self) -> Option<CallStatement> {
        match self {
            Self::Call(call) => Some(call),
            _ => None,
        }
    }

    /// Unwraps the statement into a call statement.
    ///
    /// # Panics
    ///
    /// Panics if the statement is not a call statement.
    pub fn unwrap_call(self) -> CallStatement {
        match self {
            Self::Call(stmt) => stmt,
            _ => panic!("not a call statement"),
        }
    }

    /// Attempts to get a reference to the inner [`BoundDecl`].
    ///
    /// * If `self` is a [`WorkflowStatement::Declaration`], then a reference to
    ///   the inner [`BoundDecl`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_declaration(&self) -> Option<&BoundDecl> {
        match self {
            Self::Declaration(declaration) => Some(declaration),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`BoundDecl`].
    ///
    /// * If `self` is a [`WorkflowStatement::Declaration`], then the inner
    ///   [`BoundDecl`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_declaration(self) -> Option<BoundDecl> {
        match self {
            Self::Declaration(declaration) => Some(declaration),
            _ => None,
        }
    }

    /// Unwraps the statement into a bound declaration.
    ///
    /// # Panics
    ///
    /// Panics if the statement is not a bound declaration.
    pub fn unwrap_declaration(self) -> BoundDecl {
        match self {
            Self::Declaration(declaration) => declaration,
            _ => panic!("not a bound declaration"),
        }
    }

    /// Finds the first child that can be cast to an [`WorkflowStatement`].
    ///
    /// This is meant to emulate the functionality of
    /// [`rowan::ast::support::child`] without requiring [`WorkflowStatement`]
    /// to implement the `AstNode` trait.
    pub fn child(syntax: &SyntaxNode) -> Option<Self> {
        syntax.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to an [`WorkflowStatement`].
    ///
    /// This is meant to emulate the functionality of
    /// [`rowan::ast::support::children`] without requiring
    /// [`WorkflowStatement`] to implement the `AstNode` trait.
    pub fn children(syntax: &SyntaxNode) -> impl Iterator<Item = WorkflowStatement> + use<> {
        syntax.children().filter_map(Self::cast)
    }
}

/// Represents a workflow conditional statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConditionalStatement(pub(crate) SyntaxNode);

impl ConditionalStatement {
    /// Gets the expression of the conditional statement
    pub fn expr(&self) -> Expr {
        Expr::child(&self.0).expect("expected a conditional expression")
    }

    /// Gets the statements of the conditional body.
    pub fn statements(&self) -> impl Iterator<Item = WorkflowStatement> + use<> {
        WorkflowStatement::children(&self.0)
    }
}

impl AstNode for ConditionalStatement {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::ConditionalStatementNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::ConditionalStatementNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a workflow scatter statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScatterStatement(pub(crate) SyntaxNode);

impl ScatterStatement {
    /// Gets the scatter variable identifier.
    pub fn variable(&self) -> Ident {
        token(&self.0).expect("expected a scatter variable identifier")
    }

    /// Gets the scatter expression.
    pub fn expr(&self) -> Expr {
        Expr::child(&self.0).expect("expected a scatter expression")
    }

    /// Gets the statements of the scatter body.
    pub fn statements(&self) -> impl Iterator<Item = WorkflowStatement> + use<> {
        WorkflowStatement::children(&self.0)
    }
}

impl AstNode for ScatterStatement {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::ScatterStatementNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::ScatterStatementNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a workflow call statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallStatement(pub(crate) SyntaxNode);

impl CallStatement {
    /// Gets the target of the call.
    pub fn target(&self) -> CallTarget {
        child(&self.0).expect("expected a call target")
    }

    /// Gets the optional alias for the call.
    pub fn alias(&self) -> Option<CallAlias> {
        child(&self.0)
    }

    /// Gets the after clauses for the call statement.
    pub fn after(&self) -> AstChildren<CallAfter> {
        children(&self.0)
    }

    /// Gets the inputs for the call statement.
    pub fn inputs(&self) -> AstChildren<CallInputItem> {
        children(&self.0)
    }
}

impl AstNode for CallStatement {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::CallStatementNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::CallStatementNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a target in a call statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallTarget(SyntaxNode);

impl CallTarget {
    /// Gets an iterator of the names of the call target.
    ///
    /// The last name in the iteration is considered to be the task or workflow
    /// being called.
    pub fn names(&self) -> impl Iterator<Item = Ident> + use<> {
        self.0
            .children_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .filter_map(Ident::cast)
    }
}

impl AstNode for CallTarget {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::CallTargetNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::CallTargetNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents an alias in a call statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallAlias(SyntaxNode);

impl CallAlias {
    /// Gets the alias name.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("expected a alias identifier")
    }
}

impl AstNode for CallAlias {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::CallAliasNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::CallAliasNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents an after clause in a call statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallAfter(SyntaxNode);

impl CallAfter {
    /// Gets the name from the `after` clause.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("expected an after identifier")
    }
}

impl AstNode for CallAfter {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::CallAfterNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::CallAfterNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents an input item in a call statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallInputItem(SyntaxNode);

impl CallInputItem {
    /// Gets the name of the input.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("expected an input name")
    }

    /// The optional expression for the input.
    pub fn expr(&self) -> Option<Expr> {
        Expr::child(&self.0)
    }

    /// Gets the call statement for the call input item.
    pub fn parent(&self) -> CallStatement {
        CallStatement::cast(self.0.parent().expect("should have parent")).expect("node should cast")
    }
}

impl AstNode for CallInputItem {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::CallInputItemNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::CallInputItemNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a hints section in a workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowHintsSection(pub(crate) SyntaxNode);

impl WorkflowHintsSection {
    /// Gets the items in the hints section.
    pub fn items(&self) -> AstChildren<WorkflowHintsItem> {
        children(&self.0)
    }

    /// Gets the parent of the hints section.
    pub fn parent(&self) -> WorkflowDefinition {
        WorkflowDefinition::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }
}

impl AstNode for WorkflowHintsSection {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::WorkflowHintsSectionNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::WorkflowHintsSectionNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents an item in a workflow hints section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowHintsItem(SyntaxNode);

impl WorkflowHintsItem {
    /// Gets the name of the hints item.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("expected an item name")
    }

    /// Gets the value of the hints item.
    pub fn value(&self) -> WorkflowHintsItemValue {
        child(&self.0).expect("expected an item value")
    }
}

impl AstNode for WorkflowHintsItem {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::WorkflowHintsItemNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::WorkflowHintsItemNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a workflow hints item value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkflowHintsItemValue {
    /// The value is a literal boolean.
    Boolean(LiteralBoolean),
    /// The value is a literal integer.
    Integer(LiteralInteger),
    /// The value is a literal float.
    Float(LiteralFloat),
    /// The value is a literal string.
    String(LiteralString),
    /// The value is a literal object.
    Object(WorkflowHintsObject),
    /// The value is a literal array.
    Array(WorkflowHintsArray),
}

impl WorkflowHintsItemValue {
    /// Unwraps the value into a boolean.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a boolean.
    pub fn unwrap_boolean(self) -> LiteralBoolean {
        match self {
            Self::Boolean(b) => b,
            _ => panic!("not a boolean"),
        }
    }

    /// Unwraps the value into an integer.
    ///
    /// # Panics
    ///
    /// Panics if the value is not an integer.
    pub fn unwrap_integer(self) -> LiteralInteger {
        match self {
            Self::Integer(i) => i,
            _ => panic!("not an integer"),
        }
    }

    /// Unwraps the value into a float.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a float.
    pub fn unwrap_float(self) -> LiteralFloat {
        match self {
            Self::Float(f) => f,
            _ => panic!("not a float"),
        }
    }

    /// Unwraps the value into a string.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a string.
    pub fn unwrap_string(self) -> LiteralString {
        match self {
            Self::String(s) => s,
            _ => panic!("not a string"),
        }
    }

    /// Unwraps the value into an object.
    ///
    /// # Panics
    ///
    /// Panics if the value is not an object.
    pub fn unwrap_object(self) -> WorkflowHintsObject {
        match self {
            Self::Object(o) => o,
            _ => panic!("not an object"),
        }
    }

    /// Unwraps the value into an array.
    ///
    /// # Panics
    ///
    /// Panics if the value is not an array.
    pub fn unwrap_array(self) -> WorkflowHintsArray {
        match self {
            Self::Array(a) => a,
            _ => panic!("not an array"),
        }
    }
}

impl AstNode for WorkflowHintsItemValue {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        matches!(
            kind,
            SyntaxKind::LiteralBooleanNode
                | SyntaxKind::LiteralIntegerNode
                | SyntaxKind::LiteralFloatNode
                | SyntaxKind::LiteralStringNode
                | SyntaxKind::WorkflowHintsObjectNode
                | SyntaxKind::WorkflowHintsArrayNode
        )
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::LiteralBooleanNode => Some(Self::Boolean(LiteralBoolean(syntax))),
            SyntaxKind::LiteralIntegerNode => Some(Self::Integer(LiteralInteger(syntax))),
            SyntaxKind::LiteralFloatNode => Some(Self::Float(LiteralFloat(syntax))),
            SyntaxKind::LiteralStringNode => Some(Self::String(LiteralString(syntax))),
            SyntaxKind::WorkflowHintsObjectNode => Some(Self::Object(WorkflowHintsObject(syntax))),
            SyntaxKind::WorkflowHintsArrayNode => Some(Self::Array(WorkflowHintsArray(syntax))),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Boolean(b) => &b.0,
            Self::Integer(i) => &i.0,
            Self::Float(f) => &f.0,
            Self::String(s) => &s.0,
            Self::Object(o) => &o.0,
            Self::Array(a) => &a.0,
        }
    }
}

/// Represents a workflow hints object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowHintsObject(pub(crate) SyntaxNode);

impl WorkflowHintsObject {
    /// Gets the items of the workflow hints object.
    pub fn items(&self) -> AstChildren<WorkflowHintsObjectItem> {
        children(&self.0)
    }
}

impl AstNode for WorkflowHintsObject {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::WorkflowHintsObjectNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::WorkflowHintsObjectNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a workflow hints object item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowHintsObjectItem(pub(crate) SyntaxNode);

impl WorkflowHintsObjectItem {
    /// Gets the name of the item.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("expected a name")
    }

    /// Gets the value of the item.
    pub fn value(&self) -> WorkflowHintsItemValue {
        child(&self.0).expect("expected a value")
    }
}

impl AstNode for WorkflowHintsObjectItem {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::WorkflowHintsObjectItemNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::WorkflowHintsObjectItemNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

/// Represents a workflow hints array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowHintsArray(pub(crate) SyntaxNode);

impl WorkflowHintsArray {
    /// Gets the elements of the workflow hints array.
    pub fn elements(&self) -> AstChildren<WorkflowHintsItemValue> {
        children(&self.0)
    }
}

impl AstNode for WorkflowHintsArray {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized,
    {
        kind == SyntaxKind::WorkflowHintsArrayNode
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::WorkflowHintsArrayNode => Some(Self(syntax)),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Document;
    use crate::SupportedVersion;
    use crate::VisitReason;
    use crate::Visitor;
    use crate::v1::UnboundDecl;

    #[test]
    fn workflows() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.1

workflow test {
    input {
        String name
        Boolean do_thing
    }

    output {
        String output = "hello, ~{name}!"
    }

    if (do_thing) {
        call foo.my_task

        scatter (a in [1, 2, 3]) {
            call my_task as my_task2 { input: a }
        }
    }

    call my_task as my_task3 after my_task2 after my_task { input: a = 1 }

    scatter (a in ["1", "2", "3"]) {
        # Do nothing
    }

    meta {
        description: "a test"
        foo: null
    }

    parameter_meta {
        name: {
            help: "a name to greet"
        }
    }

    hints {
        foo: "bar"
    }

    String x = "private"
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let workflows: Vec<_> = ast.workflows().collect();
        assert_eq!(workflows.len(), 1);
        assert_eq!(workflows[0].name().as_str(), "test");

        // Workflow inputs
        let input = workflows[0]
            .input()
            .expect("workflow should have an input section");
        assert_eq!(input.parent().unwrap_workflow().name().as_str(), "test");
        let decls: Vec<_> = input.declarations().collect();
        assert_eq!(decls.len(), 2);

        // First declaration
        assert_eq!(
            decls[0].clone().unwrap_unbound_decl().ty().to_string(),
            "String"
        );
        assert_eq!(
            decls[0].clone().unwrap_unbound_decl().name().as_str(),
            "name"
        );

        // Second declaration
        assert_eq!(
            decls[1].clone().unwrap_unbound_decl().ty().to_string(),
            "Boolean"
        );
        assert_eq!(
            decls[1].clone().unwrap_unbound_decl().name().as_str(),
            "do_thing"
        );

        // Workflow outputs
        let output = workflows[0]
            .output()
            .expect("workflow should have an output section");
        assert_eq!(output.parent().unwrap_workflow().name().as_str(), "test");
        let decls: Vec<_> = output.declarations().collect();
        assert_eq!(decls.len(), 1);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "String");
        assert_eq!(decls[0].name().as_str(), "output");
        let parts: Vec<_> = decls[0]
            .expr()
            .unwrap_literal()
            .unwrap_string()
            .parts()
            .collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].clone().unwrap_text().as_str(), "hello, ");
        assert_eq!(
            parts[1]
                .clone()
                .unwrap_placeholder()
                .expr()
                .unwrap_name_ref()
                .name()
                .as_str(),
            "name"
        );
        assert_eq!(parts[2].clone().unwrap_text().as_str(), "!");

        // Workflow statements
        let statements: Vec<_> = workflows[0].statements().collect();
        assert_eq!(statements.len(), 4);

        // First workflow statement
        let conditional = statements[0].clone().unwrap_conditional();
        assert_eq!(
            conditional.expr().unwrap_name_ref().name().as_str(),
            "do_thing"
        );

        // Inner statements
        let inner: Vec<_> = conditional.statements().collect();
        assert_eq!(inner.len(), 2);

        // First inner statement
        let call = inner[0].clone().unwrap_call();
        let names = call.target().names().collect::<Vec<_>>();
        assert_eq!(names.len(), 2);
        assert_eq!(names[0].as_str(), "foo");
        assert_eq!(names[1].as_str(), "my_task");
        assert!(call.alias().is_none());
        assert_eq!(call.after().count(), 0);
        assert_eq!(call.inputs().count(), 0);

        // Second inner statement
        let scatter = inner[1].clone().unwrap_scatter();
        assert_eq!(scatter.variable().as_str(), "a");
        let elements: Vec<_> = scatter
            .expr()
            .unwrap_literal()
            .unwrap_array()
            .elements()
            .collect();
        assert_eq!(elements.len(), 3);
        assert_eq!(
            elements[0]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );
        assert_eq!(
            elements[1]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            2
        );
        assert_eq!(
            elements[2]
                .clone()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            3
        );

        // Inner statements
        let inner: Vec<_> = scatter.statements().collect();
        assert_eq!(inner.len(), 1);

        // First inner statement
        let call = inner[0].clone().unwrap_call();
        let names = call.target().names().collect::<Vec<_>>();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].as_str(), "my_task");
        assert_eq!(call.alias().unwrap().name().as_str(), "my_task2");
        assert_eq!(call.after().count(), 0);
        let inputs: Vec<_> = call.inputs().collect();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].name().as_str(), "a");
        assert!(inputs[0].expr().is_none());

        // Second workflow statement
        let call = statements[1].clone().unwrap_call();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].as_str(), "my_task");
        assert_eq!(call.alias().unwrap().name().as_str(), "my_task3");
        let after: Vec<_> = call.after().collect();
        assert_eq!(after.len(), 2);
        assert_eq!(after[0].name().as_str(), "my_task2");
        assert_eq!(after[1].name().as_str(), "my_task");
        let inputs: Vec<_> = call.inputs().collect();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].name().as_str(), "a");
        assert_eq!(
            inputs[0]
                .expr()
                .unwrap()
                .unwrap_literal()
                .unwrap_integer()
                .value()
                .unwrap(),
            1
        );

        // Third workflow statement
        let scatter = statements[2].clone().unwrap_scatter();
        assert_eq!(scatter.variable().as_str(), "a");
        let elements: Vec<_> = scatter
            .expr()
            .unwrap_literal()
            .unwrap_array()
            .elements()
            .collect();
        assert_eq!(elements.len(), 3);
        assert_eq!(
            elements[0]
                .clone()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .as_str(),
            "1"
        );
        assert_eq!(
            elements[1]
                .clone()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .as_str(),
            "2"
        );
        assert_eq!(
            elements[2]
                .clone()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .as_str(),
            "3"
        );

        // Inner statements
        let inner: Vec<_> = scatter.statements().collect();
        assert_eq!(inner.len(), 0);

        // Workflow metadata
        let metadata = workflows[0]
            .metadata()
            .expect("workflow should have a metadata section");
        assert_eq!(metadata.parent().unwrap_workflow().name().as_str(), "test");
        let items: Vec<_> = metadata.items().collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name().as_str(), "description");
        assert_eq!(
            items[0].value().unwrap_string().text().unwrap().as_str(),
            "a test"
        );
        assert_eq!(items[1].name().as_str(), "foo");
        items[1].value().unwrap_null();

        // Workflow parameter metadata
        let param_meta = workflows[0]
            .parameter_metadata()
            .expect("workflow should have a parameter metadata section");
        assert_eq!(
            param_meta.parent().unwrap_workflow().name().as_str(),
            "test"
        );
        let items: Vec<_> = param_meta.items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().as_str(), "name");
        let items: Vec<_> = items[0].value().unwrap_object().items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().as_str(), "help");
        assert_eq!(
            items[0].value().unwrap_string().text().unwrap().as_str(),
            "a name to greet"
        );

        // Workflow hints
        let hints = workflows[0]
            .hints()
            .expect("workflow should have a hints section");
        assert_eq!(hints.parent().name().as_str(), "test");
        let items: Vec<_> = hints.items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().as_str(), "foo");
        assert_eq!(
            items[0].value().unwrap_string().text().unwrap().as_str(),
            "bar"
        );

        // Workflow declarations
        let decls: Vec<_> = workflows[0].declarations().collect();
        assert_eq!(decls.len(), 1);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "String");
        assert_eq!(decls[0].name().as_str(), "x");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .as_str(),
            "private"
        );

        #[derive(Default)]
        struct MyVisitor {
            workflows: usize,
            inputs: usize,
            outputs: usize,
            conditionals: usize,
            scatters: usize,
            calls: usize,
            metadata: usize,
            param_metadata: usize,
            unbound_decls: usize,
            bound_decls: usize,
        }

        impl Visitor for MyVisitor {
            type State = ();

            fn document(
                &mut self,
                _: &mut Self::State,
                _: VisitReason,
                _: &Document,
                _: SupportedVersion,
            ) {
            }

            fn workflow_definition(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &WorkflowDefinition,
            ) {
                if reason == VisitReason::Enter {
                    self.workflows += 1;
                }
            }

            fn input_section(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &InputSection,
            ) {
                if reason == VisitReason::Enter {
                    self.inputs += 1;
                }
            }

            fn output_section(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &OutputSection,
            ) {
                if reason == VisitReason::Enter {
                    self.outputs += 1;
                }
            }

            fn conditional_statement(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &ConditionalStatement,
            ) {
                if reason == VisitReason::Enter {
                    self.conditionals += 1;
                }
            }

            fn scatter_statement(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &ScatterStatement,
            ) {
                if reason == VisitReason::Enter {
                    self.scatters += 1;
                }
            }

            fn call_statement(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &CallStatement,
            ) {
                if reason == VisitReason::Enter {
                    self.calls += 1;
                }
            }

            fn metadata_section(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &MetadataSection,
            ) {
                if reason == VisitReason::Enter {
                    self.metadata += 1;
                }
            }

            fn parameter_metadata_section(
                &mut self,
                _: &mut Self::State,
                reason: VisitReason,
                _: &ParameterMetadataSection,
            ) {
                if reason == VisitReason::Enter {
                    self.param_metadata += 1;
                }
            }

            fn bound_decl(&mut self, _: &mut Self::State, reason: VisitReason, _: &BoundDecl) {
                if reason == VisitReason::Enter {
                    self.bound_decls += 1;
                }
            }

            fn unbound_decl(&mut self, _: &mut Self::State, reason: VisitReason, _: &UnboundDecl) {
                if reason == VisitReason::Enter {
                    self.unbound_decls += 1;
                }
            }
        }

        let mut visitor = MyVisitor::default();
        document.visit(&mut (), &mut visitor);
        assert_eq!(visitor.workflows, 1);
        assert_eq!(visitor.inputs, 1);
        assert_eq!(visitor.outputs, 1);
        assert_eq!(visitor.conditionals, 1);
        assert_eq!(visitor.scatters, 2);
        assert_eq!(visitor.calls, 3);
        assert_eq!(visitor.metadata, 1);
        assert_eq!(visitor.param_metadata, 1);
        assert_eq!(visitor.unbound_decls, 2);
        assert_eq!(visitor.bound_decls, 2);
    }
}
