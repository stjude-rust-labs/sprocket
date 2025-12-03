# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

#### Added

* Added documentation generation placeholder for WDL enumerations in preparation for WDL v1.3 ([#445](https://github.com/stjude-rust-labs/sprocket/pull/445)).

## 0.9.0 - 10-14-2025

#### Added

* Added a toggle for dark/light mode switching ([#367](https://github.com/stjude-rust-labs/sprocket/pull/367)).

#### Changed

* WDL documents with analysis errors **but not parse errors** can now be processed ([#402](https://github.com/stjude-rust-labs/sprocket/pull/402)).
    * prior to this, analysis errors prevented processing

## 0.8.0 - 09-15-2025

#### Changed

* A JavaScript file can be provided that will be read and have its contents embedded in the HTML source of each page ([#591](https://github.com/stjude-rust-labs/wdl/pull/591)).
* The initial left sidebar view is now set to the "Full Directory" view instead of the "Workflows" view and is also now configurable ([#591](https://github.com/stjude-rust-labs/wdl/pull/591)).

## 0.7.0 - 08-13-2025

#### Added

* Added support for ignorefiles, although by default it is not enabled ([#565](https://github.com/stjude-rust-labs/wdl/pull/565)).
* Custom logo support for the top of the left sidebar ([#559](https://github.com/stjude-rust-labs/wdl/pull/559)).

#### Removed

* Removed "smooth" left sidebar scroll animation. Scrolling the current page into view is now instant ([#571](https://github.com/stjude-rust-labs/wdl/pull/571)).

## 0.6.0 - 07-31-2025

#### Changed

* `sprocket run --name` changed to `sprocket run --entrypoint` to match downstream API change ([#550](https://github.com/stjude-rust-labs/wdl/pull/550)).

## 0.5.0 - 07-09-2025

#### Added

* `wdl-doc` crate is feature-complete-enough for a beta release :tada: ([#339](https://github.com/stjude-rust-labs/wdl/pull/339)).

## 0.4.0 - 05-27-2025

#### Dependencies

* Bumps dependencies.

## 0.3.2 - 05-05-2025

#### Dependencies

* Bumps dependencies.

## 0.3.1 - 05-02-2025

_A patch bump was required because an error was made during the release of `wdl` v0.13.0 regarding dependencies._

## 0.3.0 - 05-01-2025

#### Dependencies

* Bumps dependencies.

## 0.2.0 - 04-01-2025

#### Added

* Basic CSS styling using TailwindCSS ([#262](https://github.com/stjude-rust-labs/wdl/pull/262)).

#### Changed

* Updated to use new `wdl-ast` API ([#355](https://github.com/stjude-rust-labs/wdl/pull/355)).
* Updated to Rust 2024 edition ([#353](https://github.com/stjude-rust-labs/wdl/pull/353)).
* `wdl-doc` crate is now implemented using a `DocsTree` struct which simplifies
  the API of doc generation ([#262](https://github.com/stjude-rust-labs/wdl/pull/262)).

## 0.1.0 - 01-17-2025

#### Added

* `wdl-doc` crate for documenting WDL codebases ([#258](https://github.com/stjude-rust-labs/wdl/pull/248)).
