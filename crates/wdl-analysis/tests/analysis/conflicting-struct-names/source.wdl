#@ except: UnusedImport
## This is a test of having conflicting struct names.

version 1.1

import "foo.wdl" alias Foo as Baz

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

import "bar.wdl" alias Baz as Foo

import "baz.wdl" alias A as Qux alias B as Qux
