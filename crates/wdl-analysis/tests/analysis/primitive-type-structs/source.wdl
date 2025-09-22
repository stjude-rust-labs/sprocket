## This is a test of the primitive type structs constraint.

version 1.2

struct Foo {
    String foo
}

struct Bar {
    Foo foo
}

workflow test {
    # This is OK as the struct contains only primitive members
    File ok = write_tsv([Foo { foo: "hi" }])

    # This is not OK as the struct contains a compound member
    File bad = write_tsv([Bar { foo: Foo { foo: "hi" } }])

    output {
        File o1 = ok
        File o2 = bad
    }
}
