# This is a test for duplicate keys in a runtime section.

version 1.1

task test {
    runtime {
        foo: "first"
        bar: "first"
        baz: "first"

        foo: "dup"
        bar: "dup"
        foo: "dup"
    }

    command <<<>>>
}

# Check for duplicated aliases in the runtime section.
task test2 {
    runtime {
        docker: "foo"
        container: "bar"
    }

    command <<<>>>
}
