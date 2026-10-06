//! Validation of `command` sections.

use std::fmt;

use rowan::ast::support;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::v1::CommandPart;
use wdl_ast::v1::CommandSection;
use wdl_grammar::Diagnostic;
use wdl_grammar::Severity;
use wdl_grammar::Span;
use wdl_grammar::SupportedVersion;
use wdl_grammar::SyntaxKind;

use crate::CommandSectionIndentationRule;
use crate::Diagnostics;
use crate::Document;
use crate::VisitReason;
use crate::Visitor;
use crate::util::lines_with_offset;

/// Represents the indentation kind.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum IndentationKind {
    /// Spaces are used for the indentation.
    Spaces,
    /// Tabs are used for the indentation.
    Tabs,
}

impl fmt::Display for IndentationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spaces => write!(f, "spaces"),
            Self::Tabs => write!(f, "tabs"),
        }
    }
}

impl From<u8> for IndentationKind {
    fn from(b: u8) -> Self {
        match b {
            b' ' => Self::Spaces,
            b'\t' => Self::Tabs,
            _ => panic!("not indentation"),
        }
    }
}

/// Creates a "mixed indentation" diagnostic.
fn mixed_indentation(command: Span, span: Span, kind: IndentationKind) -> Diagnostic {
    Diagnostic::warning("mixed indentation within a command")
        .with_rule(CommandSectionIndentationRule::ID)
        .with_label(
            format!(
                "indented with {kind} until this {anti}",
                anti = match kind {
                    IndentationKind::Spaces => "tab",
                    IndentationKind::Tabs => "space",
                }
            ),
            span,
        )
        .with_label(
            "this command section uses both tabs and spaces in leading whitespace",
            command,
        )
        .with_fix("use either tabs or spaces exclusively for indentation")
}

/// A visitor of `command` sections.
#[derive(Default, Debug)]
pub struct CommandSectionVisitor {
    /// Severity of the `CommandSectionIndentation` rule.
    mixed_indentation: Option<Severity>,
}

impl Visitor for CommandSectionVisitor {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn document(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        doc: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.mixed_indentation = doc
            .config()
            .diagnostics_config()
            .command_section_indentation;
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

        if let Some(severity) = self.mixed_indentation
            && let Some((mixed_span, kind)) = check_mixed_indentation(section)
        {
            let command_keyword = support::token(section.inner(), SyntaxKind::CommandKeyword)
                .expect("should have a command keyword token");

            diagnostics.exceptable_add(
                mixed_indentation(command_keyword.text_range().into(), mixed_span, kind)
                    .with_severity(severity),
                section.inner(),
                &CommandSectionIndentationRule::EXCEPTABLE_NODES,
            );
        }
    }
}

/// Implementation for the `CommandSectionIndentation` rule.
fn check_mixed_indentation(section: &CommandSection) -> Option<(Span, IndentationKind)> {
    let mut kind = None;
    let mut mixed_span = None;
    let mut skip_next_line = false;
    'outer: for part in section.parts() {
        match part {
            CommandPart::Text(text) => {
                for (line, start, _) in lines_with_offset(text.text()) {
                    // Check to see if we should skip the next line
                    // This happens after we encounter a placeholder
                    if skip_next_line {
                        skip_next_line = false;
                        continue;
                    }

                    // Otherwise, check the leading whitespace
                    for (i, b) in line.as_bytes().iter().enumerate() {
                        match b {
                            b' ' | b'\t' => {
                                let current = IndentationKind::from(*b);
                                let kind = kind.get_or_insert(current);
                                if current != *kind {
                                    // Mixed indentation, store the span of the
                                    // first mixed
                                    // character
                                    mixed_span =
                                        Some(Span::new(text.span().start() + start + i, 1));
                                    break 'outer;
                                }
                            }
                            _ => break,
                        }
                    }
                }
            }
            CommandPart::Placeholder(_) => {
                // Encountered a placeholder, skip the next line of text as it's
                // really a part of the same line
                skip_next_line = true;
            }
        }
    }

    mixed_span.map(|span| (span, kind.expect("an indentation kind should be present")))
}
