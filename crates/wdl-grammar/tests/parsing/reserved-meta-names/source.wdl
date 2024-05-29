# This is a test of reserved names in metadata sections.

version 1.1

task test {
    meta {
        version: "1.1.1"
    }

    parameter_meta {
        task: "foo"
    }
}
