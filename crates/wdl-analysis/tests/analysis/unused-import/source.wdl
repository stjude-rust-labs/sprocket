## This is a test of an unused import
version 1.1

# This import is unused
import "bar.wdl"

# This import is unused, but excepted
#@ except: UnusedImport
import "bar.wdl" as ok
import "baz.wdl"
import "foo.wdl" alias X as Y

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
