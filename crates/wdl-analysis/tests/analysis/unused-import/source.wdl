## This is a test of an unused import

version 1.1

import "foo.wdl" alias X as Y

# This import is unused
import "bar.wdl"

# This import is unused, but excepted
#@ except: UnusedImport
import "bar.wdl" as ok

import "baz.wdl"

struct X {
    # This uses a type from `foo`
    Y y
}

workflow test {
    # This uses a workflow from `baz`
    call baz.test

    output {
        Int x = test.x
    }
}
