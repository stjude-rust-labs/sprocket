# Rules

This table documents all implemented `wdl` lint rules implemented on the `main`
branch of the `stjude-rust-labs/wdl` repository. Note that the information may
be out of sync with released packages.

## Lint Rules

| Name                             | Tags                          | Description                                                                                       |
|:---------------------------------|:------------------------------|:--------------------------------------------------------------------------------------------------|
| `BlankLinesBetweenElements`      | Spacing                       | Ensures proper blank space between elements                                                       |
| `CallInputSpacing`               | Clarity, Spacing, Style       | Ensures proper spacing for call inputs                                                            |
| `CommandSectionMixedIndentation` | Clarity, Correctness, Spacing | Ensures that lines within a command do not mix spaces and tabs.                                   |
| `CommentWhitespace`              | Spacing                       | Ensures that comments are properly spaced.                                                        |
| `ContainerValue`                 | Clarity, Portability          | Ensures that the value for `container` keys in `runtime`/`requirements` sections are well-formed. |
| `DeprecatedObject`               | Deprecated                    | Ensures that the deprecated `Object` construct is not used.                                       |
| `DeprecatedPlaceholderOption`    | Deprecated                    | Ensures that the deprecated placeholder options construct is not used.                            |
| `DescriptionMissing`             | Completeness                  | Ensures that each meta section has a description key.                                             |
| `DisallowedDeclarationName`      | Naming                        | Ensures that declaration names do not contain their type information.                             |
| `DisallowedInputName`            | Naming                        | Ensures that input names are meaningful.                                                          |
| `DisallowedOutputName`           | Naming                        | Ensures that output names are meaningful.                                                         |
| `DoubleQuotes`                   | Clarity, Style                | Ensures that strings are defined using double quotes.                                             |
| `EndingNewline`                  | Spacing, Style                | Ensures that documents end with a single newline character.                                       |
| `ExpressionSpacing`              | Spacing                       | Ensures that expressions are properly spaced.                                                     |
| `ImportPlacement`                | Clarity, Sorting              | Ensures that imports are placed between the version statement and any document items.             |
| `ImportSort`                     | Clarity, Style                | Ensures that imports are sorted lexicographically.                                                |
| `ImportWhitespace`               | Clarity, Spacing, Style       | Ensures that there is no extraneous whitespace between or within imports.                         |
| `InconsistentNewlines`           | Clarity, Style                | Ensures that newlines are used consistently within the file.                                      |
| `InputSorting`                   | Clarity, Sorting, Style       | Ensures that input declarations are sorted                                                        |
| `KeyValuePairs`                  | Style                         | Ensures that metadata objects and arrays are properly spaced.                                     |
| `LineWidth`                      | Clarity, Spacing, Style       | Ensures that lines do not exceed a certain width.                                                 |
| `MalformedLintDirective`         | Clarity, Correctness          | Ensures there are no malformed lint directives.                                                   |
| `MatchingParameterMeta`          | Completeness, Sorting         | Ensures that inputs have a matching entry in a `parameter_meta` section.                          |
| `MisplacedLintDirective`         | Clarity, Correctness          | Ensures there are no misplaced lint directives.                                                   |
| `MissingMetas`                   | Clarity, Completeness         | Ensures that tasks have both a meta and a parameter_meta section.                                 |
| `MissingOutput`                  | Completeness, Portability     | Ensures that tasks have an output section.                                                        |
| `MissingRequirements`            | Completeness, Portability     | Ensures that >=v1.2 tasks have a requirements section.                                            |
| `MissingRuntime`                 | Completeness, Portability     | Ensures that tasks have a runtime section.                                                        |
| `NoCurlyCommands`                | Clarity                       | Ensures that tasks use heredoc syntax in command sections.                                        |
| `NonmatchingOutput`              | Completeness                  | Ensures that each output field is documented in the meta section under `meta.outputs`.            |
| `PascalCase`                     | Clarity, Naming, Style        | Ensures that structs are defined with PascalCase names.                                           |
| `PreambleCommentAfterVersion`    | Clarity                       | Ensures that documents have correct comments in the preamble.                                     |
| `PreambleFormatting`             | Clarity, Spacing, Style       | Ensures that documents have correct whitespace in the preamble.                                   |
| `RedundantInputAssignment`       | Style                         | Ensures that redundant input assignments are shortened                                            |
| `RuntimeSectionKeys`             | Completeness, Deprecated      | Ensures that runtime sections have the appropriate keys.                                          |
| `SectionOrdering`                | Sorting, Style                | Ensures that sections within tasks and workflows are sorted.                                      |
| `ShellCheck`                     | Correctness, Portability      | (BETA) Ensures that command sections are free of shellcheck diagnostics.                          |
| `SnakeCase`                      | Clarity, Naming, Style        | Ensures that tasks, workflows, and variables are defined with snake_case names.                   |
| `Todo`                           | Completeness                  | Ensures that `TODO` statements are flagged for followup.                                          |
| `TrailingComma`                  | Style                         | Ensures that lists and objects in meta have a trailing comma.                                     |
| `UnknownRule`                    | Clarity                       | Ensures there are no unknown rules present in lint directives.                                    |
| `VersionFormatting`              | Style                         | Ensures correct formatting of the version statement                                               |
| `Whitespace`                     | Spacing, Style                | Ensures that a document does not contain undesired whitespace.                                    |
