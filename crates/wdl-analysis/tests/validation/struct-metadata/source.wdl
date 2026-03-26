## This is a test of struct metadata sections in a WDL 1.2 document.
## This test should have no diagnostics.

version 1.3

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
