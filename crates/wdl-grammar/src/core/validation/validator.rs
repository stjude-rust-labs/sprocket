//! Validators.

use pest::iterators::Pairs;
use pest::RuleType;

use crate::core::validation;
use crate::core::validation::Rule;

/// A validator for a WDL parse tree.
#[derive(Debug)]
pub struct Validator;

impl Validator {
    /// Validates a WDL parse tree according to a set of validation rules.
    pub fn validate<R: RuleType>(
        tree: Pairs<'_, R>,
        rules: &[Box<dyn Rule<R>>],
    ) -> validation::Result {
        rules
            .iter()
            .try_for_each(|rule| rule.validate(tree.clone()))
    }
}

#[cfg(test)]
mod tests {
    use pest::Parser as _;

    use crate::v1::Parser;
    use crate::v1::Rule;

    use super::*;

    #[test]
    fn baseline() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            "version 1.1
task test {
    output {
        String hello = \"\\.\"
    }
}",
        )?;
        let rules = crate::v1::validation::rules();
        let err = Validator::validate(tree, rules.as_ref()).unwrap_err();
        assert_eq!(
            err.to_string(),
            String::from("[v1::001] invalid escape character '\\.' in string at line 4:25")
        );

        Ok(())
    }
}
