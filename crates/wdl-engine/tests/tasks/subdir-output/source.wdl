## This is a test to ensure that task outputs may refer to existing sub-paths of a directory input.
## See: https://github.com/stjude-rust-labs/sprocket/issues/931
version 1.3

task test {
    input {
        Directory dir
    }

    command <<<
    >>>

    output {
        # These outputs should all have the same value
        Directory sub = dir + "/sub/../sub/../sub"
        Directory sub2 = join_paths(dir, "sub")
        Directory sub3 = join_paths(dir, "sub/../sub/../sub/../sub")
    }
}
