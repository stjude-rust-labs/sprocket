//! Whitespace within strings.

use std::collections::HashMap;

/// Utility methods related to whitespace within strings.
#[allow(missing_debug_implementations)]
pub struct Whitespace;

/// An error related to [`Whitespace`].
#[derive(Debug)]
pub enum Error {
    /// Mixed indention characters.
    MixedIndentationCharacters,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::MixedIndentationCharacters => write!(f, "mixed indentation characters"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

impl Whitespace {
    /// Gets the indent level and character from a reference to a [`Vec<&str>`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::str::Whitespace;
    ///
    /// let lines = "  Hello\n  World!".lines().into_iter().collect::<Vec<_>>();
    /// let (char, indent) = Whitespace::get_indent(&lines)?;
    ///
    /// assert_eq!(char, Some(' '));
    /// assert_eq!(indent, 2);
    ///
    /// let lines = "  Hello\n\tWorld!".lines().into_iter().collect::<Vec<_>>();
    /// let err = Whitespace::get_indent(&lines).unwrap_err();
    ///
    /// assert_eq!(
    ///     err.to_string(),
    ///     String::from("mixed indentation characters")
    /// );
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn get_indent(lines: &[&str]) -> Result<(Option<char>, usize)> {
        let whitespace_by_line = lines
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                line.chars().take_while(|c| c.is_whitespace()).fold(
                    HashMap::new(),
                    |mut counts, c| {
                        *counts.entry(c).or_insert(0usize) += 1usize;
                        counts
                    },
                )
            })
            .collect::<Vec<HashMap<char, usize>>>();

        let all_whitespace =
            whitespace_by_line
                .iter()
                .fold(HashMap::new(), |mut total_counts, line_counts| {
                    for (c, count) in line_counts {
                        *total_counts.entry(*c).or_insert(0usize) += count
                    }

                    total_counts
                });

        let whitespace_character = match all_whitespace.len() {
            0 => Ok(None),
            // SAFETY: we just ensured that exactly one entry exists in the
            // [`HashMap`]. As such, this will always unwrap.
            1 => Ok(Some(*all_whitespace.keys().next().unwrap())),
            _ => return Err(Error::MixedIndentationCharacters),
        }?;

        let indent = whitespace_by_line
            .into_iter()
            .map(|counts| counts.values().sum())
            .min()
            .unwrap_or_default();

        Ok((whitespace_character, indent))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works_on_spaces() {
        let lines = "
            echo 'hello,'
            echo 'there'
            echo 'world'"
            .lines()
            .collect::<Vec<_>>();

        let (c, indent) = Whitespace::get_indent(&lines).unwrap();
        assert_eq!(c, Some(' '));
        assert_eq!(indent, 12);
    }

    #[test]
    fn it_works_on_tabs() {
        let lines = "
\t\techo 'hello,'
\t\techo 'there'
\t\techo 'world'"
            .lines()
            .collect::<Vec<_>>();

        let (c, indent) = Whitespace::get_indent(&lines).unwrap();
        assert_eq!(c, Some('\t'));
        assert_eq!(indent, 2);
    }

    #[test]
    fn it_works_on_mixed_indent_levels() {
        let lines = "
        echo 'hello,'
            echo 'there'
            echo 'world'"
            .lines()
            .collect::<Vec<_>>();

        let (c, indent) = Whitespace::get_indent(&lines).unwrap();
        assert_eq!(c, Some(' '));
        assert_eq!(indent, 8);
    }

    #[test]
    fn it_works_when_there_is_no_identation() {
        let lines = "
echo 'hello,'
echo 'there'
echo 'world'"
            .lines()
            .collect::<Vec<_>>();

        let (c, indent) = Whitespace::get_indent(&lines).unwrap();
        assert_eq!(c, None);
        assert_eq!(indent, 0);
    }

    #[test]
    fn it_errors_on_mixed_spaces_and_tabs() {
        let lines = "
            \techo 'hello,'
            echo 'there'
            echo 'world'"
            .lines()
            .collect::<Vec<_>>();

        let err = Whitespace::get_indent(&lines).unwrap_err();

        assert!(matches!(err, Error::MixedIndentationCharacters));
    }
}
