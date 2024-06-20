## This is a test of import placements.

version 1.1

import "foo.wdl"    # OK
import "bar.wdl"    # OK
import "baz.wdl"    # OK

workflow test {

}

import "qux.wdl"    # BAD
import "jam.wdl"    # BAD
