# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

* Added support for cloud storage URIs (#[367](https://github.com/stjude-rust-labs/wdl/pull/367)).
* Added support reading of remote files from the stdlib file functions (#[364](https://github.com/stjude-rust-labs/wdl/pull/364))
* Added support for YAML input files (.yml and .yaml) alongside JSON ([#352](https://github.com/stjude-rust-labs/wdl/pull/352)).
* Added support for graceful cancellation of evaluation (#[327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Added support for `max_cpu` and `max_memory` hints in task evaluation (#[327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Added a Crankshaft backend with initial support for Docker (#[327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Added calculation for mounting input files for future backends that use
  containers ([#323](https://github.com/stjude-rust-labs/wdl/pull/323)).
* Added retry logic for task execution ([#320](https://github.com/stjude-rust-labs/wdl/pull/320)).
* Added a `Config` type for specifying evaluation configuration ([#320](https://github.com/stjude-rust-labs/wdl/pull/320)).
* Added progress callback to `WorkflowEvaluator` ([#310](https://github.com/stjude-rust-labs/wdl/pull/310)).

### Fixed

* Fixed support for URLs in file stdlib functions (#[369](https://github.com/stjude-rust-labs/wdl/pull/369)).
* Fixed panic when an input path in a complex type did not exist (#[327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Fixed path translation in nested placeholder evaluation (#[327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Fixed path translation to mount inputs individually (#[327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Fixed not including task temp directories in mounts (#[327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Fixed an incorrect type being used for scatter statement outputs ([#316](https://github.com/stjude-rust-labs/wdl/pull/316)).
* Fixed handling of input dependencies in workflow graph evaluation ([#360](https://github.com/stjude-rust-labs/wdl/pull/360)).

### Changed

* Make stdlib file functions asynchronous ([#359](https://github.com/stjude-rust-labs/wdl/pull/359)).
* Refactored expression evaluation to make it async ([#357](https://github.com/stjude-rust-labs/wdl/pull/357)).
* Updated for refactored `wdl-ast` API so that evaluation can now operate
  directly on AST nodes in `async` context ([#355](https://github.com/stjude-rust-labs/wdl/pull/355)).
* Updated to Rust 2024 edition ([#353](https://github.com/stjude-rust-labs/wdl/pull/353)).
* Docker backend is now the default backend (#[327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Refactored a common task management implementation to use in task execution
  backends (#[327](https://github.com/stjude-rust-labs/wdl/pull/327)).
* Workflow evaluation now uses `tokio::spawn` internally for running graph
  evaluation concurrently ([#320](https://github.com/stjude-rust-labs/wdl/pull/320)).
* Improved evaluation reporting to include how many tasks are ready for
  execution ([#320](https://github.com/stjude-rust-labs/wdl/pull/320)).

## 0.1.0 - 01-17-2025

### Fixed

* Limited the local task executor to a maximum level of concurrency ([#292](https://github.com/stjude-rust-labs/wdl/pull/292))
* Fixed regression in workflow input validation when an input is missing ([#286](https://github.com/stjude-rust-labs/wdl/pull/286)).
* Fixed input validation to not treat directly specified call inputs as missing ([#282](https://github.com/stjude-rust-labs/wdl/pull/282)).

### Added

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
* Added `InputFiles` type for parsing WDL input JSON files (#[241](https://github.com/stjude-rust-labs/wdl/pull/241)).
* Added the `wdl-engine` crate that will eventually implement a WDL execution
  engine (#[225](https://github.com/stjude-rust-labs/wdl/pull/225)).

### Changed

* Removed the `Engine` type in favor of direct use of a `WorkflowEvaluator` or
  `TaskEvaluator` ([#292](https://github.com/stjude-rust-labs/wdl/pull/292))
* Require file existence for a successul validation parse of inputs ([#281](https://github.com/stjude-rust-labs/wdl/pull/281)).
