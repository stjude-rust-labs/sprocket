# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

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

- Added `analyzer` subcommand to sprocket ([#9](https://github.com/stjude-rust-labs/sprocket/pull/9)).
- Updated dependencies to latest ([#9](https://github.com/stjude-rust-labs/sprocket/pull/9)).

### Changed

- Update to version 0.7.0 of `wdl` crate. This pulls in many new lint rules.

## 0.5.0 - 07-17-2024

### Changed

- Update to version 0.6.0 of `wdl` crate.

## 0.4.0 - 07-01-2024

### Added

- `--except` arg to `check --lint` and `lint` subcommands.

### Changed

- Update to version 0.5.0 of `wdl` crate. This enables lint directive comments (AKA `#@` comments) among other new features.

## 0.3.0 - 06-18-2024

### Added

- `check` subcommand with `--lint` parameter

### Changed

- Update to version 0.4.0 of `wdl` crate. This features a new parser implementation

## 0.2.1 - 06-05-2024

### Fixed

- exit code `2` if there are no parse errors or validation failures, but there are lint warnings.
  - exit code `1` if there are parse errors or validation failures; exit code `0` means there were no concerns found at all.

## 0.2.0 - 06-03-2024

### Added

- `explain` command

### Changed

- Update to version 0.3.0 of `wdl` crate. This pulls in new lint rules.
