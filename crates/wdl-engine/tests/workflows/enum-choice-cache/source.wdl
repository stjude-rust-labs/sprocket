## This test is to ensure the enum choice cache works across documents that have the same enum index and choice index.

version 1.3

import "other.wdl" alias Foo as OtherFoo

enum Foo {
    Bar = "baz"
}

workflow test {
    call other.test { foo = OtherFoo.Bar }

    output {
        # This should produce `baz`
        String a = value(Foo.Bar)
        # This should produce `qux`
        String b = value(OtherFoo.Bar)
        # This should also produce `qux`
        String c = value(test.bar)
    }
}
