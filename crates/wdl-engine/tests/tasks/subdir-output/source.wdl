## This is a test to ensure that task outputs may refer to existing sub-paths of a directory input.
## See: https://github.com/stjude-rust-labs/sprocket/issues/931
version 1.3

task test {
    input {
        Directory dir
    }

    # Perform some unnecessary conversions which internally map guest <-> host
    String s = dir + "/sub"
    Directory d = s + "/../sub/../sub"

    command <<<
    >>>

    output {
        # These outputs should all have the same value
        Directory sub = d
        Directory sub2 = join_paths(dir, "sub")
        Directory sub3 = join_paths(d, "../sub/../sub/../sub")
    }
}
