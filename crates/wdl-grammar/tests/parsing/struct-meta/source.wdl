## This is a test of parsing struct meta and parameter_meta sections.

version 1.4

struct Foo {
    Int a

    meta {
        foo: "bar"
    }

    parameter_meta {
        a: "foo"
        b: "bar"
    }

    String b
}
