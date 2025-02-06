//! A lint rule for running shellcheck against command sections.
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::Write;
use std::process;
use std::process::Stdio;
use std::sync::OnceLock;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use ftree::FenwickTree;
use rand::distr::Alphanumeric;
use rand::distr::SampleString;
use serde::Deserialize;
use serde_json;
use tracing::debug;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::ToSpan;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;
use wdl_ast::support;
use wdl_ast::v1::CommandPart;
use wdl_ast::v1::CommandSection;
use wdl_ast::v1::Placeholder;
use wdl_ast::v1::StrippedCommandPart;
use wdl_ast::v1::TaskDefinition;

use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::fix::Fixer;
use crate::fix::InsertionPoint;
use crate::fix::Replacement;
use crate::util::is_properly_quoted;
use crate::util::lines_with_offset;
use crate::util::program_exists;

/// The shellcheck executable
const SHELLCHECK_BIN: &str = "shellcheck";

/// Shellcheck lints that we want to suppresks.
/// These two lints always co-occur with a more
/// informative message.
const SHELLCHECK_SUPPRESS: &[&str] = &[
    "1009", // the mentioned parser error was in... (unhelpful commentary)
    "1072", // Unexpected eof (unhelpful commentary)
];

/// ShellCheck: var is referenced but not assigned.
const SHELLCHECK_REFERENCED_UNASSIGNED: usize = 2154;

/// ShellCheck wiki base url.
const SHELLCHECK_WIKI: &str = "https://www.shellcheck.net/wiki";

/// Whether or not shellcheck exists on the system
static SHELLCHECK_EXISTS: OnceLock<bool> = OnceLock::new();

/// The identifier for the command section ShellCheck rule.
const ID: &str = "ShellCheck";

/// Suggested fix for a ShellCheck diagnostic.
#[derive(Clone, Debug, Deserialize)]
struct ShellCheckFix {
    /// The replacements to perform.
    pub replacements: Vec<ShellCheckReplacement>,
}

/// A ShellCheck replacement.
///
/// This differs from a [`Replacement`] in that
/// 1) columns are 1-indexed
/// 2) it may span multiple lines and thus cannot be directly passed to a
///    [`Fixer`].
///
/// It must be normalized with `normalize_replacements` before use.
#[derive(Clone, Debug, Deserialize)]
struct ShellCheckReplacement {
    /// Line number replacement occurs on.
    pub line: usize,
    /// Line number replacement ends on.
    #[serde(rename = "endLine")]
    pub end_line: usize,
    /// Order in which replacements should happen. Highest precedence first.
    pub precedence: usize,
    /// An `InsertionPoint`.
    #[serde(rename = "insertionPoint")]
    pub insertion_point: InsertionPoint,
    /// Column replacement occurs on.
    pub column: usize,
    /// Column replacements ends on.
    #[serde(rename = "endColumn")]
    pub end_column: usize,
    /// Replacement text.
    #[serde(rename = "replacement")]
    pub value: String,
}

/// A ShellCheck diagnostic.
///
/// The `file` field is ommitted as we have no use for it.
#[derive(Clone, Debug, Deserialize)]
struct ShellCheckDiagnostic {
    /// Line number comment starts on.
    pub line: usize,
    /// Line number comment ends on.
    #[serde(rename = "endLine")]
    pub end_line: usize,
    /// Column comment starts on.
    pub column: usize,
    /// Column comment ends on.
    #[serde(rename = "endColumn")]
    pub end_column: usize,
    /// Severity of the comment.
    pub level: String,
    /// ShellCheck error code.
    pub code: usize,
    /// Message associated with the comment.
    pub message: String,
    /// Optional fixes to apply.
    pub fix: Option<ShellCheckFix>,
}

/// Convert [`ShellCheckReplacement`]s into [`Replacement`]s.
///
/// Column indices are shifted to 0-based.
/// Multi-line replacements are normalized so that column indices are
/// as though the string is on a single line.
fn normalize_replacements(
    replacements: &[ShellCheckReplacement],
    shift_tree: &FenwickTree<usize>,
) -> Vec<Replacement> {
    replacements
        .iter()
        .map(|r| {
            Replacement::new(
                r.column + shift_tree.prefix_sum(r.line - 1, 0) - 1,
                r.end_column + shift_tree.prefix_sum(r.end_line - 1, 0) - 1,
                r.insertion_point,
                r.value.clone(),
                r.precedence,
            )
        })
        .collect()
}

/// Run shellcheck on a command.
///
/// writes command text to stdin of shellcheck process
/// and returns parsed `ShellCheckDiagnostic`s
fn run_shellcheck(command: &str) -> Result<Vec<ShellCheckDiagnostic>> {
    let mut sc_proc = process::Command::new(SHELLCHECK_BIN)
        .args([
            "-s", // bash shell
            "bash",
            "-f", // output JSON
            "json",
            "-e", // errors to suppress
            &SHELLCHECK_SUPPRESS.join(","),
            "-S", // set minimum lint level to style
            "style",
            "-", // input is piped to STDIN
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("spawning the `shellcheck` process")?;
    debug!("`shellcheck` process id: {}", sc_proc.id());
    {
        let mut proc_stdin = sc_proc
            .stdin
            .take()
            .context("obtaining the STDIN handle of the `shellcheck` process")?;
        proc_stdin.write_all(command.as_bytes())?;
    }

    let output = sc_proc
        .wait_with_output()
        .context("waiting for the `shellcheck` process to complete")?;

    // shellcheck returns exit code 1 if
    // any checked files result in comments
    // so cannot check with status.success()
    match output.status.code() {
        Some(0) | Some(1) => serde_json::from_slice::<Vec<ShellCheckDiagnostic>>(&output.stdout)
            .context("deserializing STDOUT from `shellcheck` process"),
        Some(code) => bail!("unexpected `shellcheck` exit code: {}", code),
        None => bail!("the `shellcheck` process appears to have been interrupted"),
    }
}

/// Runs ShellCheck on a command section and reports diagnostics.
#[derive(Default, Debug, Clone, Copy)]
pub struct ShellCheckRule;

impl Rule for ShellCheckRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that command blocks are free of ShellCheck violations."
    }

    fn explanation(&self) -> &'static str {
        "ShellCheck (https://shellcheck.net) is a static analysis tool and linter for sh / bash. \
         The lints provided by ShellCheck help prevent common errors and pitfalls in your scripts. \
         Following its recommendations will increase the robustness of your command sections."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Correctness, Tag::Portability])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::CommandSectionNode,
        ])
    }
}

/// Convert a WDL `Placeholder` to a bash variable declaration.
///
/// Returns "WDL" + <placeholder length - 6> random alphnumeric characters.
/// The returned value is shorter than the placeholder by 3 characters so
/// that the caller may pad with other characters as necessary
/// depending on whether or not the variable needs to be treated as a
/// declaration, expansion, or literal.
fn to_bash_var(placeholder: &Placeholder) -> String {
    let placeholder_len: usize = placeholder.syntax().text_range().len().into();
    // don't start variable with numbers
    let mut bash_var = String::from("WDL");
    bash_var
        .push_str(&Alphanumeric.sample_string(&mut rand::rng(), placeholder_len.saturating_sub(6)));
    bash_var
}

/// Retrieve all input and private declarations for a task.
fn gather_task_declarations(task: &TaskDefinition) -> HashSet<String> {
    let mut decls = HashSet::new();
    if let Some(input) = task.input() {
        for decl in input.declarations() {
            decls.insert(decl.name().as_str().to_owned());
        }
    }

    for decl in task.declarations() {
        decls.insert(decl.name().as_str().to_owned());
    }
    decls
}

/// Create an appropriate 'fix' message.
///
/// Returns the following range of text:
/// start = min(diagnostic highlight start, left-most replacement start)
/// end = max(diagnostic highlight end, right-most replacement end)
/// start..end
fn create_fix_message(
    replacements: Vec<Replacement>,
    command_text: &str,
    diagnostic_span: Span,
) -> String {
    let mut fixer = Fixer::new(command_text.to_owned());
    // Get the original left-most and right-most replacement indices.
    let rep_start = replacements
        .iter()
        .map(|r| r.start())
        .min()
        .expect("replacements is non-empty");
    let rep_end = replacements
        .iter()
        .map(|r| r.end())
        .max()
        .expect("replacements is non-empty");
    let start = rep_start.min(diagnostic_span.start());
    let end = rep_end.max(diagnostic_span.end());
    fixer.apply_replacements(replacements);
    // Adjust start and end based on final tree.
    let adj_range = {
        let range = fixer.adjust_range(start..end);
        // the prefix sum does not include the value at
        // the actual index. But, we want this value because
        // we may have inserted text at the very end.
        // ftree provides no method to get this value, so
        // we must calculate it.
        let max_pos = (end + 1).min(fixer.value().len());
        let extend_by = (fixer.transform(max_pos) - fixer.transform(max_pos - 1)).saturating_sub(1);
        range.start..(range.end + extend_by)
    };
    format!("did you mean `{}`?", &fixer.value()[adj_range])
}

/// Creates a "ShellCheck lint" diagnostic from a [ShellCheckDiagnostic]
fn shellcheck_lint(
    diagnostic: &ShellCheckDiagnostic,
    command_text: &str,
    line_map: &HashMap<usize, Span>,
    shift_tree: &FenwickTree<usize>,
) -> Diagnostic {
    let label = format!(
        "SC{}[{}]: {}",
        diagnostic.code, diagnostic.level, diagnostic.message
    );
    // This span is relative to the entire document.
    let span = calculate_span(diagnostic, line_map);
    let fix_msg = match diagnostic.fix {
        Some(ref fix) => {
            let reps = normalize_replacements(&fix.replacements, shift_tree);
            // This span is relative to the command text.
            let diagnostic_span = {
                let start = diagnostic.column + shift_tree.prefix_sum(diagnostic.line - 1, 0) - 1;
                let end =
                    diagnostic.end_column + shift_tree.prefix_sum(diagnostic.end_line - 1, 0) - 1;
                Span::new(start, end - start)
            };
            create_fix_message(reps, command_text, diagnostic_span)
        }
        None => String::from("address the diagnostic as recommended in the message"),
    };
    Diagnostic::note(&diagnostic.message)
        .with_rule(ID)
        .with_label(label, span)
        .with_label(
            format!("more info: {}/SC{}", &SHELLCHECK_WIKI, diagnostic.code),
            span,
        )
        .with_fix(fix_msg)
}

/// Sanitize a [CommandSection].
///
/// Removes all trailing whitespace, replaces placeholders
/// with dummy bash variables or literals, and records declarations.
///
/// If the section contains mixed indentation, returns None.
fn sanitize_command(section: &CommandSection) -> Option<(String, HashSet<String>, usize)> {
    let amount_stripped = section.count_whitespace()?;
    let mut sanitized_command = String::new();
    let mut decls = HashSet::new();
    let mut needs_quotes = true;
    let mut is_literal = false;

    if let Some(cmd_parts) = section.strip_whitespace() {
        cmd_parts.iter().for_each(|part| match part {
            StrippedCommandPart::Text(text) => {
                sanitized_command.push_str(text);
                // if this placeholder is in a single-quoted segment
                // don't treat as an expansion but rather a literal.
                is_literal ^= !is_properly_quoted(text, '\'');
                // if this text section is not properly quoted then the
                // next placeholder does *not* need double quotes
                // because it will end up enclosed.
                needs_quotes ^= !is_properly_quoted(text, '"');
            }
            StrippedCommandPart::Placeholder(placeholder) => {
                let bash_var = to_bash_var(placeholder);
                // we need to save the var so we can suppress later
                decls.insert(bash_var.clone());

                if is_literal {
                    // pad literal with three underscores to account for ~{}
                    sanitized_command.push_str(&format!("___{bash_var}"));
                } else if needs_quotes {
                    // surround with quotes for proper form
                    sanitized_command.push_str(&format!("\"${bash_var}\""));
                } else {
                    // surround with curly braces because already
                    // inside of a quoted segment.
                    sanitized_command.push_str(&format!("${{{bash_var}}}"));
                }
            }
        });
        Some((sanitized_command, decls, amount_stripped))
    } else {
        None
    }
}

/// Maps each line as shellcheck sees it to its corresponding span in the
/// source.
fn map_shellcheck_lines(
    section: &CommandSection,
    leading_whitespace: usize,
) -> HashMap<usize, Span> {
    let mut line_map = HashMap::new();
    let mut line_num = 1;
    let mut skip_next_line = false;
    for part in section.parts() {
        match part {
            CommandPart::Text(ref text) => {
                for (line, line_start, _) in lines_with_offset(text.as_str()) {
                    // this occurs after encountering a placeholder
                    if skip_next_line {
                        skip_next_line = false;
                        continue;
                    }
                    // The first line is removed entirely, UNLESS there is content on it.
                    if line_num == 1 && line.is_empty() {
                        continue;
                    }
                    // Add back the leading whitespace that was stripped.
                    let adjusted_start = text.span().start() + line_start + leading_whitespace;
                    line_map.insert(line_num, Span::new(adjusted_start, line.len()));
                    line_num += 1;
                }
            }
            CommandPart::Placeholder(_) => {
                skip_next_line = true;
            }
        }
    }
    line_map
}

/// Calculates the correct [Span] for a [ShellCheckDiagnostic] relative to the
/// source.
fn calculate_span(diagnostic: &ShellCheckDiagnostic, line_map: &HashMap<usize, Span>) -> Span {
    // shellcheck 1-indexes columns, so subtract 1.
    let start = line_map
        .get(&diagnostic.line)
        .expect("shellcheck line corresponds to command line")
        .start()
        + diagnostic.column
        - 1;
    let len = if diagnostic.end_line > diagnostic.line {
        // this is a multiline diagnostic
        let end_line_end = line_map
            .get(&diagnostic.end_line)
            .expect("shellcheck line corresponds to command line")
            .start()
            + diagnostic.end_column
            - 1;
        end_line_end.saturating_sub(start)
    } else {
        // single line diagnostic
        (diagnostic.end_column).saturating_sub(diagnostic.column)
    };
    Span::new(start, len)
}

impl Visitor for ShellCheckRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
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

        if !SHELLCHECK_EXISTS.get_or_init(|| {
            if !program_exists(SHELLCHECK_BIN) {
                let command_keyword = support::token(section.syntax(), SyntaxKind::CommandKeyword)
                    .expect(
                        "should have a
                command keyword token",
                    );
                state.exceptable_add(
                    Diagnostic::note("running `shellcheck` on command section")
                        .with_label(
                            "could not find `shellcheck` executable.",
                            command_keyword.text_range().to_span(),
                        )
                        .with_rule(ID)
                        .with_fix(
                            "install shellcheck (https://www.shellcheck.net) or disable this lint.",
                        ),
                    SyntaxElement::from(section.syntax().clone()),
                    &self.exceptable_nodes(),
                );
                return false;
            }
            true
        }) {
            return;
        }

        // Collect declarations so we can ignore placeholder variables
        let parent_task = section.parent().into_task().expect("parent is a task");
        let mut decls = gather_task_declarations(&parent_task);

        // Replace all placeholders in the command with dummy bash variables
        let Some((sanitized_command, cmd_decls, amount_stripped)) = sanitize_command(section)
        else {
            // This is the case where the command section contains
            // mixed indentation. We silently return and allow
            // the mixed indentation lint to report this.
            return;
        };
        decls.extend(cmd_decls);
        let line_map = map_shellcheck_lines(section, amount_stripped);

        // create a Fenwick tree where each index is a line number
        // and each value is the length of the line.
        // For efficiency, we do this only once.
        let shift_values = lines_with_offset(&sanitized_command)
            .map(|(_, line_start, next_start)| next_start - line_start);
        let shift_tree = FenwickTree::from_iter(shift_values);

        match run_shellcheck(&sanitized_command) {
            Ok(diagnostics) => {
                for diagnostic in diagnostics {
                    // Skip declarations that shellcheck is unaware of.
                    // ShellCheck's message always starts with the variable name
                    // that is unassigned.
                    let target_variable =
                        diagnostic.message.split_whitespace().next().unwrap_or("");
                    if diagnostic.code == SHELLCHECK_REFERENCED_UNASSIGNED
                        && decls.contains(target_variable)
                    {
                        continue;
                    }
                    state.exceptable_add(
                        shellcheck_lint(&diagnostic, &sanitized_command, &line_map, &shift_tree),
                        SyntaxElement::from(section.syntax().clone()),
                        &self.exceptable_nodes(),
                    )
                }
            }
            Err(e) => {
                let command_keyword = support::token(section.syntax(), SyntaxKind::CommandKeyword)
                    .expect("should have a command keyword token");
                state.exceptable_add(
                    Diagnostic::error("running `shellcheck` on command section")
                        .with_label(e.to_string(), command_keyword.text_range().to_span())
                        .with_rule(ID)
                        .with_fix("address reported error."),
                    SyntaxElement::from(section.syntax().clone()),
                    &self.exceptable_nodes(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ftree::FenwickTree;
    use pretty_assertions::assert_eq;

    use super::ShellCheckReplacement;
    use super::normalize_replacements;
    use crate::fix::Fixer;
    use crate::fix::{self};
    use crate::util::lines_with_offset;

    #[test]
    fn test_normalize_replacements() {
        // shellcheck would see this as
        // ABBBB
        // BBBA
        let ref_str = String::from("ABBBB\nBBBA");
        let expected = String::from("AAAAA");
        let sc_rep = ShellCheckReplacement {
            line: 1,
            end_line: 2,
            column: 2,
            end_column: 4,
            precedence: 1,
            insertion_point: fix::InsertionPoint::AfterEnd,
            value: String::from("AAA"),
        };
        let shift_values =
            lines_with_offset(&ref_str).map(|(_, line_start, next_start)| next_start - line_start);
        let shift_tree = FenwickTree::from_iter(shift_values);
        let normalized = normalize_replacements(&[sc_rep], &shift_tree);
        let rep = &normalized[0];

        assert_eq!(rep.start(), 1);
        assert_eq!(rep.end(), 9);

        let mut fixer = Fixer::new(ref_str);
        fixer.apply_replacement(rep);
        assert_eq!(fixer.value(), expected);
    }

    #[test]
    fn test_normalize_replacements2() {
        let ref_str = String::from("ABBBBBBBA");
        let expected = String::from("AAAAA");
        let sc_rep = ShellCheckReplacement {
            line: 1,
            end_line: 1,
            column: 2,
            end_column: 9,
            precedence: 1,
            insertion_point: fix::InsertionPoint::AfterEnd,
            value: String::from("AAA"),
        };
        let shift_values =
            lines_with_offset(&ref_str).map(|(_, line_start, next_start)| next_start - line_start);
        let shift_tree = FenwickTree::from_iter(shift_values);
        let normalized = normalize_replacements(&[sc_rep], &shift_tree);
        let rep = &normalized[0];

        assert_eq!(rep.start(), 1);
        assert_eq!(rep.end(), 8);

        let mut fixer = Fixer::new(ref_str);
        fixer.apply_replacement(rep);
        assert_eq!(fixer.value(), expected);
    }
}
