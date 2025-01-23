# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

* Added executing task information to the `wdl run` progress bar ([#310](https://github.com/stjude-rust-labs/wdl/pull/310)).

### Changed

* `wdl` run now prefers running the workflow in a document containing a single
  workflow and a single task ([#310](https://github.com/stjude-rust-labs/wdl/pull/310)).
* Changed the default log level of the `wdl` binary from `error` to `warn` ([#310](https://github.com/stjude-rust-labs/wdl/pull/310)).

### Fixed

* Fixed `wdl run` not correctly updating file/directory paths in an inputs file ([#302](https://github.com/stjude-rust-labs/wdl/pull/302)).

## 0.11.0 - 01-17-2025

### Added

* Added support for workflow evaluation to `wdl run` ([#292](https://github.com/stjude-rust-labs/wdl/pull/292))
* Add `--shellcheck` flag to `wdl lint` subcommand to run shellcheck when linting ([#264](https://github.com/stjude-rust-labs/wdl/pull/264))
* Implemented the `wdl doc` subcommand for generating documentation (**currently in ALPHA testing**) ([#248](https://github.com/stjude-rust-labs/wdl/pull/248)).
* Added an `--open` flag to `wdl doc` subcommand ([#269](https://github.com/stjude-rust-labs/wdl/pull/269)).
* Added the `engine` module containing the implementation of `wdl-engine` ([#265](https://github.com/stjude-rust-labs/wdl/pull/265)).
* Implemented the `wdl run` subcommand for running tasks ([#265](https://github.com/stjude-rust-labs/wdl/pull/265)).
* Added a `validate` subcommand for validating input JSONs ([#283](https://github.com/stjude-rust-labs/wdl/pull/283)).
* Added `analyze()`, `parse_inputs()`, `validate_inputs()`, and `run()` entrypoints ([#283](https://github.com/stjude-rust-labs/wdl/pull/283)).

### Fixed

* Fixed accepting directories for the `check` and `analyze` commands for the
  `wdl` binary ([#254](https://github.com/stjude-rust-labs/wdl/pull/254)).

## 0.10.0 - 10-22-2024

### Changed

* Updated WDL crate dependencies to latest.

## 0.9.1 - 10-16-2024

### Fixed

* Fixed a bug in `wdl-format` that panicked on certain optional types ([#224](https://github.com/stjude-rust-labs/wdl/pull/224))

## 0.9.0 - 10-16-2024

### Added

* Added a `format` command to the `wdl` CLI tool ([#133](https://github.com/stjude-rust-labs/wdl/pull/133))
* Added a `verbosity` flag to the `wdl` CLI tool ([#199](https://github.com/stjude-rust-labs/wdl/pull/199)).

## 0.8.0 - 09-16-2024

### Fixed

* Fixed CLI tool to not output colors when stdio is not a terminal ([#163](https://github.com/stjude-rust-labs/wdl/pull/163)).

### Changed

* Updated `wdl` crate dependencies.
* Use `tracing-subscriber` to configure tracing env ([#172](https://github.com/stjude-rust-labs/wdl/pull/172))

## 0.7.0 - 08-22-2024

### Added

* `wdl-lsp`: introduced at v0.1.0 ([release](https://github.com/stjude-rust-labs/wdl/releases/tag/wdl-lsp-v0.1.0))
* Specified the MSRV for the crate ([#144](https://github.com/stjude-rust-labs/wdl/pull/144)).
* Promoted `wdl-analysis` to `wdl::analysis` (available behind the `analysis` feature,
  [#140](https://github.com/stjude-rust-labs/wdl/pull/140)).


## 0.6.0 - 07-17-2024

### Changed

* Changed the `check` command to perform full analysis of the given path ([#110](https://github.com/stjude-rust-labs/wdl/pull/110)).

### Added

* Added an `analysis` command to perform full analysis and to also print the
  result of the analysis; currently it just outputs a debug representation of
  the analysis results, but that will change in the future ([#110](https://github.com/stjude-rust-labs/wdl/pull/110)).

## 0.5.0 - 06-28-2024

### Changed

* Updated `wdl` crate dependencies.

## 0.4.0 - 06-13-2024

### Changed

* Updated to the new parser implementation and added a `wdl` binary ([#79](https://github.com/stjude-rust-labs/wdl/pull/79)).

## 0.3.0 - 05-31-2024

### Changed

* Updated `wdl` crate dependencies.

## 0.2.0 — 12-17-2023

### Crate Updates

* `wdl-ast`: introduced at v0.1.0 ([release](https://github.com/stjude-rust-labs/wdl/releases/tag/wdl-ast-v0.1.0))
* `wdl-core`: introduced at v0.1.0 ([release](https://github.com/stjude-rust-labs/wdl/releases/tag/wdl-core-v0.1.0))
* `wdl-gauntlet`: introduced at v0.1.0 ([release](https://github.com/stjude-rust-labs/wdl/releases/tag/wdl-gauntlet-v0.1.0))
* `wdl-grammar`: bumped from v0.1.0 to v0.2.0 ([release](https://github.com/stjude-rust-labs/wdl/releases/tag/wdl-grammar-v0.2.0))
* `wdl-macros`: introduced at v0.1.0 ([release](https://github.com/stjude-rust-labs/wdl/releases/tag/wdl-macros-v0.1.0))

## 0.1.0 — 11-22-2023

## Crate Updates

* `wdl-grammar`: introduced at v0.1.0 ([release](https://github.com/stjude-rust-labs/wdl/releases/tag/wdl-grammar-v0.1.0))
