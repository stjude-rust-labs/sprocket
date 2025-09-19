## This is a test of recursive struct definitions

version 1.1

import "foo.wdl" alias Foo as Buzz

# Recursive
struct Foo {
    Bar b
}

# Recursive
struct Bar {
    Foo f
}

# OK
struct Baz {
    Int x
}

# OK
struct Qux {
    Baz b
    Buzz buzz
}
