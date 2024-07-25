# Rules

This table documents all implemented `wdl` lint rules implemented on the `main`
branch of the `stjude-rust-labs/wdl` repository. Note that the information may
be out of sync with released packages.

## Lint Rules

| Name                             | Tags                          | Description                                                                           |
| :------------------------------- | :---------------------------- | :------------------------------------------------------------------------------------ |
| `CallInputSpacing`               | Style, Clarity, Spacing       | Ensures proper spacing for call inputs                                                |
| `CommandSectionMixedIndentation` | Clarity, Correctness, Spacing | Ensures that lines within a command do not mix spaces and tabs.                       |
| `CommentWhitespace`              | Spacing                       | Ensures that comments are properly spaced.                                            |
| `DeprecatedObject`               | Deprecated                    | Ensures that the deprecated `Object` construct is not used.                           |
| `DeprecatedPlaceholderOption`    | Deprecated                    | Ensures that the deprecated placeholder options construct is not used.                |
| `DescriptionMissing`             | Completeness                  | Ensures that each meta section has a description key.                                 |
| `DoubleQuotes`                   | Clarity, Style                | Ensures that strings are defined using double quotes.                                 |
| `EndingNewline`                  | Spacing, Style                | Ensures that documents end with a single newline character.                           |
| `ImportPlacement`                | Clarity, Sorting              | Ensures that imports are placed between the version statement and any document items. |
| `ImportSort`                     | Clarity, Style                | Ensures that imports are sorted lexicographically.                                    |
| `ImportWhitespace`               | Clarity, Style, Spacing       | Ensures that there is no extraneous whitespace between or within imports.             |
| `InconsistentNewlines`           | Clarity, Style                | Ensures that newlines are used consistently within the file.                          |
| `InputNotSorted`                 | Style                         | Ensures that input declarations are sorted                                            |
| `InputSpacing`                   | Style, Clarity, Spacing       | Ensures proper spacing for call inputs                                                |
| `LineWidth`                      | Clarity, Spacing, Style       | Ensures that lines do not exceed a certain width.                                     |
| `MatchingParameterMeta`          | Completeness                  | Ensures that inputs have a matching entry in a `parameter_meta` section.              |
| `MissingRuntime`                 | Completeness, Portability     | Ensures that tasks have a runtime section.                                            |
| `MissingMetas`                   | Completeness, Clarity         | Ensures that tasks have both a meta and a parameter_meta section.                     |
| `MissingOutput`                  | Completeness, Portability     | Ensures that tasks have an output section.                                            |
| `NoCurlyCommands`                | Clarity                       | Ensures that tasks use heredoc syntax in command sections.                            |
| `PascalCase`                     | Clarity, Naming, Style        | Ensures that structs are defined with PascalCase names.                               |
| `PreambleComments`               | Clarity, Spacing, Style       | Ensures that documents have correct comments in the preamble.                         |
| `PreambleWhitespace`             | Spacing, Style                | Ensures that documents have correct whitespace in the preamble.                       |
| `RuntimeSectionKeys`             | Completeness, Deprecated      | Ensures that runtime sections have the appropriate keys.                              |
| `SectionOrdering`                | Sorting, Style                | Ensures that sections within tasks and workflows are sorted.                          |
| `SnakeCase`                      | Clarity, Naming, Style        | Ensures that tasks, workflows, and variables are defined with snake_case names.       |
| `Todo`                           | Completeness                  | Ensures that `TODO` statements are flagged for followup.                              |
| `TrailingComma`                  | Style                         | Ensures that lists and objects in meta have a trailing comma.                         |
| `Whitespace`                     | Spacing, Style                | Ensures that a document does not contain undesired whitespace.                        |

