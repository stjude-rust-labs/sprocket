# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

* Adds the `missing_runtime_block` rule for tasks (#10, contributed by
  @markjschreiber).

## 0.2.0 - 12-17-2023

### Added

* Adds lint to suggest replacing curly command blocks (4ee030f).
* Adds the `Pedantic` lint group (bc17014).

### Revisions

* Multiple revisions to the inner data model to support the introduction of the
  `wdl-ast` crate (e2436ce).
* Clarifies that whitespace is undesired and not invalid (457c383).
* Increases visibility of `lint` and `validation` modules (59543c3).
* Adds a location, a body, and a fix suggestion to warnings (335afaf).
* Applies `s/message/subject/g` for lint rules (6dce4a2).

### Chores

* Normalizes crate description (f19ce7e).
* Updates formatting to `version = "Two"` (f63c188).
* Moves `tokio` to the workspace dependencies (66da811).
* Specifies the `dep:` prefix for the binary feature dependencies (e0b2cb5).
* Improves the binary crate documentation (a995a89).

## 0.1.0 — 11-22-2023

### Added

* Adds initial version of parsing WDL 1.x grammar.
* Adds `wdl-grammar` tool, a tool that is useful in creating and exhausitvely
  testing the `wdl-grammar` crate.
    * The following subcommands are included in the initial release:
        * `wdl-grammar create-test`: scaffolds otherwise arduous Rust tests that
        ensure a provided input and grammar rule are generated into the correct
        Pest parse tree.
        * `wdl-grammar gauntlet`: an exhaustive testing framework for ensuring
        `wdl-grammar` can parse a wide variety of grammars in the community.
        * `wdl-grammar parse`: prints the Pest parse tree for a given input and
        grammar rule or outputs errors regarding why the input could not be
        parsed.
    * This command line tool is available behind the `binaries` feature flag and
      is not intended to be used by a general audience. It is only intended for
      developers of the `wdl-grammar` crate.
