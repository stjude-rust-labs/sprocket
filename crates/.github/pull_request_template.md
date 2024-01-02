<!-- 
When creating a pull request, you should uncomment the section below that describes
the type of pull request you are submitting.
-->

<!-- START SECTION: adding a new linting/validation rule

This pull request adds a new rule to `wdl`.

- **Rule Name**: `a_rule_name`
- **Rule Kind**: Lint warning/Validation error
- **Rule Code**: `v1::W001`
- **Packages**: `wdl-ast`/`wdl-grammar`

_Describe the rules you have implemented and link to any relevant issues._

Before submitting this PR, please make sure:

- [ ] You have added a few sentences describing the PR here.
- [ ] You have added yourself or the appropriate individual as the assignee.
- [ ] You have added at least one relevant code reviewer to the PR.
- [ ] Your code builds clean without any errors or warnings.

Rule specific checks:

- [ ] You have added the rule as an entry within the the package-specific rule
      tables (`wdl-ast/src/v1.rs` for AST-based rules and 
      `wdl-grammar/src/v1.rs` for parse tree-based rules).
- [ ] You have added the rule as an entry within the the global rule
      table at `RULES.md`.
- [ ] You have added the rule to the appropriate `fn rules()`.
    - Validation rules added to `wdl-ast` should be added to `fn rules()` within 
      `wdl-ast/src/v1/validation.rs`.
    - Lint rules added to `wdl-ast` should be added to `fn rules()` within `wdl-ast/src/v1/lint.rs`.
    - Validation rules added to `wdl-grammar` should be added to `fn rules()` within 
      `wdl-grammar/src/v1/validation.rs`.
    - Lint rules added to `wdl-grammar` should be added to `fn rules()` within 
      `wdl-grammar/src/v1/lint.rs`.
- [ ] You have added a test that covers every possible setting for the rule 
      within the file where the rule is implemented.

END SECTION -->

<!-- START SECTION: any other pull request

_Describe the problem or feature in addition to a link to the issues._

Before submitting this PR, please make sure:

- [ ] You have added a few sentences describing the PR here.
- [ ] You have added yourself or the appropriate individual as the assignee.
- [ ] You have added at least one relevant code reviewer to the PR.
- [ ] Your code builds clean without any errors or warnings.
- [ ] You have added tests (when appropriate).
- [ ] You have updated the README or other documentation to account for these changes (when appropriate).

END SECTION -->