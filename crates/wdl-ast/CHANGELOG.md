# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

* Add support for the `Directory` type in WDL 1.2 ([#124](https://github.com/stjude-rust-labs/wdl/pull/124)).
* Add support for multi-line strings in WDL 1.2 ([#123](https://github.com/stjude-rust-labs/wdl/pull/123)).
* Add support for `hints` sections in WDL 1.2 ([#121](https://github.com/stjude-rust-labs/wdl/pull/121)).
* Add support for `requirements` sections in WDL 1.2 ([#117](https://github.com/stjude-rust-labs/wdl/pull/117)).
* Add support for the exponentiation operator in WDL 1.2 ([#111](https://github.com/stjude-rust-labs/wdl/pull/111)).

### Changed

* Changed the API for parsing documents; `Document::parse` now returns
  `(Document, Vec<Diagnostic>)` rather than a `Parse` type ([#110](https://github.com/stjude-rust-labs/wdl/pull/110)).
* The `Type` enumeration, and friends, in `wdl-ast` no longer implement
  `PartialOrd`  and `Ord`; those implementations have moved to the sort lint
  rule ([#110](https://github.com/stjude-rust-labs/wdl/pull/110)).
* The `PartialEq` implementation of the `Type` enumeration, and friends, is now
  implemented in terms of WDL type equivalence and not by CST node equivalence
  ([#110](https://github.com/stjude-rust-labs/wdl/pull/110)).

## 0.4.0 - 06-28-2024

### Added

* Added a method to `ImportStatement` for deriving the namespace from the
  import URI ([#91](https://github.com/stjude-rust-labs/wdl/pull/91)).
* Added validation of unique names, such as task, struct, and declarations
  ([#91](https://github.com/stjude-rust-labs/wdl/pull/91)).

### Fixed

* Fixed the validation diagnostics to be ordered by the start of the primary
  label ([#85](https://github.com/stjude-rust-labs/wdl/pull/85)).

### Changed

* Refactored the `Visitor` trait and validation visitors so that they are not
  in a `v1` module ([#95](https://github.com/stjude-rust-labs/wdl/pull/95)).

## 0.3.0 - 06-13-2024

### Fixed

* Fixed the experimental parser validation to check negative numbers in
  metadata sections ([#66](https://github.com/stjude-rust-labs/wdl/pull/66)).

### Added

* Added `parent` method to section representations in the experimental AST
  ([#70](https://github.com/stjude-rust-labs/wdl/pull/70)).
* Added validation rules for the experimental AST ([#65](https://github.com/stjude-rust-labs/wdl/pull/65)).
* Added a new experimental AST for the experimental parser; this implementation
  is currently feature-gated behind the `experimental` feature ([#64](https://github.com/stjude-rust-labs/wdl/pull/64)).

### Changed

* Removed the old AST implementation in favor of new new parser; this also
  removes the `experimental` feature from the crate ([#79](https://github.com/stjude-rust-labs/wdl/pull/79)).
* Removed dependency on `miette` and `thiserror` in the experimental parser,
  re-exported key items from `wdl-grammar`'s experimental parser implementation,
  and changed errors to use `Diagnostic` ([#68](https://github.com/stjude-rust-labs/wdl/pull/68)).

## 0.2.0 - 5-31-2024

* Fix ignoring comments in expressions ([#23](https://github.com/stjude-rust-labs/wdl/pull/23)).

### Changed

* Conform to definition of body as outlined in #12 (#62, contributed by @a-frantz)
* Changes the singular `Group` feature of lint warnings to one or more `Tags` (#37, contributed by @a-frantz)

## 0.1.0 — 12-17-2023

### Added

* Adds the initial version of the crate.
