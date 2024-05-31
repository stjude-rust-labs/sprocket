//! At most one empty line in a row.
use std::collections::VecDeque;
use std::num::NonZeroUsize;

use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_core::concern::code;
use wdl_core::concern::lint;
use wdl_core::concern::lint::Rule;
use wdl_core::concern::lint::TagSet;
use wdl_core::concern::Code;
use wdl_core::file::location::Position;
use wdl_core::file::Location;
use wdl_core::str::LinesWithOffsetsExt as _;
use wdl_core::Version;

use crate::v1;

/// At most one empty line in a row.
#[derive(Debug)]
pub struct OneEmptyLine;

impl<'a> OneEmptyLine {
    /// Creates a warning if there is more than one empty line in a row.
    fn more_than_one_empty_line(&self, location: Location) -> lint::Warning
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Low)
            .tags(self.tags())
            .push_location(location)
            .subject("more than one empty line")
            .body(self.body())
            .fix("Remove the superfluous empty lines.")
            .try_build()
            .unwrap()
    }
}

impl<'a> Rule<&'a Pair<'a, v1::Rule>> for OneEmptyLine {
    fn code(&self) -> Code {
        // SAFETY: this manually crafted to unwrap successfully every time.
        Code::try_new(code::Kind::Warning, Version::V1, 11).unwrap()
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[lint::Tag::Spacing, lint::Tag::Style])
    }

    fn body(&self) -> &'static str {
        "There should be at most one empty line in a row. Superfluous empty lines make the code \
         harder to read."
    }

    fn check(&self, tree: &'a Pair<'_, v1::Rule>) -> lint::Result {
        let mut warnings: VecDeque<_> = VecDeque::new();

        let mut n_empty_lines = 0;

        // This will never get used as we overwrite on first instance.
        let mut start: Position = Position::new(
            NonZeroUsize::try_from(1).unwrap(),
            NonZeroUsize::try_from(1).unwrap(),
            1,
        );

        for (line_no, start_byte_no, _end_byte_no, line) in tree.as_str().lines_with_offsets() {
            if line.trim().is_empty() {
                if n_empty_lines == 1 {
                    start = Position::new(
                        NonZeroUsize::try_from(line_no.get()).unwrap(),
                        NonZeroUsize::try_from(1).unwrap(),
                        start_byte_no,
                    );
                }

                n_empty_lines += 1;
            } else if n_empty_lines >= 2 {
                warnings.push_back(self.more_than_one_empty_line(Location::Span {
                    start: start.clone(),
                    end: Position::new(
                        NonZeroUsize::try_from(line_no.get() - 1).unwrap(),
                        NonZeroUsize::try_from(1).unwrap(),
                        start_byte_no,
                    ),
                }));

                n_empty_lines = 0;
            } else {
                n_empty_lines = 0;
            }
        }

        // This will not catch newlines at the EOF.
        // Those are intentionally omitted here.
        // The newline_eof rule will catch those.

        match warnings.pop_front() {
            Some(front) => {
                let mut results = NonEmpty::new(front);
                results.extend(warnings);
                Ok(Some(results))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use pest::Parser as _;
    use wdl_core::concern::lint::Rule as _;

    use super::*;
    use crate::v1::parse::Parser;
    use crate::v1::Rule;

    #[test]
    fn it_catches_too_many_empty_lines() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.0


workflow a_workflow {}"#,
        )?
        .next()
        .unwrap();

        let warnings = OneEmptyLine.check(&tree)?.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W011::[Spacing, Style]::Low] more than one empty line (3:1-3:1)"
        );

        Ok(())
    }

    #[test]
    fn it_catches_multiple_too_many_empty_lines() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.0



workflow a_workflow {


}

"#,
        )?
        .next()
        .unwrap();

        let warnings = OneEmptyLine.check(&tree)?.unwrap();
        assert_eq!(warnings.len(), 2);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W011::[Spacing, Style]::Low] more than one empty line (3:1-4:1)"
        );
        assert_eq!(
            warnings.last().to_string(),
            "[v1::W011::[Spacing, Style]::Low] more than one empty line (7:1-7:1)"
        );

        Ok(())
    }

    #[test]
    fn it_ignore_too_many_empty_lines_at_eof() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.0

workflow a_workflow {

}


"#,
        )?
        .next()
        .unwrap();

        assert!(OneEmptyLine.check(&tree)?.is_none());

        Ok(())
    }

    #[test]
    fn it_ignores_a_single_empty_line() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.0

workflow a_workflow {

}

"#,
        )?
        .next()
        .unwrap();

        assert!(OneEmptyLine.check(&tree)?.is_none());

        Ok(())
    }
}
