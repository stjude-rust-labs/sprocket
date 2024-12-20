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
use rand::distributions::Alphanumeric;
use rand::distributions::DistString;
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
use crate::util::count_leading_whitespace;
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

/// A ShellCheck diagnostic.
///
/// The `file` and `fix` fields are ommitted as we have no use for them.
#[derive(Clone, Debug, Deserialize)]
struct ShellCheckDiagnostic {
    /// line number comment starts on
    pub line: usize,
    /// line number comment ends on
    #[serde(rename = "endLine")]
    pub end_line: usize,
    /// column comment starts on
    pub column: usize,
    /// column comment ends on
    #[serde(rename = "endColumn")]
    pub end_column: usize,
    /// severity of the comment
    pub level: String,
    /// shellcheck error code
    pub code: usize,
    /// message associated with the comment
    pub message: String,
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
    bash_var.push_str(
        &Alphanumeric.sample_string(&mut rand::thread_rng(), placeholder_len.saturating_sub(6)),
    );
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

/// Creates a "ShellCheck lint" diagnostic from a `ShellCheckDiagnostic`
fn shellcheck_lint(diagnostic: &ShellCheckDiagnostic, span: Span) -> Diagnostic {
    let label = format!(
        "SC{}[{}]: {}",
        diagnostic.code, diagnostic.level, diagnostic.message
    );
    Diagnostic::note(&diagnostic.message)
        .with_rule(ID)
        .with_label(label, span)
        .with_label(
            format!("more info: {}/SC{}", &SHELLCHECK_WIKI, diagnostic.code),
            span,
        )
        .with_fix("address the diagnostic as recommended in the message")
}

/// Sanitize a `CommandSection`.
///
/// Removes all trailing whitespace, replaces placeholders
/// with dummy bash variables or literals, and records declarations.
///
/// If the section contains mixed indentation, returns None.
fn sanitize_command(section: &CommandSection) -> Option<(String, HashSet<String>)> {
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
        Some((sanitized_command, decls))
    } else {
        None
    }
}

/// Maps each line as shellcheck sees it to its corresponding start position in
/// the source.
fn map_shellcheck_lines(section: &CommandSection) -> HashMap<usize, usize> {
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
                    // Add back leading whitespace that is stripped from the sanitized command.
                    // The first line is removed entirely, UNLESS there is content on it.
                    let leading_ws = if line_num > 1 || !line.trim().is_empty() {
                        count_leading_whitespace(line)
                    } else {
                        continue;
                    };
                    let adjusted_start = text.span().start() + line_start + leading_ws;
                    line_map.insert(line_num, adjusted_start);
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

/// Calculates the correct `Span` for a `ShellCheckDiagnostic` relative to the
/// source.
fn calculate_span(diagnostic: &ShellCheckDiagnostic, line_map: &HashMap<usize, usize>) -> Span {
    // shellcheck 1-indexes columns, so subtract 1.
    let start = line_map
        .get(&diagnostic.line)
        .expect("shellcheck line corresponds to command line")
        + diagnostic.column
        - 1;
    let len = if diagnostic.end_line > diagnostic.line {
        // this is a multiline diagnostic
        let end_line_end = line_map
            .get(&diagnostic.end_line)
            .expect("shellcheck line corresponds to command line")
            + diagnostic.end_column
            - 1;
        // - 2 to discount first and last newlines
        end_line_end.saturating_sub(start) - 2
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
        let Some((sanitized_command, cmd_decls)) = sanitize_command(section) else {
            // This is the case where the command section contains
            // mixed indentation. We silently return and allow
            // the mixed indentation lint to report this.
            return;
        };
        decls.extend(cmd_decls);
        let line_map = map_shellcheck_lines(section);

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
                    let span = calculate_span(&diagnostic, &line_map);
                    state.exceptable_add(
                        shellcheck_lint(&diagnostic, span),
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
