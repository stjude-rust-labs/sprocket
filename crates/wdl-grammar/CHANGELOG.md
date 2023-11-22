# Changelog

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
