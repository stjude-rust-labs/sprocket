# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

* Added tab completions for `sprocket` commands ([#105](https://github.com/stjude-rust-labs/sprocket/pull/105)).
* Added `shellcheck` to Dockerfile ([#114](https://github.com/stjude-rust-labs/sprocket/pull/114)).

## 0.12.2 - 05-05-2025

### Fixed

* Fix `sprocket run` not printing analysis diagnostics ([#110](https://github.com/stjude-rust-labs/sprocket/pull/110)).

## 0.12.1 - 05-05-2025

### Fixed

* Fixes parsing of input files ([#106](https://github.com/stjude-rust-labs/sprocket/pull/106)).
* Removes unused `--shellcheck` argument ([#106](https://github.com/stjude-rust-labs/sprocket/pull/106)).

## 0.12.0 - 05-02-2025

### Added

* Introduced the `run` subcommand ([#102](https://github.com/stjude-rust-labs/sprocket/pull/102)).

### Changed

* Unknown `--except` rules will now emit a warning instead of being silently ignored ([#94](https://github.com/stjude-rust-labs/sprocket/pull/94))
* Changed the `validate-inputs` subcommand to the more concise `validate` subcommand ([#102](https://github.com/stjude-rust-labs/sprocket/pull/102)).
* Changed all existing subcommands to use the facilities provided in `wdl-cli` when possible ([#102](https://github.com/stjude-rust-labs/sprocket/pull/102)).
* Updates the underlying `wdl` dependency to v0.13.1 ([#102](https://github.com/stjude-rust-labs/sprocket/pull/102)).


### Added

* Added configuration file support ([#104](https://github.com/stjude-rust-labs/sprocket/pull/104)).

## 0.11.0 - 04-01-2025

### Added

* Added `--hide_notes` to `check` to filter out note diagnostics from reporting ([#84](https://github.com/stjude-rust-labs/sprocket/pull/84))
* YAML support for `validate-inputs` command ([#79](https://github.com/stjude-rust-labs/sprocket/pull/79)).
* Extend `explain` to display related rules, list tags using `--t`, show WDL definitions using `--definitions` ([#80](https://github.com/stjude-rust-labs/sprocket/pull/80)).

### Changed

* Updated WDL crates to latest ([#79](https://github.com/stjude-rust-labs/sprocket/pull/79)). This added many features and fixes. Some highlights:
    * Fixed certain misplaced highlights from the `ShellCheck` lint.
    * Relaxed the `CommentWhitespace` lint rule so it doesn't trigger for as many comments.
    * The `ImportSort` lint rule now supplies the correct order of imports in the `fix` message.
* By default, when checking a local file, suppress diagnostics from remote files. Added a `--show-remote-diagnostics`
  flag to recreate the older behavior ([#59](https://github.com/stjude-rust-labs/sprocket/pull/59)).
* Always emit any diagnostics with a `Severity::Error` regardless of other CL options that might suppress the diagnostic
  ([#59](https://github.com/stjude-rust-labs/sprocket/pull/59)).

### Fixed

* Bug introduced in [#59](https://github.com/stjude-rust-labs/sprocket/pull/59) which sometimes caused the exit message
  to have an incorrect count of Notes and Warnings ([#61](https://github.com/stjude-rust-labs/sprocket/pull/61)).

## 0.10.1 - 01-23-2025

### Fixed

* URLs can be checked/linted ([#58](https://github.com/stjude-rust-labs/sprocket/pull/58)). 

### Added

* Added a `Dockerfile` and automation to release Docker images with each Sprocket version ([#56](https://github.com/stjude-rust-labs/sprocket/pull/56)).

## 0.10.0 - 01-17-2025

### Added

* Added `--local-only` and `--single-document` args to `check/lint` ([#48](https://github.com/stjude-rust-labs/sprocket/pull/48)).
* Added a `validate-inputs` command. ([#48](https://github.com/stjude-rust-labs/sprocket/pull/48)).

### Changed

* `format` now requires one of the `--check` or `--overwrite` arguments ([#51](https://github.com/stjude-rust-labs/sprocket/pull/51)).
* Updated WDL crate to latest. This adds support for
  checking/linting remote URLs and other features and improvements ([#48](https://github.com/stjude-rust-labs/sprocket/pull/48)).
* Only allow one `file` argument to `check/lint` instead of any number of local files and directories
  ([#48](https://github.com/stjude-rust-labs/sprocket/pull/48)).

## 0.9.0 - 10-22-2024

### Changed

* Updated WDL crate to latest; this includes some important fixes to using
  `sprocket` on Windows and Linux ([#35](https://github.com/stjude-rust-labs/sprocket/pull/35)).

## 0.8.0 - 10-16-2024

### Added

* Added the `format` subcommand to sprocket ([#24](https://github.com/stjude-rust-labs/sprocket/pull/24)).
* Added the analysis rules to `sprocket explain` ([#24](https://github.com/stjude-rust-labs/sprocket/pull/24)).

### Changed

* Update to version 0.9.0 of `wdl` crate; this pulls in new lint rules,
  formatting support, and completes static analysis for the `check` and `lint`
  subcommands ([#24](https://github.com/stjude-rust-labs/sprocket/pull/24)).

## 0.7.0 - 09-16-2024

### Changed

* Implemented the `check` command as a full static analysis ([#17](https://github.com/stjude-rust-labs/sprocket/pull/17)).

### Fixed

* Fixed the progress bar from showing up for short analysis jobs; it now is
  delayed by two seconds ([#19](https://github.com/stjude-rust-labs/sprocket/pull/19)).

## 0.6.0 - 08-22-2024

### Added

* Added `analyzer` subcommand to sprocket ([#9](https://github.com/stjude-rust-labs/sprocket/pull/9)).
* Updated dependencies to latest ([#9](https://github.com/stjude-rust-labs/sprocket/pull/9)).

### Changed

* Update to version 0.7.0 of `wdl` crate. This pulls in many new lint rules.

## 0.5.0 - 07-17-2024

### Changed

* Update to version 0.6.0 of `wdl` crate.

## 0.4.0 - 07-01-2024

### Added

* `--except` arg to `check --lint` and `lint` subcommands.

### Changed

* Update to version 0.5.0 of `wdl` crate. This enables lint directive comments (AKA `#@` comments) among other new features.

## 0.3.0 - 06-18-2024

### Added

* `check` subcommand with `--lint` parameter

### Changed

* Update to version 0.4.0 of `wdl` crate. This features a new parser implementation

## 0.2.1 - 06-05-2024

### Fixed

* exit code `2` if there are no parse errors or validation failures, but there are lint warnings.
  * exit code `1` if there are parse errors or validation failures; exit code `0` means there were no concerns found at all.

## 0.2.0 - 06-03-2024

### Added

* `explain` command

### Changed

* Update to version 0.3.0 of `wdl` crate. This pulls in new lint rules.
