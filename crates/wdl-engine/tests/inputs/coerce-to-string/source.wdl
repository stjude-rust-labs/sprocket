## This is a test that non-string input types supplied by the user can coerce to
## WDL strings. See https://github.com/stjude-rust-labs/wdl/issues/527 for why this
## is needed. `error.txt` should be empty.

version 1.1

task takes_a_string {
    input {
        String number
    }

    command <<<>>>
}
