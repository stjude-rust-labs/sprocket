//! A module for utility functions for the lint rules.

use std::process::Command;
use std::process::Stdio;

use strsim::levenshtein;
use wdl_analysis::rules as analysis_rules;

use crate::rules::RULE_MAP;

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

/// Finds the nearest rule ID to the given unknown rule ID,
/// or `None` if no rule ID is close enough.
pub fn find_nearest_rule(unknown_rule_id: &str) -> Option<&'static str> {
    let threshold = calculate_threshold(unknown_rule_id.len());

    RULE_MAP
        .keys()
        .copied()
        .chain(analysis_rules().iter().map(|rule| rule.id()))
        .map(|rule_id| (rule_id, levenshtein(unknown_rule_id, rule_id)))
        .filter(|(_, distance)| *distance <= threshold)
        .min_by_key(|(_, distance)| *distance)
        .map(|(rule_id, _)| rule_id)
}

/// Calculates a threshold for string similarity based on input length.
fn calculate_threshold(input_len: usize) -> usize {
    if input_len <= 3 {
        return 1;
    }
    if input_len <= 10 {
        return input_len / 3 + 1;
    }
    5
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
    fn test_find_nearest_rule() {
        // Test exact match
        let nearest = find_nearest_rule("SnakeCase");
        assert_eq!(nearest, Some("SnakeCase"));

        // Test close match
        let nearest = find_nearest_rule("SnackCase");
        assert_eq!(nearest, Some("SnakeCase"));

        // Test another exact match
        let nearest = find_nearest_rule("PascalCase");
        assert_eq!(nearest, Some("PascalCase"));

        // Test a typo
        let nearest = find_nearest_rule("PaskalCase");
        assert_eq!(nearest, Some("PascalCase"));

        // Test a more significant typo
        let nearest = find_nearest_rule("SnakeCas");
        assert_eq!(nearest, Some("SnakeCase"));

        // Test a completely different string
        let nearest = find_nearest_rule("CompletelyDifferentRule");
        assert_eq!(nearest, None);
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
