#@ except: UnusedCall
## This is a test of having multiple namespaces in a call statement.

version 1.1

import "foo.wdl"

workflow test {
    call foo.bar.baz
}
