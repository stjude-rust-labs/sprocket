//! A lint rule for checking mixed indentation in command text.

use std::fmt;

use wdl_ast::support;
use wdl_ast::v1::CommandPart;
use wdl_ast::v1::CommandSection;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Span;
use wdl_ast::SyntaxKind;
use wdl_ast::ToSpan;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::util::lines_with_offset;
use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the command section mixed indentation rule.
const ID: &str = "CommandSectionMixedIndentation";

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
        .with_rule(ID)
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
        .with_fix("use the same whitespace character for indentation")
}

/// Detects mixed indentation in a command section.
#[derive(Debug, Clone, Copy)]
pub struct CommandSectionMixedIndentationRule;

impl Rule for CommandSectionMixedIndentationRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that lines within a command do not mix spaces and tabs."
    }

    fn explanation(&self) -> &'static str {
        "Mixing indentation (tab and space) characters within the command line causes leading \
         whitespace stripping to be skipped. Commands may be whitespace sensitive, and skipping \
         the whitespace stripping step may cause unexpected behavior."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Correctness, Tag::Spacing, Tag::Clarity])
    }
}

impl Visitor for CommandSectionMixedIndentationRule {
    type State = Diagnostics;

    fn command_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &CommandSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let mut kind = None;
        let mut mixed_span = None;
        let mut skip_next_line = false;
        'outer: for part in section.parts() {
            match part {
                CommandPart::Text(text) => {
                    for (line, start, _) in lines_with_offset(text.as_str()) {
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
                                        // Mixed indentation, store the span of the first mixed
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

        if let Some(span) = mixed_span {
            let command_keyword = support::token(section.syntax(), SyntaxKind::CommandKeyword)
                .expect("should have a command keyword token");

            state.add(mixed_indentation(
                command_keyword.text_range().to_span(),
                span,
                kind.expect("an indentation kind should be present"),
            ));
        }
    }
}
