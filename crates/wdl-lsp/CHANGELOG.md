# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Changed
* Use `tracing` events instead of the `log` crate ([#172](https://github.com/stjude-rust-labs/wdl/pull/172))

## 0.2.0 - 08-22-2024

* bump wdl-* dependency versions

## 0.1.0 - 07-17-2024

### Added

* Added the `wdl-lsp` crate for implementing an LSP server ([#143](https://github.com/stjude-rust-labs/wdl/pull/143)).

### Changed

* Removed `Workspace` caching layer ([#146](https://github.com/stjude-rust-labs/wdl/pull/146)).
