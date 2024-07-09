//! V1 AST representation for workflows.

use super::BoundDecl;
use super::Expr;
use super::InputSection;
use super::MetadataSection;
use super::OutputSection;
use super::ParameterMetadataSection;
use crate::support::child;
use crate::support::children;
use crate::token;
use crate::AstChildren;
use crate::AstNode;
use crate::AstToken;
use crate::Ident;
use crate::SyntaxElement;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::WorkflowDescriptionLanguage;

/// Represents a workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowDefinition(pub(crate) SyntaxNode);

impl WorkflowDefinition {
    /// Gets the name of the workflow.
    pub fn name(&self) -> Ident {
        token(&self.0).expect("workflow should have a name")
    }

    /// Gets the items of the workflow.
    pub fn items(&self) -> AstChildren<WorkflowItem> {
        children(&self.0)
    }

    /// Gets the input sections of the workflow.
    pub fn inputs(&self) -> AstChildren<InputSection> {
        children(&self.0)
    }

    /// Gets the output sections of the workflow.
    pub fn outputs(&self) -> AstChildren<OutputSection> {
        children(&self.0)
    }

    /// Gets the statements of the workflow.
    pub fn statements(&self) -> AstChildren<WorkflowStatement> {
        children(&self.0)
    }

    /// Gets the metadata sections of the workflow.
    pub fn metadata(&self) -> AstChildren<MetadataSection> {
        children(&self.0)
    }

    /// Gets the parameter sections of the workflow.
    pub fn parameter_metadata(&self) -> AstChildren<ParameterMetadataSection> {
        children(&self.0)
    }

    /// Gets the private declarations of the workflow.
    pub fn declarations(&self) -> AstChildren<BoundDecl> {
        children(&self.0)
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
    /// The item is a private bound declaration.
    Declaration(BoundDecl),
}

impl AstNode for WorkflowItem {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
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
                | SyntaxKind::BoundDeclNode
        )
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::InputSectionNode => Some(Self::Input(InputSection(syntax))),
            SyntaxKind::OutputSectionNode => Some(Self::Output(OutputSection(syntax))),
            SyntaxKind::ConditionalStatementNode => {
                Some(Self::Conditional(ConditionalStatement(syntax)))
            }
            SyntaxKind::ScatterStatementNode => Some(Self::Scatter(ScatterStatement(syntax))),
            SyntaxKind::CallStatementNode => Some(Self::Call(CallStatement(syntax))),
            SyntaxKind::MetadataSectionNode => Some(Self::Metadata(MetadataSection(syntax))),
            SyntaxKind::ParameterMetadataSectionNode => {
                Some(Self::ParameterMetadata(ParameterMetadataSection(syntax)))
            }
            SyntaxKind::BoundDeclNode => Some(Self::Declaration(BoundDecl(syntax))),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Input(i) => &i.0,
            Self::Output(o) => &o.0,
            Self::Conditional(s) => &s.0,
            Self::Scatter(s) => &s.0,
            Self::Call(s) => &s.0,
            Self::Metadata(m) => &m.0,
            Self::ParameterMetadata(m) => &m.0,
            Self::Declaration(d) => &d.0,
        }
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

    /// Unwraps the statement into a bound declaration.
    ///
    /// # Panics
    ///
    /// Panics if the statement is not a bound declaration.
    pub fn unwrap_bound_decl(self) -> BoundDecl {
        match self {
            Self::Declaration(stmt) => stmt,
            _ => panic!("not a bound declaration"),
        }
    }
}

impl AstNode for WorkflowStatement {
    type Language = WorkflowDescriptionLanguage;

    fn can_cast(kind: SyntaxKind) -> bool
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

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind() {
            SyntaxKind::ConditionalStatementNode => {
                Some(Self::Conditional(ConditionalStatement(syntax)))
            }
            SyntaxKind::ScatterStatementNode => Some(Self::Scatter(ScatterStatement(syntax))),
            SyntaxKind::CallStatementNode => Some(Self::Call(CallStatement(syntax))),
            SyntaxKind::BoundDeclNode => Some(Self::Declaration(BoundDecl(syntax))),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Conditional(s) => &s.0,
            Self::Scatter(s) => &s.0,
            Self::Call(s) => &s.0,
            Self::Declaration(d) => &d.0,
        }
    }
}

/// Represents a workflow conditional statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConditionalStatement(pub(crate) SyntaxNode);

impl ConditionalStatement {
    /// Gets the expression of the conditional statement
    pub fn expr(&self) -> Expr {
        child(&self.0).expect("expected a conditional expression")
    }

    /// Gets the statements of the conditional body.
    pub fn statements(&self) -> AstChildren<WorkflowStatement> {
        children(&self.0)
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
        child(&self.0).expect("expected a scatter expression")
    }

    /// Gets the statements of the scatter body.
    pub fn statements(&self) -> AstChildren<WorkflowStatement> {
        children(&self.0)
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
    /// Gets the name of the call target.
    ///
    /// The first value is an optional namespace.
    /// The second value is the call target name.
    pub fn name(&self) -> (Option<Ident>, Ident) {
        let mut children = self
            .0
            .children_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .filter_map(Ident::cast);
        let first = children.next().expect("should be at least one identifier");
        match children.next() {
            Some(second) => (Some(first), second),
            None => (None, first),
        }
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
        child(&self.0)
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::v1::UnboundDecl;
    use crate::Document;
    use crate::VisitReason;
    use crate::Visitor;

    #[test]
    fn workflows() {
        let parse = Document::parse(
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

    String x = "private"
}
"#,
        );

        let document = parse.into_result().expect("there should be no errors");
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let workflows: Vec<_> = ast.workflows().collect();
        assert_eq!(workflows.len(), 1);
        assert_eq!(workflows[0].name().as_str(), "test");

        // Workflow inputs
        let inputs: Vec<_> = workflows[0].inputs().collect();
        assert_eq!(inputs.len(), 1);

        // First input declarations
        assert_eq!(inputs[0].parent().unwrap_workflow().name().as_str(), "test");
        let decls: Vec<_> = inputs[0].declarations().collect();
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
        let outputs: Vec<_> = workflows[0].outputs().collect();
        assert_eq!(outputs.len(), 1);

        // First output declarations
        assert_eq!(
            outputs[0].parent().unwrap_workflow().name().as_str(),
            "test"
        );
        let decls: Vec<_> = outputs[0].declarations().collect();
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
        let (namespace, target) = call.target().name();
        assert_eq!(namespace.unwrap().as_str(), "foo");
        assert_eq!(target.as_str(), "my_task");
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
        let (namespace, target) = call.target().name();
        assert!(namespace.is_none());
        assert_eq!(target.as_str(), "my_task");
        assert_eq!(call.alias().unwrap().name().as_str(), "my_task2");
        assert_eq!(call.after().count(), 0);
        let inputs: Vec<_> = call.inputs().collect();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].name().as_str(), "a");
        assert!(inputs[0].expr().is_none());

        // Second workflow statement
        let call = statements[1].clone().unwrap_call();
        let (namespace, target) = call.target().name();
        assert!(namespace.is_none());
        assert_eq!(target.as_str(), "my_task");
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
        let metadata: Vec<_> = workflows[0].metadata().collect();
        assert_eq!(metadata.len(), 1);

        // First metadata
        assert_eq!(
            metadata[0].parent().unwrap_workflow().name().as_str(),
            "test"
        );
        let items: Vec<_> = metadata[0].items().collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name().as_str(), "description");
        assert_eq!(
            items[0].value().unwrap_string().text().unwrap().as_str(),
            "a test"
        );
        assert_eq!(items[1].name().as_str(), "foo");
        items[1].value().unwrap_null();

        // Workflow parameter metadata
        let param_meta: Vec<_> = workflows[0].parameter_metadata().collect();
        assert_eq!(param_meta.len(), 1);

        // First parameter metadata
        assert_eq!(
            param_meta[0].parent().unwrap_workflow().name().as_str(),
            "test"
        );
        let items: Vec<_> = param_meta[0].items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().as_str(), "name");
        let items: Vec<_> = items[0].value().unwrap_object().items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().as_str(), "help");
        assert_eq!(
            items[0].value().unwrap_string().text().unwrap().as_str(),
            "a name to greet"
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
