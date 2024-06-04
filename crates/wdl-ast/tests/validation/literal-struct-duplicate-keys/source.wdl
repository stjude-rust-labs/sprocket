# This is a test for duplicate keys in a literal structs.

version 1.1

workflow test {
    Foo foo = Foo {
        foo: "first",
        bar: "first",
        baz: "first",

        foo: "dup",
        bar: "dup",
        foo: "dup",
    }
}
