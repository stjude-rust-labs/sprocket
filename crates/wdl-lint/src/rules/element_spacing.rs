//! A lint rule for blank spacing between elements.

use rowan::NodeOrToken;
use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::SyntaxToken;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::CallStatement;
use wdl_ast::v1::CommandSection;
use wdl_ast::v1::ConditionalStatement;
use wdl_ast::v1::InputSection;
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::ParameterMetadataSection;
use wdl_ast::v1::RequirementsSection;
use wdl_ast::v1::RuntimeSection;
use wdl_ast::v1::ScatterStatement;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::TaskHintsSection;
use wdl_ast::v1::UnboundDecl;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::v1::WorkflowHintsSection;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the blanks between elements rule.
const ID: &str = "ElementSpacing";

/// Creates an excessive blank line diagnostic.
fn excess_blank_line(span: Span) -> Diagnostic {
    Diagnostic::note("extra blank line(s) found")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("remove extra blank line(s)")
}

/// Creates a missing blank line diagnostic.
fn missing_blank_line(span: Span) -> Diagnostic {
    Diagnostic::note("missing blank line")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a blank line")
}

/// Track the position within a document
#[derive(Debug, Default, Clone, Copy, PartialEq)]
enum State {
    /// Outside tracked sections
    #[default]
    Outside,
    /// In a `Input` section
    InputSection,
    /// In a `Output` section
    OutputSection,
    /// In a `Meta` section
    MetaSection,
    /// In a `Parameter Meta` section
    ParameterMetaSection,
    /// In a `Runtime` section
    RuntimeSection,
}

/// Ensures proper blank space between elements.
#[derive(Default, Debug, Clone, Copy)]
pub struct ElementSpacingRule {
    /// Store whether we are in certain blocks
    state: State,
}

impl Rule for ElementSpacingRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that WDL elements are spaced appropriately."
    }

    fn explanation(&self) -> &'static str {
        "There should be a blank line between each WDL element at the root indentation level (such \
         as the import block and any task/workflow definitions) and between sections of a WDL task \
         or workflow. Never have a blank line when indentation levels are changing (such as \
         between the opening of a workflow definition and the meta section). There should also \
         never be blanks within a `meta`, `parameter_meta`, `input`, `output`, `runtime`, \
         `requirements`, or `hints` section. For workflows, the `workflow body` includes any \
         private declarations, call statements, conditional statements, and scatter statements. A \
         `task body` is any and all private declarations. Within a workflow or task body, \
         individual elements may optionally be separated by a blank line."
    }

    fn examples(&self) -> &'static [&'static str] {
        &[
            r#"```wdl
version 1.2

workflow determine_jimmy_age {

    meta {

        description: "Determines the current age of Jimmy."

        outputs: {
            age: "The age of Jimmy."

        }

    }

    output {
        Int age = 55
    }
}
```"#,
            r#"Use instead:

```wdl
version 1.2

workflow determine_jimmy_age {
    meta {
        description: "Determines the current age of Jimmy."
        outputs: {
            age: "The age of Jimmy."
        }
    }

    output {
        Int age = 55
    }
}
```"#,
        ]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Spacing, Tag::Style])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
            SyntaxKind::StructDefinitionNode,
            SyntaxKind::InputSectionNode,
            SyntaxKind::OutputSectionNode,
            SyntaxKind::RuntimeSectionNode,
            SyntaxKind::MetadataSectionNode,
            SyntaxKind::ParameterMetadataSectionNode,
            SyntaxKind::RequirementsSectionNode,
            SyntaxKind::TaskHintsSectionNode,
            SyntaxKind::WorkflowHintsSectionNode,
            SyntaxKind::CommandSectionNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for ElementSpacingRule {
    fn reset(&mut self) {
        *self = Self::default();
    }

    // Import spacing is handled by the ImportWhitespace rule

    fn task_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let first = is_first_element(task.inner());
        let actual_start = skip_preceding_comments(task.inner());
        check_prior_spacing(
            &actual_start,
            diagnostics,
            true,
            first,
            &self.exceptable_nodes(),
        );
        check_last_token(task.inner(), diagnostics, &self.exceptable_nodes());
    }

    fn workflow_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        workflow: &WorkflowDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let first = is_first_element(workflow.inner());
        let actual_start = skip_preceding_comments(workflow.inner());
        check_prior_spacing(
            &actual_start,
            diagnostics,
            true,
            first,
            &self.exceptable_nodes(),
        );
        check_last_token(workflow.inner(), diagnostics, &self.exceptable_nodes());
    }

    fn metadata_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &MetadataSection,
    ) {
        if reason == VisitReason::Exit {
            self.state = State::Outside;
            return;
        } else {
            self.state = State::MetaSection;
        }

        let first = is_first_element(section.inner());
        let actual_start = skip_preceding_comments(section.inner());
        check_prior_spacing(
            &actual_start,
            diagnostics,
            true,
            first,
            &self.exceptable_nodes(),
        );
        flag_all_blank_lines_within(section.inner(), diagnostics, &self.exceptable_nodes());
        // flag_all_blank_lines_within() covers check_last_token()
    }

    fn parameter_metadata_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &ParameterMetadataSection,
    ) {
        if reason == VisitReason::Exit {
            self.state = State::Outside;
            return;
        } else {
            self.state = State::ParameterMetaSection;
        }

        let first = is_first_element(section.inner());
        let actual_start = skip_preceding_comments(section.inner());
        check_prior_spacing(
            &actual_start,
            diagnostics,
            true,
            first,
            &self.exceptable_nodes(),
        );
        flag_all_blank_lines_within(section.inner(), diagnostics, &self.exceptable_nodes());
        // flag_all_blank_lines_within() covers check_last_token()
    }

    fn input_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &InputSection,
    ) {
        if reason == VisitReason::Exit {
            self.state = State::Outside;
            return;
        } else {
            self.state = State::InputSection;
        }

        let first = is_first_element(section.inner());
        let actual_start = skip_preceding_comments(section.inner());
        check_prior_spacing(
            &actual_start,
            diagnostics,
            true,
            first,
            &self.exceptable_nodes(),
        );
        check_last_token(section.inner(), diagnostics, &self.exceptable_nodes());
    }

    fn command_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &CommandSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let first = is_first_element(section.inner());
        let actual_start = skip_preceding_comments(section.inner());
        check_prior_spacing(
            &actual_start,
            diagnostics,
            true,
            first,
            &self.exceptable_nodes(),
        );
        check_last_token(section.inner(), diagnostics, &self.exceptable_nodes());
    }

    fn output_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &OutputSection,
    ) {
        if reason == VisitReason::Exit {
            self.state = State::Outside;
            return;
        } else {
            self.state = State::OutputSection;
        }
        let first = is_first_element(section.inner());
        let actual_start = skip_preceding_comments(section.inner());
        check_prior_spacing(
            &actual_start,
            diagnostics,
            true,
            first,
            &self.exceptable_nodes(),
        );
        check_last_token(section.inner(), diagnostics, &self.exceptable_nodes());
    }

    fn runtime_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &RuntimeSection,
    ) {
        if reason == VisitReason::Exit {
            self.state = State::Outside;
            return;
        } else {
            self.state = State::RuntimeSection;
        }

        let first = is_first_element(section.inner());
        let actual_start = skip_preceding_comments(section.inner());
        check_prior_spacing(
            &actual_start,
            diagnostics,
            true,
            first,
            &self.exceptable_nodes(),
        );
        flag_all_blank_lines_within(section.inner(), diagnostics, &self.exceptable_nodes());
        // flag_all_blank_lines_within() covers check_last_token()
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

        // We only care about spacing for calls if they're the "first" thing in a
        // workflow body.
        let first = is_first_body(stmt.inner());

        let prev = skip_preceding_comments(stmt.inner());

        if first {
            check_prior_spacing(&prev, diagnostics, true, false, &self.exceptable_nodes());
        }
        check_last_token(stmt.inner(), diagnostics, &self.exceptable_nodes());
    }

    fn scatter_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &ScatterStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let first = is_first_body(stmt.inner());

        let prev = skip_preceding_comments(stmt.inner());

        if first {
            check_prior_spacing(&prev, diagnostics, true, false, &self.exceptable_nodes());
        }
        check_last_token(stmt.inner(), diagnostics, &self.exceptable_nodes());
    }

    fn struct_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        def: &StructDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let first = is_first_element(def.inner());
        let actual_start = skip_preceding_comments(def.inner());
        check_prior_spacing(
            &actual_start,
            diagnostics,
            true,
            first,
            &self.exceptable_nodes(),
        );
        check_last_token(def.inner(), diagnostics, &self.exceptable_nodes());
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

        let first = is_first_element(section.inner());
        let actual_start = skip_preceding_comments(section.inner());
        check_prior_spacing(
            &actual_start,
            diagnostics,
            true,
            first,
            &self.exceptable_nodes(),
        );
        flag_all_blank_lines_within(section.inner(), diagnostics, &self.exceptable_nodes());
        // flag_all_blank_lines_within() covers check_last_token()
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

        let first = is_first_element(section.inner());
        let actual_start = skip_preceding_comments(section.inner());
        check_prior_spacing(
            &actual_start,
            diagnostics,
            true,
            first,
            &self.exceptable_nodes(),
        );
        flag_all_blank_lines_within(section.inner(), diagnostics, &self.exceptable_nodes());
        // flag_all_blank_lines_within() covers check_last_token()
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

        let first = is_first_element(section.inner());
        let actual_start = skip_preceding_comments(section.inner());
        check_prior_spacing(
            &actual_start,
            diagnostics,
            true,
            first,
            &self.exceptable_nodes(),
        );
        flag_all_blank_lines_within(section.inner(), diagnostics, &self.exceptable_nodes());
        // flag_all_blank_lines_within() covers check_last_token()
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

        let prior = decl
            .inner()
            .prev_sibling_or_token()
            .and_then(SyntaxElement::into_token);
        if let Some(p) = prior
            && p.kind() == SyntaxKind::Whitespace
        {
            let count = p.text().chars().filter(|c| *c == '\n').count();
            // If we're in an `input` or `output`, we should have no blank lines, so only
            // one `\n` is allowed.
            if self.state == State::InputSection || self.state == State::OutputSection {
                if count > 1 {
                    diagnostics.exceptable_add(
                        excess_blank_line(p.text_range().into()),
                        SyntaxElement::from(decl.inner().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            } else {
                let first = is_first_body(decl.inner());

                let prev = skip_preceding_comments(decl.inner());

                if first {
                    check_prior_spacing(&prev, diagnostics, true, false, &self.exceptable_nodes());
                }
            }
        }
    }

    fn bound_decl(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, decl: &BoundDecl) {
        if reason == VisitReason::Exit {
            return;
        }

        let actual_start = skip_preceding_comments(decl.inner());

        let prior = actual_start
            .prev_sibling_or_token()
            .and_then(SyntaxElement::into_token);
        if let Some(p) = prior
            && p.kind() == SyntaxKind::Whitespace
        {
            let count = p.text().chars().filter(|c| *c == '\n').count();
            // If we're in an `input` or `output`, we should have no blank lines, so only
            // one `\n` is allowed.
            if self.state == State::InputSection || self.state == State::OutputSection {
                if count > 1 {
                    diagnostics.exceptable_add(
                        excess_blank_line(p.text_range().into()),
                        SyntaxElement::from(decl.inner().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            } else {
                let first = is_first_body(decl.inner());

                let prev = skip_preceding_comments(decl.inner());

                if first {
                    check_prior_spacing(&prev, diagnostics, true, false, &self.exceptable_nodes());
                }
            }
        }
    }

    fn conditional_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &ConditionalStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let first = is_first_body(stmt.inner());

        let prev = skip_preceding_comments(stmt.inner());

        if first {
            check_prior_spacing(&prev, diagnostics, true, false, &self.exceptable_nodes());
        }
        check_last_token(stmt.inner(), diagnostics, &self.exceptable_nodes());
    }
}

/// Check if the given syntax node is the first element in the block.
fn is_first_element(syntax: &SyntaxNode) -> bool {
    let mut prev = syntax.prev_sibling_or_token();
    let mut comment_seen = false;
    while let Some(ref cur) = prev {
        match cur {
            NodeOrToken::Token(t) => {
                if t.kind() == SyntaxKind::OpenBrace {
                    return true;
                }
                if t.kind() == SyntaxKind::Comment {
                    comment_seen = true;
                }
            }
            NodeOrToken::Node(n) => {
                if n.kind() == SyntaxKind::VersionStatementNode {
                    return !comment_seen;
                }
                return false;
            }
        }
        prev = cur.prev_sibling_or_token();
    }
    unreachable!("No prior node or open brace found");
}

/// Some sections do not allow blank lines, so detect and flag them.
fn flag_all_blank_lines_within(
    syntax: &SyntaxNode,
    diagnostics: &mut Diagnostics,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    syntax.descendants_with_tokens().for_each(|c| {
        if c.kind() == SyntaxKind::Whitespace {
            let count = c
                .as_token()
                .expect("should be a token")
                .text()
                .chars()
                .filter(|c| *c == '\n')
                .count();
            if count > 1 {
                diagnostics.exceptable_add(
                    excess_blank_line(c.text_range().into()),
                    SyntaxElement::from(syntax.clone()),
                    exceptable_nodes,
                );
            }
        }
    });
}

/// Check that an item has space prior to it.
///
/// `element_spacing_required` indicates if spacing is required (`true`) or not
/// (`false`).
fn check_prior_spacing(
    syntax: &NodeOrToken<SyntaxNode, SyntaxToken>,
    diagnostics: &mut Diagnostics,
    element_spacing_required: bool,
    first: bool,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    if let Some(prior) = syntax.prev_sibling_or_token() {
        let span = match prior {
            NodeOrToken::Token(ref t) => Span::new(
                t.text_range().start().into(),
                (syntax.text_range().start() - t.text_range().start()).into(),
            ),
            NodeOrToken::Node(ref n) => Span::new(
                n.last_token()
                    .expect("node should have tokens")
                    .text_range()
                    .start()
                    .into(),
                (syntax.text_range().start()
                    - n.last_token()
                        .expect("node should have tokens")
                        .text_range()
                        .start())
                .into(),
            ),
        };
        match prior.kind() {
            SyntaxKind::Whitespace => {
                let count = prior
                    .as_token()
                    .expect("should be a token")
                    .text()
                    .chars()
                    .filter(|c| *c == '\n')
                    .count();
                if first || !element_spacing_required {
                    // first element cannot have a blank line before it.
                    // Whitespace following the version statement is handled by the
                    // `VersionStatementFormatted` rule.
                    if count > 1
                        && prior
                            .prev_sibling_or_token()
                            .is_some_and(|p| p.kind() != SyntaxKind::VersionStatementNode)
                    {
                        diagnostics.exceptable_add(
                            excess_blank_line(prior.text_range().into()),
                            SyntaxElement::from(syntax.clone()),
                            exceptable_nodes,
                        );
                    }
                } else if count < 2 && element_spacing_required {
                    diagnostics.exceptable_add(
                        missing_blank_line(span),
                        SyntaxElement::from(syntax.clone()),
                        exceptable_nodes,
                    );
                }
            }
            // Something other than whitespace precedes
            _ => {
                // If we require between element spacing and are not the first element,
                // we're missing a blank line.
                if element_spacing_required && !first {
                    diagnostics.exceptable_add(
                        missing_blank_line(span),
                        SyntaxElement::from(syntax.clone()),
                        exceptable_nodes,
                    );
                }
            }
        }
    }
}

/// Check that the node's last token does not have a blank before it.
fn check_last_token(
    syntax: &SyntaxNode,
    diagnostics: &mut Diagnostics,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    let prev = syntax
        .last_token()
        .expect("node should have last token")
        .prev_token();
    if let Some(prev) = prev
        && prev.kind() == SyntaxKind::Whitespace
    {
        let count = prev.text().chars().filter(|c| *c == '\n').count();
        if count > 1 {
            diagnostics.exceptable_add(
                excess_blank_line(prev.text_range().into()),
                SyntaxElement::from(syntax.clone()),
                exceptable_nodes,
            );
        }
    }
}

/// For a given node, walk background until a non-comment or blank line is
/// found. This allows us to skip comments that are "attached" to the current
/// node.
fn skip_preceding_comments(syntax: &SyntaxNode) -> NodeOrToken<SyntaxNode, SyntaxToken> {
    let mut preceding_comments = Vec::new();

    let mut prev = syntax
        .prev_sibling_or_token()
        .and_then(SyntaxElement::into_token);
    while let Some(cur) = prev {
        match cur.kind() {
            SyntaxKind::Comment => {
                // Ensure this comment "belongs" to the root element.
                // A preceding comment on a blank line is considered to belong to the element.
                // Otherwise, the comment "belongs" to whatever
                // else is on that line.
                if let Some(before_cur) = cur.prev_token() {
                    match before_cur.kind() {
                        SyntaxKind::Whitespace => {
                            if before_cur.text().contains('\n') {
                                // The 'cur' comment is on is on its own line.
                                // It "belongs" to the current element.
                                preceding_comments.push(cur.clone());
                            }
                        }
                        _ => {
                            // The 'cur' comment is on the same line as this
                            // token. It "belongs" to whatever is currently
                            // being processed.
                        }
                    }
                }
            }
            SyntaxKind::Whitespace => {
                if cur.text().chars().filter(|c| *c == '\n').count() > 1 {
                    // We've backed up to an empty line, so we can stop
                    break;
                }
            }
            _ => {
                // We've backed up to non-trivia, so we can stop
                break;
            }
        }
        prev = cur.prev_token()
    }

    preceding_comments.last().map_or_else(
        || SyntaxElement::from(syntax.clone()),
        |c| SyntaxElement::from(c.clone()),
    )
}

/// Is first body element?
fn is_first_body(syntax: &SyntaxNode) -> bool {
    syntax.prev_sibling().is_some_and(|f| {
        matches!(
            f.kind(),
            SyntaxKind::InputSectionNode
                | SyntaxKind::OutputSectionNode
                | SyntaxKind::RuntimeSectionNode
                | SyntaxKind::MetadataSectionNode
                | SyntaxKind::ParameterMetadataSectionNode
                | SyntaxKind::RequirementsSectionNode
                | SyntaxKind::TaskHintsSectionNode
                | SyntaxKind::WorkflowHintsSectionNode
                | SyntaxKind::CommandSectionNode
        )
    })
}
