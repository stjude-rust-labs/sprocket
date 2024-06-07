# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

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
