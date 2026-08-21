# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

## 0.3.2 - 2026-08-21

#### Added

* Added `GitPlatform`, `TrustedIdentity`, `VerifyLockedReport`, and
  `CacheCleanStats`, along with `GitResolver::discover_default_branch`, to the
  resolver APIs
  ([#999](https://github.com/stjude-rust-labs/sprocket/pull/999)).
* Added `GitSelector::kind` for stable selector-kind labels
  ([#999](https://github.com/stjude-rust-labs/sprocket/pull/999)).
* Added `ModuleProject`, `ModuleProject::validate`,
  `ProjectValidationError`, `ManifestDocument`, `project::LockedLockfile`,
  and `ModuleProject::write_manifest`, plus reusable module-project validation
  and trust-store helpers for accepting lockfile signers
  ([#999](https://github.com/stjude-rust-labs/sprocket/pull/999)).
* Added the `remote` module with `normalize_git_remote`, which converts Git
  remotes and local paths to URLs, and `git_remote_kind` for classifying a
  remote string
  ([#999](https://github.com/stjude-rust-labs/sprocket/pull/999)).
* Added `ModulesConfig::max_transfer_bytes` along with the `TransferLimit` and
  `TransferLimitError` types, which cap the bytes accepted from a Git remote
  during one fetch
  ([#1115](https://github.com/stjude-rust-labs/sprocket/pull/1115)).

#### Changed

* `Manifest` no longer has a `version` field, and `Tool` now uses `url` and
  `ids` instead of `homepage`, `doi`, and `biotools`
  ([#999](https://github.com/stjude-rust-labs/sprocket/pull/999)).
* `DependencyEntry` no longer records a version. Git entries use `sha` instead
  of `commit`, while local path entries no longer carry a checksum or signer
  ([#999](https://github.com/stjude-rust-labs/sprocket/pull/999)).
* `ModuleSignature` now authenticates signer identity in its signed payload and
  exposes validated construction and accessor methods instead of public fields
  ([#999](https://github.com/stjude-rust-labs/sprocket/pull/999)).
* `TrustMode::Auto` is now `TrustMode::AutoAccept` and serializes as
  `"auto-accept"`
  ([#999](https://github.com/stjude-rust-labs/sprocket/pull/999)).
* Git cache reuse now verifies the requested commit and restores modified or
  untracked materialized content before resolution
  ([#999](https://github.com/stjude-rust-labs/sprocket/pull/999)).
* Moved specialized lockfile diff and relock APIs from
  `wdl_modules::resolver` to `wdl_modules::resolver::lock`; primary resolver
  construction and policy types remain available from `wdl_modules::resolver`
  ([#999](https://github.com/stjude-rust-labs/sprocket/pull/999)).
* `ManifestError::InvalidDependencyName` is now a struct variant carrying the
  offending `name` and the underlying `DependencyNameError` as its source
  ([#999](https://github.com/stjude-rust-labs/sprocket/pull/999)).

#### Fixed

* Git remotes reached over SSH now authenticate through `ssh-agent`; the
  credential callback returns only credential types that `libgit2` requested
  ([#1115](https://github.com/stjude-rust-labs/sprocket/pull/1115)).
* A failing Git credential helper now produces a `git2` error with
  `ErrorCode::Auth`, so the resolver reports it as an authentication failure
  ([#1115](https://github.com/stjude-rust-labs/sprocket/pull/1115)).
* A cache leaf whose sparse-checkout metadata cannot be parsed is now evicted
  and re-cloned, and that metadata is written through a temporary file and a
  rename ([#1115](https://github.com/stjude-rust-labs/sprocket/pull/1115)).
* Materialized-tree limit checks read blob sizes from Git object headers and
  fail when an object cannot be read, instead of loading each blob's contents
  and silently skipping unreadable ones
  ([#1115](https://github.com/stjude-rust-labs/sprocket/pull/1115)).

## 0.3.1 - 2026-08-05

## 0.3.0 - 2026-07-15

## 0.2.1 - 2026-06-26

#### Changed

* Moved from `toml` to `toml-spanner` for TOML serialization ([#918](https://github.com/stjude-rust-labs/sprocket/pull/918)).

## 0.2.0 - 2026-06-03

## 0.1.1 - 2026-05-14

#### Added

* Initial implementation of the `wdl-modules` crate, the pure-data-and-algorithms
  layer of the WDL module system. This release covers manifest and lockfile
  parsing, symbolic-path parsing, dependency-source parsing, content hashing
  per [`openwdl/wdl#765`](https://github.com/openwdl/wdl/pull/765), Ed25519
  signing and verification, OpenSSH public-key parsing, SPDX license
  expression validation, and module file-tree validation
  ([#836](https://github.com/stjude-rust-labs/sprocket/pull/836)).
* Add `resolver` feature gate with `Resolver` trait, `GitResolver`
  implementation, on-disk sparse-checkout cache, version selection, lockfile
  generation (`partial_relock`), TOFU trust handling, and module
  materialization for symbolic imports
  ([#838](https://github.com/stjude-rust-labs/sprocket/pull/838)).
* Add `GitModulePath` newtype validating Git sub-paths at parse time;
  `DependencySource::Git { path }` is now `Option<GitModulePath>` instead of
  `Option<PathBuf>`
  ([#838](https://github.com/stjude-rust-labs/sprocket/pull/838)).
* Add `ResolvedSource::Git { path }` field to the lockfile so `partial_relock`
  detects sub-path changes
  ([#838](https://github.com/stjude-rust-labs/sprocket/pull/838)).
* Add `LockfileDiff::compute` recursive walk through nested
  `LockedModule.dependencies` for transitive signer detection
  ([#838](https://github.com/stjude-rust-labs/sprocket/pull/838)).
* Add URL scheme policy (`allowed_schemes`, `allowed_transitive_schemes`) and
  ref-count limit (`max_advertised_refs`) to `ModulesConfig`
  ([#838](https://github.com/stjude-rust-labs/sprocket/pull/838)).

#### Changed

* `partial_relock` now returns `Result<RelockOutcome, ResolverError>` and
  errors when a consumer-declared dependency is absent from the
  freshly-resolved tree
  ([#838](https://github.com/stjude-rust-labs/sprocket/pull/838)).
* `satisfies()` in `partial_relock` forces re-resolution for tag and branch
  selectors (mutable refs) and compares Git sub-paths
  ([#838](https://github.com/stjude-rust-labs/sprocket/pull/838)).
* Structured cache keys include an 8-char URL hash suffix to prevent
  collisions between nested repository URLs
  ([#838](https://github.com/stjude-rust-labs/sprocket/pull/838)).
