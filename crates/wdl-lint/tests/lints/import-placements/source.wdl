#@ except: DescriptionMissing
## This is a test of import placements.

version 1.1

import "bar.wdl"    # OK
import "baz.wdl"    # OK
import "foo.wdl"    # OK

workflow test {
    meta {}
    output {}
}

import "jam.wdl"    # BAD
import "qux.wdl"    # BAD
