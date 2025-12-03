# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

#### Fixed

* Validate constraints in docker backend and point error messages to problematic `hints`/`requirements` sections ([#484](https://github.com/stjude-rust-labs/sprocket/pull/484)).

## 0.10.0 - 11-21-2025

#### Added

* Added call caching configuration to `TaskConfig` ([#461](https://github.com/stjude-rust-labs/sprocket/pull/461)).
* Implemented support for [call caching](https://github.com/stjude-rust-labs/rfcs/pull/2)
  in `TaskEvaluator` ([#461](https://github.com/stjude-rust-labs/sprocket/pull/461)).
* Added a new `fail` configuration option for controlling the default failure mode of the engine ([#444](https://github.com/stjude-rust-labs/sprocket/pull/444)).
* Added the `split` standard library function in preparation for WDL v1.3 ([#424](https://github.com/stjude-rust-labs/sprocket/pull/424)).
* Added support for `else if` and `else` clauses in conditional statements (in support of WDL v1.3) ([#411](https://github.com/stjude-rust-labs/sprocket/pull/411)).
* Added shell expansion to the `apptainer_images_dir` config option, though this is an interim workaround for HPC path awkwardness pending the removal of this option entirely in the future ([#435](https://github.com/stjude-rust-labs/sprocket/pull/435)).
* Added experimental Slurm + Apptainer backend ([#436](https://github.com/stjude-rust-labs/sprocket/pull/436)).
* Introduced pre-evaluation task type for all pre-evaluation contexts (task requirements, task hints, and task runtime sections) and expanded support of `task.previous` for post-evaluation sections in WDL v1.3 ([#432](https://github.com/stjude-rust-labs/sprocket/pull/432)).
* Added GPU support to the Docker backend ([#439](https://github.com/stjude-rust-labs/sprocket/pull/439)).

#### Changed

* Azure Storage authentication configuration has been changed to use shared key authentication rather than explicit SAS token authentication; SAS token authentication can still be used by directly adding the query parameters to any input URLs ([#454](https://github.com/stjude-rust-labs/sprocket/pull/454)).
* Changed how cancellation is supported by the engine; the engine can now wait for executing tasks to complete before canceling them (slow failure mode) or immediately cancel the executing tasks (fast failure mode) ([#444](https://github.com/stjude-rust-labs/sprocket/pull/444)).
* Added optional CPU and memory limits to the queue definitions in the LSF + Apptainer backend configuration. This is a breaking change for previous LSF configurations, as the queues are now a struct with a required `name` string field, rather than just a bare string ([#429](https://github.com/stjude-rust-labs/sprocket/pull/429)).
* Changed a number of types in the public interface in preparation for a larger refactoring ([#460](https://github.com/stjude-rust-labs/sprocket/pull/460)).
* Introduced a unified `TopLevelEvaluator` type as a common context for task and workflow evaluations ([#463](https://github.com/stjude-rust-labs/sprocket/pull/463)).
* Apptainer-based backends now store converted container images within each run directory, rather than in a user-specified directory ([#463](https://github.com/stjude-rust-labs/sprocket/pull/463)).

#### Fixed

* Improved the portability of generated Apptainer scripts ([#442](https://github.com/stjude-rust-labs/sprocket/pull/442)).
* Fixed the handling of unusual filenames in generated Apptainer scripts ([#459](https://github.com/stjude-rust-labs/sprocket/pull/459)).

#### Removed

* Removed the `codespan` cargo feature in favor of enabling codespan reporting always ([#462](https://github.com/stjude-rust-labs/sprocket/pull/462)).


## 0.9.0 - 10-14-2025

#### Added

* Added support for calling `glob` with a remote working directory ([#416](https://github.com/stjude-rust-labs/sprocket/pull/416)).
* Added `retries` configuration setting for the TES backend ([#408](https://github.com/stjude-rust-labs/sprocket/pull/408)).
* Added support for passing `None` for non-optional inputs with default
  expressions in WDL 1.2 call statements ([#356](https://github.com/stjude-rust-labs/sprocket/pull/356)).
* Added experimental LSF + Apptainer backend ([#182](https://github.com/stjude-rust-labs/sprocket/pull/182), [#372](https://github.com/stjude-rust-labs/sprocket/pull/372), [#378](https://github.com/stjude-rust-labs/sprocket/pull/378), [#379](https://github.com/stjude-rust-labs/sprocket/pull/379), [#404](https://github.com/stjude-rust-labs/sprocket/pull/404))

#### Fixed

* Fixed checking for existence of `File` and `Directory` values that are remote
  URLs ([#416](https://github.com/stjude-rust-labs/sprocket/pull/416)).
* Fixed a panic that can occur when showing debug output with the TES backend ([#397](https://github.com/stjude-rust-labs/sprocket/pull/397)).
* Make linking to download cache files more likely by using a tmp directory in
  the cache ([#393](https://github.com/stjude-rust-labs/sprocket/pull/393)).

## 0.8.1 - 09-17-2025

#### Fixed

* Fixed incorrect assertion for the TES backend ([#606](https://github.com/stjude-rust-labs/wdl/pull/606)).
* Fixed permissions issue in the Docker backend when a container runs with a
  different user ([#605](https://github.com/stjude-rust-labs/wdl/pull/605)).

## 0.8.0 - 09-15-2025

#### Added

* Added support for uploading inputs to the TES backend ([#599](https://github.com/stjude-rust-labs/wdl/pull/599)).
* Implemented coercion between `Map` <-> `Object`/`Struct` where the map key
  type <-> `String` ([#586](https://github.com/stjude-rust-labs/wdl/pull/586)).

#### Changed

* Replaced remote file downloading with using `cloud-copy` ([#599](https://github.com/stjude-rust-labs/wdl/pull/599)).
* Changed how inputs are evaluated to prevent host paths from being observed in
  evaluated command sections ([#589](https://github.com/stjude-rust-labs/wdl/pull/589)).
* Removed evaluation progress callbacks in favor of Crankshaft events channel ([#583](https://github.com/stjude-rust-labs/wdl/pull/583)).

#### Fixed

* Use `IndexMap` for stable serialization of `Config` ([#602](https://github.com/stjude-rust-labs/wdl/pull/602)).
* Fixed deserialization of `Object` to no longer require keys be WDL
  identifiers ([#586](https://github.com/stjude-rust-labs/wdl/pull/586)).
* Fixed a panic caused by an incorrect type calculation of non-empty array
  literals ([#585](https://github.com/stjude-rust-labs/wdl/pull/585)).
* Fixed incorrect common type calculations from `None` values ([#584](https://github.com/stjude-rust-labs/wdl/pull/584)).

## 0.7.0 - 08-13-2025

#### Added

* Added an experimental config flag to support golden testing that reduces
  environment-specific output ([#553](https://github.com/stjude-rust-labs/wdl/pull/553)).

#### Fixed

* Removed mistaken `-C` argument to `bash` invocations ([#558](https://github.com/stjude-rust-labs/wdl/pull/558)).

## 0.6.0 - 07-31-2025

#### Added

* Added `cpu_limit_behavior` and `memory_limit_behavior` options to task
  execution configuration ([#543](https://github.com/stjude-rust-labs/wdl/pull/543))
* Serialize `Pair` as `Object` for execution-level `inputs.json` and `outputs.json` ([#538](https://github.com/stjude-rust-labs/wdl/pull/538)).

#### Changed

* `wdl-engine::Inputs` supplied via dotted path notation (i.e. user inputs from
  input files and command line arguments) can be implicitly converted to WDL
  strings if that is what the task or workflow input expects ([#544](https://github.com/stjude-rust-labs/wdl/pull/544)).

#### Fixed

* Fixed a failure to clean input file and directory paths ([#537](https://github.com/stjude-rust-labs/wdl/pull/537)).
* Fixed a panic that may occur in array and map literal evaluation ([#529](https://github.com/stjude-rust-labs/wdl/pull/529)).

## 0.5.0 - 07-09-2025

#### Added

* TES input and outputs now include authentication query parameters ([#466](https://github.com/stjude-rust-labs/wdl/pull/466)).

#### Fixed

* Fixed guest paths for redirected stdio for both the Docker and TES backends ([#470](https://github.com/stjude-rust-labs/wdl/pull/470)).

#### Changed

* Backend configuration has changed to allow multiple backends to be defined ([#469](https://github.com/stjude-rust-labs/wdl/pull/469)).

## 0.4.0 - 05-27-2025

#### Added

* Implemented a TES task execution backend ([#454](https://github.com/stjude-rust-labs/wdl/pull/454)).
* Adds the `insecure` option to the TES backend configuration ([#459](https://github.com/stjude-rust-labs/wdl/pull/459)).

#### Dependencies

* Bumps dependencies.

## 0.3.2 - 05-05-2025

#### Fixed

* JSON and YAML files are now correctly parsed ([#440](https://github.com/stjude-rust-labs/wdl/pull/440)).
* The `From<IndexMap<String, Value>>` method was moved to the private
  constructor `wdl_engine::Object::new()`, as there are some guarantees the
  caller has to uphold that weren't obvious in the `From` implementation ([#440](https://github.com/stjude-rust-labs/wdl/pull/440)).

#### Dependencies

* Bumps dependencies.

## 0.3.1 - 05-02-2025

_A patch bump was required because an error was made during the release of `wdl` v0.13.0 regarding dependencies._

## 0.3.0 - 05-01-2025

#### Added

* Added writing `inputs.json` and `outputs.json` for each task and workflow
  that was evaluated ([#437](https://github.com/stjude-rust-labs/wdl/pull/437)).
* Implemented remote file localization for task execution ([#386](https://github.com/stjude-rust-labs/wdl/pull/386)).
* Implemented concurrent file downloads for localization for task execution ([#424](https://github.com/stjude-rust-labs/wdl/pull/424)).

#### Fixed

* Fix overly verbose call stacks in task failure messages ([#435](https://github.com/stjude-rust-labs/wdl/pull/435))
* Fix `sub` replacement of multiple instances ([#426](https://github.com/stjude-rust-labs/wdl/pull/426)).
* Fix path translation in more expressions ([#422](https://github.com/stjude-rust-labs/wdl/pull/422)).
* The `sep` placeholder option was not performing guest path translation ([#417](https://github.com/stjude-rust-labs/wdl/pull/417)).
* Placeholder options are now type checked at runtime ([#345](https://github.com/stjude-rust-labs/wdl/pull/345)).
* Whether or not a task manager state represents unlimited resources is now
  correctly calculated ([#397](https://github.com/stjude-rust-labs/wdl/pull/397)).
* Fixed environment variable values are not using guest paths for Docker
  backend ([#398](https://github.com/stjude-rust-labs/wdl/pull/398)).
* Ensure output files created by Docker tasks running as root have correct host
  user permissions ([#379](https://github.com/stjude-rust-labs/wdl/pull/379)).
* Fixes `chown` functionality by making the path absolute ([#428](https://github.com/stjude-rust-labs/wdl/pull/379)).

#### Changed

* Refactored `crankshaft` backend to the `docker` backend ([#436](https://github.com/stjude-rust-labs/wdl/pull/436)).
* Evaluation errors now contain a "backtrace" containing call locations ([#432](https://github.com/stjude-rust-labs/wdl/pull/432)).
* Changed origin path resolution in inputs to accommodate incremental command
  line parsing ([#430](https://github.com/stjude-rust-labs/wdl/pull/430)).

## 0.2.0 - 04-01-2025

#### Added

* Added support for cloud storage URIs ([#367](https://github.com/stjude-rust-labs/wdl/pull/367)).
* Added support reading of remote files from the stdlib file functions ([#364](https://github.com/stjude-rust-labs/wdl/pull/364))
* Added support for YAML input files (.yml and .yaml) alongside JSON ([#352](https://github.com/stjude-rust-labs/wdl/pull/352)).
* Added support for graceful cancellation of evaluation ([#327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Added support for `max_cpu` and `max_memory` hints in task evaluation ([#327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Added a Crankshaft backend with initial support for Docker ([#327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Added calculation for mounting input files for future backends that use
  containers ([#323](https://github.com/stjude-rust-labs/wdl/pull/323)).
* Added retry logic for task execution ([#320](https://github.com/stjude-rust-labs/wdl/pull/320)).
* Added a `Config` type for specifying evaluation configuration ([#320](https://github.com/stjude-rust-labs/wdl/pull/320)).
* Added progress callback to `WorkflowEvaluator` ([#310](https://github.com/stjude-rust-labs/wdl/pull/310)).

#### Fixed

* Fixed support for URLs in file stdlib functions ([#369](https://github.com/stjude-rust-labs/wdl/pull/369)).
* Fixed panic when an input path in a complex type did not exist ([#327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Fixed path translation in nested placeholder evaluation ([#327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Fixed path translation to mount inputs individually ([#327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Fixed not including task temp directories in mounts ([#327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Fixed an incorrect type being used for scatter statement outputs ([#316](https://github.com/stjude-rust-labs/wdl/pull/316)).
* Fixed handling of input dependencies in workflow graph evaluation ([#360](https://github.com/stjude-rust-labs/wdl/pull/360)).

#### Changed

* Make stdlib file functions asynchronous ([#359](https://github.com/stjude-rust-labs/wdl/pull/359)).
* Refactored expression evaluation to make it async ([#357](https://github.com/stjude-rust-labs/wdl/pull/357)).
* Updated for refactored `wdl-ast` API so that evaluation can now operate
  directly on AST nodes in `async` context ([#355](https://github.com/stjude-rust-labs/wdl/pull/355)).
* Updated to Rust 2024 edition ([#353](https://github.com/stjude-rust-labs/wdl/pull/353)).
* Docker backend is now the default backend ([#327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Refactored a common task management implementation to use in task execution
  backends ([#327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Workflow evaluation now uses `tokio::spawn` internally for running graph
  evaluation concurrently ([#320](https://github.com/stjude-rust-labs/wdl/pull/320)).
* Improved evaluation reporting to include how many tasks are ready for
  execution ([#320](https://github.com/stjude-rust-labs/wdl/pull/320)).
* Updates the `crankshaft` and `http-cache-stream-reqwest` dependencies to official, upstreamed crates ([#383](https://github.com/stjude-rust-labs/wdl/pull/383)).

## 0.1.0 - 01-17-2025

#### Fixed

* Limited the local task executor to a maximum level of concurrency ([#292](https://github.com/stjude-rust-labs/wdl/pull/292))
* Fixed regression in workflow input validation when an input is missing ([#286](https://github.com/stjude-rust-labs/wdl/pull/286)).
* Fixed input validation to not treat directly specified call inputs as missing ([#282](https://github.com/stjude-rust-labs/wdl/pull/282)).

#### Added

* Added evaluation support for the WDL 1.2 `env` declaration modifier ([#296](https://github.com/stjude-rust-labs/wdl/pull/296)).
* Implemented workflow evaluation ([#292](https://github.com/stjude-rust-labs/wdl/pull/292))
* Reduced size of the `Value` type ([#277](https://github.com/stjude-rust-labs/wdl/pull/277)).
* Implement task evaluation with local execution and remaining WDL 1.2
  functionality ([#265](https://github.com/stjude-rust-labs/wdl/pull/265)).
* Implement the `defined` and `length` functions from the WDL standard library ([#258](https://github.com/stjude-rust-labs/wdl/pull/258)).
* Fixed `Map` values not accepting `None` for keys ([#257](https://github.com/stjude-rust-labs/wdl/pull/257)).
* Implement the generic map functions from the WDL standard library ([#257](https://github.com/stjude-rust-labs/wdl/pull/257)).
* Implement the generic array functions from the WDL standard library ([#256](https://github.com/stjude-rust-labs/wdl/pull/256)).
* Implement the string array functions from the WDL standard library ([#255](https://github.com/stjude-rust-labs/wdl/pull/255)).
* Replaced the `Value::from_json` method with `Value::deserialize` which allows
  for deserialization from any self-describing data format; a method for
  serializing a value was also added ([#254](https://github.com/stjude-rust-labs/wdl/pull/254)).
* Implemented the file functions from the WDL standard library ([#254](https://github.com/stjude-rust-labs/wdl/pull/254)).
* Implemented the string functions from the WDL standard library ([#252](https://github.com/stjude-rust-labs/wdl/pull/252)).
* Implemented call evaluation and the numeric functions from the WDL standard
  library ([#251](https://github.com/stjude-rust-labs/wdl/pull/251)).
* Implemented WDL expression evaluation ([#249](https://github.com/stjude-rust-labs/wdl/pull/249)).
* Refactored API to remove reliance on the engine for creating values ([#249](https://github.com/stjude-rust-labs/wdl/pull/249)).
* Split value representation into primitive and compound values ([#249](https://github.com/stjude-rust-labs/wdl/pull/249)).
* Added `InputFiles` type for parsing WDL input JSON files ([#241](https://github.com/stjude-rust-labs/wdl/pull/241)).
* Added the `wdl-engine` crate that will eventually implement a WDL execution
  engine ([#225](https://github.com/stjude-rust-labs/wdl/pull/225)).

#### Changed

* Removed the `Engine` type in favor of direct use of a `WorkflowEvaluator` or
  `TaskEvaluator` ([#292](https://github.com/stjude-rust-labs/wdl/pull/292))
* Require file existence for a successful validation parse of inputs ([#281](https://github.com/stjude-rust-labs/wdl/pull/281)).
