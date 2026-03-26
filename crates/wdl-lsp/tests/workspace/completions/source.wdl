version 1.3

import "lib.wdl" as util

struct Foo {
    Bar bar
}

struct Bar {
    String baz
}

workflow main {
    Foo my_foo = Foo {
        bar: Bar {
            baz: "hello",
        },
    }

    call util.greet as t1 { name = my_foo.bar.baz }

    String n = my_foo.

    output {
        String result = t1.out
    }
}
