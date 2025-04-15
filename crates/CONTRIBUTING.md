# Welcome to the `wdl` crates!

Community contributions rock and we are psyched you're reading this document!

## Quick links

- Bug reports go [here][issues]
- Feature requests are welcome and go [here](https://github.com/stjude-rust-labs/wdl/discussions/categories/feature-requests)
- Lint rule proposals go [here](https://github.com/stjude-rust-labs/wdl/discussions/categories/rule-proposals)

## How can I start contributing?

### I don't want to write code, can I still contribute?

Sure!

You can always open a [discussion](https://github.com/stjude-rust-labs/wdl/discussions/categories/rule-proposals) with a proposal for a new lint rule or other enhancement as well contributing to any open discussions. Ensure that you provide a clear and concise title and description of the desired addition. For lint rules in particular, WDL examples and/or links to the relevant portion of the WDL spec are extremely useful.

We also welcome bug reports. If you discover a flaw in our codebase, please review the list of open issues to ensure that it is not a duplicate. Please also attempt to debug the issue locally and ensure that it is not a configuration issue. Once you have done both, please file a new issue providing the relevant information in the issue. Please provide the exact steps to reproduce the problem, specific example(s) that demonstrate the steps, and the behavior you observe as well as the behavior you expected to observe. For terminal-based use cases, a copy and paste of the command and error log (please use markdown formatting appropriately) is preferred. For interactive use cases (such as the VSCode extension), screenshots and/or GIFs are welcome ways to provide additional information to maintainers.

We also appreciate feedback on our documentation. Feel free to look over any of our `*.md` files and note any issues you find. You can also explore our lint rule documentation by [installing `sprocket`](https://stjude-rust-labs.github.io/sprocket/installation.html) and reading the output of `sprocket explain`. (n.b.: we hope to replace `sprocket explain` with a website where each rule will have a dedicated page, but that has not been realized yet).

The maintainers reserve the right to close issues and discussions as deemed necessary as well as to delete comments and interactions within the repository.

### Your first code contribution

We encourage you to reach out to the core team prior to writing up a pull request. **This is to ensure there isn't any wasted effort or duplication of work caused by miscommunication. Failure to do so may result in the rejection of the pull request.** You can get in touch with us via the [issues][issues] or hop over to the [discussions](https://github.com/stjude-rust-labs/wdl/discussions). We are also active on the [openwdl Slack workspace](https://openwdl.slack.com). (Discussion about this repo best belongs in the #sprocket channel 😃)

We encourage contributors to comment on open issues that they intend to work on to help avoid duplication of effort. If multiple individuals are interested in solving the same issue, we recommend reaching out to one another to gauge if there is potential for a collaboration.

That being said, we will not assign issues to external contributors, and commenting on an issue does not guarantee exclusive rights to work on that issue. If multiple PRs are received for the same issue, the PR that (a) most thoroughly addresses the problem being solved and (b) has the best implementation by judgement of the St. Jude Rust Labs team will be accepted in favor of the other submitted PRs.

### Review Policy

Our pull request template has an extensive checklist that must be completed prior to review. Our policy is that any PRs submitted with an incomplete checklist will not be reviewed. Part of this checklist includes ensuring that our CI checks pass. Additional guidance for satisfying the CI checks can be [found below](#the-ci-has-turned-red-how-do-i-make-it-green-again-ci-green).

Note that the maintainers reserve the right to close any submission without review for any reason.

## FAQs

### Can I use Artificial Intelligence (AI)?

We have found that AI, while helpful in some contexts, causes more confusion and work for all parties involved when interacting with a large, complex codebase such as the `wdl` family of crates. To that end, no PRs including AI-generated content—whether that be generated code, generated documentation, generated discussion via GitHub comments, or any other AI generated content—will be accepted from external contributors. Any submissions deemed to be AI-generated from external contributors will be closed without review.

### How do I set up Rust?

[The official Rust docs guide](https://www.rust-lang.org/tools/install).

### What IDE should I use?

Most of this team uses VScode with the `rust-analyzer` extension but that preference is not hardcoded anywhere. Feel free to use any IDE you want!

### What's a good first issue?

We will try to keep a handful of [issues][issues] marked `good first issue` open and ready for new contributors.

### What's the difference between `error`, `warning`, and `note`?

- an `error` is emitted when the source WDL is incorrect or invalid in some way
    - errors should not be emitted by `wdl-lint`
- a `warning` is emitted when the source WDL is confusing, problematic, error-prone, etc. but not invalid or incorrect
- a `note` is emitted in all other cases and is mostly used for issues of style or conformity

### What is gauntlet?

[Gauntlet](https://github.com/stjude-rust-labs/wdl/tree/main/gauntlet) is the main driver of our CI. Take a look at the file [`Gauntlet.toml`](https://github.com/stjude-rust-labs/wdl/blob/main/Gauntlet.toml). The entries at the top are all GitHub repositories of WDL code. The remaining entries are diagnostics emitted while analyzing those repositories. These should remain relatively static between PRs, and any change in emitted diagnostics should be reviewed carefully.

In order to turn the Gauntlet CI green, run `cargo run --release --bin gauntlet -- --bless`. The `--bless` flag will save any changes to the `Gauntlet.toml` file. This should then be committed and included in your PR.

### What is arena?

Arena is the alternate run mode of `gauntlet`. [`Arena.toml`](https://github.com/stjude-rust-labs/wdl/blob/main/Arena.toml) is very similar to `Gauntlet.toml`, except it has fewer repository entries and instead of analysis diagnostics it contains only lint diagnostics (which are not included in `Gauntlet.toml`).

In order to turn the Arena CI green, run `cargo run --release --bin gauntlet -- --arena --bless`. The `--bless` flag (in conjunction with the `--arena` flag) will save any changes to the `Arena.toml` file. This should then be committed and included in your PR.

### The CI has turned red. How do I make it green again?

There are a handful of reasons the CI may have turned red. Try the following fixes:

- `cargo +nightly fmt` to format your Rust code
- `cargo clippy --all-features` and then fix any warnings emitted
- `BLESS=1 cargo test --all-features` to "bless" any test changes
    - Please review any changes this causes to make sure they seem right!
- `cargo run --release --bin gauntlet -- --bless`
    - see the `What is gauntlet?` question for more information
- `cargo run --release --bin gauntlet -- --bless --arena`
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
5. consider which nodes should be "exceptable" for this rule
    - See "[How should I decide which nodes to add to `exceptable_nodes()`?](#how-should-i-decide-which-nodes-to-add-to-exceptable_nodes)" for more information on this process
6. consider if your new rule relates to existing ones and implement the `related_rules()` method accordingly.
    - See "[How should i decide which rules to link using related_rules()?](#how-should-i-decide-which-rules-to-link-using-related_rules)" for detailed guidance.
7. repeat

### Can you explain how rules use `exceptable_nodes()` and `exceptable_add()`?

Every lint rule has an ID which can be used in lint directives (comments beginning with `#@ except:`) to prevent them from emitting diagnostics for portions of a WDL document. Rules "excepted" during the preamble (comments which are before the version statement) will be turned off for the entire document; otherwise, lint directives will shut off a rule while processing the children of whatever node it comes before, but only if that node is in the rule's `exceptable_nodes()` list. `exceptable_add()` will check all the ancestors of `element` for nodes that match the `exceptable_nodes()` list and see if they have a lint directive disabling the current rule; if so, the diagnostic will not be added to the validation output.

### How should I decide which nodes to add to `exceptable_nodes()`?

Returning `None` from `exceptable_nodes()` will enable lint directives to work anywhere in the document. This should be used sparingly, and is generally not the desired behavior. The most common case for returning `None` is if the lint rule pertains to "trivia" (whitespace and comments) which are permitted to appear almost anywhere in a WDL document.

_Every_ rule that returns `Some(_)` for `exceptable_nodes()` should include `SyntaxKind::VersionStatementNode` as the first entry. This is to ensure the rule can be disabled for an entire document.

The other `SyntaxKind` nodes that should be returned should be _intuitive_, _minimal_, and _comprehensive_.

Intuitive: users shouldn't need to know anything about the internal CST or AST structure to determine where to add a lint directive. However, we can reasonably expect users to intuit the nested structure of a WDL document. There are top level items (workflow, task, and struct definitions), and then each of those top level items has a variety of sections contained within, which in turn may have further items contained within them. Excepting a rule somewhere "higher" in the nested structure will disable the rule for everything "beneath".

Minimal: We want to have as few exceptable nodes as possible for each rule, while still enabling the most intuitive locations to be excepted. If two nodes would cover the same set of potential diagnostics, _always_ prefer the more specific node. e.g. for a rule relating to `input` sections, the `InputSectionNode`, `TaskDefinitionNode`, and `WorkflowDefinitionNode` would all cover the exact same set of diagnostics (as there can be at most one input section for each task or workflow definition); so we would prefer to have `InputSectionNode` returned by `exceptable_nodes()` rather than either `TaskDefinitionNode` or `WorkflowDefinitionNode`.

Comprehensive: Try to cover as many _unique_ sets of potential diagnostics as possible with the nodes returned by `exceptable_nodes()`. An individual diagnostic should be able to targeted by a lint directive, and so should any unique group of diagnostics.

### How should I decide which rules to link using `related_rules()`?

When deciding which rules should link to which other rules, please consider these criteria:

- If diagnostic `X` is likely to co-occur with diagnostic `Y` within the same lint pass (addressing them often happens together):
    - rule `X` should implement `related_rules` to include `Y`'s ID, and rule `Y` should implement it to include `X`'s ID.
- If correcting diagnostic `X` naively (without considering other rules) is likely to result in diagnostic `Y` being emitted in a subsequent lint pass:
    - rule `X` should implement `related_rules` to include `Y`'s ID, but rule `Y` should _not_ link back to rule `X` in this case, as the user fixing `Y` isn't necessarily led back to the context of `X`.

## Further reading

* `exceptable_add()` defined [here](https://github.com/stjude-rust-labs/wdl/blob/wdl-v0.8.0/wdl-ast/src/validation.rs#L50).
* See [here](https://docs.rs/wdl/latest/wdl/grammar/type.SyntaxNode.html) for the `SyntaxNode` docs.
* The PR which introduced `exceptable_nodes()` and `exceptable_add()` is [#162](https://github.com/stjude-rust-labs/wdl/pull/162).
* That PR fixed issue [#135](https://github.com/stjude-rust-labs/wdl/issues/135)
* The PR which introduced `related_rules()` is [#371](https://github.com/stjude-rust-labs/wdl/pull/371)

[issues]: https://github.com/stjude-rust-labs/wdl/issues
