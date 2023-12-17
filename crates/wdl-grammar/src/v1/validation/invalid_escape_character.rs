//! Invalid escape character(s) within a string.

use std::collections::VecDeque;

use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_core::concern::code;
use wdl_core::concern::validation;
use wdl_core::concern::validation::Rule;
use wdl_core::concern::Code;
use wdl_core::fs::Location;
use wdl_core::Version;

use crate::v1;

/// Detects an invalid escape character within a string.
#[derive(Debug)]
pub struct InvalidEscapeCharacter;

impl<'a> InvalidEscapeCharacter {
    /// Generates a validation error for an invalid escape character.
    fn invalid_escape_character(&self, character: &str, location: Location) -> validation::Failure
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        validation::failure::Builder::default()
            .code(self.code())
            .subject(format!("invalid escape character: '{}'", character))
            .body("An invalid character was detected.")
            .push_location(location)
            .fix(
                "Remove the invalid character. If the character contains escaped characters \
                 (e.g., `\\n`), you may need to double escape the backslashes (e.g., `\\\\n`).",
            )
            .try_build()
            .unwrap()
    }
}

impl Rule<&Pair<'_, v1::Rule>> for InvalidEscapeCharacter {
    fn code(&self) -> Code {
        // SAFETY: this manually crafted to unwrap successfully every time.
        Code::try_new(code::Kind::Error, Version::V1, 1).unwrap()
    }

    fn validate(&self, tree: &Pair<'_, v1::Rule>) -> validation::Result {
        let mut failures = VecDeque::new();

        for node in tree.clone().into_inner().flatten() {
            if node.as_rule() == v1::Rule::char_escaped_invalid {
                let location =
                    Location::try_from(node.as_span()).map_err(validation::Error::Location)?;
                failures.push_back(self.invalid_escape_character(node.as_str(), location));
            }
        }

        match failures.pop_front() {
            Some(front) => {
                let mut results = NonEmpty::new(front);
                results.extend(failures);
                Ok(Some(results))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use pest::Parser as _;
    use wdl_core::concern::validation::Rule as _;

    use super::*;
    use crate::v1::parse::Parser;
    use crate::v1::Rule;

    #[test]
    fn it_catches_an_invalid_escape_character() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::string, "\"\\.\"")?.next().unwrap();
        let error = InvalidEscapeCharacter.validate(&tree).unwrap().unwrap();

        assert_eq!(
            error.first().to_string(),
            String::from("[v1::E001] invalid escape character: '\\.' (1:2-1:4)")
        );

        Ok(())
    }
}
