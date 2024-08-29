## This is a test for a missing required struct member in a struct literal.

version 1.1

struct Foo {
    Int a
    Int x
    Int? y
    Int z
}

task test {
    # OK
    Foo a = Foo { a: 0, x: 1, z: 3 }
    # Missing z
    Foo b = Foo { x: 1, a: 3 }
    # Missing a
    Foo c = Foo { x: 1, y: 2, z: 3 }
    # Missing a and z
    Foo d = Foo { x: 1, y: 2 }
    # Missing a, x, and z
    Foo e = Foo { y: 2 }
    # Missing a, x, and z
    Foo f = Foo { }

    command <<<>>>
}
