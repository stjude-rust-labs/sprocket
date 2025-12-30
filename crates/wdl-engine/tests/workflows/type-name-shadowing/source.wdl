version 1.3

enum Foo {
    Foo,
    Bar,
    Baz,
}

struct Bar {
    Int x
}

workflow example {
    input {
        Bar Foo = Bar { x: 1 }
    }

    # Variable Foo (of type Bar) shadows the enum type Foo
    # So Foo.x should access the struct member, not the enum
    Int x = Foo.x

    output {
        Int result = x
    }
}
