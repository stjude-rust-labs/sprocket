# This is a test of import statements.

version 1.1

import "a.wdl"
import "http://example.com/lib/stdlib.wdl" as stdlib
import "c.wdl" alias Foo as Bar alias Baz as Qux 
import "d.wdl" as d alias Foo as Bar alias Baz as Qux alias Quux as Corge
