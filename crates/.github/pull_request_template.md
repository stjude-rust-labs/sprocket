<!-- 
When creating a pull request, you should uncomment the section below that 
describes the type of pull request you are submitting.
-->

<!-- START SECTION: adding a new lint rule

This pull request adds a new rule to `wdl-lint`.

- **Rule Name**: `MyLintRule`

_Describe the rules you have implemented and link to any relevant issues._

Before submitting this PR, please make sure:

- [ ] You have added a few sentences describing the PR here.
- [ ] You have added yourself or the appropriate individual as the assignee.
      (If you are an external contributor without permissions, leave this
      unchecked and someone else will do it for you.)
- [ ] You have added at least one relevant code reviewer to the PR.
      (If you are an external contributor without permissions, leave this
      unchecked and someone else will do it for you.)
- [ ] Your code builds clean without any errors or warnings.
- [ ] You have added an entry to the relevant `CHANGELOG.md` (see
      ["keep a changelog"] for more information).
- [ ] Your commit messages follow the [conventional commit] style.
- [ ] Your PR title follows the [conventional commit] style.

Rule specific checks:

- [ ] You have added the rule as an entry within `RULES.md`.
- [ ] You have added the rule to the `rules()` function in `wdl-lint/src/lib.rs`.
- [ ] You have added a test case in `wdl-lint/tests/lints` that covers every
      possible diagnostic emitted for the rule within the file where the rule
      is implemented.
- [ ] If you have implemented a new `Visitor` callback, you have also
      overridden that callback method for the special `Validator`
      (`wdl-ast/src/validation.rs`) and `LintVisitor`
      (`wdl-lint/src/visitor.rs`) visitors. These are required to ensure the new
      visitor callback will execute.
- [ ] You have run `gauntlet --bless` to ensure that there are no 
      unintended changes to the baseline configuration file (`Gauntlet.toml`).
- [ ] You have run `gauntlet --bless --arena` to ensure that all of the 
      rules added/removed are now reflected in the baseline configuration file 
      (`Arena.toml`).

END SECTION -->

<!-- START SECTION: any other pull request

_Describe the problem or feature in addition to a link to the issues._

Before submitting this PR, please make sure:

- [ ] You have added a few sentences describing the PR here.
- [ ] You have added yourself or the appropriate individual as the assignee.
      (If you are an external contributor without permissions, leave this
      unchecked and someone else will do it for you.)
- [ ] You have added at least one relevant code reviewer to the PR.
      (If you are an external contributor without permissions, leave this
      unchecked and someone else will do it for you.)
- [ ] Your code builds clean without any errors or warnings.
- [ ] You have added tests (when appropriate).
- [ ] You have updated the README or other documentation to account for these
      changes (when appropriate).
- [ ] You have added an entry to the relevant `CHANGELOG.md` (see
      ["keep a changelog"] for more information).
- [ ] Your commit messages follow the [conventional commit] style.
- [ ] Your PR title follows the [conventional commit] style.

END SECTION -->

[conventional commit]: https://www.conventionalcommits.org/en/v1.0.0/#summary
["keep a changelog"]: https://keepachangelog.com/en/1.1.0/
