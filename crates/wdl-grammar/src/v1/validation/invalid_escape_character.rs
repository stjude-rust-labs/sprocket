//! Invalid escape character(s) within a string.

use pest::iterators::Pairs;

use crate::core::validation;
use crate::core::validation::Rule;
use crate::core::Code;
use crate::v1;
use crate::Version;

/// An invalid escape character within a string.
#[derive(Debug)]
pub struct InvalidEscapeCharacter;

impl Rule<v1::Rule> for InvalidEscapeCharacter {
    fn code(&self) -> Code {
        // SAFETY: this manually crafted to unwrap successfully every time.
        Code::try_new(Version::V1, 1).unwrap()
    }

    fn validate(&self, tree: Pairs<'_, v1::Rule>) -> validation::Result {
        tree.flatten().try_for_each(|node| match node.as_rule() {
            v1::Rule::char_escaped_invalid => {
                let (line_no, col) = node.line_col();
                Err(validation::error::Builder::default()
                    .code(self.code())
                    .message(format!(
                        "invalid escape character '{}' in string at line {}:{}",
                        node.as_str(),
                        line_no,
                        col
                    ))
                    .try_build()
                    .unwrap())
            }
            _ => Ok(()),
        })
    }
}

#[cfg(test)]
mod tests {
    use pest::Parser as _;

    use crate::core::validation::Rule as _;
    use crate::v1::parse::Parser;
    use crate::v1::Rule;

    use super::*;

    #[test]
    fn it_catches_an_invalid_escape_character() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::string, "\"\\.\"")?;
        let error = InvalidEscapeCharacter.validate(tree).unwrap_err();

        assert_eq!(
            error.to_string(),
            String::from("[v1::001] invalid escape character '\\.' in string at line 1:2")
        );

        Ok(())
    }
}
