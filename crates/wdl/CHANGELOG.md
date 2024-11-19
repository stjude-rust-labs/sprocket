# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

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
