//! A lint rule for enforcing certain options in the bash `set` builtin for
//! every `command` section.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt::Display;
use std::fmt::Formatter;

use serde::Deserialize;
use serde::Serialize;
use strum::VariantArray;
use toml_spanner::Toml;
use wdl_analysis::Diagnostics;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::CommandPart;
use wdl_ast::v1::CommandSection;

use crate::Config;
use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the bash set syntax rule.
const ID: &str = "BashSetSyntax";

/// Name of the `set` command.
const SET_COMMAND_NAME: &str = "set";

/// Long options that are only available in interactive mode.
const INTERACTIVE_ONLY_LONG: &[&str] = &[
    "emacs",
    "vi",
    "ignoreeof",
    "history",
    "histexpand",
    "monitor",
    "notify",
];

/// Short options that are only available in interactive mode.
const INTERACTIVE_ONLY_SHORT: &[char] = &['H', 'm', 'b'];

/// Creates a missing `set` command diagnostic.
fn missing_set(span: Span) -> Diagnostic {
    Diagnostic::warning("missing `set` command")
        .with_rule(ID)
        .with_highlight(span)
        .with_help("`set` commands should be on the first line of `command` sections")
}

/// Creates an interactive `set` option diagnostic.
fn interactive_only(span: Span, option: &str) -> Diagnostic {
    Diagnostic::warning("unnecessary `set` option")
        .with_rule(ID)
        .with_highlight(span)
        .with_help(format!(
            "option `{option}` is only available in interactive mode"
        ))
        .with_fix("remove the option")
}

/// Creates an unknown `set` option diagnostic.
fn unknown_option(span: Span, option: &str) -> Diagnostic {
    Diagnostic::error("unknown `set` option")
        .with_rule(ID)
        .with_highlight(span)
        .with_help(format!("option `{option}` is non-standard"))
        .with_fix("remove the option")
}

/// Creates a bad `set` syntax diagnostic.
fn bad_set_syntax(span: Span, expected_options: &[BashSetOption], fix: &str) -> Diagnostic {
    let expected_options = expected_options
        .iter()
        .map(|op| op.to_string())
        .collect::<Vec<_>>();
    Diagnostic::warning(format!("bad `{SET_COMMAND_NAME}` command"))
        .with_rule(ID)
        .with_highlight(span)
        .with_help(format!(
            "the config expects the following options to be present: {}",
            expected_options.join(", ")
        ))
        .with_fix(format!(
            "update the `{SET_COMMAND_NAME}` command to: `{fix}`"
        ))
}

/// Detects missing/invalid bash `set` commands.
#[derive(Default, Debug, Clone)]
pub struct BashSetSyntax {
    /// The minimum options expected to be enabled.
    expected_options: Vec<BashSetOption>,
}

impl BashSetSyntax {
    /// Create a new `BashSetSyntax` rule.
    pub fn new(config: &Config) -> Self {
        let mut expected_options: Vec<BashSetOption> = config.bash_set_options.clone();
        expected_options.sort();

        Self { expected_options }
    }

    /// Generates a bare-minimum `set` command based on the required options.
    fn ideal_command(&self) -> String {
        let mut cmd = String::from("set");
        let mut has_shorts = false;

        for op in &self.expected_options {
            if let Some(short) = op.short() {
                if !has_shorts {
                    cmd.push_str(" -");
                    has_shorts = true;
                }
                cmd.push(short);
                continue;
            }

            let long = op.long().expect("should have a long variant");

            // The first long option usually tails the short options.
            //
            // Like: set -euo pipefail
            // Rather than: set -eu -o pipefail
            if has_shorts {
                cmd.push_str("o ");
                cmd.push_str(long);
                has_shorts = false;
                continue;
            }

            cmd.push_str(" -o ");
            cmd.push_str(long);
        }

        cmd
    }

    /// Parses and validates the `set` command against the list of expected
    /// options.
    ///
    /// Returns `(valid, length of command)`
    fn check_set_syntax(
        &self,
        diagnostics: &mut Diagnostics,
        section: &CommandSection,
        line: &str,
        line_start: usize,
    ) -> (bool, usize) {
        /// To handle the cases of metacharacters being part of a chunk.
        /// For example, `set -eu;echo "Hello world"`.
        fn split_at_meta_char(chunk: &str) -> (&str, bool) {
            match chunk.find([';', '&', '|', '>', '<']) {
                Some(index) => (&chunk[..index], true),
                None => (chunk, false),
            }
        }

        let Some(opts) = line.strip_prefix(SET_COMMAND_NAME) else {
            return (false, 0);
        };

        // Since we know the command is `set` at the very least
        let mut last_chunk_end = SET_COMMAND_NAME.len();

        let opts_trimmed = opts.trim_start();
        if opts_trimmed.is_empty() {
            return (false, last_chunk_end);
        }

        let mut remaining_expected: HashSet<_> = self.expected_options.iter().copied().collect();
        let mut chunks = opts_trimmed.split_whitespace();

        while let Some(chunk) = chunks.next() {
            let (chunk, found_meta_char) = split_at_meta_char(chunk);
            if chunk.is_empty() {
                break;
            }

            // https://www.gnu.org/software/bash/manual/html_node/The-Set-Builtin.html:
            //
            // `--` and `-` mark the end of the options
            if chunk == "--" || chunk == "-" {
                last_chunk_end = chunk.as_ptr() as usize - line.as_ptr() as usize + chunk.len();
                break;
            }

            // And options only ever start with `-` or `+`
            if !chunk.starts_with(['-', '+']) {
                break;
            }

            let chunk_offset = chunk.as_ptr() as usize - line.as_ptr() as usize;
            last_chunk_end = chunk_offset + chunk.len();

            let is_enable = chunk.starts_with('-');
            let mode = if is_enable { '-' } else { '+' };
            let mut byte_offset = 1;

            for opt in chunk[byte_offset..].chars() {
                let opt_len = opt.len_utf8();

                let (matched_opt, opt_name_span, should_break, is_interactive_only) = if opt == 'o'
                {
                    if byte_offset + opt_len < chunk.len() {
                        // Some invalid syntax, 'o' should be at the end of the chunk
                        return (false, last_chunk_end);
                    }

                    let Some(long_opt) = chunks.next() else {
                        // Missing argument for -/+o, not a valid command anyway
                        return (false, last_chunk_end);
                    };

                    let (long_opt, trailing_meta_char) = split_at_meta_char(long_opt);
                    if long_opt.is_empty() {
                        return (false, last_chunk_end);
                    }

                    last_chunk_end =
                        long_opt.as_ptr() as usize - line.as_ptr() as usize + long_opt.len();

                    let long_opt_offset = long_opt.as_ptr() as usize - line.as_ptr() as usize;
                    let span_start = line_start + chunk_offset;
                    let span_end = line_start + long_opt_offset + long_opt.len();
                    let long_opt_span = Span::new(span_start, span_end - span_start);

                    let mut is_interactive_only = INTERACTIVE_ONLY_LONG.contains(&long_opt);

                    (
                        BashSetOption::from_long(long_opt),
                        long_opt_span,
                        trailing_meta_char,
                        is_interactive_only,
                    )
                } else {
                    let opt_span = Span::new(line_start + chunk_offset + byte_offset, opt_len);

                    let mut is_interactive_only = INTERACTIVE_ONLY_SHORT.contains(&opt);

                    (
                        BashSetOption::from_short(opt),
                        opt_span,
                        false,
                        is_interactive_only,
                    )
                };

                byte_offset += opt_len;

                let Some(matched_opt) = matched_opt else {
                    if is_interactive_only {
                        diagnostics.exceptable_add(
                            interactive_only(opt_name_span, &format!("{mode}{opt}")),
                            SyntaxElement::from(section.inner().clone()),
                            &self.exceptable_nodes(),
                        );
                    } else {
                        diagnostics.exceptable_add(
                            unknown_option(opt_name_span, &format!("{mode}{opt}")),
                            SyntaxElement::from(section.inner().clone()),
                            &self.exceptable_nodes(),
                        );
                    }

                    continue;
                };

                if is_enable {
                    remaining_expected.remove(&matched_opt);
                } else {
                    // Explicitly disabling a required option
                    return (false, last_chunk_end);
                }

                if opt == 'o' || should_break {
                    break;
                }
            }

            if found_meta_char {
                break;
            }
        }

        (remaining_expected.is_empty(), last_chunk_end)
    }
}

impl Rule for BashSetSyntax {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that all `command` sections start with a valid `set` command."
    }

    fn explanation(&self) -> &'static str {
        "Bash has many silent failure cases, which can produce invalid results and be difficult to \
        debug. The [set command](https://www.gnu.org/software/bash/manual/html_node/The-Set-Builtin.html) \
        should be used in all `command` sections to enforce stricter behavior."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

task say_hello {
    command <<<
        echo "Hello, World!"
    >>>
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: Some("Assuming the default configuration"),
                snippet: r#"version 1.2

task say_hello {
    command <<<
        set -euo pipefail
        echo "Hello, World!"
    >>>
}
"#,
            }),
        }]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Correctness])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::CommandSectionNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &[]
    }
}

/// Supported options for `set`.
///
/// See <https://www.gnu.org/software/bash/manual/html_node/The-Set-Builtin.html> for a description
/// of each option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Toml, VariantArray)]
#[serde(rename_all = "lowercase")]
#[toml(FromToml, ToToml, rename_all = "lowercase")]
#[allow(missing_docs)]
pub enum BashSetOption {
    AllExport,
    BraceExpand,
    ErrExit,
    ErrTrace,
    FuncTrace,
    HashAll,
    Keyword,
    NoClobber,
    NoExec,
    NoGlob,
    NoLog,
    NoUnset,
    OneCmd,
    Physical,
    Pipefail,
    Posix,
    Privileged,
    Restricted,
    Verbose,
    XTrace,
}

impl BashSetOption {
    /// Attempt to get a [`BashSetOption`] by its short name.
    fn from_short(opt: char) -> Option<Self> {
        Self::VARIANTS
            .iter()
            .find(|&&variant| variant.short() == Some(opt))
            .copied()
    }

    /// Attempt to get a [`BashSetOption`] by its long name.
    fn from_long(opt: &str) -> Option<Self> {
        Self::VARIANTS
            .iter()
            .find(|&&variant| variant.long() == Some(opt))
            .copied()
    }

    /// The short option name, if available.
    fn short(self) -> Option<char> {
        match self {
            BashSetOption::AllExport => Some('a'),
            BashSetOption::BraceExpand => Some('B'),
            BashSetOption::ErrExit => Some('e'),
            BashSetOption::ErrTrace => Some('E'),
            BashSetOption::FuncTrace => Some('T'),
            BashSetOption::HashAll => Some('h'),
            BashSetOption::Keyword => Some('k'),
            BashSetOption::NoClobber => Some('C'),
            BashSetOption::NoExec => Some('n'),
            BashSetOption::NoGlob => Some('f'),
            BashSetOption::NoLog => None,
            BashSetOption::NoUnset => Some('u'),
            BashSetOption::OneCmd => Some('t'),
            BashSetOption::Physical => Some('P'),
            BashSetOption::Pipefail => None,
            BashSetOption::Posix => None,
            BashSetOption::Privileged => Some('p'),
            BashSetOption::Restricted => Some('r'),
            BashSetOption::Verbose => Some('v'),
            BashSetOption::XTrace => Some('x'),
        }
    }

    /// The long option name, if available. Used in `-/+o`.
    fn long(self) -> Option<&'static str> {
        match self {
            BashSetOption::AllExport => Some("allexport"),
            BashSetOption::BraceExpand => Some("braceexpand"),
            BashSetOption::ErrExit => Some("errexit"),
            BashSetOption::ErrTrace => Some("errtrace"),
            BashSetOption::FuncTrace => Some("functrace"),
            BashSetOption::HashAll => Some("hashall"),
            BashSetOption::Keyword => Some("keyword"),
            BashSetOption::NoClobber => Some("noclobber"),
            BashSetOption::NoExec => Some("noexec"),
            BashSetOption::NoGlob => Some("noglob"),
            BashSetOption::NoLog => Some("nolog"),
            BashSetOption::NoUnset => Some("nounset"),
            BashSetOption::OneCmd => Some("onecmd"),
            BashSetOption::Physical => Some("physical"),
            BashSetOption::Pipefail => Some("pipefail"),
            BashSetOption::Posix => Some("posix"),
            BashSetOption::Privileged => Some("privileged"),
            BashSetOption::Restricted => None,
            BashSetOption::Verbose => Some("verbose"),
            BashSetOption::XTrace => Some("xtrace"),
        }
    }
}

impl Display for BashSetOption {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            BashSetOption::AllExport => "allexport",
            BashSetOption::BraceExpand => "braceexpand",
            BashSetOption::ErrExit => "errexit",
            BashSetOption::ErrTrace => "errtrace",
            BashSetOption::FuncTrace => "functrace",
            BashSetOption::HashAll => "hashall",
            BashSetOption::Keyword => "keyword",
            BashSetOption::NoClobber => "noclobber",
            BashSetOption::NoExec => "noexec",
            BashSetOption::NoGlob => "noglob",
            BashSetOption::NoLog => "nolog",
            BashSetOption::NoUnset => "nounset",
            BashSetOption::OneCmd => "onecmd",
            BashSetOption::Physical => "physical",
            BashSetOption::Pipefail => "pipefail",
            BashSetOption::Posix => "posix",
            BashSetOption::Privileged => "privileged",
            BashSetOption::Restricted => "restricted",
            BashSetOption::Verbose => "verbose",
            BashSetOption::XTrace => "xtrace",
        };

        write!(f, "{name}")
    }
}

impl PartialOrd for BashSetOption {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BashSetOption {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.short(), other.short()) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => self.long().cmp(&other.long()),
        }
    }
}

impl Visitor for BashSetSyntax {
    fn reset(&mut self) {
        *self = Self {
            expected_options: std::mem::take(&mut self.expected_options),
        };
    }

    fn command_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &CommandSection,
    ) {
        if reason != VisitReason::Enter || self.expected_options.is_empty() {
            return;
        }

        let Some(CommandPart::Text(first_chunk)) = section.parts().next() else {
            diagnostics.exceptable_add(
                missing_set(section.span()),
                SyntaxElement::from(section.inner().clone()),
                &self.exceptable_nodes(),
            );
            return;
        };

        let chunk_text = first_chunk.text();
        let chunk_start = first_chunk.span().start();

        for line_text in chunk_text.lines() {
            let trimmed = line_text.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }

            if trimmed.starts_with(SET_COMMAND_NAME)
                && (trimmed[SET_COMMAND_NAME.len()..].is_empty()
                    || trimmed[SET_COMMAND_NAME.len()..].starts_with(char::is_whitespace))
            {
                let line_offset = trimmed.as_ptr() as usize - chunk_text.as_ptr() as usize;
                let line_start = chunk_start + line_offset;

                let (is_valid, parsed_length) =
                    self.check_set_syntax(diagnostics, section, trimmed, line_start);

                if !is_valid {
                    let set_span = Span::new(line_start, parsed_length);
                    diagnostics.exceptable_add(
                        bad_set_syntax(set_span, &self.expected_options, &self.ideal_command()),
                        SyntaxElement::from(section.inner().clone()),
                        &self.exceptable_nodes(),
                    );
                }

                return;
            }

            break;
        }

        diagnostics.exceptable_add(
            missing_set(section.span()),
            SyntaxElement::from(section.inner().clone()),
            &self.exceptable_nodes(),
        );
    }
}
