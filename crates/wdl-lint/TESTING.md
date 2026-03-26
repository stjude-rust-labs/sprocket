# Testing

We make use of UI tests, which compare the exact diagnostic output from a linter via `.errors` files. These files are
automatically generated, see [Updating outputs](#updating-outputs).

## Updating outputs

Once the diagnostic output looks correct, the `.errors` file(s) can be updated by setting the `BLESS` environment variable.

For example

```bash
BLESS=1 cargo test [TEST_NAME]
```

## Configured tests

If a directory contains a `config.toml` file, the linter will be run twice. Once with the new config and once with the default
config. The runs will generate `source.errors` and `source.errors.default` respectively.
