# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

* Added functions for getting type information of task requirements and hints (#[241](https://github.com/stjude-rust-labs/wdl/pull/241)).
* Exposed information about workflow calls from an analyzed document (#[239](https://github.com/stjude-rust-labs/wdl/pull/239)).

## 0.5.0 - 10-22-2024

### Changed

* Refactored the `DocumentScope` API to simply `Document` and exposed more
  information about tasks and workflows such as their inputs and outputs (#[232](https://github.com/stjude-rust-labs/wdl/pull/232)).
* Switched to `rustls-tls` for TLS implementation rather than relying on
  OpenSSL for Linux builds (#[228](https://github.com/stjude-rust-labs/wdl/pull/228)).

## 0.4.0 - 10-16-2024

### Added

* Implemented `UnusedImport`, `UnusedInput`, `UnusedDeclaration`, and
  `UnusedCall` analysis warnings ([#211](https://github.com/stjude-rust-labs/wdl/pull/211))
* Implemented static analysis for workflows ([#199](https://github.com/stjude-rust-labs/wdl/pull/199)).

### Fixed

* Allow coercion of `Array[T]` to `Array[T]+` unless from an empty array
  literal ([#213](https://github.com/stjude-rust-labs/wdl/pull/213)).
* Improved type calculations in function calls and when determining common
  types in certain expressions ([#209](https://github.com/stjude-rust-labs/wdl/pull/209)).
* Treat a coercion to `T?` for a function argument of type `T` as a preference
  over any other coercion ([#199](https://github.com/stjude-rust-labs/wdl/pull/199)).
* Fix the signature of `select_first` such that it is monomorphic ([#199](https://github.com/stjude-rust-labs/wdl/pull/199)).
* Only consider signatures in overload resolution that have sufficient
  arguments ([#199](https://github.com/stjude-rust-labs/wdl/pull/199)).
* Allow coercion from `File` and `Directory` to `String` ([#199](https://github.com/stjude-rust-labs/wdl/pull/199)).
* Allow non-empty array literals to coerce to either empty or non-empty ([#199](https://github.com/stjude-rust-labs/wdl/pull/199)).
* Fix element type calculations for `Array` and `Map` so that `[a, b]` and
  `{"a": a, "b": b }` successfully calculates when `a` is coercible to `b` ([#199](https://github.com/stjude-rust-labs/wdl/pull/199)).
* Fix `if` expression type calculation such that `if (x) then a else b` works
  when `a` is coercible to `b` ([#199](https://github.com/stjude-rust-labs/wdl/pull/199)).
* Ensure that only equality/inequality expressions are supported on `File` and
  `Directory` now that there is a coercion to `String` ([#199](https://github.com/stjude-rust-labs/wdl/pull/199)).
* Allow index expressions on `Map` ([#199](https://github.com/stjude-rust-labs/wdl/pull/199)).

## 0.3.0 - 09-16-2024

### Added

* Implemented type checking in task runtime, requirements, and hints sections
  ([#170](https://github.com/stjude-rust-labs/wdl/pull/170)).
* Add support for the `task` variable in WDL 1.2 ([#168](https://github.com/stjude-rust-labs/wdl/pull/168)).
* Full type checking support in task definitions ([#163](https://github.com/stjude-rust-labs/wdl/pull/163)).

### Changed

* Use `tracing` events instead of the `log` crate ([#172](https://github.com/stjude-rust-labs/wdl/pull/172))
* Refactored crate layout ([#163](https://github.com/stjude-rust-labs/wdl/pull/163)).

### Fixed

* Fixed definition of `basename` and `size` functions to accept `String` ([#163](https://github.com/stjude-rust-labs/wdl/pull/163)).

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
