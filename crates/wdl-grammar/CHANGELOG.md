# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Fixed

* Fixed parsing of placeholder options in the experimental parser such
  that it can disambiguate between the `sep` option and a `sep` function
  call ([#44](https://github.com/stjude-rust-labs/wdl/pull/44)).

### Added

* Adds support for workflow statements in the experimental parser
  ([#49](https://github.com/stjude-rust-labs/wdl/pull/49)).
* Adds support for runtime sections in the experimental parser
  ([#48](https://github.com/stjude-rust-labs/wdl/pull/48)).
* Adds support for command sections in the experimental parser
  ([#47](https://github.com/stjude-rust-labs/wdl/pull/47)).
* Adds support for input and output sections in the experimental
  parser ([#46](https://github.com/stjude-rust-labs/wdl/pull/46)).
* Adds support for import statements to the experimental parser ([#43](https://github.com/stjude-rust-labs/wdl/pull/43)).
* Adds support for bound declarations and expressions in the experimental
  parser ([#42](https://github.com/stjude-rust-labs/wdl/pull/42)).
* Adds support for parsing `meta` and `parameter_meta` sections in tasks
  and workflows in the experimental parser ([#39](https://github.com/stjude-rust-labs/wdl/pull/39)).
* Adds support for parsing struct definitions to the experimental parser;
  requires the `experimental` feature to be activated ([#38](https://github.com/stjude-rust-labs/wdl/pull/38)).
* Adds a new experimental `SyntaxTree` representation; requires the 
  `experimental` feature to be activated ([#36](https://github.com/stjude-rust-labs/wdl/pull/36)).
* Adds an `experimental` module containing the start of a new
  infallible WDL parser implementation based on `logos` and `rowan` ([#30](https://github.com/stjude-rust-labs/wdl/pull/30)).
* Adds the `missing_runtime_block` rule for tasks (#10, contributed by
  @markjschreiber).
* Adds the `snake_case` rule that ensures all tasks, workflows, and variables
  are snakecase (#13, contributed by @simojoe).
* Adds the `newline_eof` rule for tasks (#18, contributed by @simojoe).
* Adds the `preamble_comment` rule for preamble comments formatting (#32,
  contributed by @simojoe).
* Adds the `one_empty_line` rule that ensures no excess of empty lines
  (#33, contributed by @simojoe).

### Changed

* Changes the singular `Group` feature of lint warnings to one or more `Tags` (#37, contributed by @a-frantz)
* Changes the tags and levels of various rules to better align with #12 (#37, contributed by @a-frantz)

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
