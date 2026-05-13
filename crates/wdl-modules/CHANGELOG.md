# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

#### Added

* Initial implementation of the `wdl-modules` crate, the pure-data-and-algorithms
  layer of the WDL module system. This release covers manifest and lockfile
  parsing, symbolic-path parsing, dependency-source parsing, content hashing
  per [`openwdl/wdl#765`](https://github.com/openwdl/wdl/pull/765), Ed25519
  signing and verification, OpenSSH public-key parsing, SPDX license
  expression validation, and module file-tree validation.
