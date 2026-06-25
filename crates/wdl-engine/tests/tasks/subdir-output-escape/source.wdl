## This is a test to ensure that task outputs cannot escape an input base directory.
## See: https://github.com/stjude-rust-labs/sprocket/issues/931
version 1.3

task test {
    input {
        Directory dir
    }

    command <<<
    >>>

    output {
        Directory ok = dir + "/../../foo/sub"
        Directory bad = dir + "/../../bar"
    }
}
