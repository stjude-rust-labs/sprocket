Fuzz targets for the `wdl-*` crates.

These are intended to be run with [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz).

## Usage

```shell
cargo fuzz run <TARGET>
```

Where `<TARGET>` is one of the targets in `./fuzz_targets`.

See the [`cargo-fuzz` book](https://rust-fuzz.github.io/book/introduction.html) for more information.