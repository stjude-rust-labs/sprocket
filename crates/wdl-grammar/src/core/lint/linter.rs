//! Linters.

use pest::iterators::Pairs;
use pest::RuleType;

use crate::core::lint;
use crate::core::lint::Rule;
use crate::core::lint::Warning;

/// A [`Result`](std::result::Result) for the [`Linter::lint`] function.
pub type Result = std::result::Result<Option<Vec<Warning>>, Box<dyn std::error::Error>>;

/// A linter for a WDL parse tree.
#[derive(Debug)]
pub struct Linter;

impl Linter {
    /// Lints a WDL parse tree according to a set of lint rules.
    pub fn lint<R: RuleType>(tree: Pairs<'_, R>, rules: &[Box<dyn Rule<R>>]) -> Result {
        let warnings = rules
            .iter()
            .map(|rule| rule.check(tree.clone()))
            .collect::<std::result::Result<Vec<Option<Vec<lint::Warning>>>, Box<dyn std::error::Error>>>()?
            .into_iter()
            .flatten()
            .flatten()
            .collect::<Vec<lint::Warning>>();

        match warnings.is_empty() {
            true => Ok(None),
            false => Ok(Some(warnings)),
        }
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
        let tree = Parser::parse(Rule::document, "version 1.1 \n \n")?;
        let rules = crate::v1::lint::rules();
        let mut results = Linter::lint(tree, rules.as_ref())?.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(
            results.pop().unwrap().to_string(),
            String::from("[v1::001::Style/Low] line 2 is empty but contains spaces")
        );
        assert_eq!(
            results.pop().unwrap().to_string(),
            String::from("[v1::001::Style/Low] trailing space at the end of line 1")
        );

        Ok(())
    }
}
