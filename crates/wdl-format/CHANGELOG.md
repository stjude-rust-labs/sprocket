# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

#### Added

* Added logic for tandem line breaks on matching tokens (e.g. open and close brackets) ([#641](https://github.com/stjude-rust-labs/sprocket/pull/641)).
* Added configuration for input section formatting (off by default) ([#640](https://github.com/stjude-rust-labs/sprocket/pull/640)).
* Added configuration for trailing commas (on by default) ([#665](https://github.com/stjude-rust-labs/sprocket/pull/665)).
* `#@ except:` comment normalization and consolidation of doc comment blocks ([#614](https://github.com/stjude-rust-labs/sprocket/pull/614)).

## 0.15.1 - 2026-02-12

#### Dependencies

- Bumps dependencies.

## 0.15.0 - 02-11-2026

#### Added

- Split long `#@ except:` lint directives across multiple lines when they exceed the configured maximum line length.

### Fixed

- Correctly format `ConditionalStatement` with `else if` and `else` ([#617](https://github.com/stjude-rust-labs/sprocket/pull/617)).

## 0.14.0 - 01-12-2026

#### Added

- Added sorting of `#@ except` directive rule names ([#505](https://github.com/stjude-rust-labs/sprocket/pull/505)).

## 0.13.0 - 11-21-2025

#### Added

- Added formatting support for WDL enumerations in preparation for WDL v1.3 ([#445](https://github.com/stjude-rust-labs/sprocket/pull/445)).
- Added support for `else if` and `else` clauses in conditional statements (in support of WDL v1.3) ([#411](https://github.com/stjude-rust-labs/sprocket/pull/411)).

## 0.12.0 - 10-14-2025

#### Changed

- Always format call statement `input`s across multiple lines instead of trying to put single inputs on the same line as the `call` ([#377](https://github.com/stjude-rust-labs/sprocket/pull/377/)).

#### Fixed

- Trailing comments at the end of documents are now captured and output correctly ([#413](https://github.com/stjude-rust-labs/sprocket/pull/413)).
- Fixed edge case in single quote to double quote conversion for literal strings ([#365](https://github.com/stjude-rust-labs/sprocket/pull/365)).
- `MaxLineLength::try_new()` and `IndentationSize::try_new()` return appropriate errors ([#365](https://github.com/stjude-rust-labs/sprocket/pull/365)).

## 0.11.0 - 09-15-2025

- Added support for sorting input sections ([#597](https://github.com/stjude-rust-labs/wdl/pull/597)).

## 0.10.0 - 08-13-2025

## 0.9.1 - 07-31-2025

#### Dependencies

- Bumps dependencies.

## 0.9.0 - 07-30-2025

Mistaken release, please use `0.9.1`

## 0.8.0 - 07-09-2025

#### Added

- Added panic documentation to functions which may panic ([#498](https://github.com/stjude-rust-labs/wdl/pull/498)).
- Added documentation to places which needed more clarity ([#498](https://github.com/stjude-rust-labs/wdl/pull/498)).

#### Changed

- Renamed some methods of `TokenStream<PreToken>` for increased clarity ([#498](https://github.com/stjude-rust-labs/wdl/pull/498)).

#### Removed

- Removed the `exactly_one!` macro ([#498](https://github.com/stjude-rust-labs/wdl/pull/498)).

## 0.7.0 - 05-27-2025

#### Dependencies

- Bumps dependencies.

## 0.6.1 - 05-02-2025

_A patch bump was required because an error was made during the release of `wdl` v0.13.0 regarding dependencies._

## 0.6.0 - 05-01-2025

#### Dependencies

- Bumps dependencies.

## 0.5.0 - 04-01-2025

#### Changed

- Updated to use new `wdl-ast` API ([#355](https://github.com/stjude-rust-labs/wdl/pull/355)).
- Updated to Rust 2024 edition ([#353](https://github.com/stjude-rust-labs/wdl/pull/353)).

## 0.4.0 - 01-17-2025

#### Added

- Leading whitespace in command text is now normalized ([#240](https://github.com/stjude-rust-labs/wdl/pull/240)).
- Line breaks are now added in order to keep lines under the max line width (default 90 characters) ([#242](https://github.com/stjude-rust-labs/wdl/pull/242)).

#### Fixed

- Multi-line placeholders in command blocks are now indented appropriately ([#240](https://github.com/stjude-rust-labs/wdl/pull/240)).
- Issue [#289](https://github.com/stjude-rust-labs/wdl/issues/289) (extraneous end line in literal structs)
  is fixed ([#290](https://github.com/stjude-rust-labs/wdl/pull/290))

## 0.3.0 - 10-22-2024

#### Fixed

- Fix panic on multiline strings in WDL 1.2 ([#227](https://github.com/stjude-rust-labs/wdl/pull/227)).

## 0.2.1 - 10-16-2024

#### Fixed

- Don't panic on certain optional types ([#224](https://github.com/stjude-rust-labs/wdl/pull/224))

## 0.2.0 - 10-16-2024

#### Added

- Adds the initial version of the crate.
