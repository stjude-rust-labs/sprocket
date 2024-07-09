## This is a test of aliasing a struct that does not exist in an import.

version 1.1

import "foo.wdl" alias Foo as Bar alias Bar as Baz alias Qux as Qux2

struct Qux {
    Int x
}
