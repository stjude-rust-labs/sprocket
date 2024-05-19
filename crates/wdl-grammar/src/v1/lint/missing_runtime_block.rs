//! Tasks should have a `runtime` block to ensure portability.

use std::collections::VecDeque;

use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_core::concern::code;
use wdl_core::concern::lint;
use wdl_core::concern::lint::Rule;
use wdl_core::concern::lint::Tag;
use wdl_core::concern::lint::TagSet;
use wdl_core::concern::Code;
use wdl_core::file::Location;
use wdl_core::Version;

use crate::v1;

/// Detects missing `runtime` blocks for tasks.
#[derive(Debug)]
pub struct MissingRuntimeBlock;

impl<'a> MissingRuntimeBlock {
    /// Creates a warning corresponding to the task with the missing `runtime`
    /// block.
    fn missing_runtime_block(&self, location: Location) -> lint::Warning
    where
        Self: Rule<&'a Pair<'a, v1::Rule>>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::High)
            .tags(self.tags())
            .push_location(location)
            .subject("missing runtime block within a task")
            .body("Tasks that don't declare runtime blocks are unlikely to be portable")
            .fix(
                "Add a runtime block to the task with desired cpu, memory and container/docker \
                 requirements",
            )
            .try_build()
            .unwrap()
    }
}

impl<'a> Rule<&'a Pair<'a, v1::Rule>> for MissingRuntimeBlock {
    fn code(&self) -> Code {
        Code::try_new(code::Kind::Warning, Version::V1, 5).unwrap()
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Completeness, Tag::Portability])
    }

    fn check(&self, tree: &'a Pair<'a, v1::Rule>) -> lint::Result {
        let mut warnings = VecDeque::new();

        for node in tree.clone().into_inner().flatten() {
            if node.as_rule() == v1::Rule::task {
                let mut runtime_block_found = false;
                for inner_node in node.clone().into_inner().flatten() {
                    if inner_node.as_rule() == v1::Rule::task_runtime {
                        runtime_block_found = true;
                        break;
                    }
                }

                if !runtime_block_found {
                    let location =
                        Location::try_from(node.as_span()).map_err(lint::Error::Location)?;
                    warnings.push_back(self.missing_runtime_block(location))
                }
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

    use super::*;
    use crate::v1::Parser;

    #[test]
    fn it_catches_missing_runtime_block() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            v1::Rule::document,
            r#"version 1.1

task hello_task {
  input {
    File infile
    String pattern
  }

  command <<<
    grep -E '~{pattern}' '~{infile}'
  >>>

  output {
    Array[String] matches = read_lines(stdout())
  }
}
"#,
        )?
        .next()
        .unwrap();
        let result = MissingRuntimeBlock.check(&tree)?;
        assert!(result.is_some());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W005::[Completeness, Portability]::High] missing runtime block within a task \
             (3:1-16:2)"
        );

        Ok(())
    }

    #[test]
    fn it_does_not_catch_runtime_block() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            v1::Rule::document,
            r#"version 1.1

task hello_task {
  input {
    File infile
    String pattern
  }

  runtime {
    docker: "ubuntu:latest"
  }

  command <<<
    grep -E '~{pattern}' '~{infile}'
  >>>

  output {
    Array[String] matches = read_lines(stdout())
  }
}
"#,
        )?
        .next()
        .unwrap();
        let result = MissingRuntimeBlock.check(&tree)?;
        assert!(result.is_none());

        Ok(())
    }

    #[test]
    fn it_catches_multiple_missing_blocks() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            v1::Rule::document,
            r#"version 1.1

task hello_task {
  input {
    File infile
    String pattern
  }

  command <<<
    grep -E '~{pattern}' '~{infile}'
  >>>

  output {
    Array[String] matches = read_lines(stdout())
  }
}

task subsitute {
  input {
    File someFile
    String sedPattern
  }

  command <<<
    sed '~{pattern}' '~{infile}'
  >>>

  output {
    Array[String] matches = read_lines(stdout())
  }
}
"#,
        )?
        .next()
        .unwrap();
        let result = MissingRuntimeBlock.check(&tree)?;
        assert!(result.is_some());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 2);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W005::[Completeness, Portability]::High] missing runtime block within a task \
             (3:1-16:2)"
        );
        assert_eq!(
            warnings.get(1).unwrap().to_string(),
            "[v1::W005::[Completeness, Portability]::High] missing runtime block within a task \
             (18:1-31:2)"
        );
        Ok(())
    }

    #[test]
    fn it_catches_one_missing_block() -> Result<(), Box<dyn std::error::Error>> {
        let tree = Parser::parse(
            v1::Rule::document,
            r#"version 1.1

task hello_task {
  input {
    File infile
    String pattern
  }

  command <<<
    grep -E '~{pattern}' '~{infile}'
  >>>

  output {
    Array[String] matches = read_lines(stdout())
  }

  runtime {
    docker: "ubuntu:latest"
  }
}

task subsitute {
  input {
    File someFile
    String sedPattern
  }

  command <<<
    sed '~{pattern}' '~{infile}'
  >>>

  output {
    Array[String] matches = read_lines(stdout())
  }
}
"#,
        )?
        .next()
        .unwrap();
        let result = MissingRuntimeBlock.check(&tree)?;
        assert!(result.is_some());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings.first().to_string(),
            "[v1::W005::[Completeness, Portability]::High] missing runtime block within a task \
             (22:1-35:2)"
        );
        Ok(())
    }
}
