//! Formatting of Markdown embedded in WDL doc comments.
//!
//! This module provides functionality to format the Markdown content within
//! doc comment blocks (`##`) using the `md-formatter` crate. It handles
//! extracting the raw Markdown from doc comment content, formatting it with
//! paragraph wrapping relative to the comment's indentation level, and
//! rebuilding the formatted content string.
//!
//! ## Content format
//!
//! `Comment::Documentation` stores each line as the text after stripping the
//! `##` prefix. For well-formed comments like `## text`, this gives `" text"`
//! (leading space). For edge cases like `##text`, this gives `"text"` (no
//! leading space). Empty doc comment lines (`##`) become `""`.
//!
//! The display logic in `post.rs` writes `## ` + content for each line, so
//! the leading space in the content becomes the space between `##` and the
//! text. This module preserves that contract: after formatting, every
//! non-empty content line will start with a single space.

use md_formatter::Formatter as MdFormatter;
use md_formatter::WrapMode;
use md_formatter::parse_markdown;

use crate::NEWLINE;

/// The width of the `"## "` prefix (2 chars for `##` + 1 char for the space)
/// that appears before content in the rendered output. Used when computing
/// how much horizontal space is available for Markdown content.
pub(crate) const DOC_COMMENT_PREFIX_WIDTH: usize = 3;

/// Formats the Markdown content of a doc comment block.
///
/// The `contents` parameter is the raw content string as stored in
/// `Comment::Documentation`—each line is the text after the `##` prefix,
/// typically starting with a space for non-empty lines (e.g., `" hello"`).
/// Lines are separated by newlines and the string ends with a trailing
/// newline.
///
/// `content_width` is the maximum width available for the Markdown text
/// (i.e., the max line length minus indentation and the `## ` prefix).
///
/// Returns the formatted content string in the same format as the input
/// (each non-empty line prefixed with a single space, separated by newlines,
/// with a trailing newline), or `None` if the content should be left
/// unchanged (e.g., the content is empty or the available width is too
/// narrow for meaningful wrapping).
pub(crate) fn format_doc_comment(contents: &str, content_width: usize) -> Option<String> {
    // Don't attempt formatting if the available width is unreasonably narrow.
    if content_width < 10 {
        return None;
    }

    // Extract pure Markdown from the content lines.
    //
    // Most lines start with a single space (from `## text` -> `" text"`),
    // which we strip. Lines without a leading space (from `##text` ->
    // `"text"`) are passed through unchanged — the formatter will normalize
    // them, and the rebuild step below will add the canonical space back.
    // Empty lines (from bare `##`) remain empty.
    let markdown: String = contents
        .lines()
        .map(|line| {
            if let Some(stripped) = line.strip_prefix(' ') {
                stripped
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if markdown.trim().is_empty() {
        return None;
    }

    // Parse and format the Markdown.
    let events = parse_markdown(&markdown);
    let mut formatter = MdFormatter::with_wrap_mode(content_width, WrapMode::Always);
    let formatted = formatter.format(events);

    // Rebuild the content string with the leading space convention.
    //
    // Every non-empty formatted line gets a leading space so that the display
    // logic produces `## text` (with the conventional space after `##`).
    // Empty lines become bare newlines so they render as `##` (blank doc
    // comment lines). The result always ends with a trailing newline to
    // match the invariant established by `push_preceding_trivia`.
    let mut result = String::new();
    for line in formatted.lines() {
        if line.is_empty() {
            result.push_str(NEWLINE);
        } else {
            result.push(' ');
            result.push_str(line);
            result.push_str(NEWLINE);
        }
    }

    // Ensure the result has a trailing newline (the invariant from
    // push_preceding_trivia, which always appends NEWLINE after each doc
    // comment line). `str::lines()` drops a trailing empty line, so if
    // `formatted` ended with a newline we might have lost it.
    if !result.ends_with(NEWLINE) {
        result.push_str(NEWLINE);
    }

    // If the formatter produced the same content, return None to avoid
    // unnecessary allocations.
    if result == contents {
        return None;
    }

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to assert every non-empty line in the result starts with a space
    /// and fits within the given content width.
    fn assert_well_formed(result: &str, content_width: usize) {
        assert!(
            result.ends_with(NEWLINE),
            "result must end with a trailing newline, got: {result:?}"
        );
        for line in result.lines() {
            if !line.is_empty() {
                assert!(
                    line.starts_with(' '),
                    "non-empty line must start with a space: {line:?}"
                );
                let text = &line[1..];
                assert!(
                    text.len() <= content_width,
                    "line too long ({} chars, max {content_width}): {text:?}",
                    text.len(),
                );
            }
        }
    }

    #[test]
    fn short_text_unchanged() {
        let contents = " Hello world\n";
        assert!(format_doc_comment(contents, 80).is_none());
    }

    #[test]
    fn long_paragraph_wraps() {
        let contents =
            " This is a very very very very very long paragraph that should wrap correctly\n";
        let result = format_doc_comment(contents, 40).expect("should wrap");
        assert_well_formed(&result, 40);
    }

    #[test]
    fn blank_lines_preserved() {
        let contents = " paragraph one\n\n paragraph two\n";
        let result = format_doc_comment(contents, 80);
        let output = result.unwrap_or_else(|| contents.to_string());
        // A blank line between paragraphs must survive as an empty line.
        assert!(
            output.contains("\n\n"),
            "blank line between paragraphs should be preserved: {output:?}"
        );
    }

    #[test]
    fn empty_content() {
        let contents = "\n";
        assert!(format_doc_comment(contents, 80).is_none());
    }

    #[test]
    fn narrow_width_returns_none() {
        let contents = " Hello world\n";
        assert!(format_doc_comment(contents, 5).is_none());
    }

    #[test]
    fn no_space_after_prefix() {
        // Simulates `##- bullet` where strip_prefix("##") gives "- bullet".
        // The formatter should normalize this so the output has a leading
        // space (i.e., `## - bullet`).
        let contents = "- bullet point\n";
        let result = format_doc_comment(contents, 80).expect("should normalize");
        assert_well_formed(&result, 80);
        assert!(
            result.contains(" - bullet point"),
            "should produce canonical ' - bullet point', got: {result:?}"
        );
    }

    #[test]
    fn no_space_extra_indent() {
        // Simulates `##    indented` where the content is "    indented".
        // strip_prefix(' ') gives "   indented" (3 spaces). After
        // formatting and rebuild, the leading space is re-added.
        let contents = "    indented text\n";
        let result = format_doc_comment(contents, 80);
        let output = result.unwrap_or_else(|| contents.to_string());
        assert!(output.ends_with(NEWLINE));
    }

    #[test]
    fn bullet_list_wrapping() {
        // A long bullet item should wrap with proper continuation indentation
        // rather than being reflowed into the previous line.
        let contents = " - this is a very very very very very very long bullet item that should \
                        wrap correctly\n - second item\n";
        let result = format_doc_comment(contents, 50).expect("should wrap");
        assert_well_formed(&result, 50);
        // The result must still contain two separate bullet items
        let bullet_count = result.lines().filter(|l| l.contains(" - ")).count();
        assert!(
            bullet_count >= 2,
            "both bullet items must be preserved, found {bullet_count} bullets in: {result:?}"
        );
    }

    #[test]
    fn bullet_continuation_not_merged() {
        // Verify that a bullet item with continuation lines doesn't get
        // merged with the next bullet item.
        let contents = " - first item\n   continuation of first\n - second item\n";
        let result = format_doc_comment(contents, 80);
        let output = result.unwrap_or_else(|| contents.to_string());
        // Both bullet items should remain as separate items.
        let lines: Vec<&str> = output.lines().collect();
        let bullet_lines: Vec<&&str> = lines
            .iter()
            .filter(|l| l.trim_start().starts_with("- "))
            .collect();
        assert!(
            bullet_lines.len() >= 2,
            "both bullet items must remain separate, found {} in: {output:?}",
            bullet_lines.len()
        );
    }

    #[test]
    fn code_block_preserved() {
        // Code blocks must not be reformatted.
        let long_code = "x".repeat(120);
        let contents = format!(" ```\n {long_code}\n ```\n");
        let result = format_doc_comment(&contents, 80);
        let output = result.unwrap_or_else(|| contents.clone());
        assert!(
            output.contains(&long_code),
            "code block content must be preserved verbatim"
        );
    }

    #[test]
    fn idempotent() {
        let contents =
            " This is a very very very very very long paragraph that should wrap correctly\n";
        let first = format_doc_comment(contents, 40).expect("should wrap");
        let second = format_doc_comment(&first, 40);
        assert!(
            second.is_none(),
            "formatting should be idempotent, but got: {second:?}"
        );
    }
}
