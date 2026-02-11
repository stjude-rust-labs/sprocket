# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

## 0.22.0 - 02-11-2026

### Dependencies

* Bumps dependencies.

## 0.21.1 - 01-12-2026

### Dependencies

* Bumps dependencies.

## 0.21.0 - 01-12-2026

### Dependencies

* Bumps dependencies.

## 0.20.0 - 11-21-2025

#### Removed

* Removed the `cli` feature and module ([#450](https://github.com/stjude-rust-labs/sprocket/pull/450)).
* Removed the `codespan` cargo feature in favor of enabling codespan reporting always ([#462](https://github.com/stjude-rust-labs/sprocket/pull/462)).

## 0.19.0 - 10-14-2025

#### Dependencies

* Updated crate dependencies to latest ([#420](https://github.com/stjude-rust-labs/sprocket/pull/420)).

## 0.18.1 - 09-17-2025

#### Dependencies

* Updated `wdl-engine` dependency to latest ([#607](https://github.com/stjude-rust-labs/wdl/pull/607)).

## 0.18.0 - 09-15-2025

#### Dependencies

* Updated Crankshaft dependency to latest ([#593](https://github.com/stjude-rust-labs/wdl/pull/593)).
* Updated dependencies to latest ([#583](https://github.com/stjude-rust-labs/wdl/pull/583)).

## 0.17.0 - 08-13-2025

## 0.16.0 - 07-31-2025

#### Dependencies

* Bumps dependencies.

## 0.15.1 - 07-10-2025

#### Dependencies

* Bumps dependencies.

## 0.15.0 - 07-09-2025

#### Dependencies

* Bumps dependencies.

## 0.14.0 - 05-27-2025

#### Dependencies

* Bumps dependencies.

## 0.13.2 - 05-05-2025

#### Dependencies

* Bumps dependencies.

## 0.13.1 - 05-02-2025

_A patch bump was required because an error was made during the release of `wdl` v0.13.0 regarding dependencies._

## 0.13.0 - 05-01-2025

#### Changed

* Changed the behaviour of `cli` to accept case insensitive `--except` args ([#423](https://github.com/stjude-rust-labs/wdl/pull/423)).
* Removed the `wdl` binary and the `cli` module in favor of `sprocket` and the `wdl-cli` package respectively ([#430](https://github.com/stjude-rust-labs/wdl/pull/430)).

## 0.12.0 - 04-01-2025

#### Added

* Added ability to compile and watch a CSS style directory for `wdl doc` ([#262](https://github.com/stjude-rust-labs/wdl/pull/262)).
* Added ability to skip CSS compilation using a precompiled stylesheet for `wdl doc` ([#262](https://github.com/stjude-rust-labs/wdl/pull/262)).
* Added graceful cancellation on SIGINT ([#327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Added `--config` option to the `run` command ([#327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Added executing task information to the `wdl run` progress bar ([#310](https://github.com/stjude-rust-labs/wdl/pull/310)).

#### Fixed

* Progress bars no longer interleave their output with the rest of tracing
  output ([#320](https://github.com/stjude-rust-labs/wdl/pull/320)).
* Fixed `wdl run` not correctly updating file/directory paths in an inputs file ([#302](https://github.com/stjude-rust-labs/wdl/pull/302)).

#### Changed

* Updated to Rust 2024 edition ([#353](https://github.com/stjude-rust-labs/wdl/pull/353)).
* `wdl` run now prefers running the workflow in a document containing a single
  workflow and a single task ([#310](https://github.com/stjude-rust-labs/wdl/pull/310)).
* Changed the default log level of the `wdl` binary from `error` to `warn` ([#310](https://github.com/stjude-rust-labs/wdl/pull/310)).
* Updates the `crankshaft` and `http-cache-stream-reqwest` dependencies to official, upstreamed crates ([#383](https://github.com/stjude-rust-labs/wdl/pull/383)).

## 0.11.0 - 01-17-2025

#### Added

* Added support for workflow evaluation to `wdl run` ([#292](https://github.com/stjude-rust-labs/wdl/pull/292))
* Add `--shellcheck` flag to `wdl lint` subcommand to run shellcheck when linting ([#264](https://github.com/stjude-rust-labs/wdl/pull/264))
* Implemented the `wdl doc` subcommand for generating documentation (**currently in ALPHA testing**) ([#248](https://github.com/stjude-rust-labs/wdl/pull/248)).
* Added an `--open` flag to `wdl doc` subcommand ([#269](https://github.com/stjude-rust-labs/wdl/pull/269)).
* Added the `engine` module containing the implementation of `wdl-engine` ([#265](https://github.com/stjude-rust-labs/wdl/pull/265)).
* Implemented the `wdl run` subcommand for running tasks ([#265](https://github.com/stjude-rust-labs/wdl/pull/265)).
* Added a `validate` subcommand for validating input JSONs ([#283](https://github.com/stjude-rust-labs/wdl/pull/283)).
* Added `analyze()`, `parse_inputs()`, `validate_inputs()`, and `run()` entrypoints ([#283](https://github.com/stjude-rust-labs/wdl/pull/283)).

#### Fixed

* Fixed accepting directories for the `check` and `analyze` commands for the
  `wdl` binary ([#254](https://github.com/stjude-rust-labs/wdl/pull/254)).

## 0.10.0 - 10-22-2024

#### Changed

* Updated WDL crate dependencies to latest.

## 0.9.1 - 10-16-2024

#### Fixed

* Fixed a bug in `wdl-format` that panicked on certain optional types ([#224](https://github.com/stjude-rust-labs/wdl/pull/224))

## 0.9.0 - 10-16-2024

#### Added

* Added a `format` command to the `wdl` CLI tool ([#133](https://github.com/stjude-rust-labs/wdl/pull/133))
* Added a `verbosity` flag to the `wdl` CLI tool ([#199](https://github.com/stjude-rust-labs/wdl/pull/199)).

## 0.8.0 - 09-16-2024

#### Fixed

* Fixed CLI tool to not output colors when stdio is not a terminal ([#163](https://github.com/stjude-rust-labs/wdl/pull/163)).

#### Changed

* Updated `wdl` crate dependencies.
* Use `tracing-subscriber` to configure tracing env ([#172](https://github.com/stjude-rust-labs/wdl/pull/172))

## 0.7.0 - 08-22-2024

#### Added

* `wdl-lsp`: introduced at v0.1.0 ([release](https://github.com/stjude-rust-labs/wdl/releases/tag/wdl-lsp-v0.1.0))
* Specified the MSRV for the crate ([#144](https://github.com/stjude-rust-labs/wdl/pull/144)).
* Promoted `wdl-analysis` to `wdl::analysis` (available behind the `analysis` feature,
  [#140](https://github.com/stjude-rust-labs/wdl/pull/140)).


## 0.6.0 - 07-17-2024

#### Changed

* Changed the `check` command to perform full analysis of the given path ([#110](https://github.com/stjude-rust-labs/wdl/pull/110)).

#### Added

* Added an `analysis` command to perform full analysis and to also print the
  result of the analysis; currently it just outputs a debug representation of
  the analysis results, but that will change in the future ([#110](https://github.com/stjude-rust-labs/wdl/pull/110)).

## 0.5.0 - 06-28-2024

#### Changed

* Updated `wdl` crate dependencies.

## 0.4.0 - 06-13-2024

#### Changed

* Updated to the new parser implementation and added a `wdl` binary ([#79](https://github.com/stjude-rust-labs/wdl/pull/79)).

## 0.3.0 - 05-31-2024

#### Changed

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
