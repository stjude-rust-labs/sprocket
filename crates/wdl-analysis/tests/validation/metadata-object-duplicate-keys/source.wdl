# This is a test for duplicate keys in a metadata objects.

version 1.1

task test {
    meta {
        foo: {
            foo: {
                bar: "first",
                baz: "first",
                foo: "first",

                bar: "dup",
            },

            foo: "dup",
        }
    }

    command <<<>>>
}
