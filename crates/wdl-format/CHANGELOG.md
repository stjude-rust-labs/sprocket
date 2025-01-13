# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

* Leading whitespace in command text is now normalized ([#240](https://github.com/stjude-rust-labs/wdl/pull/240)).

### Fixed

* Multi-line placeholders in command blocks are now indented appropriately ([#240](https://github.com/stjude-rust-labs/wdl/pull/240)).
* Issue [#289](https://github.com/stjude-rust-labs/wdl/issues/289) (extraneous end line in literal structs)
  is fixed ([#290](https://github.com/stjude-rust-labs/wdl/pull/290))

## 0.3.0 - 10-22-2024

### Fixed

* Fix panic on multiline strings in WDL 1.2 ([#227](https://github.com/stjude-rust-labs/wdl/pull/227)).

## 0.2.1 - 10-16-2024

### Fixed

* Don't panic on certain optional types ([#224](https://github.com/stjude-rust-labs/wdl/pull/224))

## 0.2.0 - 10-16-2024

### Added

* Adds the initial version of the crate.
