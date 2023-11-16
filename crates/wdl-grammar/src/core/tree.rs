//! A parse tree.

use pest::iterators::Pairs;
use pest::RuleType;

use crate::core::lint;

/// A parse tree with a set of lint [`Warning`](lint::Warning)s.
///
/// **Note:** this struct implements [`std::ops::Deref`] for the native Pest
/// parse tree ([`Pairs`]), so you can treat this exactly as if you were
/// workings with [`Pairs`] directly.
#[derive(Debug)]
pub struct Tree<'a, R: RuleType> {
    /// The inner Pest parse tree.
    inner: Pairs<'a, R>,

    /// The lint warnings associated with the parse tree.
    warnings: Option<Vec<lint::Warning>>,
}

impl<'a, R: RuleType> Tree<'a, R> {
    /// Creates a new [`Tree`].
    pub fn new(inner: Pairs<'a, R>, warnings: Option<Vec<lint::Warning>>) -> Self {
        Self { inner, warnings }
    }

    /// Gets the inner [Pest parse tree](Pairs) for the [`Tree`] by reference.
    pub fn inner(&self) -> &Pairs<'a, R> {
        &self.inner
    }

    /// Consumes `self` to return the inner [Pest parse tree](Pairs) from the
    /// [`Tree`].
    pub fn into_inner(self) -> Pairs<'a, R> {
        self.inner
    }

    /// Gets the [`Warning`](lint::Warning)s from the [`Tree`] by reference.
    pub fn warnings(&self) -> Option<&Vec<lint::Warning>> {
        self.warnings.as_ref()
    }
}

impl<'a, R: RuleType> std::ops::Deref for Tree<'a, R> {
    type Target = Pairs<'a, R>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, R: RuleType> std::ops::DerefMut for Tree<'a, R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[cfg(test)]
mod tests {
    use pest::Parser as _;

    use super::*;
    use crate::core::lint::Linter;
    use crate::v1::Parser;
    use crate::v1::Rule;

    #[test]
    fn new() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::document, "version 1.1\n \n")?;
        let lints = Linter::lint(tree.clone(), &crate::v1::lint::rules())?;

        let tree = Tree::new(tree, lints);
        assert_eq!(
            tree.warnings().unwrap().first().unwrap().to_string(),
            String::from("[v1::001::Style/Low] line 2 is empty but contains spaces")
        );
        assert_eq!(tree.into_inner().len(), 1);

        Ok(())
    }
}
