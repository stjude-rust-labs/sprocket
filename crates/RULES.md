# Rules 

This table documents all implemented `wdl` rules (validation failures and lint
warnings) implemented on the `main` branch of the `stjude-rust-labs/wdl`
repository. Note that the information may be out of sync with released packages.

# Validation Failures

| Name                       | Code       | Package                                 |
|:---------------------------|:-----------|:----------------------------------------|
| `invalid_escape_character` | `v1::E001` | [`wdl-grammar`][wdl-grammar-validation] |
| `invalid_version`          | `v1::E002` | [`wdl-grammar`][wdl-grammar-validation] |
| `duplicate_runtime_keys`   | `v1::E003` | [`wdl-grammar`][wdl-grammar-validation] |
| `missing_literal_commas`   | `v1::E004` | [`wdl-grammar`][wdl-grammar-validation] |

# Lint Warnings

| Name                      | Code       | Group        | Package                            |
|:--------------------------|:-----------|:-------------|:-----------------------------------|
| `whitespace`              | `v1::W001` | Style        | [`wdl-grammar`][wdl-grammar-lints] |
| `no_curly_commands`       | `v1::W002` | Pedantic     | [`wdl-grammar`][wdl-grammar-lints] |
| `matching_parameter_meta` | `v1::W003` | Completeness | [`wdl-ast`][wdl-ast-lints]         |
| `mixed_indentation`       | `v1::W004` | Style        | [`wdl-grammar`][wdl-grammar-lints] |
| `missing_runtime_block`   | `v1::W005`  | Completeness | [`wdl-grammar`][wdl-grammar-lints] |

[wdl-ast-lints]: https://docs.rs/wdl-ast/latest/wdl_ast/v1/index.html#lint-rules
[wdl-ast-validation]: https://docs.rs/wdl-ast/latest/wdl_ast/v1/index.html#validation-rules
[wdl-grammar-lints]: https://docs.rs/wdl-grammar/latest/wdl_grammar/v1/index.html#lint-rules
[wdl-grammar-validation]: https://docs.rs/wdl-grammar/latest/wdl_grammar/v1/index.html#validation-rules