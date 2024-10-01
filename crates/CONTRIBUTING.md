# Welcome to the `wdl` crates!

Community contributions rock and we are psyched you're reading this document!

## Quick links

- Bug reports go [here][issues]
- Feature requests are welcome and go [here](https://github.com/stjude-rust-labs/wdl/discussions/categories/feature-requests)
- Lint rule proposals go [here](https://github.com/stjude-rust-labs/wdl/discussions/categories/rule-proposals)

## FAQs

### How do I start contributing?

We encourage you to reach out to the core team prior to writing up a pull request. This is to ensure there isn't any wasted effort or duplication of work caused by miscommunication. You can get in touch with us via the [issues][issues] or hop over to the [discussions](https://github.com/stjude-rust-labs/wdl/discussions). We are also active on the [openwdl Slack workspace](https://openwdl.slack.com). (Discussion about this repo best belongs in the #sprocket channel 😃)

### How do I set up Rust?

[The official Rust docs guide](https://www.rust-lang.org/tools/install).

### What IDE should I use?

Most of this team uses VScode with the `rust-analyzer` extension but that preference is not hardcoded anywhere. Feel free to use any IDE you want!

### What's a good first issue?

We will try to keep a handful of [issues][issues] marked `good first issue` open and ready for new contributors.

### I don't want to write code, can I still contribute?

Sure!

You can always open a [discussion](https://github.com/stjude-rust-labs/wdl/discussions/categories/rule-proposals) with a proposal for a new lint rule or contribute to any open discussions.

We also appreciate feedback on our documentation. Feel free to look over any of our `*.md` files and note any issues you find. You can also explore our lint rule documentation by [installing `sprocket`](https://stjude-rust-labs.github.io/sprocket/installation.html) and reading the output of `sprocket explain`. (n.b.: we hope to replace `sprocket explain` with a website where each rule will have a dedicated page, but that has not been realized yet)

### What's the difference between `error`, `warning`, and `note`?

- an `error` is emitted when the source WDL is incorrect or invalid in some way
    - errors should not be emitted by `wdl-lint`
- a `warning` is emitted when the source WDL is confusing, problematic, error-prone, etc. but not invalid or incorrect
- a `note` is emitted in all other cases and is mostly used for issues of style or conformity

### What is gauntlet?

[Gauntlet](https://github.com/stjude-rust-labs/wdl/tree/main/gauntlet) is the main driver of our CI. Take a look at the file [`Gauntlet.toml`](https://github.com/stjude-rust-labs/wdl/blob/main/Gauntlet.toml). The entries at the top are all GitHub repositories of WDL code. The remaining entries are diagnostics emitted while analyzing those repositories. These should remain relatively static between PRs, and any change in emitted diagnostics should be reviewed carefully.

In order to turn the Gauntlet CI green, run `cargo run --release --bin gauntlet -- --refresh`. The `--refresh` flag will save any changes to the `Gauntlet.toml` file. This should then be committed and included in your PR.

### What is arena?

Arena is the alternate run mode of `gauntlet`. [`Arena.toml`](https://github.com/stjude-rust-labs/wdl/blob/main/Arena.toml) is very similar to `Gauntlet.toml`, except it has fewer repository entries and instead of analysis diagnostics it contains only lint diagnostics (which are not included in `Gauntlet.toml`).

In order to turn the Arena CI green, run `cargo run --release --bin gauntlet -- --arena --refresh`. The `--refresh` flag (in conjunction with the `--arena` flag) will save any changes to the `Arena.toml` file. This should then be committed and included in your PR.

### The CI has turned red. How do I make it green again?

There are a handful of reasons the CI may have turned red. Try the following fixes:

- `cargo +nightly fmt` to format your Rust code
- `cargo clippy --all-features` and then fix any warnings emitted
- `BLESS=1 cargo test --all-features` to "bless" any test changes
    - Please review any changes this causes to make sure they seem right!
- `cargo run --release --bin gauntlet -- --refresh`
    - see the `What is gauntlet?` question for more information
- `cargo run --release --bin gauntlet -- --refresh --arena`
    - see the `What is arena?` question for more information
- `rustup update` to update your local toolchains

### What's the general workflow for writing a new lint rule?

1. create a `wdl-lint/src/rules/<>.rs` file and tinker until you are calling `exceptable_add()` in some case
    - review the existing rules in `wdl-lint/src/rules/` for guidance on this
2. write a `wdl-lint/tests/lints/<>/source.wdl` that has cases that should and should not trigger the above call to `exceptable_add()`
3. run `BLESS=1 cargo test -p wdl-lint --all-features` to generate a `source.errors`
    - this file should not be edited by hand
4. review `source.errors` to see if it matches our expectations
    - this isn't exactly true, but I conceptualize `source.errors` as the output if a user ran a `lint` command on `source.wdl`
    - while reviewing, ask yourself if the printed diagnostics are clear and informative
5. repeat

### Can you explain how rules use `exceptable_nodes()` and `exceptable_add()`?

Every lint rule has an ID which can be used in lint directives (comments beginning with `#@ except:`) to prevent them from emitting diagnostics for portions of a WDL document. Rules "excepted" during the preamble (comments which are before the version statement) will be turned off for the entire document; otherwise, lint directives will shut off a rule while processing the children of whatever node it comes before, but only if that node is in the rule's `exceptable_nodes()` list. `exceptable_add()` will check all the ancestors of `element` for nodes that match the `exceptable_nodes()` list and see if they have a lint directive disabling the current rule; if so, the diagnostic will not be added to the validation output.

#### Further reading

* `exceptable_add()` defined [here](https://github.com/stjude-rust-labs/wdl/blob/wdl-v0.8.0/wdl-ast/src/validation.rs#L50).
* See [here](https://docs.rs/wdl/latest/wdl/grammar/type.SyntaxNode.html) for the `SyntaxNode` docs.
* The PR which introduced `exceptable_nodes()` and `exceptable_add()` is [#162](https://github.com/stjude-rust-labs/wdl/pull/162).
* That PR fixed issue [#135](https://github.com/stjude-rust-labs/wdl/issues/135)

[issues]: https://github.com/stjude-rust-labs/wdl/issues