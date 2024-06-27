# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

* Added the `InconsistentNewlines` line rule ([#104](https://github.com/stjude-rust-labs/wdl/pull/104)).
* Add support for `#@ except` comments to disable lint rules ([#101](https://github.com/stjude-rust-labs/wdl/pull/101)).
* Added the `LineWidth` lint rule (#99).
* Added the `ImportWhitespace` and `ImportSort` lint rules (#98).
* Added the `MissingMetas` and `MissingOutput` lint rules (#96).
* Added the `PascalCase` lint rule ([#90](https://github.com/stjude-rust-labs/wdl/pull/90)).
* Added the `ImportPlacement` lint rule ([#89](https://github.com/stjude-rust-labs/wdl/pull/89)).
* Added the `InputNotSorted` lint rule ([#100](https://github.com/stjude-rust-labs/wdl/pull/100)).

### Fixed

* Fixed the preamble whitespace rule to check for a blank line following the
  version statement ([#89](https://github.com/stjude-rust-labs/wdl/pull/89)).
* Fixed the preamble whitespace and preamble comment rules to look for the 
  version statement trivia based on it now being children of the version 
  statement ([#85](https://github.com/stjude-rust-labs/wdl/pull/85)).

### Changed

### Changed

* Refactored the lint rules so that they are not in a `v1` module
  ([#95](https://github.com/stjude-rust-labs/wdl/pull/95)).

## 0.1.0 - 06-13-2024

### Added

* Ported the `CommandSectionMixedIndentation` rule to `wdl-lint` ([#75](https://github.com/stjude-rust-labs/wdl/pull/75))
* Ported the `Whitespace` rule to `wdl-lint` ([#74](https://github.com/stjude-rust-labs/wdl/pull/74))
* Ported the `MatchingParameterMeta` rule to `wdl-lint` ([#73](https://github.com/stjude-rust-labs/wdl/pull/73))
* Ported the `PreambleWhitespace` and `PreambleComments` rules to `wdl-lint`
  ([#72](https://github.com/stjude-rust-labs/wdl/pull/72))
* Ported the `SnakeCase` rule to `wdl-lint` ([#71](https://github.com/stjude-rust-labs/wdl/pull/71)).
* Ported the `NoCurlyCommands` rule to `wdl-lint` ([#69](https://github.com/stjude-rust-labs/wdl/pull/69)).
* Added the `wdl-lint` as the crate implementing linting rules for the future
  ([#68](https://github.com/stjude-rust-labs/wdl/pull/68)).
