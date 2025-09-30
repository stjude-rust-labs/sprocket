# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

## 0.6.1 - 09-17-2025

#### Dependencies

* Updated `wdl-engine` dependency to latest ([#607](https://github.com/stjude-rust-labs/wdl/pull/607)).

## 0.6.0 - 09-15-2025

#### Changed

* `Analysis` had its `lint()` method replaced with `enabled_lint_tags()` and `disabled_lint_tags()` ([#592](https://github.com/stjude-rust-labs/wdl/pull/592)).

#### Fixed

* Command-line parsing of key-value pairs can now accept any special characters other than square brackets or curly braces ([#596](https://github.com/stjude-rust-labs/wdl/pull/596)).

## 0.5.0 - 08-13-2025

#### Added

* Added support for ignorefiles, although by default it is not enabled ([#565](https://github.com/stjude-rust-labs/wdl/pull/565)).

## 0.4.0 - 07-31-2025

#### Added

* Inputs on the CL can have the name of the called task or workflow specified and then ommitted from individual input pairs ([#535](https://github.com/stjude-rust-labs/wdl/pull/535)).

## 0.3.1 - 07-10-2025

#### Fixed

* Use absolute path for the origin of inputs read in from files ([#523](https://github.com/stjude-rust-labs/wdl/pull/523)).

## 0.3.0 - 07-09-2025

#### Changed

* Source now has a default implementation ([#476](https://github.com/stjude-rust-labs/wdl/pull/476)).

## 0.2.0 - 05-27-2025

#### Dependencies

* Bumps dependencies.

## 0.1.2 - 05-05-2025

#### Fixed

* JSON and YAML files are now correctly parsed ([#440](https://github.com/stjude-rust-labs/wdl/pull/440)).
* Removes the unused `shellcheck` option in `wdl::cli::Analysis` ([#441](https://github.com/stjude-rust-labs/wdl/pull/441)).

## 0.1.1 - 05-02-2025

_A patch bump was required because an error was made during the release of `wdl` v0.13.0 regarding dependencies._

## 0.1.0 - 05-01-2025

#### Added

* Adds the initial version of the crate.
