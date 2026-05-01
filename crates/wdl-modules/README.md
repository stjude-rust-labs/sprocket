<p align="center">
  <h1 align="center">
    <code>wdl-modules</code>
  </h1>

  <p align="center">
    Manifest, lockfile, content hashing, and signing for the WDL module system.
    <br />
    <a href="https://docs.rs/wdl-modules"><strong>Explore the docs »</strong></a>
    <br />
  </p>
</p>

## Overview

`wdl-modules` is the pure data-and-algorithms layer of the WDL module system
defined in [`openwdl/wdl#765`](https://github.com/openwdl/wdl/pull/765).
It is the natural seam in the dependency graph for tools that consume the
module specification: every type and algorithm defined here is `no-network`
and `no-process-spawning`, so it is safe to embed in offline analysis tools
like `wdl-analysis` and `wdl-doc`.

The crate covers:

- `Manifest` — parsed `module.json` with strict JSON enforcement
  (duplicate-key rejection, no trailing commas/comments/BOM).
- `Lockfile` — parsed `module-lock.json`, recursive shape with stable
  `BTreeMap`-ordered round-trip.
- `SymbolicPath`, `VersionRequirement`, `DependencySource`, `DependencyName`
  — strongly-typed parsing of the manifest's value types.
- `Hasher` and `ContentHash` — deterministic SHA-256 content hashing per
  the spec, with `path-clean` lexical normalization, NFC path
  canonicalization, and post-NFC duplicate detection.
- `SigningKey`, `VerifyingKey`, `Signature`, `ModuleSignature` — Ed25519
  signing and verification, with OpenSSH public-key parsing for
  interoperability with `ssh-keygen -t ed25519`.
- `LicenseExpression` — SPDX license expression validation against the
  full SPDX license list.
- `validate_tree` — module file-tree structural validation
  (reserved-filename placement, NFC uniqueness).

## Features

- `test-utils` — exposes `signing::test_utils::signing_key_from_seed` for
  cross-crate tests. Production builds and library consumers do not see
  this surface.

## License

This project is licensed as either of:

* Apache License, Version 2.0 (LICENSE-APACHE or
  https://www.apache.org/licenses/LICENSE-2.0)
* MIT license (LICENSE-MIT or https://opensource.org/licenses/MIT)

at your option.

Copyright © 2026-Present The Sprocket project contributors.
