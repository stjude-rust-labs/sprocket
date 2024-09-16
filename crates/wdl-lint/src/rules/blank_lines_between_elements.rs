//! A lint rule for blank spacing between elements.

use rowan::NodeOrToken;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::CallStatement;
use wdl_ast::v1::CommandSection;
use wdl_ast::v1::ConditionalStatement;
use wdl_ast::v1::HintsSection;
use wdl_ast::v1::InputSection;
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::ParameterMetadataSection;
use wdl_ast::v1::RequirementsSection;
use wdl_ast::v1::RuntimeSection;
use wdl_ast::v1::ScatterStatement;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::UnboundDecl;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::SyntaxToken;
use wdl_ast::ToSpan;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the blanks between elements rule.
const ID: &str = "BlankLinesBetweenElements";

/// Creates an excessive blank line diagnostic.
fn excess_blank_line(span: Span) -> Diagnostic {
    Diagnostic::note("extra blank line(s) found")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("remove the blank line(s)")
}

/// Creates a missing blank line diagnostic.
fn missing_blank_line(span: Span) -> Diagnostic {
    Diagnostic::note("missing blank line")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a blank line before this element")
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

/// Detects unsorted input declarations.
#[derive(Default, Debug, Clone, Copy)]
pub struct BlankLinesBetweenElementsRule {
    /// Store whether we are in certain blocks
    state: State,
}

impl Rule for BlankLinesBetweenElementsRule {
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
         individual elements may optionally be separated by a blank line. "
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Spacing])
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
            SyntaxKind::HintsSectionNode,
            SyntaxKind::CommandSectionNode,
        ])
    }
}

impl Visitor for BlankLinesBetweenElementsRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Enter {
            // Reset the visitor upon document entry
            *self = Default::default();
        }
    }

    // Import spacing is handled by the ImportWhitespace rule
    // fn import_statement

    fn task_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let first = is_first_element(task.syntax());
        let actual_start = skip_preceding_comments(task.syntax());
        check_prior_spacing(&actual_start, state, true, first, &self.exceptable_nodes());
    }

    fn workflow_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        workflow: &WorkflowDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let first = is_first_element(workflow.syntax());
        let actual_start = skip_preceding_comments(workflow.syntax());
        check_prior_spacing(&actual_start, state, true, first, &self.exceptable_nodes());
    }

    fn metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &MetadataSection,
    ) {
        if reason == VisitReason::Exit {
            self.state = State::Outside;
            return;
        } else {
            self.state = State::MetaSection;
        }

        let first = is_first_element(section.syntax());
        let actual_start = skip_preceding_comments(section.syntax());
        check_prior_spacing(&actual_start, state, true, first, &self.exceptable_nodes());
        flag_all_blank_lines_within(section.syntax(), state, &self.exceptable_nodes());
    }

    fn parameter_metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &ParameterMetadataSection,
    ) {
        if reason == VisitReason::Exit {
            self.state = State::Outside;
            return;
        } else {
            self.state = State::ParameterMetaSection;
        }

        let first = is_first_element(section.syntax());
        let actual_start = skip_preceding_comments(section.syntax());
        check_prior_spacing(&actual_start, state, true, first, &self.exceptable_nodes());
        flag_all_blank_lines_within(section.syntax(), state, &self.exceptable_nodes());
    }

    fn input_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &InputSection,
    ) {
        if reason == VisitReason::Exit {
            self.state = State::Outside;
            return;
        } else {
            self.state = State::InputSection;
        }

        let first = is_first_element(section.syntax());
        let actual_start = skip_preceding_comments(section.syntax());
        check_prior_spacing(&actual_start, state, true, first, &self.exceptable_nodes());
    }

    fn command_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &CommandSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let first = is_first_element(section.syntax());
        let actual_start = skip_preceding_comments(section.syntax());
        check_prior_spacing(&actual_start, state, true, first, &self.exceptable_nodes());
    }

    fn output_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &OutputSection,
    ) {
        if reason == VisitReason::Exit {
            self.state = State::Outside;
            return;
        } else {
            self.state = State::OutputSection;
        }
        let first = is_first_element(section.syntax());
        let actual_start = skip_preceding_comments(section.syntax());
        check_prior_spacing(&actual_start, state, true, first, &self.exceptable_nodes());
    }

    fn runtime_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &RuntimeSection,
    ) {
        if reason == VisitReason::Exit {
            self.state = State::Outside;
            return;
        } else {
            self.state = State::RuntimeSection;
        }

        flag_all_blank_lines_within(section.syntax(), state, &self.exceptable_nodes());
        let first = is_first_element(section.syntax());
        let actual_start = skip_preceding_comments(section.syntax());
        check_prior_spacing(&actual_start, state, true, first, &self.exceptable_nodes());
    }

    // call statement internal spacing is handled by the CallInputSpacing rule
    fn call_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &CallStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // We only care about spacing for calls if they're the "first" thing in a
        // workflow body.
        let first = is_first_body(stmt.syntax());

        let prev = skip_preceding_comments(stmt.syntax());

        if first {
            check_prior_spacing(&prev, state, true, false, &self.exceptable_nodes());
        }
    }

    fn scatter_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &ScatterStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let first = is_first_body(stmt.syntax());

        let prev = skip_preceding_comments(stmt.syntax());

        if first {
            check_prior_spacing(&prev, state, true, false, &self.exceptable_nodes());
        }
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

        let first = is_first_element(def.syntax());
        let actual_start = skip_preceding_comments(def.syntax());
        check_prior_spacing(&actual_start, state, true, first, &self.exceptable_nodes());
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

        let first = is_first_element(section.syntax());
        let actual_start = skip_preceding_comments(section.syntax());
        check_prior_spacing(&actual_start, state, true, first, &self.exceptable_nodes());
        flag_all_blank_lines_within(section.syntax(), state, &self.exceptable_nodes());
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

        let first = is_first_element(section.syntax());
        let actual_start = skip_preceding_comments(section.syntax());
        check_prior_spacing(&actual_start, state, true, first, &self.exceptable_nodes());
        flag_all_blank_lines_within(section.syntax(), state, &self.exceptable_nodes());
    }

    fn unbound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &UnboundDecl) {
        if reason == VisitReason::Exit {
            return;
        }

        let prior = decl.syntax().prev_sibling_or_token();
        if let Some(p) = prior {
            if p.kind() == SyntaxKind::Whitespace {
                let count = p.to_string().chars().filter(|c| *c == '\n').count();
                // If we're in an `input` or `output`, we should have no blank lines, so only
                // one `\n` is allowed.
                if self.state == State::InputSection || self.state == State::OutputSection {
                    if count > 1 {
                        state.exceptable_add(
                            excess_blank_line(p.text_range().to_span()),
                            SyntaxElement::from(decl.syntax().clone()),
                            &self.exceptable_nodes(),
                        );
                    }
                } else {
                    let first = is_first_body(decl.syntax());

                    let prev = skip_preceding_comments(decl.syntax());

                    if first {
                        check_prior_spacing(&prev, state, true, false, &self.exceptable_nodes());
                    }
                }
            }
        }
    }

    fn bound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &BoundDecl) {
        if reason == VisitReason::Exit {
            return;
        }

        let actual_start = skip_preceding_comments(decl.syntax());

        let prior = actual_start.prev_sibling_or_token();
        if let Some(p) = prior {
            if p.kind() == SyntaxKind::Whitespace {
                let count = p.to_string().chars().filter(|c| *c == '\n').count();
                // If we're in an `input` or `output`, we should have no blank lines, so only
                // one `\n` is allowed.
                if self.state == State::InputSection || self.state == State::OutputSection {
                    if count > 1 {
                        state.exceptable_add(
                            excess_blank_line(p.text_range().to_span()),
                            SyntaxElement::from(decl.syntax().clone()),
                            &self.exceptable_nodes(),
                        );
                    }
                } else {
                    let first = is_first_body(decl.syntax());

                    let prev = skip_preceding_comments(decl.syntax());

                    if first {
                        check_prior_spacing(&prev, state, true, false, &self.exceptable_nodes());
                    }
                }
            }
        }
    }

    fn conditional_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &ConditionalStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let first = is_first_body(stmt.syntax());

        let prev = skip_preceding_comments(stmt.syntax());

        if first {
            check_prior_spacing(&prev, state, true, false, &self.exceptable_nodes());
        }
    }
}

/// Check if the given syntax node is the first element in the block.
fn is_first_element(syntax: &SyntaxNode) -> bool {
    let mut prev = syntax.prev_sibling_or_token();
    while let Some(ref cur) = prev {
        match cur {
            NodeOrToken::Token(t) => {
                if t.kind() == SyntaxKind::OpenBrace {
                    return true;
                }
            }
            NodeOrToken::Node(_) => {
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
    state: &mut Diagnostics,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    syntax.descendants_with_tokens().for_each(|c| {
        if c.kind() == SyntaxKind::Whitespace {
            let count = c.to_string().chars().filter(|c| *c == '\n').count();
            if count > 1 {
                state.exceptable_add(
                    excess_blank_line(c.text_range().to_span()),
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
    state: &mut Diagnostics,
    element_spacing_required: bool,
    first: bool,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    if let Some(prior) = syntax.prev_sibling_or_token() {
        match prior.kind() {
            SyntaxKind::Whitespace => {
                let count = prior.to_string().chars().filter(|c| *c == '\n').count();
                if first || !element_spacing_required {
                    // first element cannot have a blank line before it
                    if count > 1 {
                        state.exceptable_add(
                            excess_blank_line(prior.text_range().to_span()),
                            SyntaxElement::from(syntax.clone()),
                            exceptable_nodes,
                        );
                    }
                } else if count < 2 && element_spacing_required {
                    state.exceptable_add(
                        missing_blank_line(syntax.text_range().to_span()),
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
                    state.exceptable_add(
                        missing_blank_line(syntax.text_range().to_span()),
                        SyntaxElement::from(syntax.clone()),
                        exceptable_nodes,
                    );
                }
            }
        }
    }
}

/// For a given node, walk background until a non-comment or blank line is
/// found. This allows us to skip comments that are "attached" to the current
/// node.
fn skip_preceding_comments(syntax: &SyntaxNode) -> NodeOrToken<SyntaxNode, SyntaxToken> {
    let mut preceding_comments = Vec::new();

    let mut prev = syntax.prev_sibling_or_token();
    while let Some(cur) = prev {
        match cur.kind() {
            SyntaxKind::Comment => {
                // Ensure this comment "belongs" to the root element.
                // A preceding comment on a blank line is considered to belong to the element.
                // Otherwise, the comment "belongs" to whatever
                // else is on that line.
                if let Some(before_cur) = cur.prev_sibling_or_token() {
                    match before_cur.kind() {
                        SyntaxKind::Whitespace => {
                            if before_cur.to_string().contains('\n') {
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
                // Ignore
                if cur.to_string().chars().filter(|c| *c == '\n').count() > 1 {
                    // We've backed up to an empty line, so we can stop
                    break;
                }
            }
            _ => {
                // We've backed up to non-trivia, so we can stop
                break;
            }
        }
        prev = cur.prev_sibling_or_token()
    }

    return preceding_comments
        .last()
        .unwrap_or(&NodeOrToken::Node(syntax.clone()))
        .clone();
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
                | SyntaxKind::HintsSectionNode
                | SyntaxKind::CommandSectionNode
        )
    })
}
