## This is a test of an unknown type name.

version 1.2

import "foo.wdl"

struct Foo {
    # Unknown
    Bar bar
    # OK
    Baz baz
}

struct Baz {
    X x
    Int y
}
