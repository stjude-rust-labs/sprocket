//! Contents of a command.

use std::convert::Infallible;

use wdl_core::str::whitespace;
use wdl_core::str::whitespace::Whitespace;

/// The line ending.
#[cfg(windows)]
const LINE_ENDING: &str = "\r\n";
/// The line ending.
#[cfg(not(windows))]
const LINE_ENDING: &str = "\n";

/// Contents of a command.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Contents(String);

impl std::ops::Deref for Contents {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::str::FromStr for Contents {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Ok(Self(String::new()));
        }

        let mut lines = s.lines();
        let mut results = Vec::new();

        // SAFETY: we just ensured that exactly one line exists, so this
        // will unwrap.
        let first_line = lines.next().unwrap();

        // Note:: the first line is treated separately from the remaining lines.
        // This is because the first line is either (a) empty, which is harmless
        // and pushes and empty line into the results, or (b) has some content,
        // which means it is on the same line as the command. In the case of
        // (b), we don't want the spacing for the first line to influence the
        // stripping of whitespace on the remaining lines. For example,
        //
        // ```
        // command <<< echo 'hello'
        //     echo 'world'
        //     exit 0
        // >>>
        // ```
        //
        // Althought the above is considered bad form, the single space on the
        // first line should not dictate the stripping of whitespace for the
        // remaining lines (which are clearly indented with four spaces).

        if !first_line.is_empty() {
            results.push(first_line.to_string());
        }

        results.extend(strip_leading_whitespace(lines.collect())?);

        // If the last line is pure whitespace, ignore it.
        if let Some(line) = results.pop() {
            if !line.trim().is_empty() {
                results.push(line)
            }
        }

        Ok(Self(results.join(LINE_ENDING)))
    }
}

/// Strips common leading whitespace from a [`Vec<&str>`].
fn strip_leading_whitespace(lines: Vec<&str>) -> Result<Vec<String>, Infallible> {
    match Whitespace::get_indent(&lines) {
        Ok((_, indent)) => Ok(lines
            .iter()
            .map(|line| {
                if line.len() >= indent {
                    line.chars().skip(indent).collect()
                } else {
                    line.to_string()
                }
            })
            .collect()),
        // Note: according to the specification, the whitespace should go
        // unmodified if there is a mix of tabs and spaces.
        Err(whitespace::Error::MixedIndentationCharacters) => {
            Ok(lines.iter().map(|line| line.to_string()).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_parses_contents_with_spaces_correctly(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let contents = "echo 'hello,'
    echo 'there'
    echo 'world'"
            .parse::<Contents>()?;

        assert_eq!(
            contents.as_str(),
            "echo 'hello,'\necho 'there'\necho 'world'"
        );

        let contents = "
    echo 'hello,'
    echo 'there'
    echo 'world'"
            .parse::<Contents>()?;

        assert_eq!(
            contents.as_str(),
            "echo 'hello,'\necho 'there'\necho 'world'"
        );

        let contents = "
        echo 'hello,'
    echo 'there'
    echo 'world'"
            .parse::<Contents>()?;

        assert_eq!(
            contents.as_str(),
            "    echo 'hello,'\necho 'there'\necho 'world'"
        );

        Ok(())
    }

    #[test]
    fn it_parses_contents_with_tabs_correctly(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let contents = "
\t\techo 'hello,'
\t\techo 'there'
\t\techo 'world'"
            .parse::<Contents>()?;
        assert_eq!(
            contents.as_str(),
            "echo 'hello,'\necho 'there'\necho 'world'"
        );

        Ok(())
    }

    #[test]
    fn it_keeps_preceeding_whitespace_on_the_same_line_as_the_command(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let contents = "    \nhello".parse::<Contents>()?;
        assert_eq!(contents.as_str(), "    \nhello");

        Ok(())
    }
}
