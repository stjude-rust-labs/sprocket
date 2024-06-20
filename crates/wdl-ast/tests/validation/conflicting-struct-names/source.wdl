## This is a test of having conflicting struct names.

version 1.1

import "foo" alias Foo as Baz

struct Foo {
    Int x
}

struct Bar {
    Int x
}

struct Foo {
    Int x
}

struct Bar {
    Int x
}

struct Baz {
    Int x
}

import "Bar" alias Baz as Foo

import "qux" alias A as Qux alias B as Qux
