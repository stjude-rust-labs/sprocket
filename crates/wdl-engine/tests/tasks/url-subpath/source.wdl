## This is a test to ensure that task outputs may refer to existing sub-paths of URL inputs
## See: https://github.com/stjude-rust-labs/sprocket/issues/931
version 1.3

task test {
    input {
        Directory dir
    }

    # Perform some unnecessary conversions which internally map guest <-> host
    String s = dir
    File f = join_paths(s, "index.html/../index.html/../index.html")

    command <<<
    >>>

    output {
        # These outputs should all have the same value
        File sub = f
        File sub2 = join_paths(dir, "index.html")
        File sub3 = join_paths(dir, "index.html/../index.html/../index.html")
    }
}
