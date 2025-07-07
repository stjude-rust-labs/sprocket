# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

#### Added

* `wdl-doc` crate is feature-complete-enough for a beta release :tada: ([#339](https://github.com/stjude-rust-labs/wdl/pull/339)).

## 0.4.0 - 05-27-2025

#### Dependencies

* Bumps dependencies.

## 0.3.2 - 05-05-2025

#### Dependencies

* Bumps dependencies.

## 0.3.1 - 05-02-2025

_A patch bump was required because an error was made during the release of `wdl` v0.13.0 regarding dependencies._

## 0.3.0 - 05-01-2025

#### Dependencies

* Bumps dependencies.

## 0.2.0 - 04-01-2025

#### Added

* Basic CSS styling using TailwindCSS ([#262](https://github.com/stjude-rust-labs/wdl/pull/262)).

#### Changed

* Updated to use new `wdl-ast` API ([#355](https://github.com/stjude-rust-labs/wdl/pull/355)).
* Updated to Rust 2024 edition ([#353](https://github.com/stjude-rust-labs/wdl/pull/353)).
* `wdl-doc` crate is now implemented using a `DocsTree` struct which simplifies
  the API of doc generation ([#262](https://github.com/stjude-rust-labs/wdl/pull/262)).

## 0.1.0 - 01-17-2025

#### Added

* `wdl-doc` crate for documenting WDL codebases ([#258](https://github.com/stjude-rust-labs/wdl/pull/248)).
