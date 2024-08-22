# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

## 0.7.0 - 08-22-2024

### Added

* Specified the MSRV for the crate ([#144](https://github.com/stjude-rust-labs/wdl/pull/144)).
* Add facilities for diving on `SyntaxNode`s ([#138](https://github.com/stjude-rust-labs/wdl/pull/138)).

### Fixed

* Improved recovery around missing closing braces/brackets/parens ([#161](https://github.com/stjude-rust-labs/wdl/pull/161)).
* Fixed the display of the `in` keyword in the CST ([#143](https://github.com/stjude-rust-labs/wdl/pull/143)).

## 0.6.0 - 07-17-2024

### Added

* Add support for `meta` and `parameter_meta` sections in struct definitions in
  WDL 1.2 ([#127](https://github.com/stjude-rust-labs/wdl/pull/127)).
* Add support for omitting `input` keyword in call statement bodies in WDL 1.2
  ([#125](https://github.com/stjude-rust-labs/wdl/pull/125)).
* Add support for the `Directory` type in WDL 1.2 ([#124](https://github.com/stjude-rust-labs/wdl/pull/124)).
* Add support for multi-line strings in WDL 1.2 ([#123](https://github.com/stjude-rust-labs/wdl/pull/123)).
* Add support for `hints` sections in WDL 1.2 ([#121](https://github.com/stjude-rust-labs/wdl/pull/121)).
* Add support for `requirements` sections in WDL 1.2 ([#117](https://github.com/stjude-rust-labs/wdl/pull/117)).
* Add support for the exponentiation operator in WDL 1.2 ([#111](https://github.com/stjude-rust-labs/wdl/pull/111)).

### Fixed

* Made the call target grammar rule more permissible in accepting more than two
  identifiers; this will still be treated as an error when resolving call
  statements ([#118](https://github.com/stjude-rust-labs/wdl/pull/118)).
* The diagnostic for missing a version statement in an empty file now points to
  the last position in the file so that the file that caused the error is
  attached to the diagnostic ([#110](https://github.com/stjude-rust-labs/wdl/pull/110)).

## 0.5.0 - 06-28-2024

### Fixed

* Fixed parsing of workflow conditional statements to require parenthesis
  surrounding the expression ([#94](https://github.com/stjude-rust-labs/wdl/pull/94)).
* Fixed attaching trivia to the CST before starting a node ([#91](https://github.com/stjude-rust-labs/wdl/pull/91)).
* Fixed the CST for an unparsable file (i.e. one without a supported version)
  to contain trivia before the version statement and correct spans for the
  unparsed token ([#89](https://github.com/stjude-rust-labs/wdl/pull/89)).
* Fixed last trivia in the file attaching as a child to the last node in the
  file instead of as a child of the root ([#89](https://github.com/stjude-rust-labs/wdl/pull/89)).
* Fixed diagnostics around encountering string member names in struct literals
  ([#87](https://github.com/stjude-rust-labs/wdl/pull/87)).
* Fixed diagnostic label spans that point at strings to include the entire span
  of the string ([#86](https://github.com/stjude-rust-labs/wdl/pull/86)).
* Fixed trivia in the CST so that it appears at consistent locations; also
  fixed the parser diagnostics to be ordered by the start of the primary label
  ([#85](https://github.com/stjude-rust-labs/wdl/pull/85)).
* Fixed a missing delimiter diagnostic to include a label for where the parser
  thinks the missing delimiter might go ([#84](https://github.com/stjude-rust-labs/wdl/pull/84)).

## 0.4.0 - 6-13-2024

### Changed

* Removed the old parser implementation in favor of the new parser
  implementation; this also removes the `experimental` feature from the crate ([#79](https://github.com/stjude-rust-labs/wdl/pull/79)).
* Removed dependency on `miette` and `thiserror` in the experimental parser,
  introduced the `Diagnostic` type as a replacement, and switched the existing
  parser errors over to use `Diagnostic` ([#68](https://github.com/stjude-rust-labs/wdl/pull/68)).

## 0.3.0 - 5-31-2024

### Fixed

* Fixed the experimental parser to correctly lookahead to disambiguate struct
  literals ([#63](https://github.com/stjude-rust-labs/wdl/pull/63)).
* Fixed the experimental parser to skip parsing if it cannot find a supported
  version statement ([#59](https://github.com/stjude-rust-labs/wdl/pull/59))
* Fixed handling of `None` literal values in expressions in the experimental
  parser ([#58](https://github.com/stjude-rust-labs/wdl/pull/58)).
* Fixed the experimental parser to accept multiple placeholder options
  ([#57](https://github.com/stjude-rust-labs/wdl/pull/57)).
* Fixed recovery in the experimental parser to move past interpolations in
  strings and commands ([#56](https://github.com/stjude-rust-labs/wdl/pull/56)).
* Fixed parsing of reserved identifiers and recovery in metadata sections
  in the experimental parser ([#52](https://github.com/stjude-rust-labs/wdl/pull/52)).
* Fixed parsing of empty inputs to a task call statement in the
  experimental parser ([#54](https://github.com/stjude-rust-labs/wdl/pull/54)).
* Fixed parsing of postfix `+` qualifier on array types in the experimental
  parser ([#53](https://github.com/stjude-rust-labs/wdl/pull/53)).
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
* Adds the `document_preamble` rule for documents (#25, contributed by @adthrasher)
* Adds the `preamble_comment` rule for preamble comments formatting (#32,
  contributed by @simojoe).
* Adds the `one_empty_line` rule that ensures no excess of empty lines
  (#33, contributed by @simojoe).
* Adds the `double_quotes` rule for quote styling in string declarations
  (contributed by @simojoe).

### Changed

* Conform to definition of body as outlined in #12 (#62, contributed by @a-frantz)
* Changes the `preamble_comment` rule so that continuous blocks of comments are reported.
  Also permits triple+ pound sign comments outside of the preamble. (#55, contributed by @a-frantz)
* Changes the `snake_case` rule so that lowercase letters can be adjacent to digits without triggering
  a warning (#55, contributed by @a-frantz)
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
