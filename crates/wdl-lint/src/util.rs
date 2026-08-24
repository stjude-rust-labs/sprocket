//! A module for utility functions for the lint rules.

use std::process::Command;
use std::process::Stdio;

/// Determines whether or not a string containing embedded quotes is balanced.
pub fn is_quote_balanced(s: &str, quote_char: char) -> bool {
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

/// Serializes a list of items using the Oxford comma.
pub fn serialize_oxford_comma<T: std::fmt::Display>(items: &[T]) -> Option<String> {
    let len = items.len();

    match len {
        0 => None,
        // SAFETY: we just checked to ensure that exactly one element exists in
        // the `items` Vec, so this should always unwrap.
        1 => Some(items.iter().next().unwrap().to_string()),
        2 => {
            let mut items = items.iter();

            Some(format!(
                "{a} and {b}",
                // SAFETY: we just checked to ensure that exactly two elements
                // exist in the `items` Vec, so the first and second elements
                // will always be present.
                a = items.next().unwrap(),
                b = items.next().unwrap()
            ))
        }
        _ => {
            let mut result = String::new();

            for item in items.iter().take(len - 1) {
                if !result.is_empty() {
                    result.push_str(", ")
                }

                result.push_str(&item.to_string());
            }

            result.push_str(", and ");
            result.push_str(&items[len - 1].to_string());
            Some(result)
        }
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

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
        assert!(is_quote_balanced(s, '"'));
        let s = "\"this string has an escaped \\\" quote.\"";
        assert!(is_quote_balanced(s, '"'));
        let s = "\"this string is missing an end quote";
        assert_eq!(is_quote_balanced(s, '"'), false);
        let s = "this string is missing an open quote\"";
        assert_eq!(is_quote_balanced(s, '"'), false);
        let s = "\"this string has an irrelevant escape \\ \"";
        assert!(is_quote_balanced(s, '"'));
        let s = "'this string has single quotes'";
        assert!(is_quote_balanced(s, '\''));
        let s = "this string has unclosed single quotes'";
        assert_eq!(is_quote_balanced(s, '\''), false);
    }

    #[test]
    fn test_itemize_oxford_comma() {
        assert_eq!(serialize_oxford_comma(&Vec::<String>::default()), None);
        assert_eq!(
            serialize_oxford_comma(&["hello"]),
            Some(String::from("hello"))
        );
        assert_eq!(
            serialize_oxford_comma(&["hello", "world"]),
            Some(String::from("hello and world"))
        );
        assert_eq!(
            serialize_oxford_comma(&["hello", "there", "world"]),
            Some(String::from("hello, there, and world"))
        );
    }
}
