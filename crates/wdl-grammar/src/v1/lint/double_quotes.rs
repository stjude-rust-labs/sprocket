//! All quotes should be double quotes.

use std::collections::VecDeque;

use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_core::concern::code;
use wdl_core::concern::lint;
use wdl_core::concern::lint::Rule;
use wdl_core::concern::lint::TagSet;
use wdl_core::concern::Code;
use wdl_core::file::Location;
use wdl_core::Version;

use crate::v1;

/// Detects strings that are not defined with double quotes.
#[derive(Debug)]
pub struct DoubleQuotes;

impl<'a> DoubleQuotes {
    /// Creates a warning for strings defined using single-quotes
    fn single_quote_string(&self, location: Location) -> lint::Warning
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Low)
            .tags(self.tags())
            .push_location(location)
            .subject("string defined with single quotes")
            .body("All strings should be defined using double quotes.")
            .fix("Change the single quotes to double quotes.")
            .try_build()
            .unwrap()
    }
}

impl<'a> Rule<&'a Pair<'a, v1::Rule>> for DoubleQuotes {
    fn code(&self) -> Code {
        // SAFETY: this manually crafted to unwrap successfully every time.
        Code::try_new(code::Kind::Warning, Version::V1, 12).unwrap()
    }

    fn tags(&self) -> lint::TagSet {
        TagSet::new(&[lint::Tag::Clarity, lint::Tag::Style])
    }

    fn check(&self, tree: &'a Pair<'_, v1::Rule>) -> lint::Result {
        let mut warnings = VecDeque::new();

        for node in tree.clone().into_inner().flatten() {
            if node.as_rule() == v1::Rule::string && node.as_str().starts_with('\'') {
                let location = Location::try_from(node.as_span()).map_err(lint::Error::Location)?;
                warnings.push_back(self.single_quote_string(location));
            }
        }

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
    fn it_ignores_a_correctly_formatted_import() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::import, r#"import "wdl-common/wdl/structs.wdl""#)?
            .next()
            .unwrap();

        assert!(DoubleQuotes.check(&tree)?.is_none());
        Ok(())
    }

    #[test]
    fn it_catches_a_single_quote_import() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::import, r#"import 'wdl-common/wdl/structs.wdl'"#)?
            .next()
            .unwrap();
        let warnings = DoubleQuotes.check(&tree)?.unwrap();

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W012::[Style, Clarity]::Low] string defined with single quotes (1:8-1:36)"
        );

        Ok(())
    }

    #[test]
    fn it_catches_a_single_quote_bound_declaration() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::bound_declaration, r#"String bad_string = 'bad'"#)?
            .next()
            .unwrap();
        let warnings = DoubleQuotes.check(&tree)?.unwrap();

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W012::[Style, Clarity]::Low] string defined with single quotes (1:21-1:26)"
        );

        Ok(())
    }

    #[test]
    fn it_catches_a_single_quote_task_metadata() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::task,
            r#"task sort {
    meta {
        description: 'Sorts'
    }

    command <<< >>>
}"#,
        )?
        .next()
        .unwrap();
        let warnings = DoubleQuotes.check(&tree)?.unwrap();

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W012::[Style, Clarity]::Low] string defined with single quotes (3:22-3:29)"
        );

        Ok(())
    }

    #[test]
    fn it_catches_placeholder_string() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::bound_declaration,
            r#"String nested = "Hello ~{if alien then 'world' else planet}!""#,
        )?
        .next()
        .unwrap();
        let warnings = DoubleQuotes.check(&tree)?.unwrap();

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W012::[Style, Clarity]::Low] string defined with single quotes (1:40-1:47)"
        );

        Ok(())
    }

    #[test]
    fn it_catches_placeholder_string2() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.1
            task foo {
                command <<<
                    echo 'this Bash string should be ignored'
                    echo "~{if foo then 'this should be flagged' else 'this one too'}"
                >>>
            }
            "#,
        )?
        .next()
        .unwrap();
        let warnings = DoubleQuotes.check(&tree)?.unwrap();

        assert_eq!(warnings.len(), 2);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W012::[Style, Clarity]::Low] string defined with single quotes (5:41-5:65)"
        );
        assert_eq!(
            warnings.last().to_string(),
            "[v1::W012::[Style, Clarity]::Low] string defined with single quotes (5:71-5:85)"
        );

        Ok(())
    }
}
