# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

## 0.7.0 - 04-01-2025

### Changed

* Updated to Rust 2024 edition ([#353](https://github.com/stjude-rust-labs/wdl/pull/353)).

### Fixed

* Fixed an issue where the LSP was expecting a directory URI instead of file URI ([#362](https://github.com/stjude-rust-labs/wdl/pull/362)).

## 0.6.0 - 01-17-2025

## 0.5.0 - 10-22-2024

### Added

* Added formatting to the LSP ([#247](https://github.com/stjude-rust-labs/wdl/pull/247)).

### Fixed

* Fixed an issue on Windows where request URIs were not being normalized ([#231](https://github.com/stjude-rust-labs/wdl/pull/231)).

## 0.4.0 - 10-16-2024

### Fixed

* Added "fixes" to LSP diagnostics ([#214](https://github.com/stjude-rust-labs/wdl/pull/214)).

## 0.3.0 - 09-16-2024

### Changed
* Use `tracing` events instead of the `log` crate ([#172](https://github.com/stjude-rust-labs/wdl/pull/172))

## 0.2.0 - 08-22-2024

* bump wdl-* dependency versions

## 0.1.0 - 07-17-2024

### Added

* Added the `wdl-lsp` crate for implementing an LSP server ([#143](https://github.com/stjude-rust-labs/wdl/pull/143)).

### Changed

* Removed `Workspace` caching layer ([#146](https://github.com/stjude-rust-labs/wdl/pull/146)).
