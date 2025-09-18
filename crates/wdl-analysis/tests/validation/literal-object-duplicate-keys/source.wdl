# This is a test for duplicate keys in a literal object.

version 1.1

workflow test {
    Object o = object {
        foo: "first",
        bar: "first",
        baz: "first",

        foo: "dup",
        bar: "dup",
        foo: "dup",
    }
}
