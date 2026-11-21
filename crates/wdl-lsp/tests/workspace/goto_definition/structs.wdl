version 1.4

struct Qux {
    Int x
}

struct Baz {
    Qux qux
}

struct Bar {
    Baz baz
}

struct Foo {
    Bar bar
}

task structs {
    input {
        Foo foo
    }

    command <<<
    >>>

    output {
        Int x = foo.bar.baz.qux.x
    }
}
