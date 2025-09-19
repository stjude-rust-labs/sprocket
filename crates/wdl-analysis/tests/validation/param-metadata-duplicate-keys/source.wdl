# This is a test for duplicate keys in a parameter metadata section.

version 1.1

task t {
    parameter_meta {
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
    parameter_meta {
        foo: "first"
        bar: "first"
        baz: "first"

        foo: "dup"
        bar: "dup"
        foo: "dup"
    }
}
