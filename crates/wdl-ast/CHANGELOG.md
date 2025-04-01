# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

## 0.11.0 - 04-01-2025

### Changed

* Refactored AST API to support different syntax tree element representations ([#355](https://github.com/stjude-rust-labs/wdl/pull/355)).
* Updated to Rust 2024 edition ([#353](https://github.com/stjude-rust-labs/wdl/pull/353)).
* Refactored whitespace counting out of `strip_whitespace` into `count_whitespace` method ([#317](https://github.com/stjude-rust-labs/wdl/pull/317)).

### Fixed

* AST validation now checks for duplicate `hints` sections in 1.2 documents ([#355](https://github.com/stjude-rust-labs/wdl/pull/355)).

## 0.10.0 - 01-17-2025

### Added

* Added AST support for the WDL 1.2 `env` declaration modifier ([#296](https://github.com/stjude-rust-labs/wdl/pull/296)).
* Added `braced_scope_span` and `heredoc_scope_span` methods to `AstNodeExt` ([#292](https://github.com/stjude-rust-labs/wdl/pull/292))
* Added constants for the task variable fields, task requirement names, and
  task hint names ([#265](https://github.com/stjude-rust-labs/wdl/pull/265)).
* Added `allows_nested_inputs` function to `Workflow` (#[241](https://github.com/stjude-rust-labs/wdl/pull/241)).
* `strip_whitespace()` method to `LiteralString` and `CommandSection` AST nodes ([#238](https://github.com/stjude-rust-labs/wdl/pull/238)).

### Changed

* Reduced allocations in stripping whitespace from commands and multiline
  strings and provided unescaping of escape sequences ([#265](https://github.com/stjude-rust-labs/wdl/pull/265)).

### Fixed

* Fixed a bug in `strip_whitespace` that left a trailing carriage return at the
  end of commands and multiline strings when using Windows line endings ([#291](https://github.com/stjude-rust-labs/wdl/pull/291)).
* Fixed bug in `strip_whitespace()` that erroneously stripped characters from the first line when it had content.
  Closed [issue #268](https://github.com/stjude-rust-labs/wdl/issues/268) ([#271](https://github.com/stjude-rust-labs/wdl/pull/271)).
* Fixed same #268 bug in mutliline strings as well as command sections  ([#272](https://github.com/stjude-rust-labs/wdl/pull/272)).

## 0.9.0 - 10-22-2024

### Changed

* Refactored the AST token struct definitions to use macros ([#233](https://github.com/stjude-rust-labs/wdl/pull/233)).

## 0.8.0 - 10-16-2024

### Changed

* Introduce a guarantee that each CST element (node or token) has one and only one analogous AST element ([#133](https://github.com/stjude-rust-labs/wdl/pull/133))

### Fixed

* Detect duplicate call inputs ([#199](https://github.com/stjude-rust-labs/wdl/pull/199)).
* Split hint section representation into `TaskHintsSection` and
  `WorkflowHintsSection` as workflow hints [do not support expressions](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#workflow-hints) ([#176](https://github.com/stjude-rust-labs/wdl/pull/176))


## 0.7.1 - 09-16-2024

### Fixed

* updated to latest wdl-grammar (v0.8.0)

## 0.7.0 - 09-16-2024

### Added

* moved "except comment" logic from `wdl-lint` into `wdl-ast`.
  This is for future support of disabling certain diagnostics such as "unused import" and the like.
  ([#162](https://github.com/stjude-rust-labs/wdl/pull/162))

### Changed

* Removed `span_of` function in favor of `AstNode` extension trait ([#163](https://github.com/stjude-rust-labs/wdl/pull/163)).

### Fixed

* Fixed detection of duplicate aliased keys in a task `hints` section ([#170](https://github.com/stjude-rust-labs/wdl/pull/170)).
* Fixed ignoring duplicate task definitions for the "counts" validation ([#170](https://github.com/stjude-rust-labs/wdl/pull/170)).

## 0.6.0 - 08-22-2024

### Added

* Specified the MSRV for the crate ([#144](https://github.com/stjude-rust-labs/wdl/pull/144)).
* Add `as_*()` and `into_*()` methods for each enum item in `Expr` and `LiteralExpr`
  ([#142](https://github.com/stjude-rust-labs/wdl/pull/142)).
* Add parsing of `container` elements within `runtime` and `requirements` blocks
  according to the [current version of the WDL
  specification](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#container)
  ([#142](https://github.com/stjude-rust-labs/wdl/pull/142)).

### Fixed

* Added validation to ensure there is at most one placeholder option on a
  placeholder ([#159](https://github.com/stjude-rust-labs/wdl/pull/159)).
* Moved validation of import statements to `wdl-ast` ([#158](https://github.com/stjude-rust-labs/wdl/pull/158)).

### Changed

* Section methods on `TaskDefinition` and `WorkflowDefinition` now return
  `Option` instead of iterator. ([#157](https://github.com/stjude-rust-labs/wdl/pull/157)).

## 0.5.0 - 07-17-2024

### Added

* Add support for `meta` and `parameter_meta` sections in struct definitions in
  WDL 1.2 ([#127](https://github.com/stjude-rust-labs/wdl/pull/127)).
* Add support for omitting `input` keyword in call statement bodies in WDL 1.2
  ([#125](https://github.com/stjude-rust-labs/wdl/pull/125)).
* Add support for the `Directory` type in WDL 1.2 ([#124](https://github.com/stjude-rust-labs/wdl/pull/124)).
* Add support for multi-line strings in WDL 1.2 ([#123](https://github.com/stjude-rust-labs/wdl/pull/123)).
* Add support for `hints` sections in WDL 1.2 ([#121](https://github.com/stjude-rust-labs/wdl/pull/121)).
* Add support for `requirements` sections in WDL 1.2 ([#117](https://github.com/stjude-rust-labs/wdl/pull/117)).
* Add support for the exponentiation operator in WDL 1.2 ([#111](https://github.com/stjude-rust-labs/wdl/pull/111)).

### Changed

* Removed `Send` and `Sync` constraints from the `Visitor` trait
  ([#128](https://github.com/stjude-rust-labs/wdl/pull/128)).
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
