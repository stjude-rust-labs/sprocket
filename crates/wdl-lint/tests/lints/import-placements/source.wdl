## This is a test of import placements.

version 1.3

import "bar.wdl"  # OK
import "baz.wdl"  # OK
import "foo.wdl"  # OK

enum Color {
    Red,
}

import "late.wdl" as Late  # BAD

workflow test {
    #@ except: MetaDescription
    meta {}

    output {}
}

import "jam.wdl"   # BAD
import "qux.wdl"   # BAD
