# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

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
