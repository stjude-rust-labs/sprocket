//! A module for utility functions for the lint rules.

use std::process::Command;
use std::process::Stdio;

use wdl_ast::AstToken;
use wdl_ast::Comment;
use wdl_ast::SyntaxKind;

/// Detect if a comment is in-line or not by looking for `\n` in the prior
/// whitespace.
pub fn is_inline_comment(token: &Comment) -> bool {
    if let Some(prior) = token.syntax().prev_sibling_or_token() {
        let whitespace = prior.kind() == SyntaxKind::Whitespace;
        if !whitespace {
            return true;
        }

        let contains_newline = prior
            .as_token()
            .expect("whitespace should be a token")
            .text()
            .contains('\n');
        let first = prior.prev_sibling_or_token().is_none();
        return !contains_newline && !first;
    }
    false
}

/// Determines whether or not a string containing embedded quotes is properly
/// quoted.
pub fn is_properly_quoted(s: &str, quote_char: char) -> bool {
    let mut closed = true;
    let mut escaped = false;
    s.chars().for_each(|c| {
        if c == '\\' {
            escaped = true;
        } else if !escaped && c == quote_char {
            closed = !closed;
        } else {
            escaped = false;
        }
    });
    closed
}

/// Iterates over the lines of a string and returns the line, starting offset,
/// and next possible starting offset.
pub fn lines_with_offset(s: &str) -> impl Iterator<Item = (&str, usize, usize)> {
    let mut offset = 0;
    std::iter::from_fn(move || {
        if offset >= s.len() {
            return None;
        }

        let start = offset;
        loop {
            match s[offset..].find(|c| ['\r', '\n'].contains(&c)) {
                Some(i) => {
                    let end = offset + i;
                    offset = end + 1;

                    if s.as_bytes().get(end) == Some(&b'\r') {
                        if s.as_bytes().get(end + 1) != Some(&b'\n') {
                            continue;
                        }

                        // There are two characters in the newline
                        offset += 1;
                    }

                    return Some((&s[start..end], start, offset));
                }
                None => {
                    offset = s.len();
                    return Some((&s[start..], start, offset));
                }
            }
        }
    })
}

/// Check whether or not a program exists.
///
/// On unix-like OSes, uses `which`.
/// On Windows, uses `where.exe`.
pub fn program_exists(exec: &str) -> bool {
    let finder = if cfg!(windows) { "where.exe" } else { "which" };
    Command::new(finder)
        .arg(exec)
        .stdout(Stdio::null())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|r| r.success())
}

/// Strips a single newline from the end of a string.
pub fn strip_newline(s: &str) -> Option<&str> {
    s.strip_suffix("\r\n").or_else(|| s.strip_suffix('\n'))
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn it_detects_inline() {
        let (tree, _) = wdl_ast::SyntaxTree::parse(
            r#"      # not an in-line comment

version 1.2

task foo {  # an in-line comment
    # not an in-line comment
}"#,
        );

        let mut comments = tree
            .root()
            .descendants_with_tokens()
            .filter(|t| t.kind() == SyntaxKind::Comment);

        let first = comments.next().expect("there should be a first comment");
        let first = Comment::cast(first.as_token().unwrap().clone()).unwrap();

        let is_inline = is_inline_comment(&first);

        assert!(!is_inline);

        let second = comments.next().expect("there should be a second comment");
        let second = Comment::cast(second.as_token().unwrap().clone()).unwrap();

        let is_inline = is_inline_comment(&second);

        assert!(is_inline);

        let third = comments.next().expect("there should be a third comment");
        let third = Comment::cast(third.as_token().unwrap().clone()).unwrap();

        let is_inline = is_inline_comment(&third);

        assert!(!is_inline);
    }

    #[test]
    fn test_strip_newline() {
        let s = "this has no newline";
        assert!(strip_newline(s).is_none());

        let s = "this has a single newline\n";
        assert_eq!(strip_newline(s), Some("this has a single newline"));

        let s = "this has a single Windows newline\r\n";
        assert_eq!(strip_newline(s), Some("this has a single Windows newline"));

        let s = "this has more than one newline\n\n";
        assert_eq!(strip_newline(s), Some("this has more than one newline\n"));

        let s = "this has more than one Windows newline\r\n\r\n";
        assert_eq!(
            strip_newline(s),
            Some("this has more than one Windows newline\r\n")
        );
    }

    #[test]
    fn test_lines_with_offset() {
        let s = "This string\nhas many\n\nnewlines, including Windows\r\n\r\nand even a \r that \
                 should not be a newline\n";
        let lines = lines_with_offset(s).collect::<Vec<_>>();
        assert_eq!(
            lines,
            &[
                ("This string", 0, 12),
                ("has many", 12, 21),
                ("", 21, 22),
                ("newlines, including Windows", 22, 51),
                ("", 51, 53),
                ("and even a \r that should not be a newline", 53, 95),
            ]
        );
    }

    #[test]
    fn test_program_exists() {
        if cfg!(windows) {
            assert!(program_exists("where.exe"));
        } else {
            assert!(program_exists("which"));
        }
    }

    #[test]
    fn test_is_properly_quoted() {
        let s = "\"this string is quoted properly.\"";
        assert!(is_properly_quoted(s, '"'));
        let s = "\"this string has an escaped \\\" quote.\"";
        assert!(is_properly_quoted(s, '"'));
        let s = "\"this string is missing an end quote";
        assert_eq!(is_properly_quoted(s, '"'), false);
        let s = "this string is missing an open quote\"";
        assert_eq!(is_properly_quoted(s, '"'), false);
        let s = "\"this string has an irrelevant escape \\ \"";
        assert!(is_properly_quoted(s, '"'));
        let s = "'this string has single quotes'";
        assert!(is_properly_quoted(s, '\''));
        let s = "this string has unclosed single quotes'";
        assert_eq!(is_properly_quoted(s, '\''), false);
    }
}
