//! V1 AST representation for workflows.

use std::fmt;

use rowan::NodeOrToken;
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
use crate::AstNode;
use crate::AstToken;
use crate::Ident;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::TreeNode;
use crate::v1::display::write_input_section;
use crate::v1::display::write_output_section;

/// The name of the `allow_nested_inputs` workflow hint. Note that this
/// is not a standard WDL v1.1 hint, but is used in WDL >=v1.2.
pub const WORKFLOW_HINT_ALLOW_NESTED_INPUTS: &str = "allow_nested_inputs";

/// The alias of the `allow_nested_inputs` workflow hint (e.g.
/// `allowNestedInputs`). Note that in WDL v1.1, this is the only
/// form of the hint.
pub const WORKFLOW_HINT_ALLOW_NESTED_INPUTS_ALIAS: &str = "allowNestedInputs";

/// The set of all valid workflow hints section keys.
pub const WORKFLOW_HINT_KEYS: &[(&str, &str)] = &[(
    WORKFLOW_HINT_ALLOW_NESTED_INPUTS,
    "If `true`, allows nested input objects for the workflow.",
)];

/// Represents a workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowDefinition<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> WorkflowDefinition<N> {
    /// Gets the name of the workflow.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("workflow should have a name")
    }

    /// Gets the items of the workflow.
    pub fn items(&self) -> impl Iterator<Item = WorkflowItem<N>> + use<'_, N> {
        WorkflowItem::children(&self.0)
    }

    /// Gets the input section of the workflow.
    pub fn input(&self) -> Option<InputSection<N>> {
        self.child()
    }

    /// Gets the output section of the workflow.
    pub fn output(&self) -> Option<OutputSection<N>> {
        self.child()
    }

    /// Gets the statements of the workflow.
    pub fn statements(&self) -> impl Iterator<Item = WorkflowStatement<N>> + use<'_, N> {
        WorkflowStatement::children(&self.0)
    }

    /// Gets the metadata section of the workflow.
    pub fn metadata(&self) -> Option<MetadataSection<N>> {
        self.child()
    }

    /// Gets the parameter section of the workflow.
    pub fn parameter_metadata(&self) -> Option<ParameterMetadataSection<N>> {
        self.child()
    }

    /// Gets the hints section of the workflow.
    pub fn hints(&self) -> Option<WorkflowHintsSection<N>> {
        self.child()
    }

    /// Gets the private declarations of the workflow.
    pub fn declarations(&self) -> impl Iterator<Item = BoundDecl<N>> + use<'_, N> {
        self.children()
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
                        let name = i.name();
                        if name.text() == WORKFLOW_HINT_ALLOW_NESTED_INPUTS
                            || name.text() == WORKFLOW_HINT_ALLOW_NESTED_INPUTS_ALIAS
                        {
                            match i.value() {
                                WorkflowHintsItemValue::Boolean(v) => Some(v.value()),
                                _ => None,
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
                    if i.name().text() == WORKFLOW_HINT_ALLOW_NESTED_INPUTS_ALIAS {
                        match i.value() {
                            MetadataValue::Boolean(v) => Some(v.value()),
                            _ => None,
                        }
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(false)
    }

    /// Writes a Markdown formatted description of the workflow.
    pub fn markdown_description(&self, f: &mut impl fmt::Write) -> fmt::Result {
        writeln!(f, "```wdl\nworkflow {}\n```\n---", self.name().text())?;

        if let Some(meta) = self.metadata()
            && let Some(desc) = meta.items().find(|i| i.name().text() == "description")
            && let MetadataValue::String(s) = desc.value()
            && let Some(text) = s.text()
        {
            writeln!(f, "# {}\n", text.text())?;
        }

        write_input_section(f, self.input().as_ref(), self.parameter_metadata().as_ref())?;
        write_output_section(
            f,
            self.output().as_ref(),
            self.parameter_metadata().as_ref(),
        )?;

        Ok(())
    }
}

impl<N: TreeNode> AstNode<N> for WorkflowDefinition<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::WorkflowDefinitionNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::WorkflowDefinitionNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an item in a workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkflowItem<N: TreeNode = SyntaxNode> {
    /// The item is an input section.
    Input(InputSection<N>),
    /// The item is an output section.
    Output(OutputSection<N>),
    /// The item is a conditional statement.
    Conditional(ConditionalStatement<N>),
    /// The item is a scatter statement.
    Scatter(ScatterStatement<N>),
    /// The item is a call statement.
    Call(CallStatement<N>),
    /// The item is a metadata section.
    Metadata(MetadataSection<N>),
    /// The item is a parameter meta section.
    ParameterMetadata(ParameterMetadataSection<N>),
    /// The item is a workflow hints section.
    Hints(WorkflowHintsSection<N>),
    /// The item is a private bound declaration.
    Declaration(BoundDecl<N>),
}

impl<N: TreeNode> WorkflowItem<N> {
    /// Returns whether or not the given syntax kind can be cast to
    /// [`WorkflowItem`].
    pub fn can_cast(kind: SyntaxKind) -> bool {
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

    /// Casts the given node to [`WorkflowItem`].
    ///
    /// Returns `None` if the node cannot be cast.
    pub fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::InputSectionNode => Some(Self::Input(
                InputSection::cast(inner).expect("input section to cast"),
            )),
            SyntaxKind::OutputSectionNode => Some(Self::Output(
                OutputSection::cast(inner).expect("output section to cast"),
            )),
            SyntaxKind::ConditionalStatementNode => Some(Self::Conditional(
                ConditionalStatement::cast(inner).expect("conditional statement to cast"),
            )),
            SyntaxKind::ScatterStatementNode => Some(Self::Scatter(
                ScatterStatement::cast(inner).expect("scatter statement to cast"),
            )),
            SyntaxKind::CallStatementNode => Some(Self::Call(
                CallStatement::cast(inner).expect("call statement to cast"),
            )),
            SyntaxKind::MetadataSectionNode => Some(Self::Metadata(
                MetadataSection::cast(inner).expect("metadata section to cast"),
            )),
            SyntaxKind::ParameterMetadataSectionNode => Some(Self::ParameterMetadata(
                ParameterMetadataSection::cast(inner).expect("parameter metadata section to cast"),
            )),
            SyntaxKind::WorkflowHintsSectionNode => Some(Self::Hints(
                WorkflowHintsSection::cast(inner).expect("workflow hints section to cast"),
            )),
            SyntaxKind::BoundDeclNode => Some(Self::Declaration(
                BoundDecl::cast(inner).expect("bound decl to cast"),
            )),
            _ => None,
        }
    }

    /// Gets a reference to the inner node.
    pub fn inner(&self) -> &N {
        match self {
            Self::Input(element) => element.inner(),
            Self::Output(element) => element.inner(),
            Self::Conditional(element) => element.inner(),
            Self::Scatter(element) => element.inner(),
            Self::Call(element) => element.inner(),
            Self::Metadata(element) => element.inner(),
            Self::ParameterMetadata(element) => element.inner(),
            Self::Hints(element) => element.inner(),
            Self::Declaration(element) => element.inner(),
        }
    }

    /// Attempts to get a reference to the inner [`InputSection`].
    ///
    /// * If `self` is a [`WorkflowItem::Input`], then a reference to the inner
    ///   [`InputSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_input_section(&self) -> Option<&InputSection<N>> {
        match self {
            Self::Input(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`InputSection`].
    ///
    /// * If `self` is a [`WorkflowItem::Input`], then the inner
    ///   [`InputSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_input_section(self) -> Option<InputSection<N>> {
        match self {
            Self::Input(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`OutputSection`].
    ///
    /// * If `self` is a [`WorkflowItem::Output`], then a reference to the inner
    ///   [`OutputSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_output_section(&self) -> Option<&OutputSection<N>> {
        match self {
            Self::Output(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`OutputSection`].
    ///
    /// * If `self` is a [`WorkflowItem::Output`], then the inner
    ///   [`OutputSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_output_section(self) -> Option<OutputSection<N>> {
        match self {
            Self::Output(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`ConditionalStatement`].
    ///
    /// * If `self` is a [`WorkflowItem::Conditional`], then a reference to the
    ///   inner [`ConditionalStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_conditional(&self) -> Option<&ConditionalStatement<N>> {
        match self {
            Self::Conditional(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`ConditionalStatement`].
    ///
    /// * If `self` is a [`WorkflowItem::Conditional`], then the inner
    ///   [`ConditionalStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_conditional(self) -> Option<ConditionalStatement<N>> {
        match self {
            Self::Conditional(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`ScatterStatement`].
    ///
    /// * If `self` is a [`WorkflowItem::Scatter`], then a reference to the
    ///   inner [`ScatterStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_scatter(&self) -> Option<&ScatterStatement<N>> {
        match self {
            Self::Scatter(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`ScatterStatement`].
    ///
    /// * If `self` is a [`WorkflowItem::Scatter`], then the inner
    ///   [`ScatterStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_scatter(self) -> Option<ScatterStatement<N>> {
        match self {
            Self::Scatter(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`CallStatement`].
    ///
    /// * If `self` is a [`WorkflowItem::Call`], then a reference to the inner
    ///   [`CallStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_call(&self) -> Option<&CallStatement<N>> {
        match self {
            Self::Call(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`CallStatement`].
    ///
    /// * If `self` is a [`WorkflowItem::Call`], then the inner
    ///   [`CallStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_call(self) -> Option<CallStatement<N>> {
        match self {
            Self::Call(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`MetadataSection`].
    ///
    /// * If `self` is a [`WorkflowItem::Metadata`], then a reference to the
    ///   inner [`MetadataSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_metadata_section(&self) -> Option<&MetadataSection<N>> {
        match self {
            Self::Metadata(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`MetadataSection`].
    ///
    /// * If `self` is a [`WorkflowItem::Metadata`], then the inner
    ///   [`MetadataSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_metadata_section(self) -> Option<MetadataSection<N>> {
        match self {
            Self::Metadata(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`ParameterMetadataSection`].
    ///
    /// * If `self` is a [`WorkflowItem::ParameterMetadata`], then a reference
    ///   to the inner [`ParameterMetadataSection`] is returned wrapped in
    ///   [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_parameter_metadata_section(&self) -> Option<&ParameterMetadataSection<N>> {
        match self {
            Self::ParameterMetadata(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`ParameterMetadataSection`].
    ///
    /// * If `self` is a [`WorkflowItem::ParameterMetadata`], then the inner
    ///   [`ParameterMetadataSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_parameter_metadata_section(self) -> Option<ParameterMetadataSection<N>> {
        match self {
            Self::ParameterMetadata(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`WorkflowHintsSection`].
    ///
    /// * If `self` is a [`WorkflowItem::Hints`], then a reference to the inner
    ///   [`WorkflowHintsSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_hints_section(&self) -> Option<&WorkflowHintsSection<N>> {
        match self {
            Self::Hints(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`WorkflowHintsSection`].
    ///
    /// * If `self` is a [`WorkflowItem::Hints`], then the inner
    ///   [`WorkflowHintsSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_hints_section(self) -> Option<WorkflowHintsSection<N>> {
        match self {
            Self::Hints(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`BoundDecl`].
    ///
    /// * If `self` is a [`WorkflowItem::Declaration`], then a reference to the
    ///   inner [`BoundDecl`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_declaration(&self) -> Option<&BoundDecl<N>> {
        match self {
            Self::Declaration(d) => Some(d),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`BoundDecl`].
    ///
    /// * If `self` is a [`WorkflowItem::Declaration`], then the inner
    ///   [`BoundDecl`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_declaration(self) -> Option<BoundDecl<N>> {
        match self {
            Self::Declaration(d) => Some(d),
            _ => None,
        }
    }

    /// Finds the first child that can be cast to a [`WorkflowItem`].
    pub fn child(node: &N) -> Option<Self> {
        node.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to a [`WorkflowItem`].
    pub fn children(node: &N) -> impl Iterator<Item = Self> + use<'_, N> {
        node.children().filter_map(Self::cast)
    }
}

/// Represents a statement in a workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkflowStatement<N: TreeNode = SyntaxNode> {
    /// The statement is a conditional statement.
    Conditional(ConditionalStatement<N>),
    /// The statement is a scatter statement.
    Scatter(ScatterStatement<N>),
    /// The statement is a call statement.
    Call(CallStatement<N>),
    /// The statement is a private bound declaration.
    Declaration(BoundDecl<N>),
}

impl<N: TreeNode> WorkflowStatement<N> {
    /// Returns whether or not the given syntax kind can be cast to
    /// [`WorkflowStatement`].
    pub fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::ConditionalStatementNode
                | SyntaxKind::ScatterStatementNode
                | SyntaxKind::CallStatementNode
                | SyntaxKind::BoundDeclNode
        )
    }

    /// Casts the given node to [`WorkflowStatement`].
    ///
    /// Returns `None` if the node cannot be cast.
    pub fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::ConditionalStatementNode => Some(Self::Conditional(
                ConditionalStatement::cast(inner).expect("conditional statement to cast"),
            )),
            SyntaxKind::ScatterStatementNode => Some(Self::Scatter(
                ScatterStatement::cast(inner).expect("scatter statement to cast"),
            )),
            SyntaxKind::CallStatementNode => Some(Self::Call(
                CallStatement::cast(inner).expect("call statement to cast"),
            )),
            SyntaxKind::BoundDeclNode => Some(Self::Declaration(
                BoundDecl::cast(inner).expect("bound decl to cast"),
            )),
            _ => None,
        }
    }

    /// Gets a reference to the inner node.
    pub fn inner(&self) -> &N {
        match self {
            Self::Conditional(s) => s.inner(),
            Self::Scatter(s) => s.inner(),
            Self::Call(s) => s.inner(),
            Self::Declaration(s) => s.inner(),
        }
    }

    /// Attempts to get a reference to the inner [`ConditionalStatement`].
    ///
    /// * If `self` is a [`WorkflowStatement::Conditional`], then a reference to
    ///   the inner [`ConditionalStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_conditional(&self) -> Option<&ConditionalStatement<N>> {
        match self {
            Self::Conditional(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`ConditionalStatement`].
    ///
    /// * If `self` is a [`WorkflowStatement::Conditional`], then the inner
    ///   [`ConditionalStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_conditional(self) -> Option<ConditionalStatement<N>> {
        match self {
            Self::Conditional(s) => Some(s),
            _ => None,
        }
    }

    /// Unwraps the statement into a conditional statement.
    ///
    /// # Panics
    ///
    /// Panics if the statement is not a conditional statement.
    pub fn unwrap_conditional(self) -> ConditionalStatement<N> {
        match self {
            Self::Conditional(s) => s,
            _ => panic!("not a conditional statement"),
        }
    }

    /// Attempts to get a reference to the inner [`ScatterStatement`].
    ///
    /// * If `self` is a [`WorkflowStatement::Scatter`], then a reference to the
    ///   inner [`ScatterStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_scatter(&self) -> Option<&ScatterStatement<N>> {
        match self {
            Self::Scatter(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`ScatterStatement`].
    ///
    /// * If `self` is a [`WorkflowStatement::Scatter`], then the inner
    ///   [`ScatterStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_scatter(self) -> Option<ScatterStatement<N>> {
        match self {
            Self::Scatter(s) => Some(s),
            _ => None,
        }
    }

    /// Unwraps the statement into a scatter statement.
    ///
    /// # Panics
    ///
    /// Panics if the statement is not a scatter statement.
    pub fn unwrap_scatter(self) -> ScatterStatement<N> {
        match self {
            Self::Scatter(s) => s,
            _ => panic!("not a scatter statement"),
        }
    }

    /// Attempts to get a reference to the inner [`CallStatement`].
    ///
    /// * If `self` is a [`WorkflowStatement::Call`], then a reference to the
    ///   inner [`CallStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_call(&self) -> Option<&CallStatement<N>> {
        match self {
            Self::Call(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`CallStatement`].
    ///
    /// * If `self` is a [`WorkflowStatement::Call`], then the inner
    ///   [`CallStatement`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_call(self) -> Option<CallStatement<N>> {
        match self {
            Self::Call(s) => Some(s),
            _ => None,
        }
    }

    /// Unwraps the statement into a call statement.
    ///
    /// # Panics
    ///
    /// Panics if the statement is not a call statement.
    pub fn unwrap_call(self) -> CallStatement<N> {
        match self {
            Self::Call(s) => s,
            _ => panic!("not a call statement"),
        }
    }

    /// Attempts to get a reference to the inner [`BoundDecl`].
    ///
    /// * If `self` is a [`WorkflowStatement::Declaration`], then a reference to
    ///   the inner [`BoundDecl`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_declaration(&self) -> Option<&BoundDecl<N>> {
        match self {
            Self::Declaration(d) => Some(d),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`BoundDecl`].
    ///
    /// * If `self` is a [`WorkflowStatement::Declaration`], then the inner
    ///   [`BoundDecl`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_declaration(self) -> Option<BoundDecl<N>> {
        match self {
            Self::Declaration(d) => Some(d),
            _ => None,
        }
    }

    /// Unwraps the statement into a bound declaration.
    ///
    /// # Panics
    ///
    /// Panics if the statement is not a bound declaration.
    pub fn unwrap_declaration(self) -> BoundDecl<N> {
        match self {
            Self::Declaration(d) => d,
            _ => panic!("not a bound declaration"),
        }
    }

    /// Finds the first child that can be cast to a [`WorkflowStatement`].
    pub fn child(node: &N) -> Option<Self> {
        node.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to a [`WorkflowStatement`].
    pub fn children(node: &N) -> impl Iterator<Item = Self> + use<'_, N> {
        node.children().filter_map(Self::cast)
    }
}

/// Represents a workflow conditional statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConditionalStatement<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> ConditionalStatement<N> {
    /// Gets the expression of the conditional statement
    pub fn expr(&self) -> Expr<N> {
        Expr::child(&self.0).expect("expected a conditional expression")
    }

    /// Gets the statements of the conditional body.
    pub fn statements(&self) -> impl Iterator<Item = WorkflowStatement<N>> + use<'_, N> {
        WorkflowStatement::children(&self.0)
    }
}

impl<N: TreeNode> AstNode<N> for ConditionalStatement<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ConditionalStatementNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::ConditionalStatementNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a workflow scatter statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScatterStatement<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> ScatterStatement<N> {
    /// Gets the scatter variable identifier.
    pub fn variable(&self) -> Ident<N::Token> {
        self.token()
            .expect("expected a scatter variable identifier")
    }

    /// Gets the scatter expression.
    pub fn expr(&self) -> Expr<N> {
        Expr::child(&self.0).expect("expected a scatter expression")
    }

    /// Gets the statements of the scatter body.
    pub fn statements(&self) -> impl Iterator<Item = WorkflowStatement<N>> + use<'_, N> {
        WorkflowStatement::children(&self.0)
    }
}

impl<N: TreeNode> AstNode<N> for ScatterStatement<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ScatterStatementNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::ScatterStatementNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a workflow call statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallStatement<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> CallStatement<N> {
    /// Gets the target of the call.
    pub fn target(&self) -> CallTarget<N> {
        self.child().expect("expected a call target")
    }

    /// Gets the optional alias for the call.
    pub fn alias(&self) -> Option<CallAlias<N>> {
        self.child()
    }

    /// Gets the after clauses for the call statement.
    pub fn after(&self) -> impl Iterator<Item = CallAfter<N>> + use<'_, N> {
        self.children()
    }

    /// Gets the inputs for the call statement.
    pub fn inputs(&self) -> impl Iterator<Item = CallInputItem<N>> + use<'_, N> {
        self.children()
    }
}

impl<N: TreeNode> AstNode<N> for CallStatement<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CallStatementNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::CallStatementNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a target in a call statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallTarget<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> CallTarget<N> {
    /// Gets an iterator of the names of the call target.
    ///
    /// The last name in the iteration is considered to be the task or workflow
    /// being called.
    pub fn names(&self) -> impl Iterator<Item = Ident<N::Token>> + use<'_, N> {
        self.0
            .children_with_tokens()
            .filter_map(NodeOrToken::into_token)
            .filter_map(Ident::cast)
    }
}

impl<N: TreeNode> AstNode<N> for CallTarget<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CallTargetNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::CallTargetNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an alias in a call statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallAlias<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> CallAlias<N> {
    /// Gets the alias name.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("expected an alias identifier")
    }
}

impl<N: TreeNode> AstNode<N> for CallAlias<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CallAliasNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::CallAliasNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an after clause in a call statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallAfter<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> CallAfter<N> {
    /// Gets the name from the `after` clause.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("expected an after identifier")
    }
}

impl<N: TreeNode> AstNode<N> for CallAfter<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CallAfterNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::CallAfterNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an input item in a call statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallInputItem<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> CallInputItem<N> {
    /// Gets the name of the input.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("expected an input name")
    }

    /// The optional expression for the input.
    pub fn expr(&self) -> Option<Expr<N>> {
        Expr::child(&self.0)
    }

    /// Gets the call statement for the call input item.
    pub fn parent(&self) -> CallStatement<N> {
        <Self as AstNode<N>>::parent(self).expect("should have parent")
    }

    /// If a call input has the same name as a declaration from the current
    /// scope, the name of the input may appear alone (without an expression) to
    /// implicitly bind the value of that declaration.
    ///
    /// For example, if a `workflow` and `task` both have inputs `x` and `z` of
    /// the same types, then `call mytask {x, y=b, z}` is equivalent to
    /// `call mytask {x=x, y=b, z=z}`.
    pub fn is_implicit_bind(&self) -> bool {
        self.expr().is_none()
    }
}

impl<N: TreeNode> AstNode<N> for CallInputItem<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CallInputItemNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::CallInputItemNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a hints section in a workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowHintsSection<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> WorkflowHintsSection<N> {
    /// Gets the items in the hints section.
    pub fn items(&self) -> impl Iterator<Item = WorkflowHintsItem<N>> + use<'_, N> {
        self.children()
    }

    /// Gets the parent of the hints section.
    pub fn parent(&self) -> WorkflowDefinition<N> {
        <Self as AstNode<N>>::parent(self).expect("should have parent")
    }
}

impl<N: TreeNode> AstNode<N> for WorkflowHintsSection<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::WorkflowHintsSectionNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::WorkflowHintsSectionNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an item in a workflow hints section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowHintsItem<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> WorkflowHintsItem<N> {
    /// Gets the name of the hints item.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("expected an item name")
    }

    /// Gets the value of the hints item.
    pub fn value(&self) -> WorkflowHintsItemValue<N> {
        self.child().expect("expected an item value")
    }
}

impl<N: TreeNode> AstNode<N> for WorkflowHintsItem<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::WorkflowHintsItemNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::WorkflowHintsItemNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a workflow hints item value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkflowHintsItemValue<N: TreeNode = SyntaxNode> {
    /// The value is a literal boolean.
    Boolean(LiteralBoolean<N>),
    /// The value is a literal integer.
    Integer(LiteralInteger<N>),
    /// The value is a literal float.
    Float(LiteralFloat<N>),
    /// The value is a literal string.
    String(LiteralString<N>),
    /// The value is a literal object.
    Object(WorkflowHintsObject<N>),
    /// The value is a literal array.
    Array(WorkflowHintsArray<N>),
}

impl<N: TreeNode> WorkflowHintsItemValue<N> {
    /// Unwraps the value into a boolean.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a boolean.
    pub fn unwrap_boolean(self) -> LiteralBoolean<N> {
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
    pub fn unwrap_integer(self) -> LiteralInteger<N> {
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
    pub fn unwrap_float(self) -> LiteralFloat<N> {
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
    pub fn unwrap_string(self) -> LiteralString<N> {
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
    pub fn unwrap_object(self) -> WorkflowHintsObject<N> {
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
    pub fn unwrap_array(self) -> WorkflowHintsArray<N> {
        match self {
            Self::Array(a) => a,
            _ => panic!("not an array"),
        }
    }
}

impl<N: TreeNode> AstNode<N> for WorkflowHintsItemValue<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
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

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralBooleanNode => Some(Self::Boolean(LiteralBoolean(inner))),
            SyntaxKind::LiteralIntegerNode => Some(Self::Integer(LiteralInteger(inner))),
            SyntaxKind::LiteralFloatNode => Some(Self::Float(LiteralFloat(inner))),
            SyntaxKind::LiteralStringNode => Some(Self::String(LiteralString(inner))),
            SyntaxKind::WorkflowHintsObjectNode => Some(Self::Object(WorkflowHintsObject(inner))),
            SyntaxKind::WorkflowHintsArrayNode => Some(Self::Array(WorkflowHintsArray(inner))),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
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
pub struct WorkflowHintsObject<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> WorkflowHintsObject<N> {
    /// Gets the items of the workflow hints object.
    pub fn items(&self) -> impl Iterator<Item = WorkflowHintsObjectItem<N>> + use<'_, N> {
        self.children()
    }
}

impl<N: TreeNode> AstNode<N> for WorkflowHintsObject<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::WorkflowHintsObjectNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::WorkflowHintsObjectNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a workflow hints object item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowHintsObjectItem<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> WorkflowHintsObjectItem<N> {
    /// Gets the name of the item.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("expected a name")
    }

    /// Gets the value of the item.
    pub fn value(&self) -> WorkflowHintsItemValue<N> {
        self.child().expect("expected a value")
    }
}

impl<N: TreeNode> AstNode<N> for WorkflowHintsObjectItem<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::WorkflowHintsObjectItemNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::WorkflowHintsObjectItemNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a workflow hints array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowHintsArray<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> WorkflowHintsArray<N> {
    /// Gets the elements of the workflow hints array.
    pub fn elements(&self) -> impl Iterator<Item = WorkflowHintsItemValue<N>> + use<'_, N> {
        self.children()
    }
}

impl<N: TreeNode> AstNode<N> for WorkflowHintsArray<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::WorkflowHintsArrayNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::WorkflowHintsArrayNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Document;

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
        assert_eq!(workflows[0].name().text(), "test");

        // Workflow inputs
        let input = workflows[0]
            .input()
            .expect("workflow should have an input section");
        assert_eq!(input.parent().unwrap_workflow().name().text(), "test");
        let decls: Vec<_> = input.declarations().collect();
        assert_eq!(decls.len(), 2);

        // First declaration
        assert_eq!(
            decls[0].clone().unwrap_unbound_decl().ty().to_string(),
            "String"
        );
        assert_eq!(decls[0].clone().unwrap_unbound_decl().name().text(), "name");

        // Second declaration
        assert_eq!(
            decls[1].clone().unwrap_unbound_decl().ty().to_string(),
            "Boolean"
        );
        assert_eq!(
            decls[1].clone().unwrap_unbound_decl().name().text(),
            "do_thing"
        );

        // Workflow outputs
        let output = workflows[0]
            .output()
            .expect("workflow should have an output section");
        assert_eq!(output.parent().unwrap_workflow().name().text(), "test");
        let decls: Vec<_> = output.declarations().collect();
        assert_eq!(decls.len(), 1);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "String");
        assert_eq!(decls[0].name().text(), "output");
        let parts: Vec<_> = decls[0]
            .expr()
            .unwrap_literal()
            .unwrap_string()
            .parts()
            .collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].clone().unwrap_text().text(), "hello, ");
        assert_eq!(
            parts[1]
                .clone()
                .unwrap_placeholder()
                .expr()
                .unwrap_name_ref()
                .name()
                .text(),
            "name"
        );
        assert_eq!(parts[2].clone().unwrap_text().text(), "!");

        // Workflow statements
        let statements: Vec<_> = workflows[0].statements().collect();
        assert_eq!(statements.len(), 4);

        // First workflow statement
        let conditional = statements[0].clone().unwrap_conditional();
        assert_eq!(
            conditional.expr().unwrap_name_ref().name().text(),
            "do_thing"
        );

        // Inner statements
        let inner: Vec<_> = conditional.statements().collect();
        assert_eq!(inner.len(), 2);

        // First inner statement
        let call = inner[0].clone().unwrap_call();
        let names = call.target().names().collect::<Vec<_>>();
        assert_eq!(names.len(), 2);
        assert_eq!(names[0].text(), "foo");
        assert_eq!(names[1].text(), "my_task");
        assert!(call.alias().is_none());
        assert_eq!(call.after().count(), 0);
        assert_eq!(call.inputs().count(), 0);

        // Second inner statement
        let scatter = inner[1].clone().unwrap_scatter();
        assert_eq!(scatter.variable().text(), "a");
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
        assert_eq!(names[0].text(), "my_task");
        assert_eq!(call.alias().unwrap().name().text(), "my_task2");
        assert_eq!(call.after().count(), 0);
        let inputs: Vec<_> = call.inputs().collect();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].name().text(), "a");
        assert!(inputs[0].expr().is_none());

        // Second workflow statement
        let call = statements[1].clone().unwrap_call();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].text(), "my_task");
        assert_eq!(call.alias().unwrap().name().text(), "my_task3");
        let after: Vec<_> = call.after().collect();
        assert_eq!(after.len(), 2);
        assert_eq!(after[0].name().text(), "my_task2");
        assert_eq!(after[1].name().text(), "my_task");
        let inputs: Vec<_> = call.inputs().collect();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].name().text(), "a");
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
        assert_eq!(scatter.variable().text(), "a");
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
                .text(),
            "1"
        );
        assert_eq!(
            elements[1]
                .clone()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "2"
        );
        assert_eq!(
            elements[2]
                .clone()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "3"
        );

        // Inner statements
        let inner: Vec<_> = scatter.statements().collect();
        assert_eq!(inner.len(), 0);

        // Workflow metadata
        let metadata = workflows[0]
            .metadata()
            .expect("workflow should have a metadata section");
        assert_eq!(metadata.parent().unwrap_workflow().name().text(), "test");
        let items: Vec<_> = metadata.items().collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name().text(), "description");
        assert_eq!(
            items[0].value().unwrap_string().text().unwrap().text(),
            "a test"
        );
        assert_eq!(items[1].name().text(), "foo");
        items[1].value().unwrap_null();

        // Workflow parameter metadata
        let param_meta = workflows[0]
            .parameter_metadata()
            .expect("workflow should have a parameter metadata section");
        assert_eq!(param_meta.parent().unwrap_workflow().name().text(), "test");
        let items: Vec<_> = param_meta.items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().text(), "name");
        let items: Vec<_> = items[0].value().unwrap_object().items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().text(), "help");
        assert_eq!(
            items[0].value().unwrap_string().text().unwrap().text(),
            "a name to greet"
        );

        // Workflow hints
        let hints = workflows[0]
            .hints()
            .expect("workflow should have a hints section");
        assert_eq!(hints.parent().name().text(), "test");
        let items: Vec<_> = hints.items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().text(), "foo");
        assert_eq!(
            items[0].value().unwrap_string().text().unwrap().text(),
            "bar"
        );

        // Workflow declarations
        let decls: Vec<_> = workflows[0].declarations().collect();
        assert_eq!(decls.len(), 1);

        // First declaration
        assert_eq!(decls[0].ty().to_string(), "String");
        assert_eq!(decls[0].name().text(), "x");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "private"
        );
    }
}
