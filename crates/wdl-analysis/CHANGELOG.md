# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

## 0.2.0 - 08-22-2024

### Added

* Implemented type checking of struct definitions ([#160](https://github.com/stjude-rust-labs/wdl/pull/160)).
* Implemented a type system and representation of the WDL standard library for
  future type checking support ([#156](https://github.com/stjude-rust-labs/wdl/pull/156)).
* Specified the MSRV for the crate ([#144](https://github.com/stjude-rust-labs/wdl/pull/144)).

### Changed

* Refactored `Analyzer` API to support change notifications ([#146](https://github.com/stjude-rust-labs/wdl/pull/146)).
* Replaced `AnalysisEngine` with `Analyzer` ([#143](https://github.com/stjude-rust-labs/wdl/pull/143)).

## 0.1.0 - 07-17-2024

### Added

* Added the `wdl-analysis` crate for analyzing WDL documents ([#110](https://github.com/stjude-rust-labs/wdl/pull/110)).
