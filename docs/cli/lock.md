# Lock Files

> [!CAUTION]
> This functionality is considered experimental. There is also currently no consumer for the information that this command writes out. In the future, the `run` command will be modified to utilize this information at execution time.

The Sprocket lock command searches the given input (or the current working directory if no input is provided) for WDL documents. Each WDL document is then loaded along with any of its imported dependencies. Within the WDL documents, each WDL `task` is checked for a `container` key in the `runtime` section. If a single literal value is found, the lock command attempts to get the detailed manifest from the container registry. For each `container` value that is found, the command gets the checksum for that image tag from the manifest. It then writes a `sprocket.lock` file with a record of each image encountered and the corresponding checksum at the time the lock command was run. It also records a timestamp of the command invocation.

> [!CAUTION]
> The following paragraph describes not-yet-implemented functionality.

The `sprocket.lock` file that is generated can then be consumed by the `run` subcommand. If the `sprocket.lock` file is available to the `run` command, then the execution engine will use the image map contained in the lock file to replace tagged image values with checksummed image values at runtime. The goal of this functionality is to help users comply with the guidance in the [specification](https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#container) to use the most specific URI to refer to containers, typically the digest, while allowing users and developers to continue using the human-readable tag values in their source. This functionality can also help ensure a level of consistency across runs for a cohort by "locking" the container values to immutable checksums, rather than mutable tags that are commonly utilized in WDL source.