# Rules

This table documents all implemented `wdl` lint rules implemented on the `main`
branch of the `stjude-rust-labs/wdl` repository. Note that the information may
be out of sync with released packages.

## Lint Rules

| Name                        | Description                                                                                         |
|:----------------------------|:----------------------------------------------------------------------------------------------------|
| `CallInputSpacing`          | Ensures proper spacing for call inputs                                                              |
| `CommandSectionIndentation` | Ensures consistent indentation (no mixed spaces/tabs) within command sections.                      |
| `CommentWhitespace`         | Ensures that comments are properly spaced.                                                          |
| `ConciseInput`              | Ensures concise input assignments are used (implicit binding when available).                       |
| `ConsistentNewlines`        | Ensures that `\n` or `\r\n` newlines are used consistently within the file.                         |
| `ContainerUri`              | Ensures that the value for the `container` key in `runtime`/`requirements` sections is well-formed. |
| `DeclarationName`           | Ensures declaration names do not redundantly include their type name.                               |
| `DeprecatedObject`          | Ensures that the deprecated `Object` construct is not used.                                         |
| `DeprecatedPlaceholder`     | Ensures that the deprecated placeholder options construct is not used.                              |
| `DescriptionLength`         | Ensures that `description` meta entries are not too long for display in documentation.              |
| `DoubleQuotes`              | Ensures that strings are defined using double quotes.                                               |
| `ElementSpacing`            | Ensures proper blank space between elements                                                         |
| `EndingNewline`             | Ensures that documents end with a single newline character.                                         |
| `ExpectedRuntimeKeys`       | Ensures that runtime sections have the appropriate keys.                                            |
| `ExpressionSpacing`         | Ensures that expressions are properly spaced.                                                       |
| `HereDocCommands`           | Ensures that tasks use heredoc syntax in command sections.                                          |
| `ImportPlacement`           | Ensures that imports are placed between the version statement and any document items.               |
| `ImportSorted`              | Ensures that imports are sorted lexicographically.                                                  |
| `ImportWhitespace`          | Ensures that there is no extraneous whitespace between or within imports.                           |
| `InputName`                 | Ensures input names are meaningful (e.g., not generic like 'input', 'in', or too short).            |
| `InputSorted`               | Ensures that input declarations are sorted                                                          |
| `KnownRules`                | Ensures only known rules are used in lint directives.                                               |
| `LineWidth`                 | Ensures that lines do not exceed a certain width.                                                   |
| `LintDirectiveFormatted`    | Ensures lint directives are correctly formatted.                                                    |
| `LintDirectiveValid`        | Ensures lint directives are placed correctly to have the intended effect.                           |
| `MatchingOutputMeta`        | Ensures that each output field is documented in the meta section under `meta.outputs`.              |
| `MetaDescription`           | Ensures the `meta` section contains a `description` key.                                            |
| `MetaKeyValueFormatting`    | Ensures that metadata objects and arrays are properly spaced.                                       |
| `MetaSections`              | Ensures that tasks and workflows have the required `meta` and `parameter_meta` sections.            |
| `OutputName`                | Ensures output names are meaningful (e.g., not generic like 'output', 'out', or too short).         |
| `ParameterMetaMatched`      | Ensures that inputs have a matching entry in a `parameter_meta` section.                            |
| `PascalCase`                | Ensures that structs are defined with PascalCase names.                                             |
| `PreambleCommentPlacement`  | Ensures that documents have correct comments in the preamble.                                       |
| `PreambleFormatted`         | Ensures that documents have correct whitespace in the preamble.                                     |
| `RedundantNone`             | Ensures optional inputs don't have redundant `None` assignments.                                    |
| `RequirementsSection`       | Ensures that >=v1.2 tasks have a requirements section.                                              |
| `RuntimeSection`            | Ensures that <v1.2 tasks have a runtime section.                                                    |
| `SectionOrdering`           | Ensures that sections within tasks and workflows are sorted.                                        |
| `ShellCheck`                | Ensures that command sections are free of shellcheck diagnostics.                                   |
| `SnakeCase`                 | Ensures that tasks, workflows, and variables are defined with snake_case names.                     |
| `TodoComment`               | Ensures that `TODO` statements are flagged for followup.                                            |
| `TrailingComma`             | Ensures that lists and objects in meta have a trailing comma.                                       |
| `VersionStatementFormatted` | Ensures the `version` statement is correctly formatted.                                             |
| `Whitespace`                | Ensures that a document does not contain undesired whitespace.                                      |
