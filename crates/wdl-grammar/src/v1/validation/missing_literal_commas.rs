//! Ensures that literals contain commas to delimit all but the last member.
//!
//! The literals the this rule checks are:
//!
//!   * Object literals.
//!   * Struct literals.
//!   * Map literals.
//!   * Arrays literals.
//!
//! Notably, pair literals are not included in this rule. This is because the
//! comma delimiting the two members within a pair literal is more obviously
//! needed and is enforced within the grammar.

use std::collections::VecDeque;

use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_core::concern::code;
use wdl_core::concern::validation;
use wdl_core::concern::validation::Rule;
use wdl_core::concern::Code;
use wdl_core::file::location::Position;
use wdl_core::file::Location;
use wdl_core::Version;
use wdl_macros::gather;

use crate::v1;

/// Detects literals that are missing required commas.
#[derive(Debug)]
pub struct MissingLiteralCommas;

impl<'a> MissingLiteralCommas {
    /// Creates an error for a missing comma within a literal.
    fn missing_comma_error(&self, literal_name: &str, location: Location) -> validation::Failure {
        // SAFETY: this error is written so that it will always unwrap.
        validation::failure::Builder::default()
            .code(self.code())
            .subject(format!(
                "missing comma within {} literal",
                literal_name.to_lowercase()
            ))
            .body(format!(
                "{} literals require a comma to delimit all but the last member.",
                literal_name
            ))
            .push_location(location)
            .fix("Add the missing comma.")
            .try_build()
            .unwrap()
    }

    /// Creates an error for a missing comma within an object literal.
    fn object_literal(&self, location: Location) -> validation::Failure
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        self.missing_comma_error("Object", location)
    }

    /// Creates an error for a missing comma within a struct literal.
    fn struct_literal(&self, location: Location) -> validation::Failure
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        self.missing_comma_error("Struct", location)
    }

    /// Creates an error for a missing comma within a map literal.
    fn map_literal(&self, location: Location) -> validation::Failure
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        self.missing_comma_error("Map", location)
    }

    /// Creates an error for a missing comma within an array literal.
    fn array_literal(&self, location: Location) -> validation::Failure
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        self.missing_comma_error("Array", location)
    }

    /// Detects missing commas in an object literal and returns any validation
    /// failures encountered during those checks.
    fn detect_missing_commas_in_object_literal(
        &self,
        object_literal: Pair<'_, v1::Rule>,
    ) -> Vec<validation::Failure> {
        let mut failures = Vec::new();
        let mut elements = object_literal.into_inner();

        let mut current = match elements.next() {
            Some(element) => element,
            None => return Vec::default(),
        };

        for next in elements {
            // Ignore whitespace and comments, as they have no bearing on the
            // presence of the trailing commas.
            if next.as_rule() == v1::Rule::WHITESPACE || next.as_rule() == v1::Rule::COMMENT {
                continue;
            }

            if current.as_rule() == v1::Rule::identifier_based_kv_pair
                && next.as_rule() == v1::Rule::identifier_based_kv_pair
            {
                // SAFETY: this should always unwrap for positions
                // received from Pest, as lines and columns start
                // counting at one.
                let position = Position::try_from(current.as_span().end_pos()).unwrap();
                let location = Location::Position(position);

                failures.push(self.object_literal(location))
            }

            current = next;
        }

        failures
    }

    /// Detects missing commas in an struct literal and returns any validation
    /// failures encountered during those checks.
    fn detect_missing_commas_in_struct_literal(
        &self,
        struct_literal: Pair<'_, v1::Rule>,
    ) -> Vec<validation::Failure> {
        let mut failures = Vec::new();
        let mut elements = struct_literal.into_inner();

        // SAFETY: the definition of the `struct_literal` rule requires that at
        // least one element (namely, an `struct_literal_name`) is present. As
        // such, this will always unwrap.
        let mut current = elements.next().unwrap();

        for next in elements {
            // Ignore whitespace and comments, as they have no bearing on the
            // presence of the trailing commas.
            if next.as_rule() == v1::Rule::WHITESPACE || next.as_rule() == v1::Rule::COMMENT {
                continue;
            }

            if current.as_rule() == v1::Rule::identifier_based_kv_pair
                && next.as_rule() == v1::Rule::identifier_based_kv_pair
            {
                // SAFETY: this should always unwrap for positions
                // received from Pest, as lines and columns start
                // counting at one.
                let position = Position::try_from(current.as_span().end_pos()).unwrap();
                let location = Location::Position(position);

                failures.push(self.struct_literal(location))
            }

            current = next;
        }

        failures
    }

    /// Detects missing commas in a map literal and returns any validation
    /// failures encountered during those checks.
    fn detect_missing_commas_in_map_literal(
        &self,
        map_literal: Pair<'_, v1::Rule>,
    ) -> Vec<validation::Failure> {
        let mut failures = Vec::new();
        let mut elements = map_literal.into_inner();

        let mut current = match elements.next() {
            Some(element) => element,
            None => return Vec::default(),
        };

        for next in elements {
            // Ignore whitespace and comments, as they have no bearing on the
            // presence of the trailing commas.
            if next.as_rule() == v1::Rule::WHITESPACE || next.as_rule() == v1::Rule::COMMENT {
                continue;
            }

            if current.as_rule() == v1::Rule::expression_based_kv_pair
                && next.as_rule() == v1::Rule::expression_based_kv_pair
            {
                // SAFETY: this should always unwrap for positions
                // received from Pest, as lines and columns start
                // counting at one.
                let position = Position::try_from(current.as_span().end_pos()).unwrap();
                let location = Location::Position(position);

                failures.push(self.map_literal(location))
            }

            current = next;
        }

        failures
    }

    /// Detects missing commas in a array literal and returns any validation
    /// failures encountered during those checks.
    fn detect_missing_commas_in_array_literal(
        &self,
        array_literal: Pair<'_, v1::Rule>,
    ) -> Vec<validation::Failure> {
        let mut failures = Vec::new();
        let mut elements = array_literal.into_inner();

        let mut current = match elements.next() {
            Some(element) => element,
            None => return Vec::default(),
        };

        for next in elements {
            // Ignore whitespace and comments, as they have no bearing on the
            // presence of the trailing commas.
            if next.as_rule() == v1::Rule::WHITESPACE || next.as_rule() == v1::Rule::COMMENT {
                continue;
            }

            if current.as_rule() == v1::Rule::expression && next.as_rule() == v1::Rule::expression {
                // SAFETY: this should always unwrap for positions
                // received from Pest, as lines and columns start
                // counting at one.
                let position = Position::try_from(current.as_span().end_pos()).unwrap();
                let location = Location::Position(position);

                failures.push(self.array_literal(location))
            }

            current = next;
        }

        failures
    }
}

impl Rule<&Pair<'_, v1::Rule>> for MissingLiteralCommas {
    fn code(&self) -> Code {
        // SAFETY: this manually crafted to unwrap successfully every time.
        Code::try_new(code::Kind::Error, Version::V1, 4).unwrap()
    }

    fn validate(&self, tree: &Pair<'_, v1::Rule>) -> validation::Result {
        let mut failures = VecDeque::new();

        for object_literal in gather!(tree.clone(), crate::v1::Rule::object_literal) {
            failures.extend(self.detect_missing_commas_in_object_literal(object_literal));
        }

        for struct_literal in gather!(tree.clone(), crate::v1::Rule::struct_literal) {
            failures.extend(self.detect_missing_commas_in_struct_literal(struct_literal));
        }

        for map_literal in gather!(tree.clone(), crate::v1::Rule::map_literal) {
            failures.extend(self.detect_missing_commas_in_map_literal(map_literal));
        }

        for array_literal in gather!(tree.clone(), crate::v1::Rule::array_literal) {
            failures.extend(self.detect_missing_commas_in_array_literal(array_literal));
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
    fn it_detects_missing_required_commas_in_an_object_literal()
    -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.1
        
workflow test {
    Object object = object {
        foo: "bar"
        baz: "quux"
        duck: null
    }
}"#,
        )?
        .next()
        .unwrap();
        let mut results = MissingLiteralCommas
            .validate(&tree)
            .unwrap()
            .unwrap()
            .into_iter();

        assert_eq!(
            results.next().unwrap().to_string(),
            String::from("[v1::E004] missing comma within object literal (5:19)")
        );
        assert_eq!(
            results.next().unwrap().to_string(),
            String::from("[v1::E004] missing comma within object literal (6:20)")
        );
        assert_eq!(results.next(), None);

        Ok(())
    }

    #[test]
    fn it_does_not_detected_an_object_literal_with_commas() -> Result<(), Box<dyn std::error::Error>>
    {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.1
        
workflow test {
    Object object = object {
        foo: "bar",
        baz: "quux",
        duck: null,
    }
}"#,
        )?
        .next()
        .unwrap();
        let results = MissingLiteralCommas.validate(&tree).unwrap();

        assert!(results.is_none());

        Ok(())
    }

    #[test]
    fn it_does_not_detected_the_optional_comma_in_an_object_literal()
    -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.1
        
workflow test {
    Object object = object {
        foo: "bar"
    }
}"#,
        )?
        .next()
        .unwrap();
        let results = MissingLiteralCommas.validate(&tree).unwrap();

        assert!(results.is_none());

        Ok(())
    }

    #[test]
    fn it_detects_missing_required_commas_in_an_struct_literal()
    -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.1

struct Hello {
    String? greeting
    String name
}

workflow test {
    Hello object = Hello {
        greeting: "Hi, "
        name: "bar"
    }
}"#,
        )?
        .next()
        .unwrap();
        let mut results = MissingLiteralCommas
            .validate(&tree)
            .unwrap()
            .unwrap()
            .into_iter();

        assert_eq!(
            results.next().unwrap().to_string(),
            String::from("[v1::E004] missing comma within struct literal (10:25)")
        );
        assert_eq!(results.next(), None);

        Ok(())
    }

    #[test]
    fn it_does_not_detected_a_struct_literal_with_commas() -> Result<(), Box<dyn std::error::Error>>
    {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.1
        
struct Hello {
    String? greeting
    String name
}

workflow test {
    Hello object = Hello {
        greeting: "Hi, ",
        name: "bar",
    }
}"#,
        )?
        .next()
        .unwrap();
        let results = MissingLiteralCommas.validate(&tree).unwrap();

        assert!(results.is_none());

        Ok(())
    }

    #[test]
    fn it_does_not_detected_the_optional_comma_in_an_struct_literal()
    -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.1

struct Hello {
    String name
}

workflow test {
    Hello object = Hello {
        name: "bar"
    }
}"#,
        )?
        .next()
        .unwrap();
        let results = MissingLiteralCommas.validate(&tree).unwrap();

        assert!(results.is_none());

        Ok(())
    }

    #[test]
    fn it_detects_missing_required_commas_in_a_map_literal()
    -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.1

workflow test {
    Map[Int, String] foo = {
        1: "a"
        2: "b"
        3: "c"
     }
}"#,
        )?
        .next()
        .unwrap();
        let mut results = MissingLiteralCommas
            .validate(&tree)
            .unwrap()
            .unwrap()
            .into_iter();

        assert_eq!(
            results.next().unwrap().to_string(),
            String::from("[v1::E004] missing comma within map literal (5:15)")
        );
        assert_eq!(
            results.next().unwrap().to_string(),
            String::from("[v1::E004] missing comma within map literal (6:15)")
        );
        assert_eq!(results.next(), None);

        Ok(())
    }

    #[test]
    fn it_does_not_detected_a_map_literal_with_commas() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.1
        
workflow test {
    Map[Int, String] foo = {
        1: "a",
        2: "b",
        3: "c",
     }
}"#,
        )?
        .next()
        .unwrap();
        let results = MissingLiteralCommas.validate(&tree).unwrap();

        assert!(results.is_none());

        Ok(())
    }

    #[test]
    fn it_does_not_detected_the_optional_comma_in_a_map_literal()
    -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.1

workflow test {
    Map[Int, String] foo = {
        1: "a"
     }
}"#,
        )?
        .next()
        .unwrap();
        let results = MissingLiteralCommas.validate(&tree).unwrap();

        assert!(results.is_none());

        Ok(())
    }

    #[test]
    fn it_detects_missing_required_commas_in_an_array_literal()
    -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.1

workflow test {
    Array[Int] foo = [
        1
        2
        3
    ]
}"#,
        )?
        .next()
        .unwrap();
        let mut results = MissingLiteralCommas
            .validate(&tree)
            .unwrap()
            .unwrap()
            .into_iter();

        assert_eq!(
            results.next().unwrap().to_string(),
            String::from("[v1::E004] missing comma within array literal (5:10)")
        );
        assert_eq!(
            results.next().unwrap().to_string(),
            String::from("[v1::E004] missing comma within array literal (6:10)")
        );
        assert_eq!(results.next(), None);

        Ok(())
    }

    #[test]
    fn it_does_not_detected_an_array_literal_with_commas() -> Result<(), Box<dyn std::error::Error>>
    {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.1
        
workflow test {
    Array[Int] foo = [
        1,
        2,
        3,
    ]
}"#,
        )?
        .next()
        .unwrap();
        let results = MissingLiteralCommas.validate(&tree).unwrap();

        assert!(results.is_none());

        Ok(())
    }

    #[test]
    fn it_does_not_detected_the_optional_comma_in_an_array_literal()
    -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            Rule::document,
            r#"version 1.1

workflow test {
    Array[Int] foo = [
        1
    ]
}"#,
        )?
        .next()
        .unwrap();
        let results = MissingLiteralCommas.validate(&tree).unwrap();

        assert!(results.is_none());

        Ok(())
    }

    #[test]
    fn a_pair_literal_missing_a_comma_is_a_parse_error() -> Result<(), Box<dyn std::error::Error>> {
        let err = Parser::parse(
            Rule::document,
            r#"version 1.1

workflow test {
    Pair[Int, Int] foo = (
        1
        2
    )
}"#,
        )
        .unwrap_err()
        .to_string();

        assert_eq!(
            err,
            String::from(
                " --> 6:9\n  |\n6 |         2\n  |         ^---\n  |\n  = expected WHITESPACE, \
                 COMMENT, COMMA, or, and, add, sub, mul, div, remainder, eq, neq, lte, gte, lt, \
                 gt, member, index, or call"
            )
        );

        Ok(())
    }
}
