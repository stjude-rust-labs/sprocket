# Rules

This table documents all implemented `wdl` lint rules implemented on the `main` 
branch of the `stjude-rust-labs/wdl` repository. Note that the information may 
be out of sync with released packages.

## Lint Rules

| Name                             | Tags                          | Description                                                                           |
|:---------------------------------|:------------------------------|:--------------------------------------------------------------------------------------|
| `CommandSectionMixedIndentation` | Clarity, Correctness, Spacing | Ensures that lines within a command do not mix spaces and tabs.                       |
| `DoubleQuotes`                   | Clarity, Style                | Ensures that strings are defined using double quotes.                                 |
| `EndingNewline`                  | Spacing, Style                | Ensures that documents end with a single newline character.                           |
| `ImportPlacement`                | Clarity                       | Ensures that imports are placed between the version statement and any document items. |
| `ImportSort`                     | Clarity, Style                | Ensures that imports are sorted lexicographically.                                    |
| `ImportWhitespace`               | Clarity, Style, Spacing       | Ensures that there is no extraneous whitespace between or within imports.             |
| `MatchingParameterMeta`          | Completeness                  | Ensures that inputs have a matching entry in a `parameter_meta` section.              |
| `MissingRuntime`                 | Completeness, Portability     | Ensures that tasks have a runtime section.                                            |
| `MissingMetas`                   | Completeness, Clarity         | Ensures that tasks have both a meta and a parameter_meta section.                     |
| `MissingOutput`                  | Completeness, Portability     | Ensures that tasks have an output section.                                            |
| `NoCurlyCommands`                | Clarity                       | Ensures that tasks use heredoc syntax in command sections.                            |
| `PascalCase`                     | Clarity, Naming, Style        | Ensures that structs are defined with PascalCase names.                               |
| `PreambleComments`               | Clarity, Spacing, Style       | Ensures that documents have correct comments in the preamble.                         |
| `PreambleWhitespace`             | Spacing, Style                | Ensures that documents have correct whitespace in the preamble.                       |
| `SnakeCase`                      | Clarity, Naming, Style        | Ensures that tasks, workflows, and variables are defined with snake_case names.       |
| `Whitespace`                     | Spacing, Style                | Ensures that a document does not contain undesired whitespace.                        |
