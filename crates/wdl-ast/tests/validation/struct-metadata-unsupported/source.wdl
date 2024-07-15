## This is a test of struct metadata sections in a WDL 1.1 document.

version 1.1

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
