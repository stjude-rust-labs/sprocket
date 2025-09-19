# This is a test for duplicate keys in a metadata section.

version 1.1

task t {
    meta {
        foo: "first"
        bar: "first"
        baz: "first"

        foo: "dup"
        bar: "dup"
        foo: "dup"
    }

    command <<<>>>
}

workflow w {
    meta {
        foo: "first"
        bar: "first"
        baz: "first"

        foo: "dup"
        bar: "dup"
        foo: "dup"
    }
}
