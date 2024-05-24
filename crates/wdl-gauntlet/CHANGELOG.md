# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Changed

* Core goal of crate is changed: **The goal of (new) `wdl-gauntlet` is to ensure the parsing of syntactically valid WDLs never regresses.**
* `LintWarnings` are ignored
* uses `libgit2` (via the `git2` crate) instead of the GitHub REST API (via `octocrab` and `reqwest` crates)
* no more persistent cache (Now uses `temp-dir`)

### Added

* more test repos!
* test repos are tracked at specific commits

## 0.1.0 — 12-17-2023

### Added

* Adds the initial version of the crate.
