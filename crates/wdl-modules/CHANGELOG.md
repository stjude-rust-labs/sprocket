# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

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
