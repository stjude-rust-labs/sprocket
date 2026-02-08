# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

* Added reading configuration from a `sprocket.toml` next to the sprocket
  executable ([#588](https://github.com/stjude-rust-labs/sprocket/pull/588)).
* Added `sprocket dev server` command for running an HTTP API server for
  workflow execution ([#540](https://github.com/stjude-rust-labs/sprocket/pull/540)).
* Added SQLite-backed database layer for tracking sessions, runs, and tasks
  ([#540](https://github.com/stjude-rust-labs/sprocket/pull/540)).
* Added index system for organizing run outputs via symlinks
  ([#540](https://github.com/stjude-rust-labs/sprocket/pull/540)).
* Added setting `check.rules` to `sprocket.toml` for controlling `wdl-lint`
  rule configuration ([#553](https://github.com/stjude-rust-labs/sprocket/pull/553)).
* Added the `--with-doc-comments` CLI option to `sprocket dev doc` to enable
  support for the experimental [documentation comments](https://sprocket.bio/subcommands/doc.html#documentation-comments)
  feature. ([#551](https://github.com/stjude-rust-labs/sprocket/pull/551))

### Changed

* The values for the `common.report_mode` setting in `sprocket.toml` have
  changed to lower kebab-case, e.g. `full` and `one-line` ([#607](https://github.com/stjude-rust-labs/sprocket/pull/607)).
* The `common.color` setting in `sprocket.toml` has been changed from being a
  boolean to an enum with values `auto` (default), `always`, and `never`. ([#607](https://github.com/stjude-rust-labs/sprocket/pull/607)).
* Replaced the `--no-color` option for a global `--color` option to control
  output colorization and made the uncolorized output consistent ([#607](https://github.com/stjude-rust-labs/sprocket/pull/607)).
* Renamed `--entrypoint` to `--target` in `sprocket run` and `sprocket validate`
  ([#540](https://github.com/stjude-rust-labs/sprocket/pull/540)).
* `sprocket dev doc` will now **fail** in the presence of analysis errors
  that would produce invalid documentation (e.g. `enum`s in versions < WDL v1.3) ([#559](https://github.com/stjude-rust-labs/sprocket/pull/559)).

### Fixed

* Fixed a race condition where canceled workflows would be marked as `Failed`
  instead of `Canceled` ([#601](https://github.com/stjude-rust-labs/sprocket/pull/601)).

### Dependencies

* Dependencies updated to latest ([#594](https://github.com/stjude-rust-labs/sprocket/pull/594)).

## 0.20.1 - 01-12-2026

### Dependencies

* Bumps dependencies.

## 0.20.0 - 01-12-2026

### Changed

* WDL v1.3 is now enabled by default; the `wdl_1_3` feature flag is deprecated
  and will emit a warning if explicitly disabled
  ([#534](https://github.com/stjude-rust-labs/sprocket/pull/534)).

### Added

* Added setting `run.tasks.digests` to `sprocket.toml` for controlling content
  digests; supported values are `strong` for full cryptographic hashing of file
  content and `weak` to digest based solely off file metadata. The default is
  now `weak` ([#503](https://github.com/stjude-rust-labs/sprocket/pull/503)).
* Added setting `run.events_capacity` to `sprocket.toml` for controlling the
  size of the events channel buffer that Sprocket uses for displaying progress
  statistics ([#491](https://github.com/stjude-rust-labs/sprocket/pull/491)).
* Added an experimental `sprocket dev test` command ([#468](https://github.com/stjude-rust-labs/sprocket/pull/468), [#489](https://github.com/stjude-rust-labs/sprocket/pull/489)).
* Added peak memory usage reporting at the DEBUG verbosity level ([#482](https://github.com/stjude-rust-labs/sprocket/pull/482)).
* Added support for WDL enumerations in preparation for WDL v1.3 ([#445](https://github.com/stjude-rust-labs/sprocket/pull/445)).

### Fixed

* `doc` now properly initializes pages in dark mode by default ([#478](https://github.com/stjude-rust-labs/sprocket/pull/478)).

## 0.19.0 - 11-21-2025

### Added

* Added `run.task.cache` and `run.task.cache_dir` settings to `sprocket.toml`
  for controlling call caching ([#461](https://github.com/stjude-rust-labs/sprocket/pull/461)).
* Added `--no-call-cache` option to `sprocket run` to disable use of the call
  cache for a specific run ([#461](https://github.com/stjude-rust-labs/sprocket/pull/461)).
* Added `--azure-account-name` and `--azure-access-key` CLI options to
  `sprocket run` ([#454](https://github.com/stjude-rust-labs/sprocket/pull/454)).
* New lint rule `DocMetaStrings` to ensure reserved meta and parameter_meta
  keys have string values ([#407](https://github.com/stjude-rust-labs/sprocket/pull/407)).
* A `run.fail` option was added to `sprocket.toml` for controlling the default
  failure mode ([#444](https://github.com/stjude-rust-labs/sprocket/pull/444)).
* Added the `split` standard library function in preparation for WDL v1.3 ([#424](https://github.com/stjude-rust-labs/sprocket/pull/424)).
* Added support for `else if` and `else` clauses in conditional statements (in
  support of WDL v1.3) ([#411](https://github.com/stjude-rust-labs/sprocket/pull/411)).
* Added feature flags support to enable experimental WDL versions ([#411](https://github.com/stjude-rust-labs/sprocket/pull/411)).
* Added shell expansion to the `apptainer_images_dir` config option, though
  this is an interim workaround for HPC path awkwardness pending the removal of
  this option entirely in the future ([#435](https://github.com/stjude-rust-labs/sprocket/pull/435)).
* Added experimental Slurm + Apptainer backend ([#436](https://github.com/stjude-rust-labs/sprocket/pull/436)).

### Changed

* Sprocket now supports "slow" and "fast" failure modes for evaluation errors
  an interruptions (Ctrl-C) ([#444](https://github.com/stjude-rust-labs/sprocket/pull/444)).
* The `wdl-analysis` config flag that enables experimental WDL v1.3 features
  was renamed from `experimental_versions` to `wdl_1_3` ([#435](https://github.com/stjude-rust-labs/sprocket/pull/435)).
* Removed the `wdl-cli` crate, absorbing its code into the `sprocket` library
  crate in preparation for future refactoring ([#450](https://github.com/stjude-rust-labs/sprocket/pull/450)).
* Apptainer-based backends now store converted container images within each run directory, rather than in a user-specified directory ([#463](https://github.com/stjude-rust-labs/sprocket/pull/463)).
* `sprocket run` now writes a `.sprocketignore` file directly to the `runs/` directory instead of the `runs/<entrypoint>/` directory ([#481](https://github.com/stjude-rust-labs/sprocket/pull/481)).


### Fixed

* Fixed a bug in `sprocket config init` where `sprocket.toml` was unnecessarily loaded and would fail if malformed ([#473](https://github.com/stjude-rust-labs/sprocket/pull/473)).
* Fixed Sprocket commands not always showing the full context of errors ([#472](https://github.com/stjude-rust-labs/sprocket/pull/472)).
* running `sprocket run` now writes a `.sprocketignore` file to the runs directory, which will tell subsequent Sprocket commands to ignore its contents ([#469](https://github.com/stjude-rust-labs/sprocket/pull/469)).
* Improved the portability of generated Apptainer scripts ([#442](https://github.com/stjude-rust-labs/sprocket/pull/442)).
* Fixed the handling of unusual filenames in generated Apptainer scripts ([#459](https://github.com/stjude-rust-labs/sprocket/pull/459)).

## 0.18.0 - 10-14-2025

### Fixed

* `doc` and `format` now work if analysis errors **and not parse errors** are
  encountered ([#402](https://github.com/stjude-rust-labs/sprocket/pull/402)).
* `sprocket inputs` now correctly handles complex values (including empty or
  interpolated Strings) ([#388](https://github.com/stjude-rust-labs/sprocket/pull/388), [#399](https://github.com/stjude-rust-labs/sprocket/pull/399)).

### Changed

* `format` subcommand has been re-implemented with a new CL API ([#365](https://github.com/stjude-rust-labs/sprocket/pull/365)).

### Added

* Added support for accepting input file paths by URL ([#386](https://github.com/stjude-rust-labs/sprocket/pull/386)).
* Accept multiple `--config` options on the Sprocket CLI ([#383](https://github.com/stjude-rust-labs/sprocket/pull/383)).
* `-c, --config` and `-s, --skip-config-search` are now global arguments (they can now appear after any subcommand) ([#365](https://github.com/stjude-rust-labs/sprocket/pull/365)).
* Added experimental LSF + Apptainer backend ([#182](https://github.com/stjude-rust-labs/sprocket/pull/182), [#372](https://github.com/stjude-rust-labs/sprocket/pull/372), [#378](https://github.com/stjude-rust-labs/sprocket/pull/378), [#379](https://github.com/stjude-rust-labs/sprocket/pull/379), [#404](https://github.com/stjude-rust-labs/sprocket/pull/404))

## 0.17.1 - 09-17-2025

### Fixed

* Allow "bad" `SPROCKET_CONFIG` environment variables to exist, although the
  user will get a warning if the specified path doesn't exist ([#178](https://github.com/stjude-rust-labs/sprocket/pull/178)).

### Dependencies

* Bumped `wdl` dependency to latest ([#179](https://github.com/stjude-rust-labs/sprocket/pull/179)).

## 0.17.0 - 09-16-2025

### Added

* Added `--unredact` option to `sprocket config resolve` ([#173](https://github.com/stjude-rust-labs/sprocket/pull/173)).
* Added options to `sprocket check/lint` for enabling and disabling sets of
  lint rules based on the rules' tags ([#169](https://github.com/stjude-rust-labs/sprocket/pull/169)).
* Added options to `sprocket dev doc` for embedding a JS file into `<script>`
  tags on each HTML page ([#170](https://github.com/stjude-rust-labs/sprocket/pull/170)).
* Added options to `sprocket run` for configuring AWS S3 and Google Cloud
  Storage authentication ([#164](https://github.com/stjude-rust-labs/sprocket/pull/164)).
* Added progress bars for file transfers ([#164](https://github.com/stjude-rust-labs/sprocket/pull/164)).

### Fixed

* `--no-color` argument to `format` is now respected ([#167](https://github.com/stjude-rust-labs/sprocket/pull/167)).
* `sprocket explain --tag <tag>` is now case-insensitive ([#168](https://github.com/stjude-rust-labs/sprocket/pull/168)).
* The `--deny-notes` argument to `check`/`lint` now correctly implies
  `--deny-warnings` ([#166](https://github.com/stjude-rust-labs/sprocket/pull/166)).

### Changed

* Enabling linting no longer runs every lint rule. Instead, a less opinionated
  set of rules are toggled on by default ([#169](https://github.com/stjude-rust-labs/sprocket/pull/169)).
* `sprocket dev doc` now initializes on the "Full Directory" view for the left
  sidebar ([#170](https://github.com/stjude-rust-labs/sprocket/pull/170)).
  * The old behavior (initializing on the "Workflows" view) can be enabled with
    an option.
* Replaced `sprocket run` progress bar implementation with one based off of
  Crankshaft events ([#164](https://github.com/stjude-rust-labs/sprocket/pull/164)).

## 0.16.0 - 08-13-2025

### Added

* Added support for `.sprocketignore` files ([#158](https://github.com/stjude-rust-labs/sprocket/pull/158)).
    * the semantics of these new "ignorefiles" are similar to `.gitignore` files
    * the commands `analyzer`, `check`/`lint`, and `doc` all respect these files
    * both parent and child directories of the current working directory are searched for `.sprocketignore` files
* Added support for custom logos in `sprocket dev doc` ([#156](https://github.com/stjude-rust-labs/sprocket/pull/156)).

## 0.15.0 - 07-31-2025

### Added

* Added `cpu_limit_behavior` and `memory_limit_behavior` config options to
  enable ignoring host resource limits ([wdl:#543](https://github.com/stjude-rust-labs/wdl/pull/543)).
* Added code completion to the LSP ([wdl:#519](https://github.com/stjude-rust-labs/wdl/pull/519)).
* Added new default output directory logic ([#149](https://github.com/stjude-rust-labs/sprocket/pull/149)).
* Individual analysis and lint rules can now be excepted when running the `
  analyzer` command ([#150](https://github.com/stjude-rust-labs/sprocket/pull/150)).
    * both command line flags and TOML config are supported

### Changed

* The `UnusedCall` analysis rule no longer emits a diagnostic for tasks and
  workflows if they have an empty or missing `output` section ([wdl:#532](https://github.com/stjude-rust-labs/wdl/pull/532)).
* `--name` option renamed to `--entrypoint` for `validate` and `run` ([#147](https://github.com/stjude-rust-labs/sprocket/pull/147)).
    * `--entrypoint` is now required if no inputs are provided.
    * `--entrypoint` will be prefixed to the key of any key-value pairs
      supplied on the command line.

### Removed

* Removed the `OutputSection` lint rule ([wdl:#532](https://github.com/stjude-rust-labs/wdl/pull/532)).

## 0.14.1 - 07-10-2025

### Fixed

* Fixed the resolution of relative input files ([wdl:#523](https://github.com/stjude-rust-labs/wdl/pull/523))

## 0.14.0 - 07-09-2025

### Changed

* Removed the `--config` option of `sprocket run`; the run command's
  configuration is now merged into `sprocket.toml` under the `run` section ([#121](github.com/stjude-rust-labs/sprocket/pull/121))

### Fixed

* The `ShellCheck` lint rule has been revisited to reduce false positives ([wdl:#457](https://github.com/stjude-rust-labs/wdl/pull/457)).
* Fixed unhelpful error message in `sprocket validate` ([#133](https://github.com/stjude-rust-labs/sprocket/pull/133)).
* Fixed run configuration to not use a default configuration when there is an
  error in the flattened engine configuration fields ([#124](https://github.com/stjude-rust-labs/sprocket/pull/124)).
* The `sprocket run`, `sprocket validate`, and `sprocket inputs` commands will
  no longer require the `--name` option if passed a WDL document containing a
  single task and no workflow ([#121](github.com/stjude-rust-labs/sprocket/pull/121)).
* The `sprocket run` command now correctly includes the workflow/task name
  prefix in the output ([#131](github.com/stjude-rust-labs/sprocket/pull/131)).

### Added

* The LSP now supports "falling back" to interpresting WDL documents as v1.2
  when the version is unrecognized (e.g. `version development`) ([wdl:#475](https://github.com/stjude-rust-labs/wdl/pull/475)).
* `check`, `lint`, and `format` will now default to the CWD if no `source`
  argument is provided ([#137](https://github.com/stjude-rust-labs/sprocket/pull/137)).
* Added `dev` subcommand to contain developmental and experimental subcommands ([#120](https://github.com/stjude-rust-labs/sprocket/pull/120)).
* Added `dev lock` subcommand to store container manifest checksums ([#120](https://github.com/stjude-rust-labs/sprocket/pull/120)).
* Added `dev doc` subcommand for documenting WDL workspaces ([#107](https://github.com/stjude-rust-labs/sprocket/pull/107)).

### Removed

* `format` no longer accepts the input `-` for STDIN ([#137](https://github.com/stjude-rust-labs/sprocket/pull/137)).

## 0.13.0 - 05-28-2025

### Added

* Added tab completions for `sprocket` commands ([#105](https://github.com/stjude-rust-labs/sprocket/pull/105)).
* Introduced the `inputs` subcommand ([#113](https://github.com/stjude-rust-labs/sprocket/pull/113)).

### Fixed

* Added `shellcheck` to Dockerfile ([#114](https://github.com/stjude-rust-labs/sprocket/pull/114)).
* Fixed `check --except` and `explain` rule not being case-insensitive ([#116](https://github.com/stjude-rust-labs/sprocket/issues/116)).

## Dependencies

* Updates dependencies (including `wdl` to `v0.14.0`).

## 0.12.2 - 05-05-2025

### Fixed

* Fix `sprocket run` not printing analysis diagnostics ([#110](https://github.com/stjude-rust-labs/sprocket/pull/110)).

## 0.12.1 - 05-05-2025

### Fixed

* Fixes parsing of input files ([#106](https://github.com/stjude-rust-labs/sprocket/pull/106)).
* Removes unused `--shellcheck` argument ([#106](https://github.com/stjude-rust-labs/sprocket/pull/106)).

## 0.12.0 - 05-02-2025

### Added

* Introduced the `run` subcommand ([#102](https://github.com/stjude-rust-labs/sprocket/pull/102)).

### Changed

* Unknown `--except` rules will now emit a warning instead of being silently ignored ([#94](https://github.com/stjude-rust-labs/sprocket/pull/94))
* Changed the `validate-inputs` subcommand to the more concise `validate` subcommand ([#102](https://github.com/stjude-rust-labs/sprocket/pull/102)).
* Changed all existing subcommands to use the facilities provided in `wdl-cli` when possible ([#102](https://github.com/stjude-rust-labs/sprocket/pull/102)).
* Updates the underlying `wdl` dependency to v0.13.1 ([#102](https://github.com/stjude-rust-labs/sprocket/pull/102)).

### Added

* Added configuration file support ([#104](https://github.com/stjude-rust-labs/sprocket/pull/104)).

## 0.11.0 - 04-01-2025

### Added

* Added `--hide_notes` to `check` to filter out note diagnostics from reporting ([#84](https://github.com/stjude-rust-labs/sprocket/pull/84))
* YAML support for `validate-inputs` command ([#79](https://github.com/stjude-rust-labs/sprocket/pull/79)).
* Extend `explain` to display related rules, list tags using `--t`, show WDL definitions using `--definitions` ([#80](https://github.com/stjude-rust-labs/sprocket/pull/80)).

### Changed

* Updated WDL crates to latest ([#79](https://github.com/stjude-rust-labs/sprocket/pull/79)). This added many features and fixes. Some highlights:
    * Fixed certain misplaced highlights from the `ShellCheck` lint.
    * Relaxed the `CommentWhitespace` lint rule so it doesn't trigger for as
      many comments.
    * The `ImportSort` lint rule now supplies the correct order of imports in
      the `fix` message.
* By default, when checking a local file, suppress diagnostics from remote
  files. Added a `--show-remote-diagnostics` flag to recreate the older
  behavior ([#59](https://github.com/stjude-rust-labs/sprocket/pull/59)).
* Always emit any diagnostics with a `Severity::Error` regardless of other CL
  options that might suppress the diagnostic
  ([#59](https://github.com/stjude-rust-labs/sprocket/pull/59)).

### Fixed

* Bug introduced in [#59](https://github.com/stjude-rust-labs/sprocket/pull/59)
  which sometimes caused the exit message to have an incorrect count of Notes
  and Warnings ([#61](https://github.com/stjude-rust-labs/sprocket/pull/61)).

## 0.10.1 - 01-23-2025

### Fixed

* URLs can be checked/linted ([#58](https://github.com/stjude-rust-labs/sprocket/pull/58)).

### Added

* Added a `Dockerfile` and automation to release Docker images with each Sprocket version ([#56](https://github.com/stjude-rust-labs/sprocket/pull/56)).

## 0.10.0 - 01-17-2025

### Added

* Added `--local-only` and `--single-document` args to `check/lint` ([#48](https://github.com/stjude-rust-labs/sprocket/pull/48)).
* Added a `validate-inputs` command. ([#48](https://github.com/stjude-rust-labs/sprocket/pull/48)).

### Changed

* `format` now requires one of the `--check` or `--overwrite` arguments ([#51](https://github.com/stjude-rust-labs/sprocket/pull/51)).
* Updated WDL crate to latest. This adds support for
  checking/linting remote URLs and other features and improvements ([#48](https://github.com/stjude-rust-labs/sprocket/pull/48)).
* Only allow one `file` argument to `check/lint` instead of any number of local
  files and directories ([#48](https://github.com/stjude-rust-labs/sprocket/pull/48)).

## 0.9.0 - 10-22-2024

### Changed

* Updated WDL crate to latest; this includes some important fixes to using
  `sprocket` on Windows and Linux ([#35](https://github.com/stjude-rust-labs/sprocket/pull/35)).

## 0.8.0 - 10-16-2024

### Added

* Added the `format` subcommand to sprocket ([#24](https://github.com/stjude-rust-labs/sprocket/pull/24)).
* Added the analysis rules to `sprocket explain` ([#24](https://github.com/stjude-rust-labs/sprocket/pull/24)).

### Changed

* Update to version 0.9.0 of `wdl` crate; this pulls in new lint rules,
  formatting support, and completes static analysis for the `check` and `lint`
  subcommands ([#24](https://github.com/stjude-rust-labs/sprocket/pull/24)).

## 0.7.0 - 09-16-2024

### Changed

* Implemented the `check` command as a full static analysis ([#17](https://github.com/stjude-rust-labs/sprocket/pull/17)).

### Fixed

* Fixed the progress bar from showing up for short analysis jobs; it now is
  delayed by two seconds ([#19](https://github.com/stjude-rust-labs/sprocket/pull/19)).

## 0.6.0 - 08-22-2024

### Added

* Added `analyzer` subcommand to sprocket ([#9](https://github.com/stjude-rust-labs/sprocket/pull/9)).
* Updated dependencies to latest ([#9](https://github.com/stjude-rust-labs/sprocket/pull/9)).

### Changed

* Update to version 0.7.0 of `wdl` crate. This pulls in many new lint rules.

## 0.5.0 - 07-17-2024

### Changed

* Update to version 0.6.0 of `wdl` crate.

## 0.4.0 - 07-01-2024

### Added

* `--except` arg to `check --lint` and `lint` subcommands.

### Changed

* Update to version 0.5.0 of `wdl` crate. This enables lint directive comments
  (AKA `#@` comments) among other new features.

## 0.3.0 - 06-18-2024

### Added

* `check` subcommand with `--lint` parameter

### Changed

* Update to version 0.4.0 of `wdl` crate. This features a new parser
  implementation

## 0.2.1 - 06-05-2024

### Fixed

* exit code `2` if there are no parse errors or validation failures, but there
  are lint warnings.
  * exit code `1` if there are parse errors or validation failures; exit code
    `0` means there were no concerns found at all.

## 0.2.0 - 06-03-2024

### Added

* `explain` command

### Changed

* Update to version 0.3.0 of `wdl` crate. This pulls in new lint rules.
