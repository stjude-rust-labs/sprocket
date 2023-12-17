//! Duplicate keys within a runtime section.

use std::collections::HashMap;
use std::collections::VecDeque;

use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_core::concern::code;
use wdl_core::concern::validation;
use wdl_core::concern::validation::Rule;
use wdl_core::concern::Code;
use wdl_core::fs::Location;
use wdl_core::Version;
use wdl_macros::gather;

use crate::v1;

/// Detects an invalid version value.
#[derive(Debug)]
pub struct DuplicateRuntimeKeys;

impl<'a> DuplicateRuntimeKeys {
    /// Generates a validation error for duplicate runtime keys.
    fn duplicate_runtime_keys(&self, key: String, locations: Vec<Location>) -> validation::Failure
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        let mut builder = validation::failure::Builder::default()
            .code(self.code())
            .subject(format!("duplicate runtime keys: '{}'", key))
            .body("Duplicate runtime keys were detected.")
            .fix("Runtime keys cannot be duplicated. Resolve and remove the duplicated keys.");

        for location in locations {
            builder = builder.push_location(location);
        }

        builder.try_build().unwrap()
    }
}

impl Rule<&Pair<'_, v1::Rule>> for DuplicateRuntimeKeys {
    fn code(&self) -> Code {
        // SAFETY: this manually crafted to unwrap successfully every time.
        Code::try_new(code::Kind::Error, Version::V1, 3).unwrap()
    }

    fn validate(&self, tree: &Pair<'_, v1::Rule>) -> validation::Result {
        let mut failures = VecDeque::new();

        for task in gather!(tree.clone(), crate::v1::Rule::task) {
            let mut keys = HashMap::<String, Vec<Location>>::new();

            for node in task.into_inner().flatten() {
                if node.as_rule() == v1::Rule::task_runtime_mapping_key {
                    let location =
                        Location::try_from(node.as_span()).map_err(validation::Error::Location)?;
                    keys.entry(node.as_str().to_string())
                        .or_default()
                        .push(location);
                }
            }

            for (key, locations) in keys {
                if locations.len() > 1 {
                    failures
                        .push_front(DuplicateRuntimeKeys.duplicate_runtime_keys(key, locations));
                }
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
    fn it_detects_duplicate_runtime_keys() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.1
        
task hello_world {
    runtime {
        cpu: 1
        cpu: 2
    }
}"#,
        )?
        .next()
        .unwrap();
        let mut results = DuplicateRuntimeKeys
            .validate(&tree)
            .unwrap()
            .unwrap()
            .into_iter();

        assert_eq!(
            results.next().unwrap().to_string(),
            String::from("[v1::E003] duplicate runtime keys: 'cpu' (5:9-5:12, 6:9-6:12)")
        );
        assert_eq!(results.next(), None);

        Ok(())
    }

    #[test]
    fn it_ignores_valid_runtime_keys() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.1
        
task hello {
    runtime {
        foo: 1
        bar: "Hello, world!"
    }
}

task world {
    runtime {
        foo: 1
        bar: "Hello, world!"
    }
}"#,
        )?
        .next()
        .unwrap();
        let results = DuplicateRuntimeKeys.validate(&tree).unwrap();
        assert!(results.is_none());

        Ok(())
    }
}
