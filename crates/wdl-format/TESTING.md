# Testing

The `wdl-format` tests compare the exact formatted output via `.formatted.wdl` files. These files are
automatically generated, see [Updating outputs](#updating-outputs).

## Updating outputs

Once the output looks correct, the `.formatted.wdl` file(s) can be updated by setting the `BLESS` environment variable.

For example:

```bash
BLESS=1 cargo test [TEST_NAME]
```

## Configured tests

If a directory contains a `config.toml` file, the formatting will be run twice. Once with the new config and once with the default
config. The runs will generate `source.formatted.wdl` and `source.default.formatted.wdl` respectively.
