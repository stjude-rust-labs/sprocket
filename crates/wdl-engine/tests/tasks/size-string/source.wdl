## This is a test to ensure the `size` function coerces `String` arguments correctly.
## See: https://github.com/stjude-rust-labs/sprocket/issues/575
version 1.2

task repro {
    input {
        File file
    }

    String file2 = file

    command <<<>>>

    output {
        Float size = size(file2)
    }
}
