//! Implementation of the `except` visitor.

use std::collections::HashSet;

use indexmap::IndexMap;
use wdl_ast::v1;
use wdl_ast::AstNode;
use wdl_ast::Comment;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Direction;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::VersionStatement;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;
use wdl_ast::Whitespace;

use crate::Rule;

/// The prefix of `except` comments.
pub const EXCEPT_COMMENT_PREFIX: &str = "#@ except:";

/// Creates an "unknown rule" diagnostic.
fn unknown_rule(id: &str, span: Span) -> Diagnostic {
    Diagnostic::note(format!("unknown lint rule `{id}`"))
        .with_label("cannot make an exception for this rule", span)
        .with_fix("remove the rule from the exception list")
}

/// A visitor that respects `#@ except` comments.
///
/// The format for the comment is `#@ except: <ids>`, where `ids` is a
/// comma-separated list of lint rule identifiers.
///
/// Any `#@ except` comments that come before the version statement will disable
/// the rule for the entire document.
///
/// Otherwise, `#@ except` comments disable the rule for the immediately
/// following AST node.
#[derive(Default)]
#[allow(missing_debug_implementations)]
pub struct ExceptVisitor {
    /// The map of rule name to visitor.
    visitors: IndexMap<&'static str, Box<dyn Visitor<State = Diagnostics>>>,
    /// The set of globally disabled rules; these rules no longer appear in the
    /// `visitors` map.
    global: HashSet<String>,
    /// A stack of exceptions; the first is the offset of the syntax element
    /// with the comment and the second is the set of exceptions.
    exceptions: Vec<(usize, HashSet<String>)>,
}

impl ExceptVisitor {
    /// Creates a new except visitor with the given rules.
    pub fn new<'a>(rules: impl Iterator<Item = &'a dyn Rule>) -> Self {
        Self {
            visitors: rules.map(|r| (r.id(), r.visitor())).collect(),
            global: Default::default(),
            exceptions: Default::default(),
        }
    }

    /// Invokes a callback on each rule's visitor provided the rule is not
    /// currently excepted.
    fn each_enabled_rule<F>(
        &mut self,
        state: &mut Diagnostics,
        reason: VisitReason,
        node: &SyntaxNode,
        mut cb: F,
    ) where
        F: FnMut(&mut Diagnostics, &mut dyn Visitor<State = Diagnostics>),
    {
        let start = node.text_range().start().into();

        if reason == VisitReason::Enter {
            let exceptions = self.exceptions_for(state, node);
            if !exceptions.is_empty() {
                self.exceptions.push((start, exceptions));
            }
        }

        for (id, visitor) in &mut self.visitors {
            if self.exceptions.iter().any(|(_, set)| set.contains(*id)) {
                continue;
            }

            cb(state, visitor.as_mut());
        }

        if reason == VisitReason::Exit {
            if let Some((prev, _)) = self.exceptions.last() {
                if *prev == start {
                    self.exceptions.pop();
                }
            }
        }
    }

    /// Gets the set of excepted rule ids for the given syntax node.
    fn exceptions_for(&self, state: &mut Diagnostics, node: &SyntaxNode) -> HashSet<String> {
        let siblings = node
            .siblings_with_tokens(Direction::Prev)
            .skip(1)
            .take_while(|s| s.kind() == SyntaxKind::Whitespace || s.kind() == SyntaxKind::Comment)
            .filter_map(SyntaxElement::into_token);

        let mut set = HashSet::default();
        for sibling in siblings {
            if sibling.kind() == SyntaxKind::Whitespace {
                continue;
            }

            if let Some(ids) = sibling.text().strip_prefix(EXCEPT_COMMENT_PREFIX) {
                let start: usize = sibling.text_range().start().into();
                let mut offset = EXCEPT_COMMENT_PREFIX.len();
                for id in ids.split(',') {
                    // First trim the start so we can determine how much whitespace was removed
                    let trimmed_start = id.trim_start();
                    // Next trim the end
                    let trimmed: &str = trimmed_start.trim_end();

                    if !self.visitors.contains_key(trimmed) && !self.global.contains(trimmed) {
                        // Calculate the span based off the current offset and how much whitespace
                        // was trimmed
                        let span = Span::new(
                            start + offset + (id.len() - trimmed_start.len()),
                            trimmed.len(),
                        );

                        state.add(unknown_rule(trimmed, span));
                    } else {
                        set.insert(trimmed.to_string());
                    }

                    offset += id.len() + 1 /* comma */;
                }
            }
        }

        set
    }
}

impl Visitor for ExceptVisitor {
    type State = Diagnostics;

    fn document(&mut self, state: &mut Self::State, reason: VisitReason, doc: &Document) {
        if reason == VisitReason::Enter {
            // Set the global exceptions
            if let Some(stmt) = doc.version_statement() {
                self.global = self.exceptions_for(state, stmt.syntax());
                for id in &self.global {
                    // This is a shift remove to maintain the original order provided at
                    // construction time; this is O(N), but both the set of visitors and exceptions
                    // is consistently small.
                    self.visitors.shift_remove(id.as_str());
                }
            }
        }

        // We don't need to check the exceptions here as the globally-disabled rules
        // were already removed.
        for (_, visitor) in &mut self.visitors {
            visitor.document(state, reason, doc);
        }
    }

    fn whitespace(&mut self, state: &mut Self::State, whitespace: &Whitespace) {
        for (id, visitor) in &mut self.visitors {
            if self.exceptions.iter().any(|(_, set)| set.contains(*id)) {
                continue;
            }

            visitor.whitespace(state, whitespace);
        }
    }

    fn comment(&mut self, state: &mut Self::State, comment: &Comment) {
        for (id, visitor) in &mut self.visitors {
            if self.exceptions.iter().any(|(_, set)| set.contains(*id)) {
                continue;
            }

            visitor.comment(state, comment);
        }
    }

    fn version_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &VersionStatement,
    ) {
        // We call every visitor here as we've already disabled the global rules based
        // on the version statement
        for (_, visitor) in &mut self.visitors {
            visitor.version_statement(state, reason, stmt);
        }
    }

    fn import_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::ImportStatement,
    ) {
        self.each_enabled_rule(state, reason, stmt.syntax(), |state, visitor| {
            visitor.import_statement(state, reason, stmt)
        });
    }

    fn struct_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        def: &v1::StructDefinition,
    ) {
        self.each_enabled_rule(state, reason, def.syntax(), |state, visitor| {
            visitor.struct_definition(state, reason, def)
        });
    }

    fn task_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        task: &v1::TaskDefinition,
    ) {
        self.each_enabled_rule(state, reason, task.syntax(), |state, visitor| {
            visitor.task_definition(state, reason, task)
        });
    }

    fn workflow_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        workflow: &v1::WorkflowDefinition,
    ) {
        self.each_enabled_rule(state, reason, workflow.syntax(), |state, visitor| {
            visitor.workflow_definition(state, reason, workflow)
        });
    }

    fn input_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::InputSection,
    ) {
        self.each_enabled_rule(state, reason, section.syntax(), |state, visitor| {
            visitor.input_section(state, reason, section)
        });
    }

    fn output_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::OutputSection,
    ) {
        self.each_enabled_rule(state, reason, section.syntax(), |state, visitor| {
            visitor.output_section(state, reason, section)
        });
    }

    fn command_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::CommandSection,
    ) {
        self.each_enabled_rule(state, reason, section.syntax(), |state, visitor| {
            visitor.command_section(state, reason, section)
        });
    }

    fn command_text(&mut self, state: &mut Self::State, text: &v1::CommandText) {
        for (id, visitor) in &mut self.visitors {
            if self.exceptions.iter().any(|(_, set)| set.contains(*id)) {
                continue;
            }

            visitor.command_text(state, text);
        }
    }

    fn runtime_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::RuntimeSection,
    ) {
        self.each_enabled_rule(state, reason, section.syntax(), |state, visitor| {
            visitor.runtime_section(state, reason, section)
        });
    }

    fn metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::MetadataSection,
    ) {
        self.each_enabled_rule(state, reason, section.syntax(), |state, visitor| {
            visitor.metadata_section(state, reason, section)
        });
    }

    fn parameter_metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::ParameterMetadataSection,
    ) {
        self.each_enabled_rule(state, reason, section.syntax(), |state, visitor| {
            visitor.parameter_metadata_section(state, reason, section)
        });
    }

    fn metadata_object(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        object: &v1::MetadataObject,
    ) {
        self.each_enabled_rule(state, reason, object.syntax(), |state, visitor| {
            visitor.metadata_object(state, reason, object)
        });
    }

    fn unbound_decl(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        decl: &v1::UnboundDecl,
    ) {
        self.each_enabled_rule(state, reason, decl.syntax(), |state, visitor| {
            visitor.unbound_decl(state, reason, decl)
        });
    }

    fn bound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &v1::BoundDecl) {
        self.each_enabled_rule(state, reason, decl.syntax(), |state, visitor| {
            visitor.bound_decl(state, reason, decl)
        });
    }

    fn expr(&mut self, state: &mut Self::State, reason: VisitReason, expr: &v1::Expr) {
        self.each_enabled_rule(state, reason, expr.syntax(), |state, visitor| {
            visitor.expr(state, reason, expr)
        });
    }

    fn string_text(&mut self, state: &mut Self::State, text: &v1::StringText) {
        for (id, visitor) in &mut self.visitors {
            if self.exceptions.iter().any(|(_, set)| set.contains(*id)) {
                continue;
            }

            visitor.string_text(state, text);
        }
    }

    fn conditional_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::ConditionalStatement,
    ) {
        self.each_enabled_rule(state, reason, stmt.syntax(), |state, visitor| {
            visitor.conditional_statement(state, reason, stmt)
        });
    }

    fn scatter_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::ScatterStatement,
    ) {
        self.each_enabled_rule(state, reason, stmt.syntax(), |state, visitor| {
            visitor.scatter_statement(state, reason, stmt)
        });
    }

    fn call_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::CallStatement,
    ) {
        self.each_enabled_rule(state, reason, stmt.syntax(), |state, visitor| {
            visitor.call_statement(state, reason, stmt)
        });
    }
}
