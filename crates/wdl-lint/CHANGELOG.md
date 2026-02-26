# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

#### Added

* `allowed_names` configuration key for allowing certain names in the SnakeCase and
  DeclarationName rules ([#660](https://github.com/stjude-rust-labs/sprocket/pull/660)).

#### Changed

* Renamed `LintDirectiveValid` to `ExceptDirectiveValid` ([#614](https://github.com/stjude-rust-labs/sprocket/pull/614)).

#### Removed

* Removed some "formatting only" lint rules (CommentWhitespace, EndingNewline, ImportWhitespace, LintDirectiveFormatted, PreambleCommentPlacement, PreambleFormatted, VersionStatementFormatted, Whitespace) ([#614](https://github.com/stjude-rust-labs/sprocket/pull/614)).
* Removed `util::is_inline_comment()` and `util::strip_newline()` ([#614](https://github.com/stjude-rust-labs/sprocket/pull/614)).

## 0.20.1 - 2026-02-12

### Dependencies

* Bumps dependencies.

## 0.20.0 - 02-11-2026

#### Added

* Added `Config` for configuring the behavior of certain lint rules ([#553](https://github.com/stjude-rust-labs/sprocket/pull/553))

#### Fixed

* Fixed `LineWidth` rule incorrectly emitting diagnostics for import statements ([#501](https://github.com/stjude-rust-labs/sprocket/pull/501)).
* Fixed `ShellCheck` diagnostic spans for command sections with leading empty lines ([#545](https://github.com/stjude-rust-labs/sprocket/pull/545)).
* Allow excepting specific runtime items with `#@ except: ExpectedRuntimeKeys`
  ([#563](https://github.com/stjude-rust-labs/sprocket/pull/563)).

## 0.19.0 - 01-12-2026

## 0.18.0 - 11-21-2025

#### Removed

* Removed the `codespan` cargo feature in favor of enabling codespan reporting always ([#462](https://github.com/stjude-rust-labs/sprocket/pull/462)).

## 0.17.0 - 10-14-2025

#### Added

* New lint rule `DocMetaStrings` to ensure reserved meta and parameter_meta keys have string values ([#407](https://github.com/stjude-rust-labs/sprocket/pull/407)).
* New `Tag::SprocketCompatibility` ([#351](https://github.com/stjude-rust-labs/sprocket/pull/351)).
* New lint rule `DescriptionLength` ([#351](https://github.com/stjude-rust-labs/sprocket/pull/351)).
* New lint rule `CallInputKeyword` ([#401](https://github.com/stjude-rust-labs/sprocket/pull/401)).

## 0.16.0 - 09-15-2025

#### Added

* New `Tag::Documentation` ([#592](https://github.com/stjude-rust-labs/wdl/pull/592)).

#### Fixed

* The `LineWidth` lint rule now ignores import statements ([#590](https://github.com/stjude-rust-labs/wdl/pull/590)).

#### Changed

* Some lint rules had their `tags()` modified ([#592](https://github.com/stjude-rust-labs/wdl/pull/592)).
* `TagSet::new()` now allows empty TagSets to be created ([#592](https://github.com/stjude-rust-labs/wdl/pull/592)).
* `TagSet::new()` no longer implicitly adds `Tag::Style` to sets including `Tag::Naming` or `Tag::Spacing` ([#592](https://github.com/stjude-rust-labs/wdl/pull/592)).

## 0.15.0 - 08-13-2025

## 0.14.0 - 07-31-2025

#### Fixed

* Updated shellcheck logic that erroneously flagged placeholders that are quoted ([#541](https://github.com/stjude-rust-labs/wdl/pull/541)).

#### Removed

* Removed the `OutputSection` lint rule ([#532](https://github.com/stjude-rust-labs/wdl/pull/532)).

## 0.13.0 - 07-09-2025

#### Changed

* `ShellCheck` now has additional logic to suppress erroneous warnings for globbing and word splitting ([#457](https://github.com/stjude-rust-labs/wdl/pull/457)).

## 0.12.0 - 05-27-2025

#### Added

* Added `RedundantNone` rule ([#444](https://github.com/stjude-rust-labs/wdl/pull/444)).

#### Dependencies

* Bumps dependencies.

## 0.11.2 - 05-05-2025

#### Dependencies

* Bumps dependencies.

## 0.11.1 - 05-02-2025

_A patch bump was required because an error was made during the release of `wdl` v0.13.0 regarding dependencies._

## 0.11.0 - 05-01-2025

#### Changed

* `util::is_properly_quoted` is now `util::is_quote_balanced` ([#341](https://github.com/stjude-rust-labs/wdl/pull/341)).
* `ShellCheck` is now based on type analysis and is no longer in "beta" ([#341](https://github.com/stjude-rust-labs/wdl/pull/341)).
* `ShellCheck` has been made part of the default lint rule set ([#341](https://github.com/stjude-rust-labs/wdl/pull/341)).
    * This removes the `optional_rule()` function.
* Linting is now based off an analyzed document instead of just a parsed AST ([#341](https://github.com/stjude-rust-labs/wdl/pull/341)).
    * This removes the `LintVisitor` struct.

#### Added

* Added `serialize_oxford_comma()` to the `util` module ([#341](https://github.com/stjude-rust-labs/wdl/pull/341)).
* Added a `Linter` struct which lints analyzed documents ([#341](https://github.com/stjude-rust-labs/wdl/pull/341)).
* made `find_nearest_rule()` pub ([#412](https://github.com/stjude-rust-labs/wdl/pull/412)).

#### Changed

* Renamed lint rules to be more consistent ([#408](https://github.com/stjude-rust-labs/wdl/pull/408)).

#### Fixed

* Downgraded `PreambleCommentPlacement` severity from `error` to `note` ([#418](https://github.com/stjude-rust-labs/wdl/pull/418)).

## 0.10.0 - 04-01-2025

#### Added

* Added suggestion for similar rule names when encountering unknown lint rules ([#334](https://github.com/stjude-rust-labs/wdl/pull/334)).
* Added `DisallowedDeclarationName` rule ([#343](https://github.com/stjude-rust-labs/wdl/pull/343)).
* Added `DEFINITIONS.md` file with centralized documentation for WDL concepts ([#195](https://github.com/stjude-rust-labs/wdl/pull/195)).
* Added `Rule::related_rules()` for linking related lint rules ([#371](https://github.com/stjude-rust-labs/wdl/pull/371)).
* Added `TryFrom` for Tags to convert strings to Tag enums ([#374](https://github.com/stjude-rust-labs/wdl/pull/374)).

#### Changed

* Added `InputSectionNode` and `OutputSectionNode` to `SnakeCase` `exceptable_nodes()` ([#343](https://github.com/stjude-rust-labs/wdl/pull/343)).
* Updated to use new `wdl-ast` API ([#355](https://github.com/stjude-rust-labs/wdl/pull/355)).
* Updated to Rust 2024 edition ([#353](https://github.com/stjude-rust-labs/wdl/pull/353)).
* Relaxed `CommentWhitespace` rule so that it doesn't fire when a comment has extra spaces before it ([#314](https://github.com/stjude-rust-labs/wdl/pull/314)).
* `fix` messages suggest the correct order of imports to the user in `ImportSort` rule ([#332](https://github.com/stjude-rust-labs/wdl/pull/332)).
* Updated `SectionOrdering` to support ordering of `struct` definitions ([#367](https://github.com/stjude-rust-labs/wdl/pull/367)
* Replaced `TryFrom` with `FromStr` for Tags ([#376](https://github.com/stjude-rust-labs/wdl/pull/376)).

#### Fixed

* Fixed the `MatchingParameterMeta` rule to also check if the order of inputs matches parameter metadata ([#354](https://github.com/stjude-rust-labs/wdl/pull/354))
* Fixed misplacement of highlighted spans for some ShellCheck lints ([#317](https://github.com/stjude-rust-labs/wdl/pull/317)).

## 0.9.0 - 01-17-2025

#### Added

* Improved `ShellCheck` rule fix messages and implemented the `fix` module ([#284](https://github.com/stjude-rust-labs/wdl/pull/284))
* Added a `ShellCheck` rule ([#264](https://github.com/stjude-rust-labs/wdl/pull/264)).
* Added a `RedundantInputAssignment` rule ([#244](https://github.com/stjude-rust-labs/wdl/pull/244)).

#### Changed

* Upgraded some `note` diagnostics to `warning` in `ContainerValue` rule  ([#244](https://github.com/stjude-rust-labs/wdl/pull/244)).

#### Fixed

* Shortened many reported spans and ensured all lint diagnostics use a `fix` message ([#260](https://github.com/stjude-rust-labs/wdl/pull/260)).
* `BlankLinesBetweenElements` logic was tweaked to prevent firing a redundant message with `VersionFormatting` rule ([#260](https://github.com/stjude-rust-labs/wdl/pull/260)).

## 0.8.0 - 10-22-2024

#### Fixed

* Fixed tests to run on Windows ([#231](https://github.com/stjude-rust-labs/wdl/pull/231)).

## 0.7.0 - 10-16-2024

#### Changed

* Change how some rules report whitespace spans ([#206](https://github.com/stjude-rust-labs/wdl/pull/206)).
* Cover a missing case in `BlankLinesBetweenElements` ([#206](https://github.com/stjude-rust-labs/wdl/pull/206)).
* Don't redundantly report the same issue from different rules or checks ([#206](https://github.com/stjude-rust-labs/wdl/pull/206)).
* `PreambleComments` and `PreambleWhitespace` have been refactored into 3 rules: `PreambleFormatting`, `VersionFormatting`, and `PreambleCommentAfterVersion` ([#187](https://github.com/stjude-rust-labs/wdl/pull/187)).
* test files have been cleaned up ([#187](https://github.com/stjude-rust-labs/wdl/pull/187)).
* Some `warning` diagnostics are now `note` diagnostics ([#187](https://github.com/stjude-rust-labs/wdl/pull/187)).

#### Added

* Added comments to the trailing whitespace check of the `Whitespace` rule ([#177](https://github.com/stjude-rust-labs/wdl/pull/177)).
* Added a `MalformedLintDirective` rule ([#194](https://github.com/stjude-rust-labs/wdl/pull/194)).

#### Fixed

* Fixed inline comment detection edge case ([#219](https://github.com/stjude-rust-labs/wdl/pull/219)).

## 0.6.0 - 09-16-2024

#### Fixed

* Lint directives finally work :tada: ([#162](https://github.com/stjude-rust-labs/wdl/pull/162)).
* Updated iter method in lines_with_offset util function to apply new clippy lint ([#172](https://github.com/stjude-rust-labs/wdl/pull/172)).

## 0.5.0 - 08-22-2024

#### Added

* Specified the MSRV for the crate ([#144](https://github.com/stjude-rust-labs/wdl/pull/144)).
* Added the `CommentWhitespace` lint rule ([#136](https://github.com/stjude-rust-labs/wdl/pull/136)).
* Added the `TrailingComma` lint rule ([#137](https://github.com/stjude-rust-labs/wdl/pull/137)).
* Added the `KeyValuePairs` lint rule ([#141](https://github.com/stjude-rust-labs/wdl/pull/141)).
* Added the `ExpressionSpacing` lint rule ([#134](https://github.com/stjude-rust-labs/wdl/pull/134))
* Added the `DisallowedInputName` and `DisallowedOutputName` lint rules ([#148](https://github.com/stjude-rust-labs/wdl/pull/148)).
* Added the `ContainerValue` lint rule ([#142](https://github.com/stjude-rust-labs/wdl/pull/142)).
* Added the `MissingRequirements` lint rule ([#142](https://github.com/stjude-rust-labs/wdl/pull/142)).

#### Fixed

* Fixed `LintVisitor` to support reuse ([#147](https://github.com/stjude-rust-labs/wdl/pull/147)).
* Fixed a bug in `MissingRuntime` that caused false positives to fire for WDL v1.2 and
  greater ([#142](https://github.com/stjude-rust-labs/wdl/pull/142)).

## 0.4.0 - 07-17-2024

#### Added

* Added the `SectionOrdering` lint rule ([#109](https://github.com/stjude-rust-labs/wdl/pull/109)).
* Added the `DeprecatedObject` lint rule ([#112](https://github.com/stjude-rust-labs/wdl/pull/112)).
* Added the `DescriptionMissing` lint rule ([#113](https://github.com/stjude-rust-labs/wdl/pull/113)).
* Added the `NonmatchingOutput` lint rule ([#114](https://github.com/stjude-rust-labs/wdl/pull/114)).
* Added the `DeprecatedPlaceholderOption` lint rule ([#120](https://github.com/stjude-rust-labs/wdl/pull/120)).
* Added the `RuntimeSectionKeys` lint rule ([#120](https://github.com/stjude-rust-labs/wdl/pull/120)).
* Added the `Todo` lint rule ([#120](https://github.com/stjude-rust-labs/wdl/pull/126)).
* Added the `BlankLinesBetweenElements` lint rule ([#131](https://github.com/stjude-rust-labs/wdl/pull/131)).

#### Fixed

* Fixed a bug in `SectionOrder` that caused false positives to fire
  ([#129](https://github.com/stjude-rust-labs/wdl/pull/129))
* Fixed a bug in the `PreambleWhitespace` rule that would cause it to fire if
  there is only a single blank line after the version statement remaining in
  the document ([#110](https://github.com/stjude-rust-labs/wdl/pull/110)).

#### Changed

* All lint rule visitations now reset their states upon document entry,
  allowing a validator to be reused between documents ([#110](https://github.com/stjude-rust-labs/wdl/pull/110)).
* Moved the `PartialOrd` implementation for types into the `InputSorting` rule.

## 0.3.0 - 06-28-2024

#### Added

* Added the `InconsistentNewlines` lint rule ([#104](https://github.com/stjude-rust-labs/wdl/pull/104)).
* Add support for `#@ except` comments to disable lint rules ([#101](https://github.com/stjude-rust-labs/wdl/pull/101)).
* Added the `LineWidth` lint rule (#99).
* Added the `ImportWhitespace` and `ImportSort` lint rules (#98).
* Added the `MissingMetas` and `MissingOutput` lint rules (#96).
* Added the `PascalCase` lint rule ([#90](https://github.com/stjude-rust-labs/wdl/pull/90)).
* Added the `ImportPlacement` lint rule ([#89](https://github.com/stjude-rust-labs/wdl/pull/89)).
* Added the `InputNotSorted` lint rule ([#100](https://github.com/stjude-rust-labs/wdl/pull/100)).
* Added the `InputSpacing` lint rule ([#102](https://github.com/stjude-rust-labs/wdl/pull/102)).

#### Fixed

* Fixed the preamble whitespace rule to check for a blank line following the
  version statement ([#89](https://github.com/stjude-rust-labs/wdl/pull/89)).
* Fixed the preamble whitespace and preamble comment rules to look for the
  version statement trivia based on it now being children of the version
  statement ([#85](https://github.com/stjude-rust-labs/wdl/pull/85)).

#### Changed

* Refactored the lint rules so that they directly implement `Visitor`; renamed
  `ExceptVisitor` to `LintVisitor` ([#103](https://github.com/stjude-rust-labs/wdl/pull/103)).
* Refactored the lint rules so that they are not in a `v1` module
  ([#95](https://github.com/stjude-rust-labs/wdl/pull/95)).

## 0.1.0 - 06-13-2024

#### Added

* Ported the `CommandSectionMixedIndentation` rule to `wdl-lint` ([#75](https://github.com/stjude-rust-labs/wdl/pull/75))
* Ported the `Whitespace` rule to `wdl-lint` ([#74](https://github.com/stjude-rust-labs/wdl/pull/74))
* Ported the `MatchingParameterMeta` rule to `wdl-lint` ([#73](https://github.com/stjude-rust-labs/wdl/pull/73))
* Ported the `PreambleWhitespace` and `PreambleComments` rules to `wdl-lint`
  ([#72](https://github.com/stjude-rust-labs/wdl/pull/72))
* Ported the `SnakeCase` rule to `wdl-lint` ([#71](https://github.com/stjude-rust-labs/wdl/pull/71)).
* Ported the `NoCurlyCommands` rule to `wdl-lint` ([#69](https://github.com/stjude-rust-labs/wdl/pull/69)).
* Added the `wdl-lint` as the crate implementing linting rules for the future
  ([#68](https://github.com/stjude-rust-labs/wdl/pull/68)).

