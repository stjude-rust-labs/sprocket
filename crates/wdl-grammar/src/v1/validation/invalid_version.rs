//! Invalid versions.

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

/// Detects an invalid version value.
#[derive(Debug)]
pub struct InvalidVersion;

impl<'a> InvalidVersion {
    /// Generates a validation error for an invalid version.
    fn invalid_version(&self, version: &str, location: Location) -> validation::Failure
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        validation::failure::Builder::default()
            .code(self.code())
            .subject(format!("invalid version: '{}'", version))
            .body("An invalid version was detected.")
            .push_location(location)
            .fix(
                "This version is not supported by the v1 parser. Either \
                change the version to WDL 1.x or specify the correct \
                WDL specification version when parsing.",
            )
            .try_build()
            .unwrap()
    }
}

impl Rule<&Pair<'_, v1::Rule>> for InvalidVersion {
    fn code(&self) -> Code {
        // SAFETY: this manually crafted to unwrap successfully every time.
        Code::try_new(code::Kind::Error, Version::V1, 2).unwrap()
    }

    fn validate(&self, tree: &Pair<'_, v1::Rule>) -> validation::Result {
        let mut failures = VecDeque::new();

        for node in tree.clone().into_inner().flatten() {
            if node.as_rule() == v1::Rule::version && !node.as_str().starts_with("version 1") {
                let location =
                    Location::try_from(node.as_span()).map_err(validation::Error::Location)?;
                failures.push_back(self.invalid_version(node.as_str(), location));
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
    fn it_ignores_a_valid_version() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::document, "version 1.1")?
            .next()
            .unwrap();
        let results = InvalidVersion.validate(&tree).unwrap();
        assert_eq!(results, None);

        Ok(())
    }

    #[test]
    fn it_catches_an_invalid_version() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(Rule::document, "version 2")?.next().unwrap();
        let error = InvalidVersion
            .validate(&tree)
            .unwrap()
            .unwrap()
            .into_iter()
            .next()
            .unwrap();

        assert_eq!(
            error.to_string(),
            String::from("[v1::E002] invalid version: 'version 2' (1:1-1:10)")
        );

        Ok(())
    }
}
