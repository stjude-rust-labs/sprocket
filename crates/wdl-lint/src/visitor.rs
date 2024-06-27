//! Implementation of the lint visitor.

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

use crate::rules;
use crate::Rule;

/// The prefix of `except` comments.
pub const EXCEPT_COMMENT_PREFIX: &str = "#@ except:";

/// Creates an "unknown rule" diagnostic.
fn unknown_rule(id: &str, span: Span) -> Diagnostic {
    Diagnostic::note(format!("unknown lint rule `{id}`"))
        .with_label("cannot make an exception for this rule", span)
        .with_fix("remove the rule from the exception list")
}

/// A visitor that runs linting rules.
///
/// By default, the visitor runs all lint rules.
///
/// This visitor respects `#@ except` comments that precede AST nodes.
///
/// The format for the comment is `#@ except: <ids>`, where `ids` is a
/// comma-separated list of lint rule identifiers.
///
/// Any `#@ except` comments that come before the version statement will disable
/// the rule for the entire document.
///
/// Otherwise, `#@ except` comments disable the rule for the immediately
/// following AST node.
#[allow(missing_debug_implementations)]
pub struct LintVisitor {
    /// The map of rule name to rule.
    rules: IndexMap<&'static str, Box<dyn Rule>>,
    /// The set of globally disabled rules; these rules no longer appear in the
    /// `visitors` map.
    global: HashSet<String>,
    /// A stack of exceptions; the first is the offset of the syntax element
    /// with the comment and the second is the set of exceptions.
    exceptions: Vec<(usize, HashSet<String>)>,
}

impl LintVisitor {
    /// Creates a new linting visitor with the given rules.
    pub fn new(rules: impl IntoIterator<Item = Box<dyn Rule>>) -> Self {
        Self {
            rules: rules.into_iter().map(|r| (r.id(), r)).collect(),
            global: Default::default(),
            exceptions: Default::default(),
        }
    }

    /// Invokes a callback on each rule provided the rule is not currently
    /// excepted.
    fn each_enabled_rule<F>(
        &mut self,
        state: &mut Diagnostics,
        reason: VisitReason,
        node: &SyntaxNode,
        mut cb: F,
    ) where
        F: FnMut(&mut Diagnostics, &mut dyn Rule),
    {
        let start = node.text_range().start().into();

        if reason == VisitReason::Enter {
            let exceptions = self.exceptions_for(state, node);
            if !exceptions.is_empty() {
                self.exceptions.push((start, exceptions));
            }
        }

        for (id, rule) in &mut self.rules {
            if self.exceptions.iter().any(|(_, set)| set.contains(*id)) {
                continue;
            }

            cb(state, rule.as_mut());
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

                    if !self.rules.contains_key(trimmed) && !self.global.contains(trimmed) {
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

impl Default for LintVisitor {
    fn default() -> Self {
        Self {
            rules: rules().into_iter().map(|r| (r.id(), r)).collect(),
            global: Default::default(),
            exceptions: Default::default(),
        }
    }
}

impl Visitor for LintVisitor {
    type State = Diagnostics;

    fn document(&mut self, state: &mut Self::State, reason: VisitReason, doc: &Document) {
        if reason == VisitReason::Enter {
            // Set the global exceptions
            if let Some(stmt) = doc.version_statement() {
                self.global = self.exceptions_for(state, stmt.syntax());
                for id in &self.global {
                    // This is a shift remove to maintain the original order provided at
                    // construction time; this is O(N), but both the set of rules and exceptions
                    // is consistently small.
                    self.rules.shift_remove(id.as_str());
                }
            }
        }

        // We don't need to check the exceptions here as the globally-disabled rules
        // were already removed.
        for (_, rule) in &mut self.rules {
            rule.document(state, reason, doc);
        }
    }

    fn whitespace(&mut self, state: &mut Self::State, whitespace: &Whitespace) {
        for (id, rule) in &mut self.rules {
            if self.exceptions.iter().any(|(_, set)| set.contains(*id)) {
                continue;
            }

            rule.whitespace(state, whitespace);
        }
    }

    fn comment(&mut self, state: &mut Self::State, comment: &Comment) {
        for (id, rule) in &mut self.rules {
            if self.exceptions.iter().any(|(_, set)| set.contains(*id)) {
                continue;
            }

            rule.comment(state, comment);
        }
    }

    fn version_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &VersionStatement,
    ) {
        // We call every rule here as we've already disabled the global rules based on
        // the version statement
        for (_, rule) in &mut self.rules {
            rule.version_statement(state, reason, stmt);
        }
    }

    fn import_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::ImportStatement,
    ) {
        self.each_enabled_rule(state, reason, stmt.syntax(), |state, rule| {
            rule.import_statement(state, reason, stmt)
        });
    }

    fn struct_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        def: &v1::StructDefinition,
    ) {
        self.each_enabled_rule(state, reason, def.syntax(), |state, rule| {
            rule.struct_definition(state, reason, def)
        });
    }

    fn task_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        task: &v1::TaskDefinition,
    ) {
        self.each_enabled_rule(state, reason, task.syntax(), |state, rule| {
            rule.task_definition(state, reason, task)
        });
    }

    fn workflow_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        workflow: &v1::WorkflowDefinition,
    ) {
        self.each_enabled_rule(state, reason, workflow.syntax(), |state, rule| {
            rule.workflow_definition(state, reason, workflow)
        });
    }

    fn input_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::InputSection,
    ) {
        self.each_enabled_rule(state, reason, section.syntax(), |state, rule| {
            rule.input_section(state, reason, section)
        });
    }

    fn output_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::OutputSection,
    ) {
        self.each_enabled_rule(state, reason, section.syntax(), |state, rule| {
            rule.output_section(state, reason, section)
        });
    }

    fn command_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::CommandSection,
    ) {
        self.each_enabled_rule(state, reason, section.syntax(), |state, rule| {
            rule.command_section(state, reason, section)
        });
    }

    fn command_text(&mut self, state: &mut Self::State, text: &v1::CommandText) {
        for (id, rule) in &mut self.rules {
            if self.exceptions.iter().any(|(_, set)| set.contains(*id)) {
                continue;
            }

            rule.command_text(state, text);
        }
    }

    fn runtime_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::RuntimeSection,
    ) {
        self.each_enabled_rule(state, reason, section.syntax(), |state, rule| {
            rule.runtime_section(state, reason, section)
        });
    }

    fn metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::MetadataSection,
    ) {
        self.each_enabled_rule(state, reason, section.syntax(), |state, rule| {
            rule.metadata_section(state, reason, section)
        });
    }

    fn parameter_metadata_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::ParameterMetadataSection,
    ) {
        self.each_enabled_rule(state, reason, section.syntax(), |state, rule| {
            rule.parameter_metadata_section(state, reason, section)
        });
    }

    fn metadata_object(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        object: &v1::MetadataObject,
    ) {
        self.each_enabled_rule(state, reason, object.syntax(), |state, rule| {
            rule.metadata_object(state, reason, object)
        });
    }

    fn unbound_decl(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        decl: &v1::UnboundDecl,
    ) {
        self.each_enabled_rule(state, reason, decl.syntax(), |state, rule| {
            rule.unbound_decl(state, reason, decl)
        });
    }

    fn bound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &v1::BoundDecl) {
        self.each_enabled_rule(state, reason, decl.syntax(), |state, rule| {
            rule.bound_decl(state, reason, decl)
        });
    }

    fn expr(&mut self, state: &mut Self::State, reason: VisitReason, expr: &v1::Expr) {
        self.each_enabled_rule(state, reason, expr.syntax(), |state, rule| {
            rule.expr(state, reason, expr)
        });
    }

    fn string_text(&mut self, state: &mut Self::State, text: &v1::StringText) {
        for (id, rule) in &mut self.rules {
            if self.exceptions.iter().any(|(_, set)| set.contains(*id)) {
                continue;
            }

            rule.string_text(state, text);
        }
    }

    fn conditional_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::ConditionalStatement,
    ) {
        self.each_enabled_rule(state, reason, stmt.syntax(), |state, rule| {
            rule.conditional_statement(state, reason, stmt)
        });
    }

    fn scatter_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::ScatterStatement,
    ) {
        self.each_enabled_rule(state, reason, stmt.syntax(), |state, rule| {
            rule.scatter_statement(state, reason, stmt)
        });
    }

    fn call_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::CallStatement,
    ) {
        self.each_enabled_rule(state, reason, stmt.syntax(), |state, rule| {
            rule.call_statement(state, reason, stmt)
        });
    }
}
