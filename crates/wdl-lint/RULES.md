# Rules

This table documets all implemented `sprocket` lin rules impleemnted on the `main` branch of the `stjude-rust-labs/sprocket` repository. Note that the information may be out of sync with released packages.

## Lint Rules

| Name | Tags | Description |
|:-|:-|:-|
| `CallInputSpacing` | Spacing, Style | Detects unsorted input declarations |
| `CallInputUnnecessary` | Deprecated, Style | Suggest to remove unncessary input: keyword when document version is >= 1.2 |
| `CommandSectionIndentation` | Spacing, Clarity, Correctness | Detects mixed indentation in a command section |
| `CommentWhitespace` | Spacing, Style | Detects improperly spaced comments |
| `ConciseInput` | Style | Detects a redundant input assignment |
| `ConsistentNewlines` | Spacing, Clarity, Portability | Detects imports that are not sorted lexicographically |
| `ContainerUri` | Clarity, Portability | Ensures that values for container keys within runtime/requirements sections are well-formed |
| `DeclarationName` | Naming, Style, Clarity | A rule that identifies declaration names that include their type names |
| `DeprecatedObject` | Deprecated | Detects the use of the deprecated Object types |
| `DeprecatedPlaceholder` | Deprecated | Detects the use of a deprecated placeholder options |
| `DescriptionLength` | SprocketCompatibility | Ensures that description meta entries are not too long for display in Sprocket documentation |
| `DoubleQuotes` | Style, Clarity | Detects strings that are not defined with double quotes |
| `ElementSpacing` | Spacing, Style | Detects unsorted input declarations |
| `EndingNewline` | Spacing, Portability | Detects missing newline at the end of the document |
| `ExpectedRuntimeKeys` | Completeness, Deprecated | Detects the use of deprecated, unknown, or missing runtime keys |
| `ExpressionSpacing` | Spacing, Style | Detects improperly spaced expressions |
| `HereDocCommands` | Clarity, Correctness | Detects curly command section for tasks |
| `ImportPlacement` | Clarity | Detects incorrect import placements |
| `ImportSorted` | Sorting | Detects imports that are not sorted lexicographically |
| `ImportWhitespace` | Spacing, Style | Detects whitespace between imports |
| `InputName` | Naming, Style | A lint rule for disallowed input names |
| `InputSorted` | Sorting | Detects unsorted input declarations |
| `KnownRules` | Clarity, Correctness, SprocketCompatibility | Detects unknown rules within lint directives |
| `LineWidth` | Spacing, Style | Detects lines that exceed a certain width |
| `LintDirectiveFormatted` | Clarity, Correctness, SprocketCompatibility | Detects a malformed lint directive |
| `LintDirectiveValid` | Clarity, Correctness, SprocketCompatibility | Detects unknown rules within lint directives |
| `MatchingOutputMeta` | Completeness, Documentation, SprocketCompatibility | Detects non-matching outputs |
| `MetaDescription` | Completeness, Documentation, SprocketCompatibility | Detects unsorted input declarations |
| `MetaKeyValueFormatting` | Spacing, Style | A lint rule for missing meta and parameter_meta sections |
| `MetaSections` | Completeness, Clarity, Documentation | A lint rule for missing meta and parameter_meta sections |
| `OutputName` | Naming, Style | A lint rule for disallowed output names |
| `ParameterMetaMatched` | Completeness, Sorting, Documentation, SprocketCompatibility | Detects missing or extraneous entries in a parameter_meta section |
| `PascalCase` | Naming, Style, Clarity | Detects structs defined without a PascalCase name |
| `PreambleCommentPlacement` | Style, Clarity, SprocketCompatibility | A lint rule for flagging preamble comments which are outside the preamble |
| `PreambleFormatted` | Spacing, Style, SprocketCompatibility | Detects incorrect comments in a document preamble |
| `RedundantNone` | Style | A rule that identifies redundant `= None` assignments for optional inputs |
| `RequirementsSection` | Completeness, Portability, Deprecated | Detects missing requirements sections for tasks |
| `RuntimeSection` | Completeness, Portability | Detects missing runtime sections for tasks |
| `SectionOrdering` | Style, Sorting | Detects section ordering issues |
| `ShellCheck` | Correctness | Runs ShellCheck on a command section and reports diagnostics |
| `SnakeCase` | Naming, Style, Clarity | Detects non-snake_cased identifiers |
| `TodoComment` | Style | Detects remaining TODOs within comments |
| `TrailingComma` | Style | Detects missing trailing commas |
| `VersionStatementFormatted` | Spacing, Style | Detects incorrect formatting of the version statement |
| `Whitespace` | Spacing, Style | Detects undesired whitespace |
