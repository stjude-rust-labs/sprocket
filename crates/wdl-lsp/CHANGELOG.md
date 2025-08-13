# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

#### Added

* Added support for semantic highlighting ([#569](https://github.com/stjude-rust-labs/wdl/pull/569)).

#### Added

* Added support for renaming ([#563](https://github.com/stjude-rust-labs/wdl/pull/563)).
* Added support for hover ([#540](https://github.com/stjude-rust-labs/wdl/pull/540)).

## 0.11.0 - 07-31-2025

#### Added

* Added support for completions ([#519](https://github.com/stjude-rust-labs/wdl/pull/519)).
* Added `exceptions` to `ServerOptions` ([#542](https://github.com/stjude-rust-labs/wdl/pull/542)).

## 0.10.0 - 07-09-2025

#### Added

* Added `goto_definition`, `find_all_references` tests ([#489](https://github.com/stjude-rust-labs/wdl/pull/489)).

#### Fixed

* Ensure the server is fully initialized before responding ([#487](https://github.com/stjude-rust-labs/wdl/pull/487)).

## 0.9.0 - 05-27-2025

#### Dependencies

* Bumps dependencies.

## 0.8.2 - 05-05-2025

#### Dependencies

* Bumps dependencies.

## 0.8.1 - 05-02-2025

_A patch bump was required because an error was made during the release of `wdl` v0.13.0 regarding dependencies._

## 0.8.0 - 05-01-2025

#### Dependencies

* Bumps dependencies.

## 0.7.0 - 04-01-2025

#### Changed

* Updated to Rust 2024 edition ([#353](https://github.com/stjude-rust-labs/wdl/pull/353)).

#### Fixed

* Fixed an issue where the LSP was expecting a directory URI instead of file URI ([#362](https://github.com/stjude-rust-labs/wdl/pull/362)).

## 0.6.0 - 01-17-2025

## 0.5.0 - 10-22-2024

#### Added

* Added formatting to the LSP ([#247](https://github.com/stjude-rust-labs/wdl/pull/247)).

#### Fixed

* Fixed an issue on Windows where request URIs were not being normalized ([#231](https://github.com/stjude-rust-labs/wdl/pull/231)).

## 0.4.0 - 10-16-2024

#### Fixed

* Added "fixes" to LSP diagnostics ([#214](https://github.com/stjude-rust-labs/wdl/pull/214)).

## 0.3.0 - 09-16-2024

#### Changed
* Use `tracing` events instead of the `log` crate ([#172](https://github.com/stjude-rust-labs/wdl/pull/172))

## 0.2.0 - 08-22-2024

* bump wdl-* dependency versions

## 0.1.0 - 07-17-2024

#### Added

* Added the `wdl-lsp` crate for implementing an LSP server ([#143](https://github.com/stjude-rust-labs/wdl/pull/143)).

#### Changed

* Removed `Workspace` caching layer ([#146](https://github.com/stjude-rust-labs/wdl/pull/146)).
